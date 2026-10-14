//! The viewer's state and what keys do to it. No terminal and no drawing: `ui` draws an
//! `App`, the event loop turns keys into actions and applies them.

use std::borrow::Cow;
use std::cell::Cell;
use std::fs::File;
use std::io;
use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use memchr::{memchr, memchr_iter, memrchr};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::filter::{
    self, Base, Classes, Engine, Filters, Indexed, Levels, Rule, TimeFilter, TimeRange, TimeSpec,
};
use crate::fold::Folds;
use crate::follow::{Event, Options, Watcher};
use crate::format::{Anchor, Level, Preset, detect, strip_ansi};
use crate::index::{self, Indexer, Progress, Record, Settings};
use crate::matcher::{self, Matcher};
use crate::source::{self, Origin, Source};
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
    /// Selects the last row and follows the file, if it [can be
    /// followed](App::origin).
    Bottom,
    /// Follows the file from its last row if the viewer doesn't, stops following if
    /// it does. Does nothing on a file that [can't be followed](App::origin).
    ToggleFollow,
    /// Shows the message column 8 columns further left.
    ScrollLeft,
    /// Shows the message column 8 columns further right.
    ScrollRight,
    ToggleThread,
    /// Shows or hides the gutter of file line numbers.
    ToggleLineNumbers,
    ToggleWrap,
    /// Reads the file in the next preset of [`Preset::ALL`].
    CyclePreset,
    /// Unfolds the selected entry if it is folded, folds it if it is not.
    ToggleFold,
    FoldAll,
    UnfoldAll,
    /// Shows the level if the level filter hides it, hides it if it shows it.
    ToggleLevel(Level),
    /// Opens the level panel ([`App::panel`]).
    LevelPanel,
    /// Shows only the class of the selected entry and the classes below it: the
    /// class filter's includes become the one rule `<class>.*`.
    OnlyClass,
    /// Hides the class of the selected entry and the classes below it: adds the
    /// exclude `!<class>.*` to the class filter.
    HideClass,
    /// Opens the class panel ([`App::panel`]).
    ClassPanel,
    /// Opens the time panel ([`App::panel`]).
    TimePanel,
    /// Opens the text filter prompt ([`App::panel`]).
    TextPanel,
    /// Clears every filter: the level, class, time and text filters.
    ClearFilters,
    /// Applies what the open panel edits and closes it.
    Confirm,
    /// Closes the open panel without applying its edits.
    Cancel,
    Quit,
}

/// The action bound to a key, `None` for keys that do nothing.
///
/// `j k ↑ ↓` move by a line, `PgUp PgDn` by a page, `Ctrl-u Ctrl-d` by half a page,
/// `g Home` go to the top, `G End` to the bottom, `← →` scroll the message column,
/// `F` toggles follow, `T` toggles the thread column, `#` the line numbers, `w`
/// wrapping, `P` cycles the preset, `Space` folds or unfolds the selected entry, `z`
/// folds every entry, `Z` unfolds every entry, `1`–`6` toggle the levels TRACE to
/// FATAL, `F2` opens the level panel, `c` shows only the selected entry's class, `C`
/// hides it, `F4` or `&` opens the text filter prompt, `F5` the time panel, `F6` the
/// class panel, `x` clears every filter, `Enter` confirms, `Esc` cancels and `q F10
/// Ctrl-c` quit. Characters ignore Shift (the character already says which case it
/// is) but not Ctrl or Alt; other keys do nothing with any modifier. Key releases do
/// nothing; held keys repeat.
pub fn action(key: KeyEvent) -> Option<Action> {
    let action = match normalized(key)? {
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
        (KeyCode::Char('F'), KeyModifiers::NONE) => Action::ToggleFollow,
        (KeyCode::Char('T'), KeyModifiers::NONE) => Action::ToggleThread,
        (KeyCode::Char('#'), KeyModifiers::NONE) => Action::ToggleLineNumbers,
        (KeyCode::Char('w'), KeyModifiers::NONE) => Action::ToggleWrap,
        (KeyCode::Char('P'), KeyModifiers::NONE) => Action::CyclePreset,
        (KeyCode::Char(' '), KeyModifiers::NONE) => Action::ToggleFold,
        (KeyCode::Char('z'), KeyModifiers::NONE) => Action::FoldAll,
        (KeyCode::Char('Z'), KeyModifiers::NONE) => Action::UnfoldAll,
        (KeyCode::Char(digit @ '1'..='6'), KeyModifiers::NONE) => {
            Action::ToggleLevel(Level::ALL[usize::from(digit as u8 - b'1')])
        }
        (KeyCode::F(2), KeyModifiers::NONE) => Action::LevelPanel,
        (KeyCode::Char('c'), KeyModifiers::NONE) => Action::OnlyClass,
        (KeyCode::Char('C'), KeyModifiers::NONE) => Action::HideClass,
        (KeyCode::F(4) | KeyCode::Char('&'), KeyModifiers::NONE) => Action::TextPanel,
        (KeyCode::F(5), KeyModifiers::NONE) => Action::TimePanel,
        (KeyCode::F(6), KeyModifiers::NONE) => Action::ClassPanel,
        (KeyCode::Char('x'), KeyModifiers::NONE) => Action::ClearFilters,
        (KeyCode::Enter, KeyModifiers::NONE) => Action::Confirm,
        (KeyCode::Esc, KeyModifiers::NONE) => Action::Cancel,
        (KeyCode::Char('q') | KeyCode::F(10), KeyModifiers::NONE)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        _ => return None,
    };
    Some(action)
}

/// The code and modifiers of `key` as the bindings see them, `None` for a release.
/// A character ignores Shift, as it already says which case it is.
fn normalized(key: KeyEvent) -> Option<(KeyCode, KeyModifiers)> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let modifiers = match key.code {
        KeyCode::Char(_) => key.modifiers - KeyModifiers::SHIFT,
        _ => key.modifiers,
    };
    Some((key.code, modifiers))
}

/// A key in a panel that is typed into.
enum Typed {
    /// `Enter`, `Esc` or `Ctrl-c`: what they stand for.
    Action(Action),
    /// A character, with or without Shift.
    Char(char),
    Backspace,
    /// Any other key without modifiers.
    Other(KeyCode),
}

/// What `key` is in a panel that is typed into, `None` for a release or a key with
/// modifiers that means nothing there.
fn typed(key: KeyEvent) -> Option<Typed> {
    let typed = match normalized(key)? {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Typed::Action(Action::Quit),
        (KeyCode::Enter, KeyModifiers::NONE) => Typed::Action(Action::Confirm),
        (KeyCode::Esc, KeyModifiers::NONE) => Typed::Action(Action::Cancel),
        (KeyCode::Char(c), KeyModifiers::NONE) => Typed::Char(c),
        (KeyCode::Backspace, KeyModifiers::NONE) => Typed::Backspace,
        (code, KeyModifiers::NONE) => Typed::Other(code),
        _ => return None,
    };
    Some(typed)
}

/// The level panel while it is open: the level filter as edited so far, not applied
/// yet, and the level its cursor is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelPanel {
    pub levels: Levels,
    pub cursor: Level,
}

impl LevelPanel {
    fn edit(&mut self, action: Action) {
        let cursor = self.cursor as usize;
        match action {
            Action::Up => self.cursor = Level::ALL[cursor.saturating_sub(1)],
            Action::Down => self.cursor = Level::ALL[(cursor + 1).min(Level::ALL.len() - 1)],
            Action::ToggleFold => self.levels.toggle(self.cursor),
            Action::ToggleLevel(level) => self.levels.toggle(level),
            _ => {}
        }
    }
}

