//! The viewer's state and what keys do to it. No terminal and no drawing: `ui` draws an
//! `App`, the event loop turns keys into actions and applies them.

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

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
    Quit,
}

/// The action bound to a key, `None` for keys that do nothing.
///
/// `j k ↑ ↓` move by a line, `PgUp PgDn` by a page, `Ctrl-u Ctrl-d` by half a page,
/// `g Home` go to the top, `G End` to the bottom, `← →` scroll the message column,
/// `T` toggles the thread column, `w` wrapping, `P` cycles the preset and
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
    /// The file's last entries, until the index is complete.
    Tail(Vec<Record>),
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

/// A log file being viewed.
#[derive(Debug)]
pub struct App {
    name: String,
    source: Arc<Source>,
    indexer: Indexer,
    records: Vec<Record>,
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

    /// Takes in the entries indexed since the last call, one row each. The selection
    /// stays where it was; with no rows before, the first row is selected. Opened at
    /// the end, the rows are the last entries until the index is complete, which
    /// then takes over with its last row selected.
    pub fn poll_index(&mut self) {
        self.progress = self.indexer.poll(&mut self.records);
        if self.progress.is_complete() && !matches!(self.opening, Opening::Index) {
            self.show_last(Opening::Index);
        } else {
            self.viewport.set_total(self.rows().len());
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

    /// The first line of row `i`'s entry as text (see [`crate::source::display`]).
    /// Not yet safe to show: it may hold control characters. Panics if `i` is not
    /// below `viewport().total()`.
    pub fn line(&self, i: usize) -> Cow<'_, str> {
        source::display(self.first_line(i))
    }

    /// The first line of row `i`'s entry as in the file, without its line ending.
    /// Panics if `i` is not below `viewport().total()`.
    pub fn first_line(&self, i: usize) -> &[u8] {
        let bytes = self.rows()[i].bytes();
        // The records index the mapped file, so their offsets fit a `usize`.
        let entry = &self.source.bytes()[bytes.start as usize..bytes.end as usize];
        source::lines(entry).next().unwrap_or_default()
    }

    /// The record of row `i`'s entry. Panics if `i` is not below
    /// `viewport().total()`.
    pub fn record(&self, i: usize) -> &Record {
        &self.rows()[i]
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
    /// preset and opens it the way [`new`](Self::new) did: opened at the end, its last
    /// entries in the new preset are shown at once, with the last one selected;
    /// opened at the top, the first row is selected once the index has it.
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
            self.show_last(Opening::Tail(tail.unwrap_or_default()));
        }
    }

    /// Switches to the rows of `opening` with the last one selected.
    fn show_last(&mut self, opening: Opening) {
        self.opening = opening;
        self.viewport.set_total(self.rows().len());
        self.viewport.go_bottom();
    }

    /// The records of the rows.
    fn rows(&self) -> &[Record] {
        match &self.opening {
            Opening::Tail(tail) => tail,
            Opening::Index | Opening::AtEnd => &self.records,
        }
    }
}
