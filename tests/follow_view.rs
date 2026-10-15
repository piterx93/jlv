//! The viewer following a file in a temporary directory through its watcher.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use jiff::tz::TimeZone;
use jlv::app::App;
use jlv::event_loop;
use jlv::follow::Options;
use jlv::format::Preset;
use jlv::source;
use jlv::ui::Theme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

/// How long a test waits for the viewer to catch up: far longer than it takes, so
/// that a slow machine doesn't fail a test.
const PATIENCE: Duration = Duration::from_secs(5);

/// Polling only, and often, so that a change shows up within milliseconds wherever
/// the test runs.
const OPTIONS: Options = Options {
    notify: false,
    poll: Duration::from_millis(10),
};

/// A viewer in `Raw` opened at the end of `app.log` in `dir`, which holds `content`,
/// on a 10-line screen and watching the file as `options` say, with the whole file
/// indexed.
fn watching(dir: &TempDir, content: &str, options: Options) -> (PathBuf, App) {
    let path = dir.path().join("app.log");
    fs::write(&path, content).unwrap();
    let mut app = App::new(
        &path,
        source::open(&path).unwrap(),
        Some(Preset::Raw),
        TimeZone::UTC,
        "2026-10-08T12:00:00Z".parse().unwrap(),
        true,
    );
    app.resize(10);
    app.watch(&path, File::open(&path).unwrap(), options)
        .unwrap();
    poll_until(&mut app, |app| app.progress().is_complete());
    (path, app)
}

/// Polls `app` until `done` says so. Fails the test after [`PATIENCE`].
fn poll_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + PATIENCE;
    loop {
        app.poll_index();
        if done(app) {
            return;
        }
        assert!(Instant::now() < deadline, "gave up after {PATIENCE:?}");
        thread::sleep(Duration::from_millis(1));
    }
}

fn append(path: &Path, text: &str) {
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

#[test]
fn appended_lines_show_up_while_watching() {
    let dir = TempDir::new().unwrap();
    let (path, mut app) = watching(&dir, "one\n", OPTIONS);
    append(&path, "two\n");
    poll_until(&mut app, |app| app.viewport().total() == 2);
    assert_eq!(app.line(1), "two");
    assert_eq!(app.viewport().selected(), Some(1));
}

#[test]
fn deleted_file_is_waited_for() {
    let dir = TempDir::new().unwrap();
    let (path, mut app) = watching(&dir, "one\n", OPTIONS);
    fs::remove_file(&path).unwrap();
    poll_until(&mut app, App::waiting_for_file);
    assert_eq!(app.line(0), "one");
}

#[test]
fn event_loop_waits_at_most_100_ms_at_a_time_while_watching() {
    let dir = TempDir::new().unwrap();
    let (_path, mut app) = watching(&dir, "one\n", OPTIONS);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut waits = Vec::new();
    event_loop(
        &mut terminal,
        &mut app,
        Theme::Styled,
        Instant::now,
        |timeout| {
            waits.push(timeout);
            let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
            Ok::<_, io::Error>(Some(Event::Key(q)))
        },
    )
    .unwrap();
    assert_eq!(waits.len(), 1);
    assert!(
        waits[0].is_some_and(|wait| wait > Duration::ZERO && wait <= Duration::from_millis(100)),
        "{waits:?}"
    );
}

#[test]
fn file_cut_short_is_shown_as_truncated_before_the_watcher_looks_again() {
    let dir = TempDir::new().unwrap();
    let slow = Options {
        notify: false,
        poll: Duration::from_secs(3600),
    };
    let (path, mut app) = watching(&dir, "one\ntwo\nthree\n", slow);
    fs::write(&path, "new\n").unwrap();
    app.poll_index();
    // The old rows lie past the file's end now; reading them would crash.
    assert_eq!(app.line(0), "── file truncated ──");
    poll_until(&mut app, |app| app.viewport().total() == 2);
    assert_eq!(app.line(1), "new");
}
