//! The viewer's state and what keys do to it. No terminal and no drawing: `ui` draws an
//! `App`, the event loop turns keys into actions and applies them.

use std::borrow::Cow;
use std::cell::Cell;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use memchr::{memchr, memchr_iter, memrchr};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::fold::Folds;
use crate::format::{Anchor, Preset, detect};
use crate::index::{self, Indexer, Progress, Record, Settings};
use crate::source::{self, Source};
use crate::viewport::Viewport;

#[cfg(test)]
mod tests;

/// Something the user asked the viewer to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    Top,
    Bottom,
    /// Shows the message column 8 columns further left.
    ScrollLeft,
    /// Shows the message column 8 columns further right.
    ScrollRight,
    ToggleThread,
    ToggleWrap,
    /// Reads the file in the next preset of [`Preset::ALL`].
    CyclePreset,
    /// Unfolds the selected entry if it is folded, folds it if it is not.
    ToggleFold,
    FoldAll,
    UnfoldAll,
    Quit,
}

/// The action bound to a key, `None` for keys that do nothing.
///
/// `j k ↑ ↓` move by a line, `PgUp PgDn` by a page, `Ctrl-u Ctrl-d` by half a page,
/// `g Home` go to the top, `G End` to the bottom, `← →` scroll the message column,
/// `T` toggles the thread column, `w` wrapping, `P` cycles the preset, `Space` folds
/// or unfolds the selected entry, `z` folds every entry, `Z` unfolds every entry and
/// `q F10 Ctrl-c` quit. Characters
/// ignore Shift (the character already says which case it is) but not Ctrl or Alt;
/// other keys do nothing with any modifier. Key releases do nothing; held keys repeat.
pub fn action(key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let modifiers = match key.code {
        KeyCode::Char(_) => key.modifiers - KeyModifiers::SHIFT,
        _ => key.modifiers,
    };
    let action = match (key.code, modifiers) {
        (KeyCode::Char('k') | KeyCode::Up, KeyModifiers::NONE) => Action::Up,
        (KeyCode::Char('j') | KeyCode::Down, KeyModifiers::NONE) => Action::Down,
        (KeyCode::PageUp, KeyModifiers::NONE) => Action::PageUp,
        (KeyCode::PageDown, KeyModifiers::NONE) => Action::PageDown,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => Action::HalfPageUp,
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => Action::HalfPageDown,
        (KeyCode::Char('g') | KeyCode::Home, KeyModifiers::NONE) => Action::Top,
        (KeyCode::Char('G') | KeyCode::End, KeyModifiers::NONE) => Action::Bottom,
        (KeyCode::Left, KeyModifiers::NONE) => Action::ScrollLeft,
        (KeyCode::Right, KeyModifiers::NONE) => Action::ScrollRight,
        (KeyCode::Char('T'), KeyModifiers::NONE) => Action::ToggleThread,
        (KeyCode::Char('w'), KeyModifiers::NONE) => Action::ToggleWrap,
        (KeyCode::Char('P'), KeyModifiers::NONE) => Action::CyclePreset,
        (KeyCode::Char(' '), KeyModifiers::NONE) => Action::ToggleFold,
        (KeyCode::Char('z'), KeyModifiers::NONE) => Action::FoldAll,
        (KeyCode::Char('Z'), KeyModifiers::NONE) => Action::UnfoldAll,
        (KeyCode::Char('q') | KeyCode::F(10), KeyModifiers::NONE)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        _ => return None,
    };
    Some(action)
}

/// How many columns [`Action::ScrollLeft`] and [`Action::ScrollRight`] move the
/// message column by.
const SCROLL_STEP: usize = 8;

/// How many bytes [`index::tail`] reads from the end of the file at a time, 64 KiB:
/// a screen of entries, stack traces included, mostly fits.
const TAIL_WINDOW: usize = 65_536;

/// How many bytes from the end of the file [`index::tail`] may read, 4 MiB, before
/// the viewer gives up on the tail and waits for the whole index.
const TAIL_LIMIT: usize = 4_194_304;

/// Which records the rows are while the viewer opens.
#[derive(Debug)]
enum Opening {
    /// The index, as far as it got.
    Index,
    /// The index, until a screen says how many entries to read from the end.
    AtEnd,
    /// The file's last entries and their fold state, until the index is complete.
    Tail(Vec<Record>, Folds),
}

impl Opening {
    fn new(follow: bool) -> Self {
        if follow {
            Opening::AtEnd
        } else {
            Opening::Index
        }
    }
}

