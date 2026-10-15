//! Following a log file in a temporary directory through appends, rotations,
//! truncation and deletion, as the app sees it through the watcher's events.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use jlv::follow::{Event, Options, Watcher};
use notify::{RecursiveMode, Watcher as _};
use tempfile::TempDir;

/// How long a test waits for the watcher to catch up: far longer than it takes, so
/// that a slow machine doesn't fail a test.
const PATIENCE: Duration = Duration::from_secs(5);

/// What the app would show while following a file: the text read so far, with a
/// marker where it starts over on a truncated or another file, and whether it waits
/// for the file to come back.
struct Follower {
    watcher: Watcher,
    file: File,
    read: u64,
    text: String,
    waiting: bool,
    /// How many events came so far.
    events: usize,
}

impl Follower {
    /// Reads `file`, opened at `path`, up to `read` bytes and starts watching it.
    fn new(path: &Path, file: File, read: u64, options: Options) -> Follower {
        let watcher =
            Watcher::spawn(path.to_path_buf(), file.try_clone().unwrap(), read, options).unwrap();
        let mut follower = Follower {
            watcher,
            file,
            read: 0,
            text: String::new(),
            waiting: false,
            events: 0,
        };
        follower.read_to(read);
        follower
    }

    fn apply(&mut self, event: Event) {
        self.events += 1;
        match event {
            Event::Grew { len } => {
                assert!(len > self.read, "grew to {len} bytes, read {}", self.read);
                self.read_to(len);
            }
            Event::Truncated { len } => {
                assert!(!self.waiting, "truncated while gone");
                self.text.push_str("[truncated]");
                self.read = 0;
                self.read_to(len);
            }
            Event::Replaced { file, len } => {
                assert!(!self.waiting, "replaced while gone");
                self.switch(file, len);
            }
            Event::Gone => {
                assert!(!self.waiting, "gone twice");
                self.waiting = true;
            }
            Event::Back { file, len } => {
                assert!(self.waiting, "back without being gone");
                self.waiting = false;
                self.switch(file, len);
            }
        }
    }

    fn switch(&mut self, file: File, len: u64) {
        self.text.push_str("[new file]");
        self.file = file;
        self.read = 0;
        self.read_to(len);
    }

    fn read_to(&mut self, len: u64) {
        let mut bytes = vec![0; (len - self.read) as usize];
        self.file.read_exact_at(&mut bytes, self.read).unwrap();
        self.text.push_str(&String::from_utf8_lossy(&bytes));
        self.read = len;
    }

    /// Takes in events until the follower shows `text` and `waiting`, for at most
    /// `within`.
    fn wait_for(&mut self, within: Duration, text: &str, waiting: bool) {
        let deadline = Instant::now() + within;
        while (self.text.as_str(), self.waiting) != (text, waiting) {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.watcher.events().recv_timeout(left) {
                Ok(event) => self.apply(event),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => panic!("the watcher stopped"),
            }
        }
        assert_eq!((self.text.as_str(), self.waiting), (text, waiting));
    }

    /// Asserts that no event comes for `period`.
    fn assert_quiet(&self, period: Duration) {
        match self.watcher.events().recv_timeout(period) {
            Ok(event) => panic!("unexpected {event:?}"),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => panic!("the watcher stopped"),
        }
    }
}

/// A temporary directory holding `app.log` with `text` in it.
fn log_with(text: &str) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    fs::write(&path, text).unwrap();
    (dir, path)
}

/// Follows the file at `path`, read to its end, with the default options.
fn follow(path: &Path) -> Follower {
    let file = File::open(path).unwrap();
    let read = file.metadata().unwrap().len();
    Follower::new(path, file, read, Options::default())
}

/// Follows `app.log`, read while it held "a\n", with `options`, once the look at start
/// has found the "b\n" appended after that.
fn follow_past_first_look(options: Options) -> (TempDir, PathBuf, Follower) {
    let (dir, path) = log_with("a\n");
    let file = File::open(&path).unwrap();
    append(&path, "b\n");
    let mut follower = Follower::new(&path, file, 2, options);
    follower.wait_for(PATIENCE, "a\nb\n", false);
    (dir, path, follower)
}

