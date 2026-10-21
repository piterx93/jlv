//! The PRD §13 numbers criterion can't take, printed once each: the heap the
//! viewer keeps per entry, the release binary's first screen and idle CPU on a
//! pseudo-terminal made by `script(1)`, and the follow latency of vilog's event loop.
//! See `docs/benchmarks.md`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use memchr::memmem;
use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Size};
use tempfile::TempDir;
use vilog::config::Config;
use vilog::filter::{Filters, Levels};
use vilog::format::Level;
use vilog::index::Record;
use vilog::ui::Theme;

mod support;

use support::BenchLog;
use support::generator;

const ROWS: u16 = 50;
const COLUMNS: u16 = 200;
const LAUNCHES: usize = 5;
const APPENDS: usize = 20;
const IDLE: Duration = Duration::from_secs(20);

#[global_allocator]
static HEAP: Counting = Counting;

/// Bytes allocated and not yet freed.
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every call goes to `System` as it came; the counter only watches.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        unsafe { System.dealloc(block, layout) };
        ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(block, layout, size) };
        if !moved.is_null() {
            ALLOCATED.fetch_add(size, Ordering::Relaxed);
            ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        moved
    }
}

fn main() {
    let log = BenchLog::get();
    println!("log: {} ({})", log.path().display(), log.size_label());
    heap_per_entry(&log);
    let vilog = env!("CARGO_BIN_EXE_vilog");
    first_screen(vilog, &log);
    idle_cpu(vilog, &log);
    follow_latency(&log);
}

fn heap_per_entry(log: &BenchLog) {
    let source = Arc::new(log.open());
    let records = support::index(&source, &support::settings(log.path(), &source));
    let entries = records.len();
    let per_entry = |bytes: usize| bytes as f64 / entries as f64;
    println!(
        "entries: {entries}; index: {} B per record, {:.1} B per entry with the Vec's spare room",
        mem::size_of::<Record>(),
        per_entry(records.capacity() * mem::size_of::<Record>()),
    );
    drop(records);
    let before = ALLOCATED.load(Ordering::Relaxed);
    let mut app = log.viewer(false);
    app.resize(usize::from(ROWS));
    support::poll_until(&mut app, |app| app.progress().is_complete());
    let unfiltered = ALLOCATED.load(Ordering::Relaxed) - before;
    app.set_filters(Filters {
        levels: Levels::only(&[Level::Debug, Level::Info, Level::Warn, Level::Error]),
        ..Filters::default()
    });
    support::poll_until(&mut app, |app| !app.filtering());
    let filtered = ALLOCATED.load(Ordering::Relaxed) - before;
    println!(
        "viewer heap: {:.1} B per entry, {:.1} with a level filter on",
        per_entry(unfiltered),
        per_entry(filtered),
    );
}

/// From starting `script` to the log's last entry on the screen, over a few launches.
fn first_screen(vilog: &str, log: &BenchLog) {
    let last = last_entry_token(log.path());
    let mut times = Vec::new();
    // The first launch isn't counted: macOS checks a freshly built binary then.
    for launch in 0..=LAUNCHES {
        let started = Instant::now();
        let session = Session::start(vilog, log.path());
        let shown = session.wait_for(&last, Duration::from_secs(30));
        if launch > 0 {
            times.push(shown.duration_since(started));
        }
        session.quit();
    }
    times.sort();
    println!(
        "first screen: median {} ms, max {} ms over {LAUNCHES} launches",
        times[LAUNCHES / 2].as_millis(),
        times[LAUNCHES - 1].as_millis(),
    );
}

/// The CPU vilog takes once it has indexed the file and nothing changes.
fn idle_cpu(vilog: &str, log: &BenchLog) {
    let session = Session::start(vilog, log.path());
    session.wait_for(&last_entry_token(log.path()), Duration::from_secs(30));
    let pid = session.vilog_pid();
    wait_until_idle(pid);
    let cpu = cpu_time(pid);
    thread::sleep(IDLE);
    let busy = cpu_time(pid) - cpu;
    println!(
        "idle CPU: {:.2}% over {} s",
        busy.as_secs_f64() / IDLE.as_secs_f64() * 100.0,
        IDLE.as_secs()
    );
    session.quit();
}

