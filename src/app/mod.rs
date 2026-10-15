//! The viewer's state and what keys do to it. No terminal and no drawing: `ui` draws an
//! `App`, the event loop turns keys into actions and applies them.

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::mem;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use memchr::{memchr, memchr_iter, memrchr};

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::clipboard::{Clipboard, CopyError, entry_text};
use crate::config::{Colors, Config, Ui};
use crate::export;
use crate::filter::{
    self, Base, Classes, Engine, Filtered, Filters, Indexed, Levels, Rule, TimeFilter, TimeRange,
    TimeSpec,
};
use crate::fold::Folds;
use crate::follow::{Event, Liveness, Options, Watcher};
use crate::format::{Anchor, Level, Preset, detect, strip_ansi};
use crate::histogram::{Counter, Histogram};
use crate::index::{self, Indexer, Progress, Record, Settings};
use crate::keymap;
use crate::matcher::{self, Matcher};
use crate::mouse::{HitMap, Target};
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
    /// Moves the [focus](App::focus) to the next pane that is shown.
    CycleFocus,
    /// Shows the histogram ([`App::shows_histogram`]) or hides it.
    ToggleHistogram,
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
    /// Opens the prompt for a file line number to jump to ([`App::panel`]).
    LinePrompt,
    /// Opens the prompt for a time to jump to ([`App::panel`]).
    TimePrompt,
    /// Opens the prompt for a search down the rows ([`App::panel`]).
    SearchForward,
    /// Opens the prompt for a search up the rows ([`App::panel`]).
    SearchBackward,
    /// Selects the next matching entry in the direction of the search ([`App::search`]).
    NextMatch,
    /// Selects the next matching entry against the direction of the search.
    PreviousMatch,
    /// Selects the next entry the rows show that has an exception.
    NextException,
    /// Selects the previous entry the rows show that has an exception.
    PreviousException,
    /// Bookmarks the selected entry, or removes its bookmark if it has one.
    ToggleBookmark,
    /// Selects the next bookmarked entry the rows show, wrapping around to the first
    /// with the notice `bookmarks wrapped`, or says `no bookmarks`.
    NextBookmark,
    /// Selects the previous bookmarked entry the rows show, wrapping around to the
    /// last with the notice `bookmarks wrapped`, or says `no bookmarks`.
    PreviousBookmark,
    /// Opens the export prompt ([`App::panel`]), which suggests the file name
    /// `<name>.filtered.log`, `<name>` being the viewed file's name without `.gz`.
    Export,
    /// Puts the selected entry on the [clipboard](App::set_clipboard): every line of
    /// it, folded or not, as [`entry_text`](crate::clipboard::entry_text) gives it.
    /// The [notice](App::notice) says `copied to clipboard`; with no row selected it says
    /// `nothing to copy`, without a clipboard `no clipboard`, for an entry OSC 52
    /// refuses `entry too large for OSC 52 (over 100 KB)`, and when the terminal
    /// can't be written `copy failed: ` and the error.
    Copy,
    /// Applies what the open panel edits and closes it. With no panel open, opens
    /// the details pane ([`App::shows_details`]), or closes it.
    Confirm,
    /// Closes the open panel without applying its edits. With no panel open, drops an
    /// export that waits for the filters.
    Cancel,
    /// Opens the help overlay ([`App::panel`]), which lists the
    /// [key bindings](crate::keymap::bindings).
    Help,
    Quit,
}

/// The action bound to a key in the [keymap](crate::keymap::bindings), `None` for
/// keys that do nothing.
///
/// Characters ignore Shift (the character already says which case it is) but not Ctrl
/// or Alt; other keys do nothing with any modifier. Key releases do nothing; held keys
/// repeat.
pub fn action(key: KeyEvent) -> Option<Action> {
    let (code, modifiers) = normalized(key)?;
    keymap::bindings()
        .iter()
        .find(|binding| binding.code == code && binding.modifiers == modifiers)
        .map(|binding| binding.action)
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

/// The pane that takes the moves `Up`, `Down`, `PageUp` and `PageDown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Table,
    /// The details pane ([`App::shows_details`]).
    Details,
    /// The histogram ([`App::shows_histogram`]).
    Histogram,
}

/// The scroll of the details pane and what it was last told about its size.
#[derive(Debug, Clone, Copy, Default)]
struct Details {
    /// Where the shown entry starts in the file.
    entry: Option<usize>,
    scroll: usize,
    height: usize,
    lines: usize,
}

impl Details {
    fn max_scroll(&self) -> usize {
        self.lines.saturating_sub(self.height)
    }
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

/// What a jump prompt jumps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// The entry holding a file line, by its number in the gutter.
    Line,
    /// The first entry at or after a time.
    Time,
}

/// A jump prompt while it is open: what it jumps to, the text typed so far, and why
/// the text last confirmed says nothing to jump to, if it doesn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpPrompt {
    pub to: Jump,
    pub text: String,
    pub error: Option<String>,
}

impl JumpPrompt {
    fn new(to: Jump) -> JumpPrompt {
        JumpPrompt {
            to,
            text: String::new(),
            error: None,
        }
    }

    /// Edits the text as `key` asks, or returns the action it stands for here.
    fn key(&mut self, key: KeyEvent) -> Option<Action> {
        match typed(key)? {
            Typed::Action(action) => return Some(action),
            Typed::Char(c) => self.text.push(c),
            Typed::Backspace => {
                self.text.pop();
            }
            Typed::Other(_) => {}
        }
        None
    }
}

/// The export prompt while it is open: the path to export to as typed so far, read
/// from the [current directory](App::set_current_dir) when relative; why it can't be
/// exported to, if the path last confirmed can't; and whether the prompt asks to
/// overwrite the existing file the path names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportPrompt {
    pub text: String,
    pub error: Option<String>,
    pub overwrite: bool,
}

impl ExportPrompt {
    /// Edits the path as `key` asks, or returns the action it stands for here. While
    /// asking to overwrite, `y` confirms and any other key goes back to the path.
    fn key(&mut self, key: KeyEvent) -> Option<Action> {
        let typed = typed(key)?;
        if self.overwrite {
            return match typed {
                Typed::Action(Action::Quit) => Some(Action::Quit),
                Typed::Char('y') => Some(Action::Confirm),
                _ => {
                    self.overwrite = false;
                    None
                }
            };
        }
        match typed {
            Typed::Action(action) => return Some(action),
            Typed::Char(c) => self.text.push(c),
            Typed::Backspace => {
                self.text.pop();
            }
            Typed::Other(_) => {}
        }
        self.error = None;
        None
    }
}

/// An export that waits for the index and the filters to be done.
#[derive(Debug)]
struct PendingExport {
    target: PathBuf,
    /// The path as the user typed it, for the notice.
    typed: String,
}

/// The panel open over the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Panel {
    Levels(LevelPanel),
    Classes(ClassPanel),
    Time(TimePanel),
    /// The text filter prompt.
    Text(Prompt),
    Jump(JumpPrompt),
    Export(ExportPrompt),
    /// The search prompt, for a search up the rows if `backward`.
    Search {
        prompt: Prompt,
        backward: bool,
    },
    Help(HelpPanel),
}

/// The help overlay, which lists the lines of [`keymap::help`](crate::keymap::help).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpPanel {
    /// How many of its lines are scrolled off above the overlay.
    pub scroll: usize,
    height: usize,
    lines: usize,
}

impl HelpPanel {
    fn max_scroll(&self) -> usize {
        self.lines.saturating_sub(self.height)
    }

