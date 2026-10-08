//! Helpers shared by the unit tests.

use std::fs;
use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::format::Preset;
use crate::source::{self, Source};

/// The example lines of PRD §6.1, one per preset but `Raw`.
pub const EXAMPLES: [(Preset, &str); 6] = [
    (
        Preset::Logback,
        "2026-10-07 12:00:00.123 [http-nio-8080-exec-3] INFO  com.example.OrderService - Order 42 created",
    ),
    (
        Preset::LogbackBasic,
        "12:00:00.123 [main] INFO  com.example.App -- Started",
    ),
    (
        Preset::Spring3,
        "2026-10-07T12:00:00.123+02:00  INFO 12345 --- [myapp] [           main] o.s.b.SpringApplication                  : Started App in 2.1s",
    ),
    (
        Preset::Spring2,
        "2026-10-07 12:00:00.123  INFO 12345 --- [           main] o.s.b.SpringApplication                  : Started App",
    ),
    (
        Preset::Wildfly,
        "2026-10-07 12:00:00,123 INFO  [org.jboss.as.server] (Controller Boot Thread) WFLYSRV0025: WildFly Full 33.0.0.Final started",
    ),
    (
        Preset::WildflyConsole,
        "12:00:00,123 INFO  [org.jboss.as] (MSC service thread 1-2) WFLYSRV0049: ...",
    ),
];

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