/// What a row shows.
#[derive(Debug, Clone, Copy)]
pub struct ShownLine<'a> {
    /// The line as in the file, without its line ending.
    pub line: &'a [u8],
    /// The record of the line's entry.
    pub record: &'a Record,
    /// Whether the line is its entry's first, not a continuation line.
    pub first: bool,
    /// How many lines of the entry are folded away.
    pub hidden: usize,
}

/// Where line `line` of the entry starting at byte `entry` starts: byte `at`.
#[derive(Debug, Clone, Copy, Default)]
struct LineMark {
    entry: usize,
    line: usize,
    at: usize,
}

/// A log file being viewed.
#[derive(Debug)]
pub struct App {
    name: String,
    source: Arc<Source>,
    indexer: Indexer,
    records: Vec<Record>,
    /// The fold state of `records`.
    folds: Folds,
    /// The last line looked up. Rows on screen are neighbours, so stepping from it is
    /// quicker than from the entry's start when an unfolded entry is long.
    line_mark: Cell<LineMark>,
    opening: Opening,
    follow: bool,
    progress: Progress,
    settings: Settings,
    /// Whether detection found no preset and the file is read in `Raw` for it.
    unrecognized: bool,
    viewport: Viewport,
    /// How many columns the message column is scrolled to the right.
    scroll: usize,
    shows_thread: bool,
    wraps: bool,
    quit: bool,
}

impl App {
    /// A viewer over `source`, which starts indexing it in the background in the
    /// preset `format` names. Without one the preset is detected from the file's first
    /// lines ([`detect`](crate::format::detect)); when none fits, the file is read in
    /// `Raw` and [`format_unrecognized`](Self::format_unrecognized) says so. `path` is
    /// where the file was opened from; its name and `mtime`, the file's modification
    /// time, give the anchor for dating a time-only log (see
    /// [`Anchor::for_file`](crate::format::Anchor::for_file)). Timestamps without an
    /// offset are local time in `zone`.
    ///
    /// It has no rows until [`poll_index`](Self::poll_index) takes in the first
    /// entries, and the screen has no lines until the first [`resize`](Self::resize).
    ///
    /// With `follow` it opens at the end of the file. As soon as it has a screen, it
    /// shows the file's last entries, read from the end of the file
    /// ([`index::tail`](crate::index::tail)), with the last one selected, until the
    /// index of the whole file is complete and takes over, still with the last entry
    /// selected. When those entries start too far from the end, it shows no rows
    /// until then. Without `follow` it opens at the top and shows the first entries
    /// as soon as the index has them.
    pub fn new(
        path: &Path,
        source: Source,
        format: Option<Preset>,
        zone: TimeZone,
        mtime: Timestamp,
        follow: bool,
    ) -> Self {
        let picked = format.or_else(|| detect(source::lines(source.bytes())));
        // A path like `..` has no last component; showing it whole is the next best thing.
        let name = path.file_name().unwrap_or(path.as_os_str());
        let anchor = Anchor::for_file(path, mtime, &zone);
        let source = Arc::new(source);
        let settings = Settings {
            preset: picked.unwrap_or(Preset::Raw),
            zone,
            anchor,
        };
        let progress = Progress {
            indexed: 0,
            total: source.bytes().len() as u64,
        };
        Self {
            name: name.to_string_lossy().into_owned(),
            indexer: Indexer::spawn(Arc::clone(&source), settings.clone()),
            source,
            records: Vec::new(),
            folds: Folds::default(),
            line_mark: Cell::default(),
            opening: Opening::new(follow),
            follow,
            progress,
            settings,
            unrecognized: picked.is_none(),
            viewport: Viewport::new(0, 0),
            scroll: 0,
            shows_thread: false,
            wraps: false,
            quit: false,
        }
    }

    /// Takes in the entries indexed since the last call, folded or unfolded as the
    /// last [`Action::FoldAll`] or [`Action::UnfoldAll`] says. The selection stays
    /// where it was; with no rows before, the first row is selected. Opened at the
    /// end, the rows are the last entries until the index is complete, which then
    /// takes over with its last row selected. Only folding or unfolding all carries
    /// over from the last entries to the index.
    pub fn poll_index(&mut self) {
        let known = self.records.len();
        self.progress = self.indexer.poll(&mut self.records);
        push_entries(&mut self.folds, self.source.bytes(), &self.records[known..]);
        if self.progress.is_complete() && !matches!(self.opening, Opening::Index) {
            self.show_last(Opening::Index);
        } else {
            self.viewport.set_total(self.shown().1.row_count());
        }
    }

    /// How far indexing got as of the last [`poll_index`](Self::poll_index):
    /// nothing of the file's size before the first.
    pub fn progress(&self) -> Progress {
        self.progress
    }

