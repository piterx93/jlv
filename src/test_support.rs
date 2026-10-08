//! Helpers shared by the unit tests.

use std::fs;
use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::source::{self, Source};

/// `content` mapped from a file. The file is gone by the time this returns; the
/// mapping stays valid.
pub fn source(content: &[u8]) -> Source {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app.log");
    fs::write(&path, content).unwrap();
    source::open(&path).unwrap()
}

/// A viewer over `content`, opened as `/var/log/app.log`.
pub fn app(content: &[u8]) -> App {
    named_app("/var/log/app.log", content)
}

/// A viewer over `content`, opened as `path`.
pub fn named_app(path: impl AsRef<Path>, content: &[u8]) -> App {
    App::new(path.as_ref(), source(content))
}

/// `n` log lines that say which line they are, `… - step 1` to `… - step n`.
pub fn numbered_log(n: usize) -> String {
    (1..=n)
        .map(|i| format!("12:00:00.{i:03} [main] INFO  com.example.App - step {i}\n"))
        .collect()
}

pub fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub fn char_key(c: char) -> KeyEvent {
    key(KeyCode::Char(c))
}

pub fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

/// Screen line `y` as it looks, without trailing blanks: a wide character's second
/// column is not repeated.
pub fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    let mut x = 0;
    while x < buffer.area.width {
        let symbol = buffer[(x, y)].symbol();
        text.push_str(symbol);
        x += symbol.width().max(1) as u16;
    }
    text.trim_end().to_string()
}