/// The class panel while it is open: the class rules as edited so far, one per line,
/// not applied yet, the line its cursor is on, and why the rules last confirmed
/// don't parse, if they don't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassPanel {
    pub lines: Vec<String>,
    pub cursor: usize,
    pub error: Option<String>,
}

impl ClassPanel {
    /// Opens on `classes`, one rule per line, with the cursor on an empty line after
    /// them.
    fn new(classes: &Classes) -> ClassPanel {
        let mut lines = classes.rules();
        lines.push(String::new());
        ClassPanel {
            cursor: lines.len() - 1,
            lines,
            error: None,
        }
    }

    /// Edits the panel as `key` asks, or returns the action it stands for here.
    /// `lines` is never empty, so the cursor always has a line to be on.
    fn key(&mut self, key: KeyEvent) -> Option<Action> {
        let typed = typed(key)?;
        let line = &mut self.lines[self.cursor];
        match typed {
            Typed::Action(action) => return Some(action),
            Typed::Char(c) => line.push(c),
            Typed::Backspace => {
                if line.pop().is_none() && self.lines.len() > 1 {
                    self.lines.remove(self.cursor);
                    self.cursor = self.cursor.saturating_sub(1);
                }
            }
            Typed::Other(KeyCode::Up) => self.cursor = self.cursor.saturating_sub(1),
            Typed::Other(KeyCode::Down) => {
                if !line.is_empty() && self.cursor + 1 == self.lines.len() {
                    self.lines.push(String::new());
                }
                self.cursor = (self.cursor + 1).min(self.lines.len() - 1);
            }
            _ => {}
        }
        None
    }
}

/// The time panel while it is open: the time spec as typed so far and the base, not
/// applied yet, and why the spec last confirmed doesn't parse, if it doesn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimePanel {
    pub spec: String,
    pub base: Base,
    pub error: Option<String>,
}

/// What `↓` and `↑` step through in the time panel.
const TIME_PRESETS: [&str; 6] = ["1m", "5m", "10m", "30m", "60m", "24h"];

impl TimePanel {
    /// Opens on `filter`, or on an empty spec counted from the file without one.
    fn new(filter: Option<&TimeFilter>) -> TimePanel {
        TimePanel {
            spec: filter.map_or_else(String::new, |filter| filter.spec.to_string()),
            base: filter.map_or_else(Base::default, |filter| filter.base),
            error: None,
        }
    }

    /// Edits the panel as `key` asks, or returns the action it stands for here.
    fn key(&mut self, key: KeyEvent) -> Option<Action> {
        let preset = TIME_PRESETS.iter().position(|&preset| preset == self.spec);
        let last = TIME_PRESETS.len() - 1;
        match typed(key)? {
            Typed::Action(action) => return Some(action),
            Typed::Char(c) => self.spec.push(c),
            Typed::Backspace => {
                self.spec.pop();
            }
            Typed::Other(KeyCode::Tab) => self.base = self.base.toggled(),
            Typed::Other(KeyCode::Down) => {
                self.spec = TIME_PRESETS[preset.map_or(0, |i| (i + 1).min(last))].to_string();
            }
            Typed::Other(KeyCode::Up) => {
                self.spec = TIME_PRESETS[preset.map_or(last, |i| i.saturating_sub(1))].to_string();
            }
            Typed::Other(_) => {}
        }
        None
    }

    /// The time filter the panel sets, `None` for an empty spec.
    fn filter(&self) -> Result<Option<TimeFilter>, String> {
        if self.spec.trim().is_empty() {
            return Ok(None);
        }
        let spec = TimeSpec::parse(&self.spec)?;
        Ok(Some(TimeFilter {
            spec,
            base: self.base,
        }))
    }
}

/// A pattern prompt while it is open: the pattern as typed so far and how it is
/// read, not applied yet, and why the pattern last confirmed doesn't compile, if it
/// doesn't. The text filter prompt is one; search reads its pattern the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub pattern: String,
    pub options: matcher::Options,
    pub error: Option<String>,
}

impl Prompt {
    /// Opens on `matcher`'s pattern and options, or on an empty plain,
    /// case-sensitive pattern without one.
    fn new(matcher: Option<&Matcher>) -> Prompt {
        Prompt {
            pattern: matcher.map_or_else(String::new, |matcher| matcher.pattern().to_owned()),
            options: matcher.map_or_else(matcher::Options::default, Matcher::options),
            error: None,
        }
    }

    /// Edits the prompt as `key` asks, or returns the action it stands for here.
    fn key(&mut self, key: KeyEvent) -> Option<Action> {
        // Before `typed`, which has no use for keys with Ctrl but `Ctrl-c`.
        match normalized(key) {
            Some((KeyCode::Char('r'), KeyModifiers::CONTROL)) => {
                self.options.regex = !self.options.regex;
                return None;
            }
            Some(
                (KeyCode::Char('i'), KeyModifiers::CONTROL) | (KeyCode::Tab, KeyModifiers::NONE),
            ) => {
                self.options.ignore_case = !self.options.ignore_case;
                return None;
            }
            _ => {}
        }
        match typed(key)? {
            Typed::Action(action) => return Some(action),
            Typed::Char(c) => self.pattern.push(c),
            Typed::Backspace => {
                self.pattern.pop();
            }
            Typed::Other(_) => {}
        }
        None
    }

    /// The text filter the prompt sets, `None` for an empty pattern.
    fn matcher(&self) -> Result<Option<Matcher>, String> {
        if self.pattern.is_empty() {
            return Ok(None);
        }
        Matcher::new(&self.pattern, self.options).map(Some)
    }
}

/// The panel open over the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Panel {
    Levels(LevelPanel),
    Classes(ClassPanel),
    Time(TimePanel),
    /// The text filter prompt.
    Text(Prompt),
}

impl Panel {
    fn edit(&mut self, action: Action) {
        match self {
            Panel::Levels(panel) => panel.edit(action),
            // Their text is edited through `App::key`, not actions.
            Panel::Classes(_) | Panel::Time(_) | Panel::Text(_) => {}
        }
    }
}

/// What [`App::notice`] says after `c` or `C` on an entry without a class.
const NO_CLASS: &str = "the selected entry has no class";

/// How many columns [`Action::ScrollLeft`] and [`Action::ScrollRight`] move the
/// message column by.
const SCROLL_STEP: usize = 8;

/// How many bytes [`index::tail`] reads from the end of the file at a time, 64 KiB:
/// a screen of entries, stack traces included, mostly fits.
const TAIL_WINDOW: usize = 65_536;

/// How many bytes from the end of the file [`index::tail`] may read, 4 MiB, before
/// the viewer gives up on the tail and waits for the whole index.
const TAIL_LIMIT: usize = 4_194_304;

/// How many entries the filter engine evaluates per chunk. It holds the index for a
/// chunk at a time, and taking in what the followed file appends has to wait for it.
const FILTER_CHUNK: usize = 1024;

/// How long the viewer waits for the filter engine to evaluate the few entries that
/// changed with the index, so that the matches among them don't leave the rows until
/// the next poll: the last rows would flicker, and a selection on them would move.
const CATCH_UP: Duration = Duration::from_millis(10);

