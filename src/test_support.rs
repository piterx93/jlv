//! Helpers shared by the unit tests.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use flate2::Compression;
use flate2::write::GzEncoder;
use jiff::Timestamp;
use jiff::tz::TimeZone;

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

/// `bytes` as a gzip file, stored without compression: quick to make, and as big as
/// its content.
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::none());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

/// A viewer in `Raw` over the gzip file `/var/log/app.log.gz` holding `gzip`, opened
/// as jlv opens a file by default, at the end. It hasn't taken in any entries yet.
pub fn gzip_app(gzip: &[u8]) -> App {
    App::new(
        Path::new("/var/log/app.log.gz"),
        source(gzip),
        Some(Preset::Raw),
        zone(),
        mtime(),
        true,
    )
}

/// The zone the test viewers read timestamps in.
pub fn zone() -> TimeZone {
    TimeZone::UTC
}

/// The modification time the test viewers' files have.
pub fn mtime() -> Timestamp {
    "2026-10-08T12:00:00Z".parse().unwrap()
}

/// A viewer over `content` in `Raw`, opened at the top as `/var/log/app.log`, with
/// the whole file indexed.
pub fn app(content: &[u8]) -> App {
    named_app("/var/log/app.log", content)
}

/// A viewer over `content` in `Raw`, opened at the top as `path`, with the whole
/// file indexed.
pub fn named_app(path: impl AsRef<Path>, content: &[u8]) -> App {
    let mut app = App::new(
        path.as_ref(),
        source(content),
        Some(Preset::Raw),
        zone(),
        mtime(),
        false,
    );
    wait_for_index(&mut app);
    app
}

/// A viewer over `content` in the preset detected from it, opened at the top as
/// `/var/log/app.log`, with the whole file indexed.
pub fn detected_app(content: &[u8]) -> App {
    let mut app = opened(None, content, false);
    wait_for_index(&mut app);
    app
}

/// A viewer over `content` in `preset`, opened at the top as `/var/log/app.log`,
/// with the whole file indexed.
pub fn app_in(preset: Preset, content: &[u8]) -> App {
    let mut app = unindexed_app(preset, content);
    wait_for_index(&mut app);
    app
}

/// A viewer over `content` in `preset`, opened at the top as `/var/log/app.log`,
/// that hasn't taken in any entries yet.
pub fn unindexed_app(preset: Preset, content: &[u8]) -> App {
    opened(Some(preset), content, false)
}

/// A viewer over `content` in `preset`, opened at the end as `/var/log/app.log` to
/// follow it, that hasn't taken in any entries yet.
pub fn following_app(preset: Preset, content: &[u8]) -> App {
    opened(Some(preset), content, true)
}

fn opened(format: Option<Preset>, content: &[u8], follow: bool) -> App {
    App::new(
        Path::new("/var/log/app.log"),
        source(content),
        format,
        zone(),
        mtime(),
        follow,
    )
}

/// How long indexing a test file of a few kilobytes may take before the test fails.
/// It takes milliseconds, even in a debug build.
pub const INDEX_DEADLINE: Duration = Duration::from_secs(1);

/// How long indexing a test file of some 2 MB may take before the test fails. It
/// takes well under a second on a few cores, even in a debug build.
pub const LARGE_INDEX_DEADLINE: Duration = Duration::from_secs(5);

/// Polls `app`'s index until it is complete. Fails the test after
/// [`INDEX_DEADLINE`].
pub fn wait_for_index(app: &mut App) {
    wait_for_index_within(app, INDEX_DEADLINE);
}

/// Polls `app`'s index until it is complete. Fails the test after `within`.
pub fn wait_for_index_within(app: &mut App, within: Duration) {
    let deadline = Instant::now() + within;
    loop {
        app.poll_index();
        if app.progress().is_complete() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "indexing took longer than {within:?}: {:?}",
            app.progress()
        );
        thread::sleep(Duration::from_millis(1));
    }
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
