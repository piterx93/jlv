//! Filtering the index (PRD §9.3): which entries pass every active filter, and the
//! level counters of the header (PRD §7.1).
//!
//! The filters combine with AND. An [`Engine`] evaluates them on a background
//! thread, a chunk of entries at a time, and publishes what it found after each
//! chunk. Setting new filters starts a new generation and cancels the one running.
//! Entries appended to the index are evaluated as they come, and an entry the index
//! read again is evaluated again.
//!
//! The level, time and class checks read the record and the class column of the
//! entry's first line; only the text check reads the rest of the entry.

use std::ops::Range;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Instant;

use jiff::Timestamp;
use memchr::memmem;
use regex::bytes::CaptureLocations;

use crate::format::{self, EntryParser, Level, Preset, strip_ansi_into};
use crate::index::Record;
use crate::matcher::Matcher;
use crate::source;

#[cfg(test)]
mod tests;

/// The active filters. The default filters nothing out.
#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub levels: Levels,
    pub classes: Classes,
    /// Only entries in this time range, while set. Entries without a timestamp are
    /// hidden while it is set.
    pub time: Option<TimeRange>,
    /// Only entries whose message matches, while set: the message column of the
    /// first line and every continuation line, with ANSI escape codes stripped. An
    /// entry without columns (the headless entry, any entry under `Raw`) is matched
    /// over all its lines.
    pub text: Option<Matcher>,
}

/// Which levels are shown. All of them by default. An entry without a level is
/// shown only while every level is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Levels {
    hidden: [bool; 6],
}

impl Levels {
    /// Shows `level` if `on`, else hides it.
    pub fn set(&mut self, level: Level, on: bool) {
        self.hidden[level as usize] = !on;
    }

    fn shows(&self, level: Option<Level>) -> bool {
        match level {
            Some(level) => !self.hidden[level as usize],
            None => !self.hidden.contains(&true),
        }
    }
}

/// Rules on the full class name, matched case-sensitively as substrings. An entry
/// passes if its class contains no `exclude` and, unless `include` is empty, at
/// least one `include`. An entry without a class passes only while `include` is
/// empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Classes {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Classes {
    fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    fn pass(&self, class: Option<&[u8]>) -> bool {
        let names = |rule: &String| {
            class.is_some_and(|class| memmem::find(class, rule.as_bytes()).is_some())
        };
        !self.exclude.iter().any(names)
            && (self.include.is_empty() || self.include.iter().any(names))
    }
}

/// From `start` (inclusive) to `end` (exclusive), or open-ended without `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start: Timestamp,
    pub end: Option<Timestamp>,
}

impl TimeRange {
    fn contains(&self, timestamp: Option<Timestamp>) -> bool {
        timestamp.is_some_and(|at| at >= self.start && self.end.is_none_or(|end| at < end))
    }
}

/// A file and its index, shared between the viewer, which keeps them up to date,
/// and the engine, which reads them a chunk at a time.
#[derive(Debug)]
pub struct Indexed<B> {
    /// The records of the file's entries, as the index gives them.
    pub records: Vec<Record>,
    /// The file the records index.
    pub bytes: Arc<B>,
}

/// How an [`Engine`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Entries per chunk, at least 1. Results are published and cancellation is
    /// noticed between chunks.
    pub chunk: usize,
}

/// What the current generation found so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filtered {
    /// How many entries, from the first on, are evaluated.
    pub evaluated: usize,
    /// The evaluated entries that pass every filter, as indices into the records,
    /// in ascending order.
    pub entries: Vec<u32>,
    /// Per level, indexed by `Level as usize`: the evaluated entries of that level
    /// that pass every filter but the level filter.
    pub counts: [u32; 6],
}

/// Evaluates the filters over an index on a background thread. Dropping it stops
/// the thread.
#[derive(Debug)]
pub struct Engine {
    jobs: Sender<Job>,
    updates: Receiver<Update>,
    /// Bumped whenever the results start over, so that updates of an earlier job,
    /// still in the channel, are told apart and dropped.
    generation: u64,
    filters: Arc<Filters>,
    filtered: Filtered,
    /// Per evaluated entry, the level it adds to `filtered.counts`, so that entries
    /// read again can be taken out of them.
    counted: Vec<Option<Level>>,
}