/// The time range of a time filter while there is no entry with a timestamp: none.
const NO_TIME: TimeRange = TimeRange {
    start: Timestamp::MAX,
    end: Some(Timestamp::MAX),
};

/// How the range of the time filter changed.
enum Slide {
    None,
    /// Its start moved to this later time and its end stayed.
    Later(Timestamp),
    Restart,
}

/// The index, shared with the filter engine. Only `App` writes it.
type SharedIndex = Arc<RwLock<Indexed<Source>>>;

/// The filters at work on the index: their engine, and the fold state of the entries
/// that pass, entry `i` being the engine's `i`th match. Each match folds as the
/// index's fold state says when it comes in; after that, folding changes both.
#[derive(Debug)]
struct Filter {
    engine: Engine,
    folds: Folds,
}

impl Filter {
    /// The filter of `index`, read in `preset`, whose matches fold as `all`, the
    /// index's fold state, folds them.
    fn spawn(index: &SharedIndex, preset: Preset, filters: Filters, all: &Folds) -> Self {
        let options = filter::Options {
            chunk: FILTER_CHUNK,
        };
        Filter {
            engine: Engine::spawn(Arc::clone(index), preset, filters, options),
            // In the index's mode, only the matches it excepts are exceptions here.
            folds: all.empty_like(),
        }
    }

    /// The index entry that is match `i`.
    fn entry(&self, i: usize) -> usize {
        self.engine.filtered().entries[i] as usize
    }

    /// Takes in the matches the engine found since the last call, folded as `all`,
    /// the index's fold state, folds them.
    fn take_in(&mut self, all: &Folds) {
        self.catch_up(all, 0, Instant::now());
    }

    /// Starts over with `filters`, from no match.
    fn restart(&mut self, filters: Filters) {
        self.engine.set_filters(filters);
        self.folds.truncate(0);
    }

    /// [`take_in`](Self::take_in), but waiting for the engine until it evaluated the
    /// first `evaluated` entries or `deadline` passed.
    fn catch_up(&mut self, all: &Folds, evaluated: usize, deadline: Instant) {
        let known = self.engine.filtered().entries.len();
        let entries = &self.engine.poll_until(evaluated, deadline).entries[known..];
        for (i, &entry) in (known..).zip(entries) {
            let entry = entry as usize;
            self.folds.push(all.continuation_lines(entry));
            if self.folds.hidden_lines(i) != all.hidden_lines(entry) {
                self.folds.toggle(i);
            }
        }
    }

    /// Drops the matches from index entry `from` on, which the index read again or
    /// dropped, and has the engine evaluate the entries from there.
    fn reread(&mut self, from: usize) {
        let kept = self.matches_before(from);
        self.engine.reread(from);
        self.folds.truncate(kept);
    }

    /// Like [`reread`](Self::reread), with `filters` that only drop entries.
    fn refilter_from(&mut self, filters: Filters, from: usize) {
        let kept = self.matches_before(from);
        self.engine.set_filters_from(filters, from);
        self.folds.truncate(kept);
    }

    fn matches_before(&self, entry: usize) -> usize {
        let entries = &self.engine.filtered().entries;
        entries.partition_point(|&match_| (match_ as usize) < entry)
    }
}

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

/// What happened to the followed file, as the viewer takes it in: a
/// [`follow::Event`](crate::follow::Event), with the file mapped again where there
/// is something new to read.
#[derive(Debug)]
pub enum FileChange {
    /// The file grew. The source maps all of it, the part already read included.
    Grew(Source),
    /// The file was cut short, and maybe written again since. The source maps what
    /// it holds now.
    Truncated(Source),
    /// Another file is at the path, after a rotation or after the file was gone. The
    /// source maps it.
    Replaced(Source),
    /// Nothing is at the path.
    Gone,
}

/// What a row shows.
#[derive(Debug, Clone, Copy)]
pub struct ShownLine<'a> {
    /// The line as in the file, without its line ending.
    pub line: &'a [u8],
    /// The record of the line's entry.
    pub record: Record,
    /// Whether the line is its entry's first, not a continuation line.
    pub first: bool,
    /// The line's number in the file, `None` while it isn't known (see
    /// [`Record::line`]).
    pub number: Option<u64>,
    /// How many lines of the entry are folded away.
    pub hidden: usize,
}

/// The text of the row that marks a truncation.
const TRUNCATION_ROW: &str = "── file truncated ──";

/// Where line `line` of the entry starting at byte `entry` starts: byte `at`.
#[derive(Debug, Clone, Copy, Default)]
struct LineMark {
    entry: usize,
    line: usize,
    at: usize,
}

/// The watcher of the followed file, and the file it follows, for mapping it again.
#[derive(Debug)]
struct Watch {
    watcher: Watcher,
    file: File,
}

impl Watch {
    /// What the watcher reported since the last call.
    fn changes(&mut self) -> Vec<FileChange> {
        let mut changes = Vec::new();
        while let Ok(event) = self.watcher.events().try_recv() {
            changes.extend(self.change(event));
        }
        changes
    }

    /// What `event` means for the viewer, with the file mapped again. `None` if it
    /// can't be mapped: the next event maps it again.
    fn change(&mut self, event: Event) -> Option<FileChange> {
        let change = match event {
            Event::Grew { .. } => FileChange::Grew(self.map()?),
            Event::Truncated { .. } => FileChange::Truncated(self.map()?),
            Event::Replaced { file, .. } | Event::Back { file, .. } => {
                self.file = file;
                FileChange::Replaced(self.map()?)
            }
            Event::Gone => FileChange::Gone,
        };
        Some(change)
    }

    /// [`FileChange::Truncated`] if the file is now shorter than `mapped` bytes,
    /// whether or not the watcher has seen it yet.
    fn truncation(&self, mapped: u64) -> Option<FileChange> {
        let len = self.file.metadata().ok()?.len();
        if len < mapped {
            self.map().map(FileChange::Truncated)
        } else {
            None
        }
    }

    fn map(&self) -> Option<Source> {
        self.file.try_clone().and_then(Source::map).ok()
    }
}

