//! The key bindings: the one table that [`Keymap::action`] looks keys up in and the
//! help overlay lists, so that the overlay always shows the keys that work.

use std::str::FromStr;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

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

/// A key as the configuration file names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

/// The key syntax of the `[keys]` section, case-sensitive: a single character other
/// than a control character, `F1`–`F12`, `Enter`, `Esc`, `Space`, `Tab`, `Up`,
/// `Down`, `Left`, `Right`, `PgUp`, `PgDn`, `Home` and `End`, each of them optionally
/// after `Ctrl-`, as `Ctrl-x`. Anything else is an error that quotes the text.
impl FromStr for Key {
    type Err = String;

    fn from_str(text: &str) -> Result<Key, String> {
        let unknown = || format!("unknown key '{text}'");
        let (modifiers, base) = match text.strip_prefix("Ctrl-") {
            Some(base) => (KeyModifiers::CONTROL, base),
            None => (KeyModifiers::NONE, text),
        };
        // Terminals report Ctrl with a letter in lower case, so `Ctrl-F` would never fire.
        if modifiers == KeyModifiers::CONTROL
            && base.len() == 1
            && base.as_bytes()[0].is_ascii_uppercase()
        {
            return Err(unknown());
        }
        let code = match base {
            "Enter" => KeyCode::Enter,
            "Esc" => KeyCode::Esc,
            "Space" => KeyCode::Char(' '),
            "Tab" => KeyCode::Tab,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            "PgUp" => KeyCode::PageUp,
            "PgDn" => KeyCode::PageDown,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            _ => function_key(base)
                .or_else(|| single_char(base))
                .ok_or_else(unknown)?,
        };
        Ok(Key { code, modifiers })
    }
}

/// The key bindings in effect: the defaults with the `[keys]` section of the
/// configuration applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap {
            bindings: DEFAULTS.to_vec(),
        }
    }
}

impl Keymap {
    /// The default bindings with each action of `remaps` bound to its keys instead
    /// of its defaults. A remapped action's keys take the place of its first default
    /// key, in the order given, each once. A key bound to two actions is an error
    /// that [names](name) both actions and the key.
    pub fn remapped(remaps: &[(Action, Vec<Key>)]) -> Result<Keymap, String> {
        let remap = |action| remaps.iter().find(|(a, _)| *a == action);
        let mut bindings: Vec<Binding> = Vec::new();
        for default in DEFAULTS {
            let Some((action, keys)) = remap(default.action) else {
                bindings.push(*default);
                continue;
            };
            if bindings.iter().any(|b| b.action == *action) {
                continue;
            }
            for key in keys {
                let binding = Binding {
                    code: key.code,
                    modifiers: key.modifiers,
                    action: *action,
                };
                if !bindings.contains(&binding) {
                    bindings.push(binding);
                }
            }
        }
        for (i, first) in bindings.iter().enumerate() {
            let clash = bindings[i + 1..]
                .iter()
                .find(|b| (b.code, b.modifiers) == (first.code, first.modifiers));
            if let Some(second) = clash {
                return Err(format!(
                    "key {} is bound to both {} and {}",
                    key_name(first.code, first.modifiers),
                    name(first.action),
                    name(second.action),
                ));
            }
        }
        Ok(Keymap { bindings })
    }

    /// The bindings, in the order the help overlay lists them, each key at most once.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The action bound to `key`, `None` for keys that do nothing.
    ///
    /// Characters ignore Shift (the character already says which case it is) but not
    /// Ctrl or Alt; other keys do nothing with any other modifier. Key releases do
    /// nothing; held keys repeat.
    pub fn action(&self, key: KeyEvent) -> Option<Action> {
        let (code, modifiers) = normalized(key)?;
        self.bindings
            .iter()
            .find(|binding| binding.code == code && binding.modifiers == modifiers)
            .map(|binding| binding.action)
    }

