//! The key bindings: the one table that [`action`](crate::app::action) looks keys up
//! in and the help overlay lists, so that the overlay always shows the keys that work.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use crate::app::Action;
use crate::format::Level;

#[cfg(test)]
mod tests;

/// A key and the action it is bound to. A character binding has no Shift: the
/// character already says which case it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub action: Action,
}

/// The default bindings, in the order the help overlay lists them, each key at most
/// once.
pub fn bindings() -> &'static [Binding] {
    BINDINGS
}

const fn plain(code: KeyCode, action: Action) -> Binding {
    Binding {
        code,
        modifiers: KeyModifiers::NONE,
        action,
    }
}

const fn ch(c: char, action: Action) -> Binding {
    plain(KeyCode::Char(c), action)
}

const fn ctrl(c: char, action: Action) -> Binding {
    Binding {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::CONTROL,
        action,
    }
}

static BINDINGS: &[Binding] = &[
    ch('k', Action::Up),
    plain(KeyCode::Up, Action::Up),
    ch('j', Action::Down),
    plain(KeyCode::Down, Action::Down),
    plain(KeyCode::PageUp, Action::PageUp),
    plain(KeyCode::PageDown, Action::PageDown),
    ctrl('u', Action::HalfPageUp),
    ctrl('d', Action::HalfPageDown),
    ch('g', Action::Top),
    plain(KeyCode::Home, Action::Top),
    ch('G', Action::Bottom),
    plain(KeyCode::End, Action::Bottom),
    plain(KeyCode::Left, Action::ScrollLeft),
    plain(KeyCode::Right, Action::ScrollRight),
    plain(KeyCode::Tab, Action::CycleFocus),
    plain(KeyCode::Enter, Action::Confirm),
    plain(KeyCode::Esc, Action::Cancel),
    ch('F', Action::ToggleFollow),
    ch('P', Action::CyclePreset),
    ch('/', Action::SearchForward),
    ch('?', Action::SearchBackward),
    ch('n', Action::NextMatch),
    ch('N', Action::PreviousMatch),
    ch(':', Action::LinePrompt),
    ch('t', Action::TimePrompt),
    ch('1', Action::ToggleLevel(Level::Trace)),
    ch('2', Action::ToggleLevel(Level::Debug)),
    ch('3', Action::ToggleLevel(Level::Info)),
    ch('4', Action::ToggleLevel(Level::Warn)),
    ch('5', Action::ToggleLevel(Level::Error)),
    ch('6', Action::ToggleLevel(Level::Fatal)),
    plain(KeyCode::F(2), Action::LevelPanel),
    plain(KeyCode::F(4), Action::TextPanel),
    ch('&', Action::TextPanel),
    plain(KeyCode::F(5), Action::TimePanel),
    plain(KeyCode::F(6), Action::ClassPanel),
    ch('c', Action::OnlyClass),
    ch('C', Action::HideClass),
    ch('x', Action::ClearFilters),
    ch(' ', Action::ToggleFold),
    ch('z', Action::FoldAll),
    ch('Z', Action::UnfoldAll),
    ch(']', Action::NextException),
    ch('[', Action::PreviousException),
    ch('m', Action::ToggleBookmark),
    ch('\'', Action::NextBookmark),
    ch('"', Action::PreviousBookmark),
    ch('y', Action::Copy),
    ch('e', Action::Export),
    plain(KeyCode::F(8), Action::Export),
    ch('T', Action::ToggleThread),
    ch('#', Action::ToggleLineNumbers),
    ch('H', Action::ToggleHistogram),
    ch('w', Action::ToggleWrap),
    plain(KeyCode::F(1), Action::Help),
    ch('q', Action::Quit),
    plain(KeyCode::F(10), Action::Quit),
    ctrl('c', Action::Quit),
];

