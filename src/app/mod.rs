//! The viewer's state and what keys do to it. No terminal and no drawing: `ui` draws an
//! `App`, the event loop turns keys into actions and applies them.

use std::borrow::Cow;
use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::source::{self, LineIndex, Source};
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
    Quit,
}

/// The action bound to a key, `None` for keys that do nothing.
///
/// `j k ↑ ↓` move by a line, `PgUp PgDn` by a page, `Ctrl-u Ctrl-d` by half a page,
/// `g Home` go to the top, `G End` to the bottom, `q F10 Ctrl-c` quit. Characters
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
        (KeyCode::Char('q') | KeyCode::F(10), KeyModifiers::NONE)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        _ => return None,
    };
    Some(action)
}

/// A log file being viewed.
#[derive(Debug)]
pub struct App {
    name: String,
    source: Source,
    lines: LineIndex,
    viewport: Viewport,
    quit: bool,
}

impl App {
    /// A viewer over `source` at the top of the file with the first line selected.
    /// `path` is where the file was opened from. The screen has no lines until the
    /// first [`resize`](Self::resize).
    pub fn new(path: &Path, source: Source) -> Self {
        // A path like `..` has no last component; showing it whole is the next best thing.
        let name = path.file_name().unwrap_or(path.as_os_str());
        let lines = LineIndex::new(source.bytes());
        Self {
            name: name.to_string_lossy().into_owned(),
            viewport: Viewport::new(lines.len(), 0),
            source,
            lines,
            quit: false,
        }
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

    /// Line `i` as text (see [`crate::source::display`]). Not yet safe to show: it may
    /// hold control characters. Panics if `i` is not below `viewport().total()`.
    pub fn line(&self, i: usize) -> Cow<'_, str> {
        source::display(self.lines.line(self.source.bytes(), i))
    }

    /// Which lines are on screen and which one is selected.
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Sets the number of screen lines the file is shown in.
    pub fn resize(&mut self, height: usize) {
        self.viewport.resize(height);
    }

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
            Action::Quit => self.quit = true,
        }
    }

    /// Whether [`Action::Quit`] has been applied.
    pub fn should_quit(&self) -> bool {
        self.quit
    }
}
