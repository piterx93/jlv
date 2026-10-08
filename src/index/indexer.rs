//! Indexing a whole file on background threads.

use std::collections::BTreeMap;
use std::iter;
use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use super::{Chunk, Record, Settings, dated, set_timestamps};

/// Bytes per chunk. The threads only look at the stop flag between chunks, so a
/// chunk has to be quick to index even in a debug build, where a megabyte of blank
/// lines takes over a second. What each chunk costs besides its lines, joining it
/// and handing its records over, is still small at this size.
const CHUNK_SIZE: usize = 128 << 10;

/// How far an [`Indexer`] got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Bytes of the file indexed so far, counted from its start. It reaches `total`
    /// only once the index is complete.
    pub indexed: u64,
    /// The file's size in bytes.
    pub total: u64,
}

impl Progress {
    /// Whether the whole file is indexed. An empty file always is.
    pub fn is_complete(self) -> bool {
        self.indexed == self.total
    }

    /// `indexed` as a whole percentage of `total`, rounded down, so 100 means
    /// complete. 100 for an empty file.
    pub fn percent(self) -> u8 {
        // In u128 the product can't overflow, and as `indexed <= total` the quotient
        // is at most 100.
        (u128::from(self.indexed) * 100)
            .checked_div(u128::from(self.total))
            .map_or(100, |percent| percent as u8)
    }
}

/// Indexes a file on background threads, one chunk per thread at a time, and hands
/// the records over in file order as they are found. Dropping it stops indexing:
/// its threads end soon after, without finishing the file.
#[derive(Debug)]
pub struct Indexer {
    deliveries: Receiver<Delivery>,
    progress: Progress,
    stop: Arc<AtomicBool>,
}

/// What the background threads hand over.
#[derive(Debug)]
enum Delivery {
    /// Records that no later part of the file can change, and the bytes indexed.
    Found { records: Vec<Record>, indexed: u64 },
    /// The last records, and the timestamps of all records of a time-only preset.
    Complete {
        records: Vec<Record>,
        timestamps: Vec<i64>,
    },
}

impl Indexer {
    /// Starts indexing `bytes`, the whole file, as `settings` say.
    pub fn spawn<B>(bytes: Arc<B>, settings: Settings) -> Indexer
    where
        B: AsRef<[u8]> + Send + Sync + 'static,
    {
        let total = (*bytes).as_ref().len() as u64;
        let (sender, deliveries) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        thread::spawn(move || index_file((*bytes).as_ref(), &settings, &stopped, &sender));
        Indexer {
            deliveries,
            progress: Progress { indexed: 0, total },
            stop,
        }
    }

    /// Appends the records found since the last call to `records`, which holds what
    /// the earlier calls gave, and says how far indexing got.
    ///
    /// Progress never goes back. Once it is complete, `records` is the index of the
    /// whole file, as [`Chunk::finish`] gives it, and later calls change nothing.
    /// Until then `records` holds the first records of the index; under a time-only
    /// preset they have no timestamp yet, since the dates are counted back from the
    /// end of the file.
    pub fn poll(&mut self, records: &mut Vec<Record>) -> Progress {
        for delivery in self.deliveries.try_iter() {
            match delivery {
                Delivery::Found {
                    records: found,
                    indexed,
                } => {
                    records.extend(found);
                    self.progress.indexed = indexed;
                }
                Delivery::Complete {
                    records: last,
                    timestamps,
                } => {
                    records.extend(last);
                    set_timestamps(records, &timestamps);
                    self.progress.indexed = self.progress.total;
                }
            }
        }
        self.progress
    }
}

impl Drop for Indexer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Indexes `bytes` in chunks on a thread per core and hands the records over in file
/// order, until the whole file is done or `stop` is set.
fn index_file(bytes: &[u8], settings: &Settings, stop: &AtomicBool, deliveries: &Sender<Delivery>) {
    let count = bytes.len().div_ceil(CHUNK_SIZE);
    let next = &AtomicUsize::new(0);
    let threads = thread::available_parallelism().map_or(1, NonZero::get);
    let (sender, chunks) = mpsc::channel();
    thread::scope(|scope| {
        for sender in iter::repeat_n(sender, threads.min(count)) {
            // Each thread takes the first chunk nobody has taken, until none is left.
            scope.spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= count {
                        break;
                    }
                    let range = i * CHUNK_SIZE..bytes.len().min((i + 1) * CHUNK_SIZE);
                    if sender
                        .send((i, Chunk::index(bytes, 0, range, settings)))
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        merge(chunks, count, settings, deliveries);
    });
}

/// Joins the chunks in file order as they come in, out of order, and hands over the
/// records no later chunk can change. Once all `count` chunks are in, it dates them
/// if need be and hands over the rest.
fn merge(
    chunks: Receiver<(usize, Chunk)>,
    count: usize,
    settings: &Settings,
    deliveries: &Sender<Delivery>,
) {
    let mut early = BTreeMap::new();
    let mut joined = Chunk::empty();
    let mut merged = 0;
    // Sending fails only once the indexer is dropped. Nobody needs the records then,
    // and the threads stop sending chunks soon.
    for (i, chunk) in chunks {
        early.insert(i, chunk);
        while let Some(chunk) = early.remove(&merged) {
            joined.join(chunk);
            merged += 1;
            if merged < count {
                let records = joined.take_closed();
                let indexed = (merged * CHUNK_SIZE) as u64;
                let _ = deliveries.send(Delivery::Found { records, indexed });
            }
        }
    }
    // Indexing stopped before the end otherwise.
    if merged == count {
        let timestamps = dated(&joined.times, settings);
        let records = joined.records;
        let _ = deliveries.send(Delivery::Complete {
            records,
            timestamps,
        });
    }
}