/// A log file being viewed.
#[derive(Debug)]
pub struct App {
    name: String,
    source: Arc<Source>,
    indexer: Indexer,
    /// The records of the file's entries, and `source` again for the filter engine.
    index: SharedIndex,
    /// The fold state of the index's entries.
    folds: Folds,
    /// Per level, the entries of the index, as of the last change to it.
    level_counts: [u32; 6],
    filters: Filters,
    /// What `filters.time` is made of while set.
    time_filter: Option<TimeFilter>,
    /// The last entry of the index with a timestamp, and that timestamp, kept while
    /// there is a time filter.
    last_dated: Option<(usize, Timestamp)>,
    clock: fn() -> Timestamp,
    /// At work since the first [`set_filters`](Self::set_filters).
    filter: Option<Filter>,
    /// The last line looked up. Rows on screen are neighbours, so stepping from it is
    /// quicker than from the entry's start when an unfolded entry is long.
    line_mark: Cell<LineMark>,
    opening: Opening,
    /// The `follow` given to `new`: where a file that can't be followed opens again
    /// after a preset change.
    opens_at_end: bool,
    following: bool,
    /// The entries that came in while not following, see [`App::new_entries`]. Kept
    /// at 0 while following.
    unseen: usize,
    /// Whether the entries the index takes in count as new: they are the content of
    /// a file that took the place of the one shown.
    index_brings_new: bool,
    /// Whether the rows start with the truncation row.
    truncated: bool,
    waiting: bool,
    /// The file grown while the index wasn't complete yet, mapped.
    grown: Option<Source>,
    watch: Option<Watch>,
    progress: Progress,
    settings: Settings,
    /// Whether detection found no preset and the file is read in `Raw` for it.
    unrecognized: bool,
    viewport: Viewport,
    /// How many columns the message column is scrolled to the right.
    scroll: usize,
    shows_thread: bool,
    shows_line_numbers: bool,
    wraps: bool,
    panel: Option<Panel>,
    notice: Option<&'static str>,
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
    /// It watches no file until [`watch`](Self::watch).
    ///
    /// With `follow` it opens at the end of the file. As soon as it has a screen, it
    /// shows the file's last entries, read from the end of the file
    /// ([`index::tail`](crate::index::tail)), with the last one selected, until the
    /// index of the whole file is complete and takes over, still with the last entry
    /// selected. When those entries start too far from the end, it shows no rows
    /// until then; it [follows](Self::following) the file from there. Without
    /// `follow` it opens at the top, not following, and shows the first entries as
    /// soon as the index has them.
    pub fn new(
        path: &Path,
        source: Source,
        format: Option<Preset>,
        zone: TimeZone,
        mtime: Timestamp,
        follow: bool,
    ) -> Self {
        let picked = format.or_else(|| detect(source::lines(source.bytes())));
        let following = follow && source.origin() == Origin::Plain;
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
        let index = Indexed {
            records: Vec::new(),
            bytes: Arc::clone(&source),
        };
        Self {
            name: name.to_string_lossy().into_owned(),
            indexer: Indexer::spawn(Arc::clone(&source), settings.clone()),
            source,
            index: Arc::new(RwLock::new(index)),
            folds: Folds::default(),
            level_counts: [0; 6],
            filters: Filters::default(),
            time_filter: None,
            last_dated: None,
            clock: Timestamp::now,
            filter: None,
            line_mark: Cell::default(),
            opening: Opening::new(follow),
            opens_at_end: follow,
            following,
            unseen: 0,
            index_brings_new: false,
            truncated: false,
            waiting: false,
            grown: None,
            watch: None,
            progress,
            settings,
            unrecognized: picked.is_none(),
            viewport: Viewport::new(0, 0),
            scroll: 0,
            shows_thread: false,
            shows_line_numbers: true,
            wraps: false,
            panel: None,
            notice: None,
            quit: false,
        }
    }

    /// Takes in the entries indexed since the last call, folded or unfolded as the
    /// last [`Action::FoldAll`] or [`Action::UnfoldAll`] says. The selection stays
    /// where it was; with no rows before, the first row is selected. Opened at the
    /// end, the rows are the last entries until the index is complete, which then
    /// takes over with its last row selected. Only folding or unfolding all carries
    /// over from the last entries to the index.
    ///
    /// While a file is [watched](Self::watch), it also takes in what happened to the
    /// file since the last call, as [`file_changed`](Self::file_changed) does.
    pub fn poll_index(&mut self) {
        // Before the index changes, which starts the engine over from the first entry
        // it changes and drops what it found past it.
        if let Some(filter) = &mut self.filter {
            filter.take_in(&self.folds);
        }
        let changes = self.watch.as_mut().map_or_else(Vec::new, Watch::changes);
        for change in changes {
            self.file_changed(change);
        }
        // Rows past the file's new end would raise SIGBUS at the next draw, and the
        // watcher may only report the truncation at its next poll.
        let truncation = self.watch.as_ref().and_then(|w| w.truncation(self.size()));
        if let Some(change) = truncation {
            self.file_changed(change);
        }
        // Once complete, the index is kept up to date by `grow`, not by the indexer.
        if !self.progress.is_complete() {
            let mut index = write(&self.index);
            let known = index.records.len();
            self.progress = self.indexer.poll(&mut index.records);
            let found = &index.records[known..];
            push_entries(&mut self.folds, self.source.bytes(), found);
            count_levels(&mut self.level_counts, found, Count::Add);
            let found = found.len();
            drop(index);
            if self.index_brings_new {
                self.count_new(found);
            }
            // A time-only preset's entries get their timestamps once the index is
            // complete, all of them at once.
            let dated = self.progress.is_complete()
                && self
                    .settings
                    .preset
                    .entry_parser()
                    .is_some_and(|parser| !parser.has_date());
            if dated {
                self.reread(0);
            } else if found > 0 {
                self.reread(known);
            }
        }
        if self.progress.is_complete() {
            if !matches!(self.opening, Opening::Index) {
                self.show_last(Opening::Index);
            }
            if let Some(grown) = self.grown.take() {
                self.grow(grown);
            }
        }
        self.update_rows();
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

    /// Row `i`'s line as text (see [`crate::source::display`]), `── file truncated ──`
    /// for the row that marks a truncation. Not yet safe to show: it may hold control
    /// characters. Panics if `i` is not below `viewport().total()`.
    pub fn line(&self, i: usize) -> Cow<'_, str> {
        match self.row(i) {
            Some(shown) => source::display(shown.line),
            None => Cow::Borrowed(TRUNCATION_ROW),
        }
    }