/// How long lines appended to the file take to show, written at times spread over
/// the 100 ms the event loop waits between looks. This runs vilog's own event loop and
/// watcher in process, drawing on a [`TestBackend`]: on a real terminal, ratatui only
/// writes the cells that changed, which leaves no text to look for in the output.
fn follow_latency(log: &BenchLog) {
    let mut app = log.viewer(true);
    let file = File::open(log.path()).expect("the log");
    app.watch(log.path(), file, vilog::watch_options(&Config::default()))
        .expect("a watch on the log");
    // Latency while indexing goes on isn't what §13 asks about.
    support::poll_until(&mut app, |app| app.progress().is_complete());
    let source = log.open();
    let preset = support::settings(log.path(), &source).format;
    let generated = source.bytes().len() as u64;
    drop(source);
    let probe = Arc::new(Mutex::new(Probe::default()));
    let backend = Watched {
        inner: TestBackend::new(COLUMNS, ROWS),
        probe: Arc::clone(&probe),
    };
    let mut terminal = Terminal::new(backend).expect("a test terminal");
    let path = log.path().to_owned();
    let appending = Arc::clone(&probe);
    let appender = thread::spawn(move || {
        (0..APPENDS)
            .map(|i| {
                thread::sleep(Duration::from_millis(150 + (i as u64 * 37) % 100));
                let text = format!("req-follow-{i} appended");
                lock(&appending).look_for(&text);
                let written = Instant::now();
                append(&path, &generator::line(preset, &text));
                wait_shown(&appending, Duration::from_secs(5)).duration_since(written)
            })
            .collect::<Vec<_>>()
    });
    let quit = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    vilog::event_loop(
        &mut terminal,
        &mut app,
        Theme::Styled,
        Instant::now,
        |timeout| {
            // Also once the appends failed, which `join` then reports.
            if appender.is_finished() {
                return Ok(Some(Event::Key(quit)));
            }
            // No key comes, so waiting for one is waiting out the timeout.
            thread::sleep(timeout.unwrap_or(Duration::from_millis(100)));
            Ok(None)
        },
    )
    .expect("the event loop");
    let mut latencies = appender.join().expect("the appends");
    latencies.sort();
    println!(
        "follow latency: median {} ms, max {} ms over {APPENDS} appends",
        latencies[APPENDS / 2].as_millis(),
        latencies[APPENDS - 1].as_millis(),
    );
    // A kept log is to stay as generated.
    let file = OpenOptions::new()
        .write(true)
        .open(log.path())
        .expect("the log");
    file.set_len(generated).expect("the appends taken off");
}

/// When the probe's text showed, panicking after `timeout`.
fn wait_shown(probe: &Mutex<Probe>, timeout: Duration) -> Instant {
    let deadline = Instant::now() + timeout;
    loop {
        let probe = lock(probe);
        if let Some(shown) = probe.shown {
            return shown;
        }
        assert!(
            Instant::now() < deadline,
            "{:?} not on screen after {timeout:?}",
            probe.text
        );
        drop(probe);
        thread::sleep(Duration::from_millis(1));
    }
}

/// A text to look out for on the screen, and when it showed.
#[derive(Default)]
struct Probe {
    text: Option<String>,
    shown: Option<Instant>,
}

impl Probe {
    fn look_for(&mut self, text: &str) {
        self.text = Some(text.to_owned());
        self.shown = None;
    }
}

/// A [`TestBackend`] that notes when the probe's text first shows on its screen.
struct Watched {
    inner: TestBackend,
    probe: Arc<Mutex<Probe>>,
}

impl Backend for Watched {
    type Error = <TestBackend as Backend>::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    // `Terminal::draw` flushes once per frame, after drawing it.
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()?;
        let mut probe = lock(&self.probe);
        if probe.shown.is_none()
            && let Some(text) = &probe.text
            && screen_text(self.inner.buffer()).contains(text.as_str())
        {
            probe.shown = Some(Instant::now());
        }
        Ok(())
    }
}

fn screen_text(buffer: &Buffer) -> String {
    buffer.content().iter().map(Cell::symbol).collect()
}

/// Waits until vilog takes less than 100 ms of CPU in a second, a fraction of what
/// indexing takes: the index is complete.
fn wait_until_idle(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut cpu = cpu_time(pid);
    loop {
        assert!(
            Instant::now() < deadline,
            "vilog was still busy after two minutes"
        );
        thread::sleep(Duration::from_secs(1));
        let now = cpu_time(pid);
        if now - cpu < Duration::from_millis(100) {
            return;
        }
        cpu = now;
    }
}