fn append(path: &Path, text: &str) {
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

/// Where rotation moves the file at `path`.
fn rotated(path: &Path) -> PathBuf {
    path.with_extension("log.1")
}

#[test]
fn default_options_watch_and_poll_every_250_ms() {
    let expected = Options {
        notify: true,
        poll: Duration::from_millis(250),
    };
    assert_eq!(Options::default(), expected);
}

#[test]
fn shows_an_append() {
    let (_dir, path) = log_with("a\n");
    let mut follower = follow(&path);
    append(&path, "b\n");
    follower.wait_for(PATIENCE, "a\nb\n", false);
}

#[test]
fn follows_the_new_file_after_rename_and_create() {
    let (_dir, path) = log_with("a\n");
    let mut follower = follow(&path);
    let old = rotated(&path);
    fs::rename(&path, &old).unwrap();
    // The writer still has the old file open and writes once more before it reopens.
    append(&old, "b\n");
    fs::write(&path, "c\n").unwrap();
    follower.wait_for(PATIENCE, "a\nb\n[new file]c\n", false);
    append(&path, "d\n");
    follower.wait_for(PATIENCE, "a\nb\n[new file]c\nd\n", false);
}

#[test]
fn reads_the_old_file_to_its_end_before_following_its_replacement() {
    let (_dir, path) = log_with("a\n");
    let options = Options {
        notify: false,
        poll: Duration::from_secs(1),
    };
    let mut follower = Follower::new(&path, File::open(&path).unwrap(), 2, options);
    // A rotation that never leaves the path empty: the old file keeps a second name
    // and gets one more line right before a new file is moved over the path.
    fs::hard_link(&path, rotated(&path)).unwrap();
    append(&rotated(&path), "b\n");
    let fresh = path.with_extension("new");
    fs::write(&fresh, "c\n").unwrap();
    fs::rename(&fresh, &path).unwrap();
    follower.wait_for(PATIENCE, "a\nb\n[new file]c\n", false);
}

#[test]
fn starts_over_after_copytruncate() {
    let (_dir, path) = log_with("a\n");
    let mut follower = follow(&path);
    fs::copy(&path, rotated(&path)).unwrap();
    fs::write(&path, "c\n").unwrap();
    follower.wait_for(PATIENCE, "a\n[truncated]c\n", false);
}

#[test]
fn follows_the_new_file_after_delete_and_recreate() {
    let (_dir, path) = log_with("a\n");
    let mut follower = follow(&path);
    fs::remove_file(&path).unwrap();
    fs::write(&path, "c\n").unwrap();
    follower.wait_for(PATIENCE, "a\n[new file]c\n", false);
}

#[test]
fn waits_while_the_file_is_missing() {
    let (_dir, path) = log_with("a\n");
    let mut follower = follow(&path);
    fs::remove_file(&path).unwrap();
    follower.wait_for(PATIENCE, "a\n", true);
    // More looks at the missing path, more than two polls' worth, say nothing new.
    follower.assert_quiet(Duration::from_millis(600));
}

#[test]
fn follows_the_file_when_it_reappears() {
    let (_dir, path) = log_with("a\n");
    let mut follower = follow(&path);
    fs::remove_file(&path).unwrap();
    follower.wait_for(PATIENCE, "a\n", true);
    fs::write(&path, "c\n").unwrap();
    follower.wait_for(PATIENCE, "a\n[new file]c\n", false);
}

#[test]
fn follows_by_polling_when_the_parent_directory_cannot_be_watched() {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("logs");
    fs::create_dir(&parent).unwrap();
    let path = parent.join("app.log");
    fs::write(&path, "a\n").unwrap();
    let file = File::open(&path).unwrap();
    // A directory that doesn't exist can't be watched.
    fs::remove_file(&path).unwrap();
    fs::remove_dir(&parent).unwrap();
    let mut follower = Follower::new(&path, file, 2, Options::default());
    follower.wait_for(PATIENCE, "a\n", true);
    fs::create_dir(&parent).unwrap();
    fs::write(&path, "c\n").unwrap();
    follower.wait_for(PATIENCE, "a\n[new file]c\n", false);
}

#[test]
fn looks_once_at_start_and_then_only_when_woken() {
    let options = Options {
        notify: false,
        poll: Duration::from_secs(3600),
    };
    let (_dir, path, follower) = follow_past_first_look(options);
    // With neither notify nor a poll due, nothing looks at the file again.
    append(&path, "c\n");
    follower.assert_quiet(Duration::from_millis(500));
}

#[test]
fn stays_idle_with_notify_while_the_file_does_not_change() {
    let options = Options {
        notify: true,
        poll: Duration::from_secs(3600),
    };
    let (dir, _path, _follower) = follow_past_first_look(options);
    // A look opens the file, which is an event in the directory too. Listening for
    // anything there shows whether the watcher wakes itself up and looks again.
    let (seen, events) = mpsc::channel();
    let mut listener = notify::recommended_watcher(move |event| {
        let _ = seen.send(event);
    })
    .unwrap();
    listener
        .watch(dir.path(), RecursiveMode::NonRecursive)
        .unwrap();
    if let Ok(event) = events.recv_timeout(Duration::from_millis(500)) {
        panic!("the directory was touched: {event:?}");
    }
}

#[test]
fn notify_reports_a_change_between_polls() {
    let options = Options {
        notify: true,
        poll: Duration::from_secs(3600),
    };
    let (_dir, path, mut follower) = follow_past_first_look(options);
    append(&path, "c\n");
    follower.wait_for(PATIENCE, "a\nb\nc\n", false);
}

#[test]
fn dropping_the_watcher_closes_its_descriptor_of_the_file() {
    let dir = tempfile::tempdir().unwrap();
    // A pipe stands in for the followed file, so that its reader sees the moment the
    // watcher's copy of it is closed. With nothing at the path the watcher keeps it.
    let (mut reader, writer) = io::pipe().unwrap();
    let file = File::from(OwnedFd::from(writer));
    let options = Options {
        notify: true,
        poll: Duration::from_secs(3600),
    };
    let watcher = Watcher::spawn(dir.path().join("app.log"), file, 0, options).unwrap();
    assert!(matches!(
        watcher.events().recv_timeout(PATIENCE),
        Ok(Event::Gone)
    ));
    drop(watcher);
    let (done, closed) = mpsc::channel();
    thread::spawn(move || {
        let _ = reader.read_to_end(&mut Vec::new());
        let _ = done.send(());
    });
    closed
        .recv_timeout(PATIENCE)
        .expect("the file is still open");
}

#[test]
fn takes_in_a_burst_of_writes_completely_in_few_events() {
    const WRITES: usize = 5000;
    let (_dir, path) = log_with("");
    let mut follower = follow(&path);
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    let mut expected = String::new();
    for i in 0..WRITES {
        let line = format!("line {i}\n");
        file.write_all(line.as_bytes()).unwrap();
        expected.push_str(&line);
    }
    follower.wait_for(PATIENCE, &expected, false);
    assert!(
        follower.events < WRITES / 10,
        "{} events for {WRITES} writes",
        follower.events
    );
}