    /// What row `i` shows: the first line of an entry, or a continuation line of an
    /// unfolded one. `None` for the row that marks a truncation (see
    /// [`FileChange::Truncated`]). Panics if `i` is not below `viewport().total()`.
    pub fn row(&self, i: usize) -> Option<ShownLine<'_>> {
        let folds = self.shown_folds();
        let (entry, line) = folds.line_at(self.fold_row(i)?);
        let record = self.shown_record(entry);
        Some(ShownLine {
            line: self.entry_line(&record, line),
            record,
            first: line == 0,
            number: record.line().map(|first| first + line as u64),
            hidden: folds.hidden_lines(entry),
        })
    }

    /// Whether the viewer follows the file: keeps its last row selected as entries
    /// come in.
    pub fn following(&self) -> bool {
        self.following
    }

    /// How many entries came in while the viewer didn't follow the file, since it
    /// stopped or since it opened at the top; 0 while it follows. Lines added to the
    /// last entry don't make a new entry.
    pub fn new_entries(&self) -> usize {
        self.unseen
    }

    /// Whether the file is gone from its path ([`FileChange::Gone`]) and no file has
    /// come back there yet.
    pub fn waiting_for_file(&self) -> bool {
        self.waiting
    }

    /// Whether the file is plain or gzip. A gzip file is read-only: the viewer opens
    /// it at the end or the top as [`new`](Self::new) says, but never follows it, and
    /// never [watches](Self::watch) it. [`Action::Bottom`] goes to the last row
    /// without following, and [`Action::ToggleFollow`] does nothing.
    pub fn origin(&self) -> Origin {
        self.source.origin()
    }

    fn can_follow(&self) -> bool {
        self.origin() == Origin::Plain
    }

    /// Takes in what happened to the followed file.
    ///
    /// - [`Grew`](FileChange::Grew): the entries appended are taken in as soon as the
    ///   index of the file is complete, the last one read again
    ///   ([`index::append`](crate::index::append)), as folded or unfolded as before.
    ///   A source shorter than what is indexed changes nothing: the file was cut short
    ///   since, and a `Truncated` follows.
    /// - [`Truncated`](FileChange::Truncated): the rows so far are dropped, as their
    ///   bytes are gone, and the file is indexed again from the start. The rows are
    ///   `── file truncated ──`, then the entries of the new content. Folding or
    ///   unfolding all carries over to them.
    /// - [`Replaced`](FileChange::Replaced): the same without the truncation row, and
    ///   the viewer no longer waits for the file.
    /// - [`Gone`](FileChange::Gone): the rows stay, and the viewer
    ///   [waits for the file](Self::waiting_for_file).
    ///
    /// While the viewer follows the file, the last row stays selected. While it
    /// doesn't, every new entry counts in [`new_entries`](Self::new_entries); after
    /// growth the view stays where it was, after a truncation or another file the
    /// first row is selected and the count is the new file's entries.
    pub fn file_changed(&mut self, change: FileChange) {
        match change {
            FileChange::Grew(source) if self.progress.is_complete() => self.grow(source),
            FileChange::Grew(source) => self.grown = Some(source),
            FileChange::Truncated(source) => self.reopen(source, true),
            FileChange::Replaced(source) => {
                self.waiting = false;
                self.reopen(source, false);
            }
            FileChange::Gone => self.waiting = true,
        }
        self.update_rows();
    }

    /// Watches the file at `path` from now on, with a [`Watcher`](crate::follow::Watcher)
    /// set up as `options` say. `file` is the file opened at `path` that the source
    /// maps. [`poll_index`](Self::poll_index) takes in what the watcher reports,
    /// mapping the file again, or the file that took its place. Fails if the watcher
    /// can't start. A file that can't be followed isn't watched.
    pub fn watch(&mut self, path: &Path, file: File, options: Options) -> io::Result<()> {
        if !self.can_follow() {
            return Ok(());
        }
        let watcher = Watcher::spawn(path.to_owned(), file.try_clone()?, self.size(), options)?;
        self.watch = Some(Watch { watcher, file });
        Ok(())
    }

    /// Whether the app has more to take in at the next [`poll_index`](Self::poll_index)
    /// without anything happening on screen: the index isn't complete, a watcher may
    /// report changes to the file, or the filters' results are still coming in.
    pub(crate) fn expects_updates(&self) -> bool {
        !self.progress.is_complete() || self.watch.is_some() || self.filtering()
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

    /// Whether the gutter of file line numbers is shown; it is until
    /// [`Action::ToggleLineNumbers`] hides it.
    pub fn shows_line_numbers(&self) -> bool {
        self.shows_line_numbers
    }

    /// The number in the file of the last line of the last entry: of the index, or
    /// of the file's last entries while they are shown. `None` while there are none
    /// or their numbers aren't known. Entries the filters hide count too, so that the
    /// gutter keeps its width while the filters change.
    pub fn last_line_number(&self) -> Option<u64> {
        let index;
        let (records, folds) = match &self.opening {
            Opening::Tail(records, folds) => (records, folds),
            Opening::Index | Opening::AtEnd => {
                index = read(&self.index);
                (&index.records, &self.folds)
            }
        };
        let last = records.len().checked_sub(1)?;
        let first = records[last].line()?;
        Some(first + folds.continuation_lines(last) as u64)
    }

    /// Shows only the entries that pass `filters` (see [`crate::filter`]), as their
    /// results come in from the background at each [`poll_index`](Self::poll_index):
    /// entries appended to the file are filtered as they come, and an entry read
    /// again is filtered again. The rows are the matching entries, folded or unfolded
    /// as before, with the truncation row still first. The first row is selected, the
    /// last one while following.
    ///
    /// Opened at the end, the file's last entries are shown unfiltered until the
    /// index is complete and takes over, filtered.
    pub fn set_filters(&mut self, filters: Filters) {
        self.filters = filters;
        // The index shows as it is, which is cheaper than an engine passing every entry.
        if hides_nothing(&self.filters) {
            self.filter = None;
        } else {
            self.start_filter();
        }
        self.viewport = Viewport::new(0, self.viewport.height());
        self.update_rows();
    }

    /// Sets the time filter, keeping the other filters, or clears it with `None`.
    /// It covers what [`TimeFilter::range`] says for the last entry with a timestamp
    /// and the [clock](Self::set_clock), in the app's zone; while there is no such
    /// entry, it lets none through. As entries come in, or get their dates once a
    /// time-only log is indexed, it covers what it says for the new last entry: the
    /// last N slides along, and the entries that fall out of it are no longer shown.
    /// Entries without a timestamp are hidden while it is set.
    pub fn set_time_filter(&mut self, filter: Option<TimeFilter>) {
        self.time_filter = filter;
        self.filters.time = None;
        self.last_dated = None;
        self.slide_time_filter(0);
        let filters = std::mem::take(&mut self.filters);
        self.set_filters(filters);
    }

    /// The time filter [`set_time_filter`](Self::set_time_filter) set; none at first.
    pub fn time_filter(&self) -> Option<&TimeFilter> {
        self.time_filter.as_ref()
    }

    /// Reads the wall clock from `now` from here on, for a time filter on
    /// [`Base::Now`](crate::filter::Base::Now). It is the system clock at first.
    pub fn set_clock(&mut self, now: fn() -> Timestamp) {
        self.clock = now;
    }

    /// The filters [`set_filters`](Self::set_filters) set; none at first.
    pub fn filters(&self) -> &Filters {
        &self.filters
    }

    /// Whether the filters' results aren't complete yet: entries the index has are
    /// still to be evaluated.
    pub fn filtering(&self) -> bool {
        self.filter.as_ref().is_some_and(|filter| {
            filter.engine.filtered().evaluated < read(&self.index).records.len()
        })
    }

    /// Whether the rows would be the entries that pass the filters, but none does:
    /// the index is complete and has entries, and the filters' results are in.
    pub fn nothing_matches(&self) -> bool {
        self.progress.is_complete()
            && !self.filtering()
            && self.filter.as_ref().is_some_and(|filter| {
                filter.engine.filtered().entries.is_empty() && !read(&self.index).records.is_empty()
            })
    }

    /// The header's level counters, indexed by `Level as usize`: per level, the
    /// entries of that level that pass every filter but the level filter, so that a
    /// hidden level doesn't drop to zero. Entries without a level aren't counted.
    ///
    /// Without a class, time or text filter they are the index's entries per level
    /// as of the last [`poll_index`](Self::poll_index), counted as the index takes
    /// them in. With one, they come in with the filters' results, as
    /// [`filtering`](Self::filtering) says.
    pub fn level_counts(&self) -> [u32; 6] {
        match &self.filter {
            Some(filter) if narrows_beyond_levels(&self.filters) => filter.engine.filtered().counts,
            _ => self.level_counts,
        }
    }

    /// The open panel, if any: [`Panel::Levels`] while the level panel is open,
    /// [`Panel::Classes`] while the class panel is, [`Panel::Time`] while the time
    /// panel is, [`Panel::Text`] while the text filter prompt is.
    ///
    /// [`Action::LevelPanel`] opens the level panel with the
    /// current level filter and the cursor on TRACE. While it is open, up and down
    /// (`Action::Up`, `Action::Down`) move its cursor, stopping at TRACE and FATAL,
    /// [`Action::ToggleFold`] toggles the level under the cursor and
    /// [`Action::ToggleLevel`] the level it names, in the panel only.
    /// [`Action::Confirm`] sets the level filter to the panel's, keeping the other
    /// filters, and closes it; [`Action::Cancel`] closes it and keeps the filters as
    /// they were. [`Action::Quit`] still quits; every other action does nothing.
    ///
    /// [`Action::ClassPanel`] opens the class panel with the current class rules, one
    /// per line ([`Classes::rules`](crate::filter::Classes::rules)), and an empty line
    /// after them with the cursor on it. [`App::key`] edits it.
    ///
    /// [`Action::TimePanel`] opens the time panel with the current time filter's spec
    /// and base, or an empty spec counted from the file without one. [`App::key`]
    /// edits it.
    ///
    /// [`Action::TextPanel`] opens the text filter prompt with the current text
    /// filter's pattern and options ([`Matcher::pattern`](crate::matcher::Matcher::pattern),
    /// [`Matcher::options`](crate::matcher::Matcher::options)), or an empty plain,
    /// case-sensitive pattern without one. [`App::key`] edits it.
    pub fn panel(&self) -> Option<&Panel> {
        self.panel.as_ref()
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
    /// While the viewer follows the file, a move away from the last row (up, a page
    /// or half a page up, the top) stops following; the other actions keep it.
    /// [`Action::Bottom`] and [`Action::ToggleFollow`] follow it again from the last
    /// row, and the count of new entries starts again from 0. Moving down onto the
    /// last row doesn't follow again. A file that [can't be followed](Self::origin)
    /// never is: [`Action::Bottom`] only selects the last row and
    /// [`Action::ToggleFollow`] does nothing.
    ///
    /// Folding and unfolding keep the selected line on its screen line, as far as the
    /// ends allow (in wrap mode, the selected row stays as many rows below the top).
    /// When the selected line gets folded away, its entry's first line takes its
    /// place.
    pub fn apply(&mut self, action: Action) {
        if let Some(panel) = self.panel.take() {
            self.apply_in_panel(panel, action);
            return;
        }
        if matches!(
            action,
            Action::Up | Action::PageUp | Action::HalfPageUp | Action::Top
        ) {
            self.following = false;
        }
        let can_follow = self.can_follow();
        let viewport = &mut self.viewport;
        match action {
            Action::Up => viewport.up(1),
            Action::Down => viewport.down(1),
            Action::PageUp => viewport.page_up(),
            Action::PageDown => viewport.page_down(),
            Action::HalfPageUp => viewport.half_page_up(),
            Action::HalfPageDown => viewport.half_page_down(),
            Action::Top => viewport.go_top(),
            Action::Bottom if can_follow => self.follow(),
            Action::Bottom => viewport.go_bottom(),
            Action::ToggleFollow if !can_follow => {}
            Action::ToggleFollow if self.following => self.following = false,
            Action::ToggleFollow => self.follow(),
            Action::ScrollLeft => self.scroll = self.scroll.saturating_sub(SCROLL_STEP),
            Action::ScrollRight => self.scroll = self.scroll.saturating_add(SCROLL_STEP),
            Action::ToggleThread => self.shows_thread = !self.shows_thread,
            Action::ToggleLineNumbers => self.shows_line_numbers = !self.shows_line_numbers,
            Action::ToggleWrap => self.wraps = !self.wraps,
            Action::CyclePreset => self.cycle_preset(),
            Action::ToggleFold => self.toggle_fold(),
            Action::FoldAll => self.set_all_folded(true),
            Action::UnfoldAll => self.set_all_folded(false),
            Action::ToggleLevel(level) => {
                let mut levels = self.filters.levels;
                levels.toggle(level);
                self.set_levels(levels);
            }
            Action::LevelPanel => {
                self.panel = Some(Panel::Levels(LevelPanel {
                    levels: self.filters.levels,
                    cursor: Level::Trace,
                }));
            }
            Action::OnlyClass => self.change_classes(|classes, class| {
                classes.include = vec![Rule::Prefix(class)];
            }),
            Action::HideClass => self.change_classes(|classes, class| {
                classes.exclude.push(Rule::Prefix(class));
            }),
            Action::ClassPanel => {
                self.panel = Some(Panel::Classes(ClassPanel::new(&self.filters.classes)));
            }
            Action::TimePanel => {
                self.panel = Some(Panel::Time(TimePanel::new(self.time_filter.as_ref())));
            }
            Action::TextPanel => {
                let prompt = Prompt::new(self.filters.text.as_ref());
                self.panel = Some(Panel::Text(prompt));
            }
            Action::ClearFilters => {
                self.time_filter = None;
                self.last_dated = None;
                self.set_filters(Filters::default());
            }
            Action::Confirm | Action::Cancel => {}
            Action::Quit => self.quit = true,
        }
    }

    /// Does what `key` asks. While the class panel is open, the key edits it: a
    /// character, with or without Shift, is typed at the end of the cursor's line,
    /// `Backspace` deletes the line's last character, or the line itself if it is
    /// empty and not the only one (the cursor going to the line above), `↑ ↓` move the
    /// cursor between the lines, stopping at the ends, and `↓` on the last line adds
    /// an empty line after it unless it is empty itself. `Enter` sets the class filter
    /// to the rules of the lines ([`Classes::parse`](crate::filter::Classes::parse)),
    /// keeping the other filters, and closes the panel; if they don't parse, the
    /// panel stays open with the [error](ClassPanel::error) and the filters stay as
    /// they were. `Esc` closes the panel without applying it, `Ctrl-c` quits, and
    /// other keys do nothing.
    ///
    /// The time panel is edited the same way, its one line being the spec, except
    /// that `Backspace` only deletes, `↓ ↑` step through the presets `1m 5m 10m 30m
    /// 60m 24h`, stopping at the ends (from a spec that is none of them, `↓` goes to
    /// the first and `↑` to the last), and `Tab` toggles the base. `Enter` sets the
    /// time filter ([`set_time_filter`](Self::set_time_filter)) to the spec and base,
    /// or clears it for a blank spec; one that doesn't parse keeps the panel open with
    /// the [error](TimePanel::error).
    ///
    /// The text filter prompt is edited like the time panel, except that `Ctrl-r`
    /// toggles [regex](crate::matcher::Options::regex) and `Tab` or `Ctrl-i`
    /// [ignore case](crate::matcher::Options::ignore_case). `Enter` sets the text
    /// filter to the pattern read as the options say, keeping the other filters, or
    /// clears it for an empty pattern; an invalid regex keeps the prompt open with
    /// the [error](Prompt::error).
    ///
    /// Otherwise it [applies](Self::apply) the [`action`] bound to the key, if any.
    pub fn key(&mut self, key: KeyEvent) {
        self.notice = None;
        let action = match &mut self.panel {
            Some(Panel::Classes(panel)) => panel.key(key),
            Some(Panel::Time(panel)) => panel.key(key),
            Some(Panel::Text(prompt)) => prompt.key(key),
            _ => action(key),
        };
        if let Some(action) = action {
            self.apply(action);
        }
    }

    /// What the viewer has to tell about the last key, until the next one: `the
    /// selected entry has no class` after [`Action::OnlyClass`] or
    /// [`Action::HideClass`] on an entry without a class (or with no row selected),
    /// which then change nothing.
    pub fn notice(&self) -> Option<&str> {
        self.notice
    }

    /// Whether [`Action::Quit`] has been applied.
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Indexes the file again in the preset after the current one in
    /// [`Preset::ALL`]. It opens again at the end while following, at the top
    /// otherwise; a file that can't be followed opens again as `new` opened it.
    fn cycle_preset(&mut self) {
        let presets = Preset::ALL;
        // `ALL` holds every preset, so the current one is always found.
        let current = presets.iter().position(|&p| p == self.settings.preset);
        self.settings.preset = presets[current.map_or(0, |i| (i + 1) % presets.len())];
        self.unrecognized = false;
        self.folds = Folds::default();
        self.truncated = false;
        self.index_brings_new = false;
        let at_end = if self.can_follow() {
            self.following
        } else {
            self.opens_at_end
        };
        self.index_again(Opening::new(at_end));
        // The engine reads the entries in the preset it started with.
        if self.filter.is_some() {
            self.start_filter();
        }
        self.read_tail();
    }

    /// Starts evaluating the filters over the index, in place of any engine at work.
    fn start_filter(&mut self) {
        let filters = self.filters.clone();
        let preset = self.settings.preset;
        self.filter = Some(Filter::spawn(&self.index, preset, filters, &self.folds));
    }

    /// Indexes the source from the start, the rows opening as `opening` says.
    fn index_again(&mut self, opening: Opening) {
        // Dropping the old indexer stops it.
        self.indexer = Indexer::spawn(Arc::clone(&self.source), self.settings.clone());
        write(&self.index).records.clear();
        self.level_counts = [0; 6];
        self.reread(0);
        self.progress = Progress {
            indexed: 0,
            total: self.size(),
        };
        self.opening = opening;
        self.viewport = Viewport::new(0, self.viewport.height());
    }

    /// Has the filters evaluate the index's entries from `from` on again, after they
    /// changed, and waits for them when there are only a few left to evaluate.
    fn reread(&mut self, from: usize) {
        let slide = self.slide_time_filter(from);
        let Some(filter) = &mut self.filter else {
            return;
        };
        match slide {
            Slide::Restart => filter.restart(self.filters.clone()),
            Slide::Later(start) => {
                // Only matches dated before the new start drop out; the others stay.
                let from = first_dropped(&filter.engine, &read(&self.index).records, start)
                    .map_or(from, |entry| entry.min(from));
                filter.refilter_from(self.filters.clone(), from);
            }
            Slide::None => filter.reread(from),
        }
        let total = read(&self.index).records.len();
        if total.saturating_sub(filter.engine.filtered().evaluated) <= FILTER_CHUNK {
            filter.catch_up(&self.folds, total, Instant::now() + CATCH_UP);
        }
    }

    /// Brings the time range of the time filter, if any, up to date with the index,
    /// whose entries from `from` on are new or read again. How it changed.
    fn slide_time_filter(&mut self, from: usize) -> Slide {
        let Some(time_filter) = &self.time_filter else {
            return Slide::None;
        };
        self.last_dated = last_dated(&read(&self.index).records, from, self.last_dated);
        let range = match self.last_dated {
            Some((_, last)) => time_filter.range(last, (self.clock)(), &self.settings.zone),
            None => NO_TIME,
        };
        let old = self.filters.time.replace(range);
        match old {
            Some(old) if old == range => Slide::None,
            Some(old) if old.end == range.end && old.start <= range.start => {
                Slide::Later(range.start)
            }
            _ => Slide::Restart,
        }
    }

    /// Selects the last row and follows the file from there.
    fn follow(&mut self) {
        self.following = true;
        self.unseen = 0;
        self.viewport.go_bottom();
    }

    /// Counts `entries` new entries unless the viewer follows the file.
    fn count_new(&mut self, entries: usize) {
        if !self.following {
            self.unseen += entries;
        }
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
        self.viewport.set_total(self.row_count());
        self.viewport.go_bottom();
    }

    /// Tells the viewport how many rows there are, and keeps the last one selected
    /// while following.
    fn update_rows(&mut self) {
        self.viewport.set_total(self.row_count());
        if self.following {
            self.viewport.go_bottom();
        }
    }

    fn row_count(&self) -> usize {
        self.lead_rows() + self.shown_folds().row_count()
    }

    /// The rows before the entries: the truncation row, if there is one.
    fn lead_rows(&self) -> usize {
        usize::from(self.truncated)
    }

    /// The row of the fold state that viewport row `row` shows, `None` for the
    /// truncation row.
    fn fold_row(&self, row: usize) -> Option<usize> {
        row.checked_sub(self.lead_rows())
    }

    fn set_source(&mut self, source: Source) {
        self.source = Arc::new(source);
        write(&self.index).bytes = Arc::clone(&self.source);
        // The mark points into the old mapping, whose bytes may be gone.
        self.line_mark.take();
    }

    /// Takes in `source`, the file grown, unless it is shorter than what is indexed.
    fn grow(&mut self, source: Source) {
        if source.bytes().len() <= self.source.bytes().len() {
            return;
        }
        self.set_source(source);
        let mut index = write(&self.index);
        let known = index.records.len();
        count_levels(
            &mut self.level_counts,
            &index.records[appended_from(known)..],
            Count::Remove,
        );
        index::append(&mut index.records, self.source.bytes(), &self.settings);
        count_levels(
            &mut self.level_counts,
            &index.records[appended_from(known)..],
            Count::Add,
        );
        push_appended(&mut self.folds, self.source.bytes(), &index.records, known);
        let found = index.records.len().saturating_sub(known);
        drop(index);
        self.count_new(found);
        self.reread(appended_from(known));
        self.progress = Progress {
            indexed: self.size(),
            total: self.size(),
        };
    }

    /// Shows `source`, the file that took the place of the one shown, from its start,
    /// after the truncation row if `truncated`.
    fn reopen(&mut self, source: Source, truncated: bool) {
        self.set_source(source);
        self.grown = None;
        self.truncated = truncated;
        self.index_brings_new = true;
        self.unseen = 0;
        self.folds.truncate(0);
        self.index_again(Opening::Index);
    }

    /// The fold state of the entries the rows show: the file's last entries, the
    /// matches of the filters or the index.
    fn shown_folds(&self) -> &Folds {
        match (&self.opening, &self.filter) {
            (Opening::Tail(_, folds), _) => folds,
            (_, Some(filter)) => &filter.folds,
            (_, None) => &self.folds,
        }
    }

    fn shown_folds_mut(&mut self) -> &mut Folds {
        match (&mut self.opening, &mut self.filter) {
            (Opening::Tail(_, folds), _) => folds,
            (_, Some(filter)) => &mut filter.folds,
            (_, None) => &mut self.folds,
        }
    }

    /// The record of the rows' `entry`th entry.
    fn shown_record(&self, entry: usize) -> Record {
        match (&self.opening, &self.filter) {
            (Opening::Tail(records, _), _) => records[entry],
            (_, Some(filter)) => read(&self.index).records[filter.entry(entry)],
            (_, None) => read(&self.index).records[entry],
        }
    }

    fn toggle_fold(&mut self) {
        if let Some(row) = self.viewport.selected().and_then(|row| self.fold_row(row)) {
            let (entry, _) = self.shown_folds().line_at(row);
            self.refold(|folds| folds.toggle(entry));
            // The index keeps the fold state of a match for when the filters change.
            if let (Opening::Index | Opening::AtEnd, Some(filter)) = (&self.opening, &self.filter) {
                let entry = filter.entry(entry);
                self.folds.toggle(entry);
            }
        }
    }

    /// `action` while `panel` is open, taken out of `self.panel`: Confirm applies it,
    /// Cancel leaves it closed, Quit quits with it open, anything else edits it and puts
    /// it back.
    fn apply_in_panel(&mut self, mut panel: Panel, action: Action) {
        match action {
            Action::Confirm => match panel {
                Panel::Levels(panel) => self.set_levels(panel.levels),
                Panel::Classes(mut panel) => {
                    match Classes::parse(panel.lines.iter().map(String::as_str)) {
                        Ok(classes) => self.set_classes(classes),
                        Err(error) => {
                            panel.error = Some(error);
                            self.panel = Some(Panel::Classes(panel));
                        }
                    }
                }
                Panel::Time(mut panel) => match panel.filter() {
                    Ok(filter) => self.set_time_filter(filter),
                    Err(error) => {
                        panel.error = Some(error);
                        self.panel = Some(Panel::Time(panel));
                    }
                },
                Panel::Text(mut prompt) => match prompt.matcher() {
                    Ok(text) => self.set_text(text),
                    Err(error) => {
                        prompt.error = Some(error);
                        self.panel = Some(Panel::Text(prompt));
                    }
                },
            },
            Action::Cancel => {}
            Action::Quit => {
                self.quit = true;
                self.panel = Some(panel);
            }
            _ => {
                panel.edit(action);
                self.panel = Some(panel);
            }
        }
    }

    /// Sets the level filter to `levels`, keeping the other filters.
    fn set_levels(&mut self, levels: Levels) {
        let others = std::mem::take(&mut self.filters);
        self.set_filters(Filters { levels, ..others });
    }

    /// Sets the class filter to `classes`, keeping the other filters.
    fn set_classes(&mut self, classes: Classes) {
        let others = std::mem::take(&mut self.filters);
        self.set_filters(Filters { classes, ..others });
    }

    /// Sets the text filter to `text`, keeping the other filters.
    fn set_text(&mut self, text: Option<Matcher>) {
        let others = std::mem::take(&mut self.filters);
        self.set_filters(Filters { text, ..others });
    }

    /// Lets `change` change the class filter with the selected entry's class, or
    /// says that it has none.
    fn change_classes(&mut self, change: impl FnOnce(&mut Classes, String)) {
        let Some(class) = self.selected_class() else {
            self.notice = Some(NO_CLASS);
            return;
        };
        let mut classes = self.filters.classes.clone();
        change(&mut classes, class);
        self.set_classes(classes);
    }

    /// The full class name of the selected row's entry, as its first line has it.
    fn selected_class(&self) -> Option<String> {
        let row = self.fold_row(self.viewport.selected()?)?;
        let (entry, _) = self.shown_folds().line_at(row);
        let record = self.shown_record(entry);
        let first = strip_ansi(self.entry_line(&record, 0));
        let class = first.get(record.class()?)?;
        Some(String::from_utf8_lossy(class).into_owned())
    }

    fn set_all_folded(&mut self, folded: bool) {
        let set: fn(&mut Folds) = if folded {
            Folds::fold_all
        } else {
            Folds::unfold_all
        };
        self.refold(set);
        // The fold states not shown take the mode too, so that it holds once they
        // are. Setting it again on the one shown changes nothing.
        set(&mut self.folds);
        if let Some(filter) = &mut self.filter {
            set(&mut filter.folds);
        }
    }

    /// Changes the fold state of the entries shown, keeping the selected line on its
    /// screen line, or its entry's first line if it gets folded away.
    fn refold(&mut self, change: impl FnOnce(&mut Folds)) {
        let viewport = &self.viewport;
        let selected = viewport.selected().and_then(|row| {
            let line = self.shown_folds().line_at(self.fold_row(row)?);
            Some((line, row - viewport.top()))
        });
        let lead = self.lead_rows();
        let folds = self.shown_folds_mut();
        change(folds);
        let selected = selected.map(|((entry, line), screen_line)| {
            let folded = folds.hidden_lines(entry) > 0;
            let line = if folded { 0 } else { line };
            (lead + folds.row_of(entry, line), screen_line)
        });
        self.viewport.set_total(self.row_count());
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

/// Brings `folds`, which holds the first `known` entries of `records`, up to date
/// after [`index::append`] added to `records`. The entries read again keep their
/// fold state.
fn push_appended(folds: &mut Folds, bytes: &[u8], records: &[Record], known: usize) {
    let from = appended_from(known);
    let was: Vec<_> = (from..known).map(|entry| unfolded(folds, entry)).collect();
    folds.truncate(from);
    push_entries(folds, bytes, &records[from..]);
    for (entry, was) in (from..records.len()).zip(was) {
        if was.is_some() && unfolded(folds, entry) != was {
            folds.toggle(entry);
        }
    }
}

/// The first of `known` records that [`index::append`] may have changed.
fn appended_from(known: usize) -> usize {
    // `append` reads the last entry again, and a partial last line that looked like a
    // header can turn out to continue the entry before it.
    known.saturating_sub(2)
}

// Only `App` writes the index, so only a panic of its own can poison it, and that
// ends the viewer.
fn read(index: &SharedIndex) -> RwLockReadGuard<'_, Indexed<Source>> {
    index.read().unwrap_or_else(PoisonError::into_inner)
}

fn write(index: &SharedIndex) -> RwLockWriteGuard<'_, Indexed<Source>> {
    index.write().unwrap_or_else(PoisonError::into_inner)
}

/// The first of the engine's matches dated before `start`.
fn first_dropped(engine: &Engine, records: &[Record], start: Timestamp) -> Option<usize> {
    let mut entries = engine
        .filtered()
        .entries
        .iter()
        .map(|&entry| entry as usize);
    entries.find(|&entry| {
        let dated = records.get(entry).and_then(Record::timestamp);
        dated.is_some_and(|at| at < start)
    })
}

/// The last of `records` with a timestamp, and that timestamp. `known` is what this
/// was before the records from `from` on changed, so only those are looked at again
/// unless it was among them.
fn last_dated(
    records: &[Record],
    from: usize,
    known: Option<(usize, Timestamp)>,
) -> Option<(usize, Timestamp)> {
    let from = from.min(records.len());
    let dated = |entries: Range<usize>| {
        entries
            .rev()
            .find_map(|entry| Some((entry, records[entry].timestamp()?)))
    };
    dated(from..records.len()).or_else(|| match known {
        Some((entry, _)) if entry < from => known,
        Some(_) => dated(0..from),
        None => None,
    })
}

/// Whether `filters` let every entry through.
fn hides_nothing(filters: &Filters) -> bool {
    filters.levels == Levels::default() && !narrows_beyond_levels(filters)
}

/// Whether `filters` hide entries by their class, time or text.
fn narrows_beyond_levels(filters: &Filters) -> bool {
    filters.classes != Classes::default() || filters.time.is_some() || filters.text.is_some()
}

enum Count {
    Add,
    Remove,
}

/// Adds the levels of `records` to `counts`, or takes them out.
fn count_levels(counts: &mut [u32; 6], records: &[Record], count: Count) {
    for level in records.iter().filter_map(Record::level) {
        let n = &mut counts[level as usize];
        *n = match count {
            Count::Add => *n + 1,
            Count::Remove => n.saturating_sub(1),
        };
    }
}

/// Whether `entry` shows its continuation lines, `None` if it has none.
fn unfolded(folds: &Folds, entry: usize) -> Option<bool> {
    let shown = folds.row_of(entry + 1, 0) - folds.row_of(entry, 0) - 1;
    (shown + folds.hidden_lines(entry) > 0).then_some(shown > 0)
}