    fn edit(&mut self, action: Action) {
        let scroll = match action {
            Action::Up => self.scroll.saturating_sub(1),
            Action::Down => self.scroll + 1,
            Action::PageUp => self.scroll.saturating_sub(self.height),
            Action::PageDown => self.scroll + self.height,
            _ => return,
        };
        self.scroll = scroll.min(self.max_scroll());
    }
}

/// What `key` is while the help overlay is open: `q` closes it like `Esc` and `F1`,
/// and only the scrolling keys and `Ctrl-c` do anything else.
fn help_action(key: KeyEvent) -> Option<Action> {
    match normalized(key)? {
        (KeyCode::Char('q'), KeyModifiers::NONE) => Some(Action::Cancel),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),
        _ => action(key).filter(|action| {
            matches!(
                action,
                Action::Up
                    | Action::Down
                    | Action::PageUp
                    | Action::PageDown
                    | Action::Cancel
                    | Action::Help
            )
        }),
    }
}

/// A search: what it looks for and which way [`Action::NextMatch`] goes, up the rows
/// if `backward`.
#[derive(Debug, Clone)]
pub struct Search {
    pub matcher: Matcher,
    pub backward: bool,
}

impl Panel {
    fn edit(&mut self, action: Action) {
        match self {
            Panel::Levels(panel) => panel.edit(action),
            // Their text is edited through `App::key`, not actions.
            Panel::Classes(_)
            | Panel::Time(_)
            | Panel::Text(_)
            | Panel::Jump(_)
            | Panel::Export(_)
            | Panel::Search { .. } => {}
            Panel::Help(panel) => panel.edit(action),
        }
    }
}

/// What [`App::notice`] says after `c` or `C` on an entry without a class.
const NO_CLASS: &str = "the selected entry has no class";

/// How many columns [`Action::ScrollLeft`] and [`Action::ScrollRight`] move the
/// message column by.
const SCROLL_STEP: usize = 8;
/// Rows or lines one notch of the mouse wheel scrolls.
const WHEEL_STEP: usize = 3;
/// The longest between two clicks on a row that make a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

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

/// How an engine evaluates again after the index or the time range changed.
enum Rerun {
    /// From the first entry.
    Restart,
    /// From this entry, which the index read again or dropped.
    Reread(usize),
    /// From this entry, with filters that only drop entries from there on.
    Drop(usize),
}

impl Rerun {
    /// What the engine has to do for `slide`, the entries from `from` on being new or
    /// read again. Only the matches dated before a later start drop out.
    fn after(slide: &Slide, engine: &Engine, records: &[Record], from: usize) -> Rerun {
        match *slide {
            Slide::Restart => Rerun::Restart,
            Slide::Later(start) => {
                let dropped = first_dropped(engine, records, start);
                Rerun::Drop(dropped.map_or(from, |entry| entry.min(from)))
            }
            Slide::None => Rerun::Reread(from),
        }
    }

    fn apply(&self, engine: &mut Engine, filters: &Filters) {
        match *self {
            Rerun::Restart => engine.set_filters(filters.clone()),
            Rerun::Reread(from) => engine.reread(from),
            Rerun::Drop(from) => engine.set_filters_from(filters.clone(), from),
        }
    }
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