/// How the help overlay names a key: a character as itself, `Space` for a space,
/// `↑ ↓ ← →` for the arrows, `PgUp`, `PgDn`, `Home`, `End`, `Tab`, `Enter`, `Esc`,
/// `F1`–`F12`, prefixed with `Ctrl-` and `Alt-` for those modifiers, as `Ctrl-u`.
pub fn key_name(code: KeyCode, modifiers: KeyModifiers) -> String {
    let mut name = String::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        name.push_str("Ctrl-");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        name.push_str("Alt-");
    }
    match code {
        KeyCode::Char(' ') => name.push_str("Space"),
        KeyCode::Char(c) => name.push(c),
        KeyCode::F(n) => name.push_str(&format!("F{n}")),
        KeyCode::Up => name.push('↑'),
        KeyCode::Down => name.push('↓'),
        KeyCode::Left => name.push('←'),
        KeyCode::Right => name.push('→'),
        KeyCode::PageUp => name.push_str("PgUp"),
        KeyCode::PageDown => name.push_str("PgDn"),
        KeyCode::Home => name.push_str("Home"),
        KeyCode::End => name.push_str("End"),
        KeyCode::Tab => name.push_str("Tab"),
        KeyCode::Enter => name.push_str("Enter"),
        KeyCode::Esc => name.push_str("Esc"),
        other => name.push_str(&format!("{other:?}")),
    }
    name
}

/// What the help overlay says `action` does, the same for every
/// [`Action::ToggleLevel`].
pub fn description(action: Action) -> &'static str {
    match action {
        Action::Up => "Move up",
        Action::Down => "Move down",
        Action::PageUp => "Page up",
        Action::PageDown => "Page down",
        Action::HalfPageUp => "Half page up",
        Action::HalfPageDown => "Half page down",
        Action::Top => "Go to top",
        Action::Bottom => "Go to bottom and follow",
        Action::ScrollLeft => "Scroll left",
        Action::ScrollRight => "Scroll right",
        Action::CycleFocus => "Focus next pane",
        Action::Confirm => "Details panel",
        Action::Cancel => "Cancel waiting export",
        Action::ToggleFollow => "Toggle follow",
        Action::CyclePreset => "Cycle format preset",
        Action::SearchForward => "Search forward",
        Action::SearchBackward => "Search backward",
        Action::NextMatch => "Next match",
        Action::PreviousMatch => "Previous match",
        Action::LinePrompt => "Jump to line",
        Action::TimePrompt => "Jump to time",
        Action::ToggleLevel(_) => "Toggle level (TRACE to FATAL)",
        Action::LevelPanel => "Level filter",
        Action::TextPanel => "Text filter",
        Action::TimePanel => "Time filter",
        Action::ClassPanel => "Class filter",
        Action::OnlyClass => "Only this class",
        Action::HideClass => "Hide this class",
        Action::ClearFilters => "Clear all filters",
        Action::ToggleFold => "Fold or unfold entry",
        Action::FoldAll => "Fold all",
        Action::UnfoldAll => "Unfold all",
        Action::NextException => "Next exception",
        Action::PreviousException => "Previous exception",
        Action::ToggleBookmark => "Toggle bookmark",
        Action::NextBookmark => "Next bookmark",
        Action::PreviousBookmark => "Previous bookmark",
        Action::Copy => "Copy entry",
        Action::Export => "Export filtered",
        Action::ToggleThread => "Toggle thread column",
        Action::ToggleLineNumbers => "Toggle line numbers",
        Action::ToggleHistogram => "Toggle histogram",
        Action::ToggleWrap => "Toggle wrap",
        Action::Help => "Help",
        Action::Quit => "Quit",
    }
}

/// The lines of the help overlay, made from [`bindings`]: one per [`description`], in
/// the order of its first binding, with the [names](key_name) of the keys bound to
/// actions with that description, in keymap order and a space apart, as
/// `("k ↑", "Move up")`.
pub fn help() -> Vec<(String, &'static str)> {
    let mut lines: Vec<(String, &'static str)> = Vec::new();
    for binding in bindings() {
        let name = key_name(binding.code, binding.modifiers);
        let description = description(binding.action);
        match lines.iter_mut().find(|(_, d)| *d == description) {
            Some((keys, _)) => {
                keys.push(' ');
                keys.push_str(&name);
            }
            None => lines.push((name, description)),
        }
    }
    lines
}
