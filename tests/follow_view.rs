//! The viewer following a file in a temporary directory through its watcher.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use jiff::tz::TimeZone;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;
use vilog::app::App;
use vilog::event_loop;
use vilog::filter::Filters;
use vilog::follow::Options;
use vilog::format::Format;
use vilog::matcher::{self, Matcher};
use vilog::source;
use vilog::ui::Theme;

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
fn watching(dir: &Path, content: &str, options: Options) -> (PathBuf, App) {
    let path = dir.join("app.log");
    fs::write(&path, content).unwrap();
    let mut app = App::new(
        &path,
        source::open(&path).unwrap(),
        Format::by_id("raw"),
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
    let (path, mut app) = watching(dir.path(), "one\n", OPTIONS);
    append(&path, "two\n");
    poll_until(&mut app, |app| app.viewport().total() == 2);
    assert_eq!(app.line(1), "two");
    assert_eq!(app.viewport().selected(), Some(1));
}

/// A file as this process can be seen to hold it: by inode for its open
/// descriptors, by path for its mappings.
struct Held {
    inode: u64,
    path: String,
}

impl Held {
    fn at(path: &Path) -> Held {
        Held {
            inode: fs::metadata(path).unwrap().ino(),
            path: path.canonicalize().unwrap().to_str().unwrap().to_owned(),
        }
    }

    /// Whether this process holds the file, also after it was deleted: a descriptor
    /// is open on it, as listed in `/proc/self/fd` on Linux or `/dev/fd` on macOS,
    /// where a stat of an entry gives the inode of the file the descriptor is open on
    /// (the device is devfs's on macOS); or, on Linux only, a mapping of it is left:
    /// `/proc/self/maps` names it by the path it was opened at, with ` (deleted)`
    /// after it once that name is removed.
    fn by_this_process(&self) -> bool {
        let fds = if cfg!(target_os = "linux") {
            "/proc/self/fd"
        } else {
            "/dev/fd"
        };
        let open = fs::read_dir(fds).unwrap().any(|entry| {
            fs::metadata(entry.unwrap().path())
                .is_ok_and(|meta| meta.is_file() && meta.ino() == self.inode)
        });
        open || cfg!(target_os = "linux")
            && fs::read_to_string("/proc/self/maps")
                .unwrap()
                .contains(&self.path)
    }
}

/// With filters set, so that their results are let go of too.
#[test]
fn deleted_file_is_let_go_and_the_next_one_shown_from_its_start() {
    let dir = TempDir::new().unwrap();
    let (path, mut app) = watching(dir.path(), "one\n", OPTIONS);
    app.set_filters(Filters {
        text: Some(Matcher::new("o", matcher::Options::default()).unwrap()),
        ..Filters::default()
    });
    poll_until(&mut app, |app| !app.filtering());
    let file = Held::at(&path);
    assert!(
        file.by_this_process(),
        "the viewer's hold on its file is not seen"
    );
    // A second name keeps the inode from being freed, so that no file another test
    // creates meanwhile can take it and be mistaken for the viewer's hold.
    fs::hard_link(&path, dir.path().join("kept")).unwrap();
    fs::remove_file(&path).unwrap();
    poll_until(&mut app, |app| {
        app.waiting_for_file() && app.viewport().total() == 0 && !file.by_this_process()
    });
    fs::write(&path, "two\n").unwrap();
    poll_until(&mut app, |app| app.viewport().total() == 1);
    assert_eq!(app.line(0), "two");
    assert!(!app.waiting_for_file());
}

#[test]
fn removed_directory_empties_the_view_until_the_file_is_back() {
    let dir = TempDir::new().unwrap();
    let logs = dir.path().join("logs");
    fs::create_dir(&logs).unwrap();
    let options = Options {
        notify: true,
        ..OPTIONS
    };
    let (path, mut app) = watching(&logs, "one\n", options);
    fs::remove_dir_all(&logs).unwrap();
    poll_until(&mut app, |app| {
        app.waiting_for_file() && app.viewport().total() == 0
    });
    fs::create_dir(&logs).unwrap();
    fs::write(&path, "two\n").unwrap();
    poll_until(&mut app, |app| app.viewport().total() == 1);
    assert_eq!(app.line(0), "two");
}

#[test]
fn event_loop_waits_at_most_100_ms_at_a_time_while_watching() {
    let dir = TempDir::new().unwrap();
    let (_path, mut app) = watching(dir.path(), "one\n", OPTIONS);
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
    let (path, mut app) = watching(dir.path(), "one\ntwo\nthree\n", slow);
    fs::write(&path, "new\n").unwrap();
    app.poll_index();
    // The old rows lie past the file's end now; reading them would crash.
    assert_eq!(app.line(0), "── file truncated ──");
    poll_until(&mut app, |app| app.viewport().total() == 2);
    assert_eq!(app.line(1), "new");
}