    /// Has the engine evaluate again as `rerun` says, dropping the fold state of the
    /// matches it drops.
    fn rerun(&mut self, rerun: &Rerun, filters: &Filters) {
        let kept = match *rerun {
            Rerun::Restart => 0,
            Rerun::Reread(from) | Rerun::Drop(from) => self.matches_before(from),
        };
        rerun.apply(&mut self.engine, filters);
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

/// The histogram's counts of the entries the rows come from, and what was worked out
/// from them.
#[derive(Debug, Default)]
struct HistogramState {
    counter: Counter,
    /// How many of the entries (the filters' matches, or the index's entries without
    /// filters) the counter took in.
    fed: usize,
    /// Bumped when `counter` changes, to tell when `cached` is stale.
    version: u64,
    cached: Option<CachedHistogram>,
    bucket: usize,
}

#[derive(Debug)]
struct CachedHistogram {
    width: usize,
    version: u64,
    histogram: Option<Histogram>,
}

impl HistogramState {
    /// Counts the entries again from the first, as those from `kept` on are gone or
    /// changed.
    fn invalidate_from(&mut self, kept: usize) {
        if kept < self.fed {
            self.reset();
        }
    }

    /// Takes the entries from `from` on, which the index is about to read again, out
    /// of the counter, as `records` has them still. With `filter`, `from` is an index
    /// entry and the counted ones are its matches.
    fn uncount(&mut self, filter: Option<&Filter>, records: &[Record], from: usize) {
        let kept = filter.map_or(from, |filter| filter.matches_before(from));
        for i in kept..self.fed {
            let record = counted_record(filter, records, i);
            if let Some((at, level)) = record.and_then(|r| Some((r.timestamp()?, r.level()))) {
                self.counter.remove(at, level);
            }
        }
        self.fed = self.fed.min(kept);
        self.version += 1;
    }

    fn reset(&mut self) {
        self.counter = Counter::default();
        self.fed = 0;
        self.version += 1;
    }
}

/// The latest time the app was told, and the time of the file's last change.
#[derive(Debug, Clone, Copy)]
struct Clock {
    now: Instant,
    changed: Instant,
}

impl Clock {
    fn since_change(self) -> Duration {
        self.now.saturating_duration_since(self.changed)
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
    search: Option<Search>,
    /// The byte offsets of the bookmarks, which hold across changes to the rows.
    bookmarks: BTreeSet<u64>,
    /// Counts the entries the filters show that match `search`; at work while there
    /// is one.
    matches: Option<Engine>,
    /// The way a move to a match is waiting to go, for matches not yet counted.
    pending_step: Option<bool>,
    /// An export waiting for the index and the filters' results.
    pending_export: Option<PendingExport>,
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
    /// Set by the first [`tick`](Self::tick).
    ticks: Option<Clock>,
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
    timestamps: Timestamps,
    colors: Colors,
    shows_details: bool,
    shows_histogram: bool,
    focus: Focus,
    details: Details,
    histogram: HistogramState,
    panel: Option<Panel>,
    notice: Option<Cow<'static, str>>,
    /// Where the file was opened from, to tell an export onto it.
    path: PathBuf,
    /// What a relative export path is read from.
    current_dir: PathBuf,
    hit_map: HitMap,
    /// The row and time of the last click, until a double click uses it up.
    last_click: Option<(usize, Instant)>,
    /// Where [`Action::Copy`] puts the selected entry.
    clipboard: Option<Box<dyn Clipboard>>,
    quit: bool,
}

/// How the table shows a timestamp from 100 columns on.
#[derive(Debug, Clone)]
pub struct Timestamps {
    /// In strftime syntax.
    pub format: String,
    /// How many columns the formatted text takes.
    pub width: usize,
}

impl Timestamps {
    fn of(ui: &Ui) -> Self {
        Timestamps {
            format: ui.timestamp_format.clone(),
            width: ui.timestamp_width(),
        }
    }
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
        let defaults = Config::default();
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
            search: None,
            bookmarks: BTreeSet::new(),
            matches: None,
            pending_step: None,
            pending_export: None,
            line_mark: Cell::default(),
            opening: Opening::new(follow),
            opens_at_end: follow,
            following,
            unseen: 0,
            index_brings_new: false,
            truncated: false,
            waiting: false,
            ticks: None,
            grown: None,
            watch: None,
            progress,
            settings,
            unrecognized: picked.is_none(),
            viewport: Viewport::new(0, 0),
            scroll: 0,
            shows_details: false,
            shows_histogram: false,
            focus: Focus::Table,
            details: Details::default(),
            histogram: HistogramState::default(),
            shows_thread: false,
            shows_line_numbers: true,
            wraps: false,
            timestamps: Timestamps::of(&defaults.ui),
            colors: defaults.colors,
            panel: None,
            notice: None,
            path: path.to_owned(),
            current_dir: std::env::current_dir().unwrap_or_default(),
            hit_map: HitMap::default(),
            last_click: None,
            clipboard: None,
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
        // Before the index changes, which starts the engines over from the first entry
        // it changes and drops what they found past it.
        if let Some(filter) = &mut self.filter {
            filter.take_in(&self.folds);
        }
        if let Some(matches) = &mut self.matches {
            matches.poll();
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
        if let Some(backward) = self.pending_step.take() {
            self.step(backward);
        }
        if self.progress.is_complete()
            && !self.filtering()
            && let Some(export) = self.pending_export.take()
        {
            self.finish_export(&export);
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

    /// Whether row `i` shows a line of a bookmarked entry; `false` for the row that
    /// marks a truncation. Panics if `i` is not below `viewport().total()`.
    ///
    /// A bookmark is the byte offset of the entry it was set on, so it lasts through
    /// filtering and preset cycling: the entry bookmarked is the one whose bytes hold
    /// the offset. A truncation or another file at the path
    /// ([`file_changed`](Self::file_changed)) removes every bookmark.
    pub fn bookmarked(&self, i: usize) -> bool {
        let Some(row) = self.fold_row(i) else {
            return false;
        };
        let (entry, _) = self.shown_folds().line_at(row);
        self.is_bookmarked(&self.shown_record(entry))
    }

    fn is_bookmarked(&self, record: &Record) -> bool {
        self.bookmarks.range(record.bytes()).next().is_some()
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

    /// Tells the viewer the time it is, for its [`liveness`](Self::liveness). The
    /// first tick is when it opened the file; every change to the file it takes in
    /// afterwards ([`file_changed`](Self::file_changed), also through
    /// [`poll_index`](Self::poll_index)) happened at the latest tick.
    pub fn tick(&mut self, now: Instant) {
        let clock = self.ticks.get_or_insert(Clock { now, changed: now });
        clock.now = now;
    }

    /// How recently the file changed, as of the latest [`tick`](Self::tick):
    /// [`Liveness::since`] its last change, or since it opened if it hasn't changed
    /// since. A gzip file, and a file the viewer [waits for](Self::waiting_for_file),
    /// are [`Idle`](Liveness::Idle). Before the first tick the file is
    /// [`Live`](Liveness::Live). Growth, a truncation and another file at the path
    /// count as changes, whether or not the viewer follows the file.
    pub fn liveness(&self) -> Liveness {
        if !self.can_follow() || self.waiting {
            return Liveness::Idle;
        }
        self.ticks.map_or(Liveness::Live, |clock| {
            Liveness::since(clock.since_change())
        })
    }

    /// How long after the latest [`tick`](Self::tick) the dot in the header changes if
    /// nothing else happens ([`Liveness::changes_in`]); `None` while the file is
    /// [`Idle`](Liveness::Idle), as the dot can't change any more.
    pub(crate) fn dot_changes_in(&self) -> Option<Duration> {
        if self.liveness() == Liveness::Idle {
            return None;
        }
        self.ticks
            .and_then(|clock| Liveness::changes_in(clock.since_change()))
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
        if let Some(clock) = &mut self.ticks {
            clock.changed = clock.now;
        }
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
    /// report changes to the file, or the filters' results are still coming in, or the histogram shown is still counting them.
    pub(crate) fn expects_updates(&self) -> bool {
        !self.progress.is_complete()
            || self.watch.is_some()
            || self.filtering()
            || self.searching()
            || self.histogram_behind()
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

    /// Takes the `[ui]` section and the colours of `config`: whether the thread
    /// column, the histogram ([`shows_histogram`](Self::shows_histogram)) and the
    /// line numbers are shown and messages wrap, the timestamp format of the table
    /// and the styles of the levels and the line numbers. The toggles still change
    /// them afterwards. A viewer that is never configured shows the line numbers
    /// only, with the timestamps and colours of [`Config::default`].
    pub fn configure(&mut self, config: &Config) {
        let Config { ui, colors, .. } = config;
        self.shows_thread = ui.show_thread;
        self.shows_histogram = ui.show_histogram;
        self.shows_line_numbers = ui.show_line_numbers;
        self.wraps = ui.wrap;
        self.timestamps = Timestamps::of(ui);
        self.colors = *colors;
    }

    /// The timestamp format, and the width of the column it makes, from 100 columns.
    pub fn timestamps(&self) -> &Timestamps {
        &self.timestamps
    }

    /// The styles of the levels and the line numbers.
    pub fn colors(&self) -> &Colors {
        &self.colors
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
        self.cancel_export();
        self.filters = filters;
        self.histogram.reset();
        // The index shows as it is, which is cheaper than an engine passing every entry.
        if hides_nothing(&self.filters) {
            self.filter = None;
        } else {
            self.start_filter();
        }
        if let Some(matches) = &mut self.matches {
            matches.set_filters(self.filters.clone());
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

    /// Reads a relative export path from `dir` from here on. It is the process's
    /// current directory at first.
    pub fn set_current_dir(&mut self, dir: &Path) {
        dir.clone_into(&mut self.current_dir);
    }

    /// Reads the wall clock from `now` from here on, for a time filter on
    /// [`Base::Now`](crate::filter::Base::Now). It is the system clock at first.
    pub fn set_clock(&mut self, now: fn() -> Timestamp) {
        self.clock = now;
    }

    /// Puts what [`Action::Copy`] copies on `clipboard` from here on. There is none
    /// at first.
    pub fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = Some(clipboard);
    }

    fn copy_entry(&mut self) {
        let Some(entry) = self.details_entry() else {
            self.notice = Some(Cow::Borrowed("nothing to copy"));
            return;
        };
        let text = entry_text(entry);
        let Some(clipboard) = self.clipboard.as_mut() else {
            self.notice = Some(Cow::Borrowed("no clipboard"));
            return;
        };
        self.notice = Some(match clipboard.copy(&text) {
            Ok(_) => Cow::Borrowed("copied to clipboard"),
            Err(err @ CopyError::TooLarge) => Cow::Owned(err.to_string()),
            Err(err) => Cow::Owned(format!("copy failed: {err}")),
        });
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
    ///
    /// [`Action::LinePrompt`] and [`Action::TimePrompt`] open an empty jump prompt
    /// for a line or a time. [`App::key`] edits it.
    ///
    /// [`Action::SearchForward`] and [`Action::SearchBackward`] open the search
    /// prompt, empty and with the options of the last search. [`App::key`] edits it
    /// like the text filter prompt.
    pub fn panel(&self) -> Option<&Panel> {
        self.panel.as_ref()
    }

    /// The search set in the search prompt, if any; none at first.
    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    /// `(position, total)` of the search's matches, `None` without a search: `total`
    /// is how many entries the filters show match the search, `position` how many of
    /// them are at or before the selected row's entry.
    pub fn match_count(&self) -> Option<(usize, usize)> {
        let entries = &self.matches.as_ref()?.filtered().entries;
        let position = self.selected_entry().map_or(0, |selected| {
            entries.partition_point(|&entry| entry as usize <= selected)
        });
        Some((position, entries.len()))
    }

    /// Whether the search's matches aren't all counted yet: entries the filters show
    /// are still to be searched.
    pub fn searching(&self) -> bool {
        self.matches
            .as_ref()
            .is_some_and(|matches| matches.filtered().evaluated < read(&self.index).records.len())
    }

    /// Whether a line that folding hides from row `i`'s entry matches the search.
    pub fn hides_match(&self, i: usize) -> bool {
        let (Some(search), Some(shown)) = (&self.search, self.row(i)) else {
            return false;
        };
        // The engine already knows which entries have a match; most folded ones don't.
        let counted = self
            .index_entry(i)
            .is_some_and(|entry| self.is_match(entry));
        if shown.hidden == 0 || !counted {
            return false;
        }
        let entry = &self.source.bytes()[file_range(&shown.record)];
        source::lines(entry)
            .skip(1)
            .any(|line| search.matcher.is_match(&strip_ansi(line)))
    }

    /// Whether the details pane is shown: the selected row's entry in full, below the
    /// rows. [`Action::Confirm`] with no panel open shows it and hides it again.
    pub fn shows_details(&self) -> bool {
        self.shows_details
    }

    /// The pane that takes the moves. [`Action::CycleFocus`] moves it down the
    /// screen to the next pane that is shown, from the last one back to the table:
    /// table, histogram, details pane. The histogram only takes it while it is shown
    /// and there is a histogram to show ([`histogram`](Self::histogram)). Hiding the
    /// pane that has the focus gives it back to the table.
    ///
    /// With the focus on the details pane, `Up` and `Down` scroll it by a line and
    /// `PageUp` and `PageDown` by its height (see [`fit_details`](Self::fit_details)),
    /// stopping at its first and last lines; they neither move the selection nor stop
    /// following.
    ///
    /// With the focus on the histogram, [`Action::ScrollLeft`] and
    /// [`Action::ScrollRight`] select the bucket before and after the selected one
    /// ([`histogram_bucket`](Self::histogram_bucket)) instead of scrolling the
    /// messages, and [`Action::Confirm`] sets the time filter to what the selected
    /// bucket covers instead of showing the details pane (see
    /// [`histogram_bucket`](Self::histogram_bucket)).
    ///
    /// Every other action works on the table as usual.
    pub fn focus(&self) -> Focus {
        self.focus
    }

    /// How many lines the details pane is scrolled down. It starts at 0 for each
    /// entry selected: moving the selection to another entry scrolls it back to the
    /// top, while moving between the lines of the same entry, or the entry being read
    /// again after an append, keeps it.
    pub fn details_scroll(&self) -> usize {
        self.details.scroll
    }

    /// Sets how many screen lines the details pane shows, `height`, and how many the
    /// selected entry takes in it, `lines`. Scrolls the pane back so that it goes no
    /// further than its last line at the bottom.
    pub fn fit_details(&mut self, height: usize, lines: usize) {
        self.track_details_entry();
        self.details.height = height;
        self.details.lines = lines;
        self.details.scroll = self.details.scroll.min(self.details.max_scroll());
    }

    /// Sets how many lines the help overlay shows at a time, `height`. Scrolls it back
    /// so that it goes no further than its last line at the bottom.
    ///
    /// While the overlay is open, `j ↓` and `k ↑` scroll it by a line, `PgDn` and
    /// `PgUp` by `height` lines, never above its first line nor further than its last
    /// line at the bottom; an overlay that shows every line doesn't scroll. `Esc`,
    /// `F1` and `q` close it, `Ctrl-c` quits, and every other key does nothing. It
    /// opens at its first line.
    pub fn fit_help(&mut self, height: usize) {
        if let Some(Panel::Help(panel)) = &mut self.panel {
            panel.height = height;
            panel.lines = keymap::help().len();
            panel.scroll = panel.scroll.min(panel.max_scroll());
        }
    }

    /// The selected row's entry, every line of it as in the file whether folded or
    /// not, line endings included. `None` with no row selected or with the row that
    /// marks a truncation selected.
    pub fn details_entry(&self) -> Option<&[u8]> {
        let record = self.selected_record()?;
        Some(&self.source.bytes()[file_range(&record)])
    }

    fn selected_record(&self) -> Option<Record> {
        let row = self.fold_row(self.viewport.selected()?)?;
        let (entry, _) = self.shown_folds().line_at(row);
        Some(self.shown_record(entry))
    }

    /// Scrolls the details back to the top when the selection moved to another entry.
    fn track_details_entry(&mut self) {
        let start = self
            .selected_record()
            .map(|record| file_range(&record).start);
        if start != self.details.entry {
            self.details = Details {
                entry: start,
                scroll: 0,
                ..self.details
            };
        }
    }

    /// Takes the entries the rows come from that the counter hasn't seen into it, for
    /// at most [`CATCH_UP`] so that a frame never waits on a big file.
    fn feed_histogram(&mut self) {
        let deadline = Instant::now() + CATCH_UP;
        let index = read(&self.index);
        let filter = self.filter.as_ref();
        let available = histogram_sources(filter, &index.records);
        let state = &mut self.histogram;
        let before = state.fed;
        while state.fed < available && Instant::now() < deadline {
            let end = (state.fed + FILTER_CHUNK).min(available);
            for i in state.fed..end {
                let record = counted_record(filter, &index.records, i);
                if let Some(at) = record.and_then(Record::timestamp) {
                    state.counter.add(at, record.and_then(Record::level));
                }
            }
            state.fed = end;
        }
        if state.fed > before {
            state.version += 1;
        }
    }

    /// Whether the histogram is shown and has entries left to count.
    fn histogram_behind(&self) -> bool {
        let available = histogram_sources(self.filter.as_ref(), &read(&self.index).records);
        self.shows_histogram && self.histogram.fed < available
    }

    fn toggle_histogram(&mut self) {
        self.shows_histogram = !self.shows_histogram;
        if self.focus == Focus::Histogram {
            self.focus = Focus::Table;
        }
        if self.shows_histogram && !self.has_histogram() && !self.histogram_behind() {
            self.notice = Some(Cow::Borrowed("no entry has a timestamp"));
        }
    }

    fn has_histogram(&mut self) -> bool {
        self.feed_histogram();
        !self.histogram.counter.is_empty()
    }

    fn cycle_focus(&mut self) {
        let histogram = self.shows_histogram && self.has_histogram();
        let panes = [
            (Focus::Table, true),
            (Focus::Histogram, histogram),
            (Focus::Details, self.shows_details),
        ];
        let shown = panes
            .iter()
            .filter(|(_, shown)| *shown)
            .map(|(pane, _)| *pane);
        let mut cycle = shown
            .clone()
            .chain(shown)
            .skip_while(|&pane| pane != self.focus);
        self.focus = cycle.nth(1).unwrap_or(Focus::Table);
    }

    fn act_on_histogram(&mut self, action: Action) {
        let buckets = self.histogram_buckets();
        let bucket = self.histogram.bucket;
        match action {
            Action::ScrollLeft => self.histogram.bucket = bucket.saturating_sub(1),
            Action::ScrollRight => {
                self.histogram.bucket = (bucket + 1).min(buckets.saturating_sub(1));
            }
            _ => self.filter_to_bucket(bucket),
        }
    }

    fn histogram_buckets(&self) -> usize {
        let cached = self.histogram.cached.as_ref();
        cached
            .and_then(|c| c.histogram.as_ref())
            .map_or(0, |h| h.counts.len())
    }

    /// Sets the time filter to what bucket `bucket` of the last histogram covers and
    /// selects the first bucket again.
    fn filter_to_bucket(&mut self, bucket: usize) {
        let cached = self.histogram.cached.as_ref();
        let Some(histogram) = cached.and_then(|c| c.histogram.as_ref()) else {
            return;
        };
        let range = histogram.range(bucket);
        let end = range.end.unwrap_or(Timestamp::MAX);
        let spec = TimeSpec::covering(range.start, end, &self.settings.zone);
        self.histogram.bucket = 0;
        self.set_time_filter(Some(TimeFilter {
            spec,
            base: Base::File,
        }));
    }

    fn toggle_details(&mut self) {
        self.shows_details = !self.shows_details;
        self.focus = Focus::Table;
        self.details = Details::default();
    }

    fn scroll_details(&mut self, action: Action) {
        let Details { height, scroll, .. } = self.details;
        let scroll = match action {
            Action::Up => scroll.saturating_sub(1),
            Action::Down => scroll + 1,
            Action::PageUp => scroll.saturating_sub(height),
            Action::PageDown => scroll + height,
            _ => scroll,
        };
        self.details.scroll = scroll.min(self.details.max_scroll());
    }

    /// Whether the histogram row is shown, under the header and the filter bar.
    /// [`Action::ToggleHistogram`] shows and hides it; it is hidden at first. Showing
    /// it while there is no histogram ([`histogram`](Self::histogram)) sets the
    /// [notice](Self::notice) `no entry has a timestamp`, and the row stays hidden until
    /// there is one.
    pub fn shows_histogram(&self) -> bool {
        self.shows_histogram
    }

    /// The histogram ([`crate::histogram`]) of the index's entries that pass the
    /// filters, in at most `width` buckets aligned on the clock of the app's zone;
    /// `None` while none of them has a timestamp.
    ///
    /// The entries are counted as they come in, the index's or the filters' results
    /// at each [`poll_index`](Self::poll_index), and counted again from the first only
    /// when those results start over. The buckets are worked out again only when what
    /// is counted or `width` changed since the last call, so that calling this for
    /// every frame costs next to nothing. The selected bucket, and the time filter it
    /// sets, are those of the histogram this last returned.
    pub fn histogram(&mut self, width: usize) -> Option<&Histogram> {
        self.feed_histogram();
        let state = &mut self.histogram;
        let stale = state
            .cached
            .as_ref()
            .is_none_or(|cached| cached.width != width || cached.version != state.version);
        if stale {
            let histogram = state.counter.histogram(width, &self.settings.zone);
            let last = histogram.as_ref().map_or(0, |h| h.counts.len() - 1);
            state.bucket = state.bucket.min(last);
            state.cached = Some(CachedHistogram {
                width,
                version: state.version,
                histogram,
            });
        }
        let found = state.cached.as_ref().is_some_and(|c| c.histogram.is_some());
        if !found && self.focus == Focus::Histogram {
            self.focus = Focus::Table;
        }
        self.histogram.cached.as_ref()?.histogram.as_ref()
    }

    /// The selected bucket of the histogram, 0 at first. With the focus on the
    /// histogram, [`Action::ScrollLeft`] and [`Action::ScrollRight`] move it by one,
    /// stopping at the first and the last bucket, and [`Action::Confirm`] sets the time
    /// filter ([`set_time_filter`](Self::set_time_filter)) to the range the bucket
    /// covers ([`Histogram::range`], [`TimeSpec::covering`]) and selects the first
    /// bucket again. Before the first histogram they do nothing.
    pub fn histogram_bucket(&self) -> usize {
        self.histogram.bucket
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
        self.dispatch(action);
        if self.shows_details {
            self.track_details_entry();
        }
    }

    fn dispatch(&mut self, action: Action) {
        self.pending_step = None;
        if let Some(panel) = self.panel.take() {
            self.apply_in_panel(panel, action);
            return;
        }
        let moves = matches!(
            action,
            Action::Up | Action::Down | Action::PageUp | Action::PageDown
        );
        if moves && self.focus == Focus::Details {
            self.scroll_details(action);
            return;
        }
        let histogram_action = matches!(
            action,
            Action::ScrollLeft | Action::ScrollRight | Action::Confirm
        );
        if histogram_action && self.focus == Focus::Histogram {
            self.act_on_histogram(action);
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
            Action::CycleFocus => self.cycle_focus(),
            Action::ToggleHistogram => self.toggle_histogram(),
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
            Action::LinePrompt => self.panel = Some(Panel::Jump(JumpPrompt::new(Jump::Line))),
            Action::TimePrompt => self.panel = Some(Panel::Jump(JumpPrompt::new(Jump::Time))),
            Action::SearchForward => self.open_search(false),
            Action::SearchBackward => self.open_search(true),
            Action::NextMatch => self.step_search(false),
            Action::PreviousMatch => self.step_search(true),
            Action::NextException => self.jump_exception(false),
            Action::PreviousException => self.jump_exception(true),
            Action::ToggleBookmark => self.toggle_bookmark(),
            Action::NextBookmark => self.jump_bookmark(false),
            Action::PreviousBookmark => self.jump_bookmark(true),
            Action::Export => self.panel = Some(Panel::Export(self.export_prompt())),
            Action::Copy => self.copy_entry(),
            Action::Confirm => self.toggle_details(),
            Action::Cancel => self.cancel_export(),
            Action::Help => {
                self.panel = Some(Panel::Help(HelpPanel {
                    scroll: 0,
                    height: 0,
                    lines: 0,
                }))
            }
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
    /// The search prompt is edited the same way. `Enter` sets the [search](Self::search)
    /// to the pattern, or clears it for an empty pattern, and selects the first
    /// matching entry after the selected one (before it, searching backward), as
    /// [`Action::NextMatch`] does; `Esc` keeps the last search. The matches are all
    /// the lines of an entry, folded away or not, with ANSI escape codes stripped.
    /// [`Action::NextMatch`] and [`Action::PreviousMatch`] stop following and select the
    /// next and previous matching entry among those the filters show, wrapping around
    /// with the [notice](Self::notice) `search wrapped`, or say `pattern not found`.
    /// Matches are counted in the background; a move that needs matches not counted
    /// yet is made at the [`poll_index`](Self::poll_index) that has them.
    ///
    /// A jump prompt is edited like the time panel, without `Tab` and the presets.
    /// `Enter` closes it and jumps, or only closes it while its text is blank; text
    /// that says nothing to jump to keeps it open with the [error](JumpPrompt::error).
    /// Either jump stops following and selects the first line of the entry it lands
    /// on, folding nothing and unfolding nothing; in a file without rows it selects
    /// nothing.
    ///
    /// - [`Jump::Line`]: a whole number `n`, the gutter's line numbers, lands on the
    ///   entry holding file line `n`, or on the line itself while that entry is
    ///   unfolded. If the filters hide that entry, it lands on the next entry they
    ///   show; past the last one, on the last one. `0` lands on the first row, a
    ///   number past the end on the last. Anything else is the error `'<text>' is not
    ///   a line number`.
    /// - [`Jump::Time`]: a [`TimeSpec`] lands on the first entry the filters show,
    ///   in file order, whose timestamp is at or after the spec's
    ///   [start](TimeSpec::start), dated by the last entry with a timestamp. Past the
    ///   last such entry it lands on the last row. With no entry with a timestamp it
    ///   stays where it is, and the [notice](Self::notice) says `no entry has a
    ///   timestamp`.
    ///
    /// The export prompt is edited like a jump prompt. `Enter` exports, in turn:
    /// - a path naming the viewed file, by its inode or its canonical path, keeps the
    ///   prompt open with the error `cannot export to the file being viewed`;
    /// - a path naming an existing regular file keeps the prompt open,
    ///   [asking](ExportPrompt::overwrite) to overwrite it: `y` goes on, any other
    ///   key goes back to editing the path (`Ctrl-c` still quits); a directory goes
    ///   straight on and fails as below;
    /// - the prompt closes. While the index or the filters are still at work, the
    ///   notice says `export waits for the filters` and [`poll_index`](Self::poll_index)
    ///   finishes the export once they are done; `Esc` with no panel open, or any
    ///   change to the filters, drops a waiting export with the notice `export
    ///   cancelled`;
    /// - if the filters show no entry, the notice says `nothing to export` and no file
    ///   is written;
    /// - otherwise it writes every entry the filters show, in file order, with all its
    ///   lines, folded or not, as [`export::write_entries`](crate::export::write_entries)
    ///   does, and the notice says `exported N entries to <path as typed>` (`1 entry`
    ///   for one). If the file can't be written, the notice says `export failed: ` and
    ///   the error.
    ///
    /// Otherwise it [applies](Self::apply) the [`action`] bound to the key, if any.
    pub fn key(&mut self, key: KeyEvent) {
        self.notice = None;
        self.last_click = None;
        let action = match &mut self.panel {
            Some(Panel::Classes(panel)) => panel.key(key),
            Some(Panel::Time(panel)) => panel.key(key),
            Some(Panel::Text(prompt)) => prompt.key(key),
            Some(Panel::Jump(prompt)) => prompt.key(key),
            Some(Panel::Search { prompt, .. }) => prompt.key(key),
            Some(Panel::Export(prompt)) => prompt.key(key),
            Some(Panel::Help(_)) => help_action(key),
            _ => action(key),
        };
        if let Some(action) = action {
            self.apply(action);
        }
    }

    /// Where the last screen drew what the mouse can act on, set by
    /// [`ui::render`](crate::ui::render); empty before the first screen.
    pub fn hit_map(&self) -> &HitMap {
        &self.hit_map
    }

    /// Keeps `map` as the layout of the last screen drawn.
    pub fn set_hit_map(&mut self, map: HitMap) {
        self.hit_map = map;
    }

    /// Acts on a mouse event that came at `at`, by what the [last screen
    /// drawn](Self::hit_map) shows under it. Nothing happens while a panel is open,
    /// and only the wheel and the left button do anything:
    ///
    /// - the wheel over the details pane scrolls it by 3 lines, like 3 times
    ///   [`Action::Up`] or [`Action::Down`] with the focus there; anywhere else it
    ///   scrolls the rows by 3: the screen and the selection move 3 rows together,
    ///   like a page of 3 rows ([`Action::PageUp`], [`Action::PageDown`]), and
    ///   scrolling up stops following the file;
    /// - a click on a row selects it, like [`Viewport::goto`], and stops following the
    ///   file; a row gone since the screen was drawn is left alone. A second click on
    ///   the same row (on any of its screen lines) at most 400 ms after the first, with
    ///   no key in between, is a double click, which also
    ///   [toggles its fold](Action::ToggleFold); the click after a double click starts
    ///   over;
    /// - a click on a bucket of the histogram sets the time filter to it, as
    ///   [`Action::Confirm`] does with the focus on the histogram
    ///   ([`histogram_bucket`](Self::histogram_bucket));
    /// - a click anywhere else does nothing.
    pub fn mouse(&mut self, event: MouseEvent, at: Instant) {
        if self.panel.is_some() {
            return;
        }
        self.pending_step = None;
        let target = self.hit_map.target(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollUp => self.wheel(target, true),
            MouseEventKind::ScrollDown => self.wheel(target, false),
            MouseEventKind::Down(MouseButton::Left) => self.click(target, at),
            _ => {}
        }
        if self.shows_details {
            self.track_details_entry();
        }
    }

    fn wheel(&mut self, target: Option<Target>, up: bool) {
        if target == Some(Target::Details) {
            let action = if up { Action::Up } else { Action::Down };
            for _ in 0..WHEEL_STEP {
                self.scroll_details(action);
            }
        } else if up {
            self.following = false;
            self.viewport.scroll_up(WHEEL_STEP);
        } else {
            self.viewport.scroll_down(WHEEL_STEP);
        }
    }

    fn click(&mut self, target: Option<Target>, at: Instant) {
        match target {
            Some(Target::Row(row)) if row < self.viewport.total() => {
                let double = self.last_click.is_some_and(|(last, time)| {
                    last == row && at.saturating_duration_since(time) <= DOUBLE_CLICK
                });
                self.following = false;
                self.viewport.goto(row);
                if double {
                    self.toggle_fold();
                    self.last_click = None;
                } else {
                    self.last_click = Some((row, at));
                }
            }
            Some(Target::Bucket(bucket)) => self.filter_to_bucket(bucket),
            _ => {}
        }
    }

    /// What the viewer has to tell about the last key, until the next one: `the
    /// selected entry has no class` after [`Action::OnlyClass`] or
    /// [`Action::HideClass`] on an entry without a class (or with no row selected),
    /// which then change nothing.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
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
            self.histogram.reset();
        }
        self.start_search();
        self.read_tail();
    }

    /// Opens the search prompt, empty and reading its pattern as the last search did.
    fn open_search(&mut self, backward: bool) {
        let options = self
            .search
            .as_ref()
            .map_or_else(matcher::Options::default, |search| search.matcher.options());
        let prompt = Prompt {
            options,
            ..Prompt::new(None)
        };
        self.panel = Some(Panel::Search { prompt, backward });
    }

    /// Makes `search` the search and selects its first match from the selected entry,
    /// or drops the search for `None`.
    fn set_search(&mut self, search: Option<Search>) {
        self.search = search;
        self.start_search();
        self.step_search(false);
    }

    /// Starts counting the matches of the search, in place of any engine at work.
    fn start_search(&mut self) {
        self.matches = self.search.as_ref().map(|search| {
            let options = filter::Options {
                chunk: FILTER_CHUNK,
            };
            Engine::spawn_search(
                Arc::clone(&self.index),
                self.settings.preset,
                self.filters.clone(),
                search.matcher.clone(),
                options,
            )
        });
    }

    /// [`step`](Self::step) in the direction of the search, or against it if `reverse`.
    fn step_search(&mut self, reverse: bool) {
        if let Some(search) = &self.search {
            self.step(search.backward != reverse);
        }
    }

    /// Selects the next entry that matches the search, up the rows if `backward`. When
    /// the matches that tell aren't counted yet, it is done as soon as they are.
    fn step(&mut self, backward: bool) {
        let Some(matches) = &self.matches else {
            return;
        };
        let counted = self.progress.is_complete() && !self.searching();
        let next = find_match(matches.filtered(), self.selected_entry(), backward, counted);
        match next {
            Next::Wait => self.pending_step = Some(backward),
            Next::Nothing => self.notice = Some(Cow::Borrowed("pattern not found")),
            Next::Match { entry, wrapped } => match self.entry_row(entry) {
                // The filters' results haven't caught up with the entry yet.
                None => self.pending_step = Some(backward),
                Some(row) => {
                    self.following = false;
                    self.viewport.goto(row);
                    self.notice = wrapped.then_some(Cow::Borrowed("search wrapped"));
                }
            },
        }
    }

    fn jump_exception(&mut self, backward: bool) {
        let found = self.find_shown(backward, Record::has_exception);
        self.land(found, "exceptions wrapped", "no exceptions");
    }

    fn jump_bookmark(&mut self, backward: bool) {
        if self.bookmarks.is_empty() {
            self.notice = Some(Cow::Borrowed("no bookmarks"));
            return;
        }
        let found = self.find_shown(backward, |record| self.is_bookmarked(record));
        self.land(found, "bookmarks wrapped", "no bookmarks");
    }

    fn toggle_bookmark(&mut self) {
        let Some(row) = self.viewport.selected().and_then(|row| self.fold_row(row)) else {
            return;
        };
        let (entry, _) = self.shown_folds().line_at(row);
        let bytes = self.shown_record(entry).bytes();
        let inside: Vec<u64> = self.bookmarks.range(bytes.clone()).copied().collect();
        if inside.is_empty() {
            self.bookmarks.insert(bytes.start);
            self.notice = Some(Cow::Borrowed("bookmarked"));
        } else {
            for offset in inside {
                self.bookmarks.remove(&offset);
            }
            self.notice = Some(Cow::Borrowed("bookmark removed"));
        }
    }

    /// Moves to the row of `found`, an entry and whether the walk wrapped to reach
    /// it, or says `none`.
    fn land(&mut self, found: Option<(usize, bool)>, wrapped: &'static str, none: &'static str) {
        let Some((entry, did_wrap)) = found else {
            self.notice = Some(Cow::Borrowed(none));
            return;
        };
        let row = self.lead_rows() + self.shown_folds().row_of(entry, 0);
        self.following = false;
        self.viewport.goto(row);
        self.notice = did_wrap.then_some(Cow::Borrowed(wrapped));
    }

    /// The next shown entry whose record satisfies `matches`, up the rows if
    /// `backward`, and whether the walk wrapped around to reach it. It walks the shown
    /// entries rather than the index so that it also covers the file's last entries
    /// shown before the index is complete.
    fn find_shown(
        &self,
        backward: bool,
        matches: impl Fn(&Record) -> bool,
    ) -> Option<(usize, bool)> {
        let folds = self.shown_folds();
        let count = folds.entries();
        let current = self
            .viewport
            .selected()
            .and_then(|row| self.fold_row(row))
            .map(|row| folds.line_at(row).0);
        let split = if backward {
            current.unwrap_or(count)
        } else {
            current.map_or(0, |entry| entry + 1)
        };
        // One guard for the whole scan: a lock per entry would queue behind the indexer.
        let index = read(&self.index);
        let hit = |entry: &usize| {
            let record = match (&self.opening, &self.filter) {
                (Opening::Tail(records, _), _) => &records[*entry],
                (_, Some(filter)) => &index.records[filter.entry(*entry)],
                (_, None) => &index.records[*entry],
            };
            matches(record)
        };
        let find = |mut entries: Range<usize>| {
            if backward {
                entries.rfind(hit)
            } else {
                entries.find(hit)
            }
        };
        let (first, second) = if backward {
            (0..split, split..count)
        } else {
            (split..count, 0..split)
        };
        find(first)
            .map(|entry| (entry, false))
            .or_else(|| find(second).map(|entry| (entry, true)))
    }

    /// The index entry that viewport row `row` shows, `None` for the truncation row and
    /// while the rows are the file's last entries, which the index doesn't number.
    fn index_entry(&self, row: usize) -> Option<usize> {
        let (shown, _) = self.shown_folds().line_at(self.fold_row(row)?);
        match (&self.opening, &self.filter) {
            (Opening::Tail(..), _) => None,
            (_, Some(filter)) => Some(filter.entry(shown)),
            (_, None) => Some(shown),
        }
    }

    fn selected_entry(&self) -> Option<usize> {
        self.index_entry(self.viewport.selected()?)
    }

    /// The viewport row of the first line of index entry `entry`, `None` while the
    /// rows don't show it.
    fn entry_row(&self, entry: usize) -> Option<usize> {
        let shown = match (&self.opening, &self.filter) {
            (Opening::Tail(..), _) => return None,
            (_, Some(filter)) => filter
                .engine
                .filtered()
                .entries
                .binary_search(&(entry as u32))
                .ok()?,
            (_, None) => entry,
        };
        let folds = self.shown_folds();
        (shown < folds.entries()).then(|| self.lead_rows() + folds.row_of(shown, 0))
    }

    /// Whether the search's matches counted so far include index entry `entry`.
    fn is_match(&self, entry: usize) -> bool {
        self.matches.as_ref().is_some_and(|matches| {
            let entries = &matches.filtered().entries;
            entries.binary_search(&(entry as u32)).is_ok()
        })
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
        let index = read(&self.index);
        if let Some(matches) = &mut self.matches {
            Rerun::after(&slide, matches, &index.records, from).apply(matches, &self.filters);
        }
        let Some(filter) = &mut self.filter else {
            self.histogram.invalidate_from(from);
            return;
        };
        let rerun = Rerun::after(&slide, &filter.engine, &index.records, from);
        drop(index);
        filter.rerun(&rerun, &self.filters);
        self.histogram
            .invalidate_from(filter.engine.filtered().entries.len());
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
        self.histogram
            .uncount(self.filter.as_ref(), &index.records, appended_from(known));
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
        self.bookmarks.clear();
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
                Panel::Jump(mut prompt) => {
                    if let Err(error) = self.jump(prompt.to, &prompt.text) {
                        prompt.error = Some(error);
                        self.panel = Some(Panel::Jump(prompt));
                    }
                }
                Panel::Export(mut prompt) => {
                    if !self.export(&mut prompt) {
                        self.panel = Some(Panel::Export(prompt));
                    }
                }
                Panel::Search {
                    mut prompt,
                    backward,
                } => match prompt.matcher() {
                    Ok(matcher) => {
                        self.set_search(matcher.map(|matcher| Search { matcher, backward }))
                    }
                    Err(error) => {
                        prompt.error = Some(error);
                        self.panel = Some(Panel::Search { prompt, backward });
                    }
                },
                Panel::Help(panel) => self.panel = Some(Panel::Help(panel)),
            },
            Action::Cancel => {}
            Action::Help if matches!(panel, Panel::Help(_)) => {}
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

    fn cancel_export(&mut self) {
        if self.pending_export.take().is_some() {
            self.notice = Some(Cow::Borrowed("export cancelled"));
        }
    }

    fn export_prompt(&self) -> ExportPrompt {
        let name = self.name.strip_suffix(".gz").unwrap_or(&self.name);
        ExportPrompt {
            text: format!("{name}.filtered.log"),
            error: None,
            overwrite: false,
        }
    }

    /// Exports as `prompt` asks and returns whether it is done with the prompt. If
    /// not, `prompt` says why, or asks to overwrite the file. While the index or the
    /// filters are still at work, the export waits for them in `pending_export`.
    fn export(&mut self, prompt: &mut ExportPrompt) -> bool {
        let target = self.current_dir.join(&prompt.text);
        if self.is_viewed_file(&target) {
            prompt.error = Some("cannot export to the file being viewed".to_owned());
            return false;
        }
        if !prompt.overwrite && target.is_file() {
            prompt.overwrite = true;
            return false;
        }
        let export = PendingExport {
            target,
            typed: mem::take(&mut prompt.text),
        };
        if self.progress.is_complete() && !self.filtering() {
            self.finish_export(&export);
        } else {
            self.notice = Some(Cow::Borrowed("export waits for the filters"));
            self.pending_export = Some(export);
        }
        true
    }

    /// Whether `target` is the file being viewed, however it is named or linked to.
    /// Creating it would truncate the mapped file.
    fn is_viewed_file(&self, target: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let (Ok(viewed), Ok(target)) = (fs::metadata(&self.path), fs::metadata(target)) {
                return (viewed.dev(), viewed.ino()) == (target.dev(), target.ino());
            }
        }
        let viewed = fs::canonicalize(&self.path).ok();
        viewed.is_some() && fs::canonicalize(target).ok() == viewed
    }

    /// Writes the export once the index and the filters' results are complete.
    fn finish_export(&mut self, export: &PendingExport) {
        let count = self.shown_ranges(&read(&self.index)).count();
        let notice = if count == 0 {
            "nothing to export".to_owned()
        } else {
            match self.write_export(&export.target) {
                Ok(()) => {
                    let noun = if count == 1 { "entry" } else { "entries" };
                    format!("exported {count} {noun} to {}", export.typed)
                }
                Err(error) => format!("export failed: {error}"),
            }
        };
        self.notice = Some(Cow::Owned(notice));
    }

    /// The byte ranges of the entries the filters show, in file order, from `index`.
    fn shown_ranges<'a>(
        &'a self,
        index: &'a Indexed<Source>,
    ) -> impl Iterator<Item = Range<usize>> + 'a {
        let all = self.filter.is_none().then(|| index.records.iter());
        let matched = self.filter.iter().flat_map(|filter| {
            filter
                .engine
                .filtered()
                .entries
                .iter()
                .map(|&entry| &index.records[entry as usize])
        });
        all.into_iter().flatten().chain(matched).map(file_range)
    }

    /// Streams the entries the filters show from the file to `target`.
    fn write_export(&self, target: &Path) -> io::Result<()> {
        let mut out = io::BufWriter::new(File::create(target)?);
        let index = read(&self.index);
        export::write_entries(&mut out, self.source.bytes(), self.shown_ranges(&index))?;
        out.flush()
    }

    /// Jumps as a prompt for `to` asks with `text` typed into it. A blank text jumps
    /// nowhere; an error says why `text` names nothing to jump to.
    fn jump(&mut self, to: Jump, text: &str) -> Result<(), String> {
        if text.trim().is_empty() {
            return Ok(());
        }
        let row = match to {
            Jump::Line => self.line_row(text)?,
            Jump::Time => self.time_row(text)?,
        };
        if let Some(row) = row {
            self.following = false;
            self.viewport.goto(row);
        }
        Ok(())
    }

    /// The row of file line `text`, `None` to stay where the viewer is.
    fn line_row(&mut self, text: &str) -> Result<Option<usize>, String> {
        let text = text.trim();
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("'{text}' is not a line number"));
        }
        // Too many digits for a `u64` is a line past the end of any file.
        let line: u64 = text.parse().unwrap_or(u64::MAX);
        if matches!(self.opening, Opening::Tail(..)) {
            self.notice = Some(Cow::Borrowed(
                "line numbers are not known until the file is indexed",
            ));
            return Ok(None);
        }
        let folds = self.shown_folds();
        let entries = folds.entries();
        if entries == 0 {
            return Ok(None);
        }
        if line == 0 {
            return Ok(Some(0));
        }
        let first = |entry: usize| self.shown_record(entry).line().unwrap_or(0);
        // The first entry that has the line or comes after it; the last one past the end.
        let ends_before = |entry: usize| {
            let last = first(entry) + folds.continuation_lines(entry) as u64;
            last < line
        };
        let entry = partition_point(entries, ends_before).min(entries - 1);
        let inside = if folds.hidden_lines(entry) > 0 {
            0
        } else {
            let last = folds.continuation_lines(entry) as u64;
            line.saturating_sub(first(entry)).min(last) as usize
        };
        Ok(Some(self.lead_rows() + folds.row_of(entry, inside)))
    }

    /// The row of the first entry shown at or after the time `text` says, `None` to
    /// stay where the viewer is.
    fn time_row(&mut self, text: &str) -> Result<Option<usize>, String> {
        let spec = TimeSpec::parse(text)?;
        let entries = self.shown_folds().entries();
        if entries == 0 {
            return Ok(None);
        }
        // One guard for the whole scan: taking it per entry would stall the UI behind
        // a pending writer.
        let index = read(&self.index);
        let (records, filtered) = match (&self.opening, &self.filter) {
            (Opening::Tail(records, _), _) => (records.as_slice(), None),
            (_, filter) => (index.records.as_slice(), filter.as_ref()),
        };
        let Some((_, last)) = last_dated(records, 0, None) else {
            self.notice = Some(Cow::Borrowed("no entry has a timestamp"));
            return Ok(None);
        };
        let start = spec.start(last, &self.settings.zone);
        let at_or_after = |record: &Record| record.timestamp().is_some_and(|at| at >= start);
        // The engine may know of matches the fold state doesn't yet.
        let found = match filtered {
            Some(filter) => filter
                .engine
                .filtered()
                .entries
                .iter()
                .take(entries)
                .position(|&entry| at_or_after(&records[entry as usize])),
            None => records.iter().take(entries).position(at_or_after),
        };
        Ok(Some(found.map_or(self.row_count() - 1, |entry| {
            self.lead_rows() + self.shown_folds().row_of(entry, 0)
        })))
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
            self.notice = Some(Cow::Borrowed(NO_CLASS));
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

/// How many entries the histogram counts: the filters' matches, or the index's
/// entries without filters.
fn histogram_sources(filter: Option<&Filter>, records: &[Record]) -> usize {
    filter.map_or(records.len(), |filter| {
        filter.engine.filtered().entries.len()
    })
}

/// The record of the histogram's `i`th source entry, see [`histogram_sources`].
fn counted_record<'a>(
    filter: Option<&Filter>,
    records: &'a [Record],
    i: usize,
) -> Option<&'a Record> {
    match filter {
        None => records.get(i),
        Some(filter) => records.get(*filter.engine.filtered().entries.get(i)? as usize),
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

/// Where a move to a match goes.
enum Next {
    /// The matches that tell aren't counted yet.
    Wait,
    Nothing,
    Match {
        entry: usize,
        wrapped: bool,
    },
}

/// The match `matches`, the counted entries that match, has after index entry
/// `selected` or, `backward`, before it. Past the last one it wraps to the first, once
/// `counted` says there are no more to come.
fn find_match(matches: &Filtered, selected: Option<usize>, backward: bool, counted: bool) -> Next {
    let entries = &matches.entries;
    let (near, far) = if backward {
        // Matches before the selected entry are all in only once it is evaluated.
        if !counted && selected.is_none_or(|selected| matches.evaluated <= selected) {
            return Next::Wait;
        }
        let before = selected.map_or(entries.len(), |selected| {
            entries.partition_point(|&entry| (entry as usize) < selected)
        });
        (
            before.checked_sub(1).and_then(|i| entries.get(i)),
            entries.last(),
        )
    } else {
        let after = selected.map_or(0, |selected| {
            entries.partition_point(|&entry| entry as usize <= selected)
        });
        (entries.get(after), entries.first())
    };
    match (near, far) {
        (Some(&entry), _) => Next::Match {
            entry: entry as usize,
            wrapped: false,
        },
        _ if !counted => Next::Wait,
        (None, Some(&entry)) => Next::Match {
            entry: entry as usize,
            wrapped: true,
        },
        (None, None) => Next::Nothing,
    }
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

/// The first of `0..len` that `before` is false for, `before` being true for a
/// prefix of them: what `slice::partition_point` is for a range.
fn partition_point(len: usize, before: impl Fn(usize) -> bool) -> usize {
    let (mut low, mut high) = (0, len);
    while low < high {
        let middle = low + (high - low) / 2;
        if before(middle) {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

/// Whether `entry` shows its continuation lines, `None` if it has none.
fn unfolded(folds: &Folds, entry: usize) -> Option<bool> {
    let shown = folds.row_of(entry + 1, 0) - folds.row_of(entry, 0) - 1;
    (shown + folds.hidden_lines(entry) > 0).then_some(shown > 0)
}