    /// The lines of the help overlay, made from [`bindings`](Self::bindings): one per
    /// [`description`], in the order of its first binding, with the
    /// [names](key_name) of the keys bound to actions with that description, in
    /// keymap order and a space apart, as `("k ↑", "Move up")`.
    pub fn help(&self) -> Vec<(String, &'static str)> {
        let mut lines: Vec<(String, &'static str)> = Vec::new();
        for binding in &self.bindings {
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
}

/// The code and modifiers of `key` as the bindings see them, `None` for a release.
/// A character ignores Shift, as it already says which case it is.
pub(crate) fn normalized(key: KeyEvent) -> Option<(KeyCode, KeyModifiers)> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let modifiers = match key.code {
        KeyCode::Char(_) => key.modifiers - KeyModifiers::SHIFT,
        _ => key.modifiers,
    };
    Some((key.code, modifiers))
}

fn function_key(text: &str) -> Option<KeyCode> {
    let digits = text.strip_prefix('F')?;
    if digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits
        .parse()
        .ok()
        .filter(|n| (1..=12).contains(n))
        .map(KeyCode::F)
}

fn single_char(text: &str) -> Option<KeyCode> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_control() => Some(KeyCode::Char(c)),
        _ => None,
    }
}

/// The name of `action` in the `[keys]` section: the snake_case of its variant, as
/// `toggle_follow`, with the level after `toggle_level_`, as `toggle_level_info`.
pub fn name(action: Action) -> &'static str {
    match action {
        Action::Up => "up",
        Action::Down => "down",
        Action::PageUp => "page_up",
        Action::PageDown => "page_down",
        Action::HalfPageUp => "half_page_up",
        Action::HalfPageDown => "half_page_down",
        Action::Top => "top",
        Action::Bottom => "bottom",
        Action::ScrollLeft => "scroll_left",
        Action::ScrollRight => "scroll_right",
        Action::CycleFocus => "cycle_focus",
        Action::Confirm => "confirm",
        Action::Cancel => "cancel",
        Action::ToggleFollow => "toggle_follow",
        Action::CyclePreset => "cycle_preset",
        Action::SearchForward => "search_forward",
        Action::SearchBackward => "search_backward",
        Action::NextMatch => "next_match",
        Action::PreviousMatch => "previous_match",
        Action::LinePrompt => "line_prompt",
        Action::TimePrompt => "time_prompt",
        Action::ToggleLevel(Level::Trace) => "toggle_level_trace",
        Action::ToggleLevel(Level::Debug) => "toggle_level_debug",
        Action::ToggleLevel(Level::Info) => "toggle_level_info",
        Action::ToggleLevel(Level::Warn) => "toggle_level_warn",
        Action::ToggleLevel(Level::Error) => "toggle_level_error",
        Action::ToggleLevel(Level::Fatal) => "toggle_level_fatal",
        Action::LevelPanel => "level_panel",
        Action::TextPanel => "text_panel",
        Action::TimePanel => "time_panel",
        Action::ClassPanel => "class_panel",
        Action::OnlyClass => "only_class",
        Action::HideClass => "hide_class",
        Action::ClearFilters => "clear_filters",
        Action::ToggleFold => "toggle_fold",
        Action::FoldAll => "fold_all",
        Action::UnfoldAll => "unfold_all",
        Action::NextException => "next_exception",
        Action::PreviousException => "previous_exception",
        Action::ToggleBookmark => "toggle_bookmark",
        Action::NextBookmark => "next_bookmark",
        Action::PreviousBookmark => "previous_bookmark",
        Action::Copy => "copy",
        Action::Export => "export",
        Action::ToggleThread => "toggle_thread",
        Action::ToggleLineNumbers => "toggle_line_numbers",
        Action::ToggleHistogram => "toggle_histogram",
        Action::ToggleWrap => "toggle_wrap",
        Action::Help => "help",
        Action::Quit => "quit",
    }
}

/// The action whose [`name`] is `text`, `None` for a name vilog doesn't know.
pub fn named(text: &str) -> Option<Action> {
    DEFAULTS
        .iter()
        .map(|binding| binding.action)
        .find(|action| name(*action) == text)
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

static DEFAULTS: &[Binding] = &[
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
        Action::ClassPanel => "Logger filter",
        Action::OnlyClass => "Only this logger",
        Action::HideClass => "Hide this logger",
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