/// `req-<n>` of the log's last entry, as the generator writes it. It is no prefix of
/// another entry's, as none has a higher `n`. It ends before the space after it, as
/// ratatui leaves out unchanged cells, a blank between two words among them.
fn last_entry_token(path: &Path) -> Vec<u8> {
    let mut file = File::open(path).expect("the log");
    let len = file.metadata().expect("the log's size").len();
    // Past the longest line the generator writes.
    let tail_len = len.min(1 << 20);
    file.seek(SeekFrom::Start(len - tail_len)).expect("a seek");
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).expect("the log's end");
    let at = memmem::rfind(&tail, b"req-").expect("a generated log");
    let digits = tail[at + 4..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    // Probe lines left behind by a run that failed would end the log otherwise.
    assert!(digits > 0, "the log doesn't end with a generated entry");
    tail[at..at + 4 + digits].to_vec()
}

fn append(path: &Path, line: &[u8]) {
    let mut file = OpenOptions::new().append(true).open(path).expect("the log");
    file.write_all(line).expect("an append");
}

/// vilog running on a pseudo-terminal of `ROWS` x `COLUMNS`, with everything it draws
/// collected as it comes.
struct Session {
    script: Child,
    keys: ChildStdin,
    screen: Arc<Mutex<Vec<u8>>>,
    /// An empty config dir, so that vilog runs on the defaults rather than the user's
    /// configuration.
    _config: TempDir,
}

impl Session {
    fn start(vilog: &str, log: &Path) -> Session {
        let shell = format!("stty rows {ROWS} cols {COLUMNS}; exec \"$VILOG\" \"$LOG\"");
        let mut command = Command::new("script");
        // BSD and util-linux `script` take the command differently.
        if cfg!(target_os = "linux") {
            command.args(["-qefc", &shell, "/dev/null"]);
        } else {
            command.args(["-q", "/dev/null", "sh", "-c", &shell]);
        }
        let config = TempDir::new().expect("a temp config dir");
        let mut script = command
            .env("XDG_CONFIG_HOME", config.path())
            .env("VILOG", vilog)
            .env("LOG", log)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("script(1) to start");
        let keys = script.stdin.take().expect("piped stdin");
        let mut output = script.stdout.take().expect("piped stdout");
        let screen = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&screen);
        thread::spawn(move || {
            let mut buffer = [0; 1 << 16];
            while let Ok(read @ 1..) = output.read(&mut buffer) {
                lock(&collected).extend_from_slice(&buffer[..read]);
            }
        });
        Session {
            script,
            keys,
            screen,
            _config: config,
        }
    }

    /// When `text` showed up in what vilog drew.
    fn wait_for(&self, text: &[u8], timeout: Duration) -> Instant {
        let deadline = Instant::now() + timeout;
        let shown = String::from_utf8_lossy(text);
        loop {
            if memmem::find(&lock(&self.screen), text).is_some() {
                return Instant::now();
            }
            assert!(
                Instant::now() < deadline,
                "{shown:?} not on screen after {timeout:?}"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// vilog's process, which `script` started and the shell replaced itself with.
    fn vilog_pid(&self) -> u32 {
        let children = Command::new("pgrep")
            .args(["-P", &self.script.id().to_string()])
            .output()
            .expect("pgrep to run");
        String::from_utf8_lossy(&children.stdout)
            .trim()
            .parse()
            .expect("script to have one child")
    }

    fn quit(mut self) {
        self.keys.write_all(b"q").expect("vilog to read keys");
        drop(self.keys);
        self.script.wait().expect("script to end");
    }
}

/// A panic elsewhere fails the run anyway; what it left behind is still worth a look.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The CPU time `pid` has used, as `ps` gives it: `[[dd-]hh:]mm:ss[.ff]`, to the
/// hundredth of a second on macOS, whole seconds on Linux.
fn cpu_time(pid: u32) -> Duration {
    let ps = Command::new("ps")
        .args(["-o", "time=", "-p", &pid.to_string()])
        .output()
        .expect("ps to run");
    let text = String::from_utf8_lossy(&ps.stdout);
    let text = text.trim();
    let (days, clock) = text.split_once('-').unwrap_or(("0", text));
    let seconds = clock.split(':').fold(0.0, |total, part| {
        total * 60.0 + part.parse::<f64>().expect("a number in ps's time")
    });
    let days: f64 = days.parse().expect("a number of days");
    Duration::from_secs_f64(days * 86_400.0 + seconds)
}