impl Engine {
    /// Starts evaluating `filters` over the entries of `indexed`, read in `preset`.
    pub fn spawn<B>(
        indexed: Arc<RwLock<Indexed<B>>>,
        preset: Preset,
        filters: Filters,
        options: Options,
    ) -> Engine
    where
        B: AsRef<[u8]> + Send + Sync + 'static,
    {
        let (jobs, job_receiver) = mpsc::channel();
        let (update_sender, updates) = mpsc::channel();
        let worker = Worker {
            indexed,
            parser: preset.entry_parser(),
            chunk: options.chunk.max(1),
            jobs: job_receiver,
            updates: update_sender,
        };
        thread::spawn(move || worker.run());
        let mut engine = Engine {
            jobs,
            updates,
            generation: 0,
            filters: Arc::new(filters),
            filtered: Filtered::default(),
            counted: Vec::new(),
        };
        engine.start();
        engine
    }

    /// Starts a new generation with `filters`. The results start over from no
    /// entry evaluated, and nothing the previous generation finds is published.
    pub fn set_filters(&mut self, filters: Filters) {
        self.filters = Arc::new(filters);
        self.truncate(0);
        self.start();
    }

    /// Tells the engine that the records from `from` on were read again or are new,
    /// after the viewer changed them. From this call on, the results cover at most
    /// the first `from` entries until the others are evaluated again.
    pub fn reread(&mut self, from: usize) {
        self.truncate(from);
        self.start();
    }

    /// The results of the current generation so far, with whatever the thread
    /// published since the last call taken in. Once `evaluated` reaches the number
    /// of records, they are complete until the records or the filters change.
    pub fn poll(&mut self) -> &Filtered {
        while let Ok(update) = self.updates.try_recv() {
            if update.generation == self.generation {
                self.take_in(update);
            }
        }
        &self.filtered
    }

    /// [`poll`](Self::poll), but waiting for the thread's updates until at least
    /// `evaluated` entries are evaluated or `deadline` passes, whichever comes first.
    pub fn poll_until(&mut self, evaluated: usize, deadline: Instant) -> &Filtered {
        self.poll();
        while self.filtered.evaluated < evaluated {
            let left = deadline.saturating_duration_since(Instant::now());
            let Ok(update) = self.updates.recv_timeout(left) else {
                break;
            };
            if update.generation == self.generation {
                self.take_in(update);
            }
        }
        &self.filtered
    }

    /// The results as the last [`poll`](Self::poll) left them, cut short by any
    /// [`reread`](Self::reread) or [`set_filters`](Self::set_filters) since.
    pub fn filtered(&self) -> &Filtered {
        &self.filtered
    }

    /// Hands the thread a new job from where the results end, which cancels the one
    /// it is on.
    fn start(&mut self) {
        self.generation += 1;
        // The thread only ends once `self.jobs` is dropped, so sending can't fail.
        let _ = self.jobs.send(Job {
            generation: self.generation,
            filters: Arc::clone(&self.filters),
            from: self.filtered.evaluated,
        });
    }

    fn truncate(&mut self, evaluated: usize) {
        if evaluated >= self.filtered.evaluated {
            return;
        }
        for level in self.counted.drain(evaluated..).flatten() {
            self.filtered.counts[level as usize] -= 1;
        }
        let entries = &mut self.filtered.entries;
        entries.truncate(entries.partition_point(|&entry| (entry as usize) < evaluated));
        self.filtered.evaluated = evaluated;
    }

    fn take_in(&mut self, update: Update) {
        for level in update.counted.iter().flatten() {
            self.filtered.counts[*level as usize] += 1;
        }
        self.filtered.evaluated += update.counted.len();
        self.filtered.entries.extend(update.entries);
        self.counted.extend(update.counted);
    }
}

/// What the thread is to evaluate: the entries from `from` on.
struct Job {
    generation: u64,
    filters: Arc<Filters>,
    from: usize,
}

/// What the thread found in the chunk of entries that follows the ones published
/// before in the same generation.
struct Update {
    generation: u64,
    /// The indices of the entries that pass every filter.
    entries: Vec<u32>,
    /// Per entry of the chunk, the level it adds to the counters, if any.
    counted: Vec<Option<Level>>,
}