    /// How the file is indexed: the preset, the zone and the anchor `new` made of
    /// its arguments.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The last component of the path, decoded lossily. Not yet safe to show: it may
    /// hold control characters.
    pub fn file_name(&self) -> &str {
        &self.name
    }

    /// The file's size in bytes.
    pub fn size(&self) -> u64 {
        self.source.bytes().len() as u64
    }

    /// Row `i`'s line as text (see [`crate::source::display`]). Not yet safe to
    /// show: it may hold control characters. Panics if `i` is not below
    /// `viewport().total()`.
    pub fn line(&self, i: usize) -> Cow<'_, str> {
        source::display(self.row(i).line)
    }

    /// What row `i` shows: the first line of an entry, or a continuation line of an
    /// unfolded one. Panics if `i` is not below `viewport().total()`.
    pub fn row(&self, i: usize) -> ShownLine<'_> {
        let (records, folds) = self.shown();
        let (entry, line) = folds.line_at(i);
        let record = &records[entry];
        ShownLine {
            line: self.entry_line(record, line),
            record,
            first: line == 0,
            hidden: folds.hidden_lines(entry),
        }
    }

    /// Whether the preset was detected, found none fitting and fell back to `Raw`.
    /// Cycling the preset ends this.
    pub fn format_unrecognized(&self) -> bool {
        self.unrecognized
    }

    /// Which lines are on screen and which one is selected.
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Sets how many rows fit on the screen: one per screen line, fewer when wrapped
    /// rows take more than one.
    pub fn resize(&mut self, height: usize) {
        self.viewport.resize(height);
        self.read_tail();
    }

    /// How many columns the message column is scrolled to the right.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Scrolls the message column back to at most `max` columns, where the widest
    /// message on screen ends at the right edge.
    pub fn limit_scroll(&mut self, max: usize) {
        self.scroll = self.scroll.min(max);
    }

    /// Whether the thread column is shown.
    pub fn shows_thread(&self) -> bool {
        self.shows_thread
    }

    /// Whether long messages wrap onto the following screen lines instead of being
    /// cut at the right edge.
    pub fn wraps(&self) -> bool {
        self.wraps
    }

    /// Does what `action` asks. Cycling the preset indexes the file again in the next
    /// preset and opens it the way [`new`](Self::new) did, every entry folded: opened
    /// at the end, its last entries in the new preset are shown at once, with the
    /// last one selected; opened at the top, the first row is selected once the index
    /// has it.
    ///
    /// Folding and unfolding keep the selected line on its screen line, as far as the
    /// ends allow (in wrap mode, the selected row stays as many rows below the top).
    /// When the selected line gets folded away, its entry's first line takes its
    /// place.
    pub fn apply(&mut self, action: Action) {
        let viewport = &mut self.viewport;
        match action {
            Action::Up => viewport.up(1),
            Action::Down => viewport.down(1),
            Action::PageUp => viewport.page_up(),
            Action::PageDown => viewport.page_down(),
            Action::HalfPageUp => viewport.half_page_up(),
            Action::HalfPageDown => viewport.half_page_down(),
            Action::Top => viewport.go_top(),
            Action::Bottom => viewport.go_bottom(),
            Action::ScrollLeft => self.scroll = self.scroll.saturating_sub(SCROLL_STEP),
            Action::ScrollRight => self.scroll = self.scroll.saturating_add(SCROLL_STEP),
            Action::ToggleThread => self.shows_thread = !self.shows_thread,
            Action::ToggleWrap => self.wraps = !self.wraps,
            Action::CyclePreset => self.cycle_preset(),
            Action::ToggleFold => self.toggle_fold(),
            Action::FoldAll => self.set_all_folded(true),
            Action::UnfoldAll => self.set_all_folded(false),
            Action::Quit => self.quit = true,
        }
    }

    /// Whether [`Action::Quit`] has been applied.
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Indexes the file again in the preset after the current one in
    /// [`Preset::ALL`], and opens it again as `new` did.
    fn cycle_preset(&mut self) {
        let presets = Preset::ALL;
        // `ALL` holds every preset, so the current one is always found.
        let current = presets.iter().position(|&p| p == self.settings.preset);
        self.settings.preset = presets[current.map_or(0, |i| (i + 1) % presets.len())];
        self.unrecognized = false;
        // Dropping the old indexer stops it.
        self.indexer = Indexer::spawn(Arc::clone(&self.source), self.settings.clone());
        self.records.clear();
        self.folds = Folds::default();
        self.progress.indexed = 0;
        self.opening = Opening::new(self.follow);
        self.viewport = Viewport::new(0, self.viewport.height());
        self.read_tail();
    }

    /// Opened at the end and given a screen, shows the file's last entries.
    fn read_tail(&mut self) {
        let height = self.viewport.height();
        if height > 0 && matches!(self.opening, Opening::AtEnd) {
            let bytes = self.source.bytes();
            let tail = index::tail(bytes, height, TAIL_WINDOW, TAIL_LIMIT, &self.settings);
            let tail = tail.unwrap_or_default();
            let mut folds = Folds::default();
            push_entries(&mut folds, bytes, &tail);
            self.show_last(Opening::Tail(tail, folds));
        }
    }

    /// Switches to the rows of `opening` with the last one selected.
    fn show_last(&mut self, opening: Opening) {
        self.opening = opening;
        self.viewport.set_total(self.shown().1.row_count());
        self.viewport.go_bottom();
    }

    /// The records of the entries shown and their fold state.
    fn shown(&self) -> (&[Record], &Folds) {
        match &self.opening {
            Opening::Tail(tail, folds) => (tail, folds),
            Opening::Index | Opening::AtEnd => (&self.records, &self.folds),
        }
    }

    fn shown_folds_mut(&mut self) -> &mut Folds {
        match &mut self.opening {
            Opening::Tail(_, folds) => folds,
            Opening::Index | Opening::AtEnd => &mut self.folds,
        }
    }

    fn toggle_fold(&mut self) {
        if let Some(row) = self.viewport.selected() {
            let (entry, _) = self.shown().1.line_at(row);
            self.refold(|folds| folds.toggle(entry));
        }
    }

    fn set_all_folded(&mut self, folded: bool) {
        let set: fn(&mut Folds) = if folded {
            Folds::fold_all
        } else {
            Folds::unfold_all
        };
        // The index behind the last entries takes the mode too, so that it holds
        // once the index takes over.
        if matches!(self.opening, Opening::Tail(..)) {
            set(&mut self.folds);
        }
        self.refold(set);
    }

    /// Changes the fold state of the entries shown, keeping the selected line on its
    /// screen line, or its entry's first line if it gets folded away.
    fn refold(&mut self, change: impl FnOnce(&mut Folds)) {
        let viewport = &self.viewport;
        let selected = viewport
            .selected()
            .map(|row| (self.shown().1.line_at(row), row - viewport.top()));
        let folds = self.shown_folds_mut();
        change(folds);
        let total = folds.row_count();
        let selected = selected.map(|((entry, line), screen_line)| {
            let folded = folds.hidden_lines(entry) > 0;
            let line = if folded { 0 } else { line };
            (folds.row_of(entry, line), screen_line)
        });
        self.viewport.set_total(total);
        if let Some((row, screen_line)) = selected {
            self.viewport.select_on_line(row, screen_line);
        }
    }

    /// Line `line` of `record`'s entry, without its line ending.
    fn entry_line(&self, record: &Record, line: usize) -> &[u8] {
        let bytes = self.source.bytes();
        let entry = file_range(record);
        let mut mark = self.line_mark.get();
        // A mark further away than the entry's start is no help.
        if mark.entry != entry.start || mark.line.abs_diff(line) > line {
            mark = LineMark {
                entry: entry.start,
                line: 0,
                at: entry.start,
            };
        }
        for _ in line..mark.line {
            // A line after the first starts right after a `\n`.
            let before = &bytes[entry.start..mark.at - 1];
            mark.at = memrchr(b'\n', before).map_or(entry.start, |i| entry.start + i + 1);
        }
        for _ in mark.line..line {
            let rest = &bytes[mark.at..entry.end];
            mark.at = memchr(b'\n', rest).map_or(entry.end, |i| mark.at + i + 1);
        }
        mark.line = line;
        self.line_mark.set(mark);
        source::lines(&bytes[mark.at..entry.end])
            .next()
            .unwrap_or_default()
    }
}

/// Where `record`'s entry is in the file. The records index the mapped file, so its
/// offsets fit a `usize`.
fn file_range(record: &Record) -> Range<usize> {
    let bytes = record.bytes();
    bytes.start as usize..bytes.end as usize
}

/// Adds the entries of `records`, which index `bytes`, to `folds`.
fn push_entries(folds: &mut Folds, bytes: &[u8], records: &[Record]) {
    for record in records {
        let entry = &bytes[file_range(record)];
        let newlines = memchr_iter(b'\n', entry).count();
        // Only the file's last entry can end without a `\n`; its last line counts all
        // the same.
        let lines = newlines + usize::from(!entry.ends_with(b"\n"));
        folds.push(lines.saturating_sub(1));
    }
}