struct Worker<B> {
    indexed: Arc<RwLock<Indexed<B>>>,
    parser: Option<EntryParser>,
    chunk: usize,
    jobs: Receiver<Job>,
    updates: Sender<Update>,
}

impl<B: AsRef<[u8]>> Worker<B> {
    /// Evaluates a chunk at a time until the job is done, then waits for the next.
    /// Ends when the engine is dropped, or when the viewer panicked holding the index.
    fn run(self) {
        let mut scratch = Scratch {
            buffer: Vec::new(),
            parser: self.parser.map(|parser| (parser, parser.locations())),
        };
        let Ok(job) = self.jobs.recv() else {
            return;
        };
        let Some(mut job) = self.newest(job) else {
            return;
        };
        loop {
            let Ok(indexed) = self.indexed.read() else {
                return;
            };
            let total = indexed.records.len();
            if job.from >= total {
                drop(indexed);
                let Some(next) = self.jobs.recv().ok().and_then(|next| self.newest(next)) else {
                    return;
                };
                job = next;
                continue;
            }
            let chunk = job.from..total.min(job.from + self.chunk);
            let check = Check {
                filters: &job.filters,
            };
            let update = check.chunk(&indexed, chunk.clone(), job.generation, &mut scratch);
            drop(indexed);
            if self.updates.send(update).is_err() {
                return;
            }
            job.from = chunk.end;
            let Some(newest) = self.newest(job) else {
                return;
            };
            job = newest;
        }
    }

    /// The last of `job` and the jobs waiting behind it, as each one cancels the
    /// one before. `None` once the engine is dropped.
    fn newest(&self, mut job: Job) -> Option<Job> {
        loop {
            match self.jobs.try_recv() {
                Ok(newer) => job = newer,
                Err(TryRecvError::Empty) => return Some(job),
                Err(TryRecvError::Disconnected) => return None,
            }
        }
    }
}

/// What the thread reuses from one entry to the next: room to strip escape codes
/// into, and to find the columns of a first line in.
struct Scratch {
    buffer: Vec<u8>,
    parser: Option<(EntryParser, CaptureLocations)>,
}

/// The filters at work on entries.
struct Check<'a> {
    filters: &'a Filters,
}

impl Check<'_> {
    fn chunk<B: AsRef<[u8]>>(
        &self,
        indexed: &Indexed<B>,
        chunk: Range<usize>,
        generation: u64,
        scratch: &mut Scratch,
    ) -> Update {
        let bytes = indexed.bytes.as_ref().as_ref();
        let mut update = Update {
            generation,
            entries: Vec::new(),
            counted: Vec::with_capacity(chunk.len()),
        };
        for (index, record) in chunk.clone().zip(&indexed.records[chunk]) {
            let range = record.bytes();
            let entry = bytes
                .get(range.start as usize..range.end as usize)
                .unwrap_or_default();
            let passes = self.passes_all_but_level(record, entry, scratch);
            update.counted.push(record.level().filter(|_| passes));
            if passes && self.filters.levels.shows(record.level()) {
                // 2^32 entries would take an index of 128 GiB at 32 bytes each.
                update.entries.push(index as u32);
            }
        }
        update
    }

    fn passes_all_but_level(&self, record: &Record, entry: &[u8], scratch: &mut Scratch) -> bool {
        let filters = self.filters;
        if !filters
            .time
            .is_none_or(|range| range.contains(record.timestamp()))
        {
            return false;
        }
        if filters.classes.is_empty() && filters.text.is_none() {
            return true;
        }
        let mut lines = source::lines(entry);
        let first = strip_ansi_into(lines.next().unwrap_or_default(), &mut scratch.buffer);
        if !filters
            .classes
            .pass(record.class().and_then(|class| first.get(class)))
        {
            return false;
        }
        let Some(text) = &filters.text else {
            return true;
        };
        let head = match &mut scratch.parser {
            Some((parser, locations)) => parser.parse_with(first, locations),
            None => None,
        };
        let message = head.map_or(first, |head| &first[head.message]);
        text.is_match(message) || lines.any(|line| text.is_match(&format::strip_ansi(line)))
    }
}
