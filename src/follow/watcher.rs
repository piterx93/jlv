//! The thread that watches the followed file and reports what happens to it.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SendError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use super::rotation::{Change, Observation, PREFIX_LEN, Snapshot, Tracker};

#[cfg(test)]
mod tests;

/// What happened to the followed file, in the order it happened. Every length is
/// what the watcher saw of the file, and the next event about the same file starts
/// from there: it has been read up to `len`. A file that didn't change sends nothing.
#[derive(Debug)]
pub enum Event {
    /// The followed file now holds `len` bytes, more than it was read up to; read on
    /// to `len`. Comes for the file followed so far, also right before
    /// [`Gone`](Event::Gone), [`Replaced`](Event::Replaced) or [`Back`](Event::Back),
    /// when it got more bytes since it was last read: its end is read before the
    /// watcher lets go of it.
    Grew { len: u64 },
    /// The followed file was cut short, and maybe written again since. It holds `len`
    /// bytes; read it from the start up to there.
    Truncated { len: u64 },
    /// Another file now lives at the path. `file` is that file, opened, holding `len`
    /// bytes; read it from the start up to there and follow it from now on.
    Replaced { file: File, len: u64 },
    /// Nothing is at the path. What comes next is [`Back`](Event::Back), once a file
    /// is there again, maybe after the old file's last [`Grew`](Event::Grew).
    Gone,
    /// A file is at the path again after [`Gone`](Event::Gone). As with
    /// [`Replaced`](Event::Replaced), `file` is that file, opened, holding `len`
    /// bytes, followed from now on.
    Back { file: File, len: u64 },
}

/// How the watcher finds out that the path may have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Whether to also watch the path's parent directory with `notify`, so that a
    /// change is seen as soon as it happens and not only at the next poll.
    pub notify: bool,
    /// How long the watcher waits between two looks at the path when nothing woke it.
    pub poll: Duration,
}

/// `notify` on, and a look every 250 ms for file systems where it sees nothing,
/// such as NFS (PRD §8).
impl Default for Options {
    fn default() -> Self {
        Self {
            notify: true,
            poll: Duration::from_millis(250),
        }
    }
}

/// Follows a file from a background thread, `tail -F` style, and sends what happens
/// to it as [`Event`]s.
///
/// The thread looks at the path once as it starts, then whenever `notify` reports
/// anything in the parent directory and when [`Options::poll`] has passed since the
/// last look; in between it blocks. Changes that pile up while it is busy reach it as
/// one look. A parent directory that can't be watched leaves it polling. Each look
/// stats the path and reads its first bytes, and [`Tracker`] says what changed.
/// After [`Replaced`](Event::Replaced) and [`Back`](Event::Back) the watcher follows
/// the file it opened, as its open descriptor describes it, whatever was at the path
/// when it looked.
///
/// Dropping the watcher stops the thread without waiting for the next poll.
///
/// [`Tracker`]: super::rotation::Tracker
#[derive(Debug)]
pub struct Watcher {
    events: Receiver<Event>,
    wake: Sender<Wake>,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    /// Starts following `file`, the file the caller opened at `path` and has read
    /// up to `read` bytes. Fails only if `file` can't be inspected.
    pub fn spawn(path: PathBuf, file: File, read: u64, options: Options) -> io::Result<Watcher> {
        let tracker = Tracker::new(snapshot(&file)?);
        let (sender, events) = mpsc::channel();
        let (wake, wakes) = mpsc::channel();
        let follow = Follow {
            path,
            file,
            read,
            tracker,
            gone: false,
            events: sender,
        };
        let notifier = options.notify.then(|| wake.clone());
        let thread = thread::Builder::new()
            .name("jlv-follow".into())
            .spawn(move || follow.run(&wakes, notifier, options.poll))?;
        Ok(Watcher {
            events,
            wake,
            thread: Some(thread),
        })
    }

    /// Where the events arrive. It disconnects only if the thread ends unexpectedly.
    pub fn events(&self) -> &Receiver<Event> {
        &self.events
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // Sending fails only if the thread has already ended, and then there is nothing
        // to stop.
        let _ = self.wake.send(Wake::Stop);
        if let Some(thread) = self.thread.take() {
            // A panic in the thread has nowhere better to go than the dropped events.
            let _ = thread.join();
        }
    }
}

/// How long the thread lets changes pile up after `notify` wakes it. The deadline
/// doesn't move with later wakeups, so a file that is written to all the time is
/// still looked at this often.
const SETTLE: Duration = Duration::from_millis(10);

/// What the thread is woken for.
enum Wake {
    Look,
    Stop,
}

/// The thread's side of the watcher: the file it follows and how far it was read.
struct Follow {
    path: PathBuf,
    /// The watcher's own descriptor of the followed file: it shows how far the file
    /// grew after it left the path, and keeps its inode from being reused meanwhile.
    file: File,
    read: u64,
    tracker: Tracker,
    /// Whether [`Event::Gone`] was sent and no file came back since.
    gone: bool,
    events: Sender<Event>,
}

impl Follow {
    fn run(mut self, wakes: &Receiver<Wake>, notifier: Option<Sender<Wake>>, poll: Duration) {
        // Watching starts before the first look, so that no change slips in between.
        let _notify = notifier.and_then(|wake| watch_parent(&self.path, wake));
        loop {
            if self.look().is_err() {
                return;
            }
            match wakes.recv_timeout(poll) {
                Ok(Wake::Look) => {
                    if !settle(wakes) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Ok(Wake::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Fails only when nobody listens to the events any more.
    fn look(&mut self) -> Result<(), SendError<Event>> {
        // A path that can't be opened for another reason than being missing, such as
        // a permission being taken away, is looked at again on the next wakeup.
        let Ok(found) = open(&self.path) else {
            return Ok(());
        };
        let Some((file, seen)) = found else {
            return self.missing();
        };
        let len = seen.size;
        let mut tracker = self.tracker.clone();
        let change = tracker.observe(&Observation::Present(seen), self.read);
        let event = match change {
            Change::Unchanged => None,
            Change::Grew => Some(Event::Grew { len }),
            Change::Truncated => Some(Event::Truncated { len }),
            Change::Replaced | Change::Back => {
                // Without a descriptor of its own the watcher can't follow the new file,
                // so it leaves the tracker as it was and tries again on the next look.
                let Ok(own) = file.try_clone() else {
                    return Ok(());
                };
                self.drain()?;
                self.file = own;
                self.gone = false;
                Some(if change == Change::Back {
                    Event::Back { file, len }
                } else {
                    Event::Replaced { file, len }
                })
            }
            Change::Gone => unreachable!("a file was found at the path"),
        };
        self.tracker = tracker;
        match event {
            Some(event) => {
                self.read = len;
                self.events.send(event)
            }
            None => Ok(()),
        }
    }

    fn missing(&mut self) -> Result<(), SendError<Event>> {
        self.tracker.observe(&Observation::Missing, self.read);
        if self.gone {
            return Ok(());
        }
        self.gone = true;
        self.drain()?;
        self.events.send(Event::Gone)
    }

    /// Reports the bytes the followed file got since it was last read, before the
    /// watcher lets go of it.
    fn drain(&mut self) -> Result<(), SendError<Event>> {
        // A descriptor that can't be inspected can't be read from either.
        let Ok(len) = self.file.metadata().map(|meta| meta.len()) else {
            return Ok(());
        };
        if len <= self.read {
            return Ok(());
        }
        self.read = len;
        self.events.send(Event::Grew { len })
    }
}

/// Takes in the wakeups that come for [`SETTLE`] after one, so that a burst of writes
/// is read in one look rather than one per write. `false` if the thread is to stop.
fn settle(wakes: &Receiver<Wake>) -> bool {
    let deadline = Instant::now() + SETTLE;
    loop {
        match wakes.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Wake::Look) => {}
            Err(RecvTimeoutError::Timeout) => return true,
            Ok(Wake::Stop) | Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

/// Watches the directory `path` lives in, so that `wake` hears of any change in it.
/// `None` if it can't be watched, which leaves the thread to polling.
fn watch_parent(path: &Path, wake: Sender<Wake>) -> Option<RecommendedWatcher> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let name = path.file_name().map(OsString::from);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if concerns(&event, name.as_deref()) {
            // Fails only once the thread has stopped, when there is nothing left to wake.
            let _ = wake.send(Wake::Look);
        }
    })
    .ok()?;
    watcher.watch(parent, RecursiveMode::NonRecursive).ok()?;
    Some(watcher)
}

/// Whether `event` may have changed the file called `name`. Reads are left out: the
/// thread's own look opens the file, and on Linux that is an event too, which would
/// wake the thread again and again. An error or a lost event may hide anything.
fn concerns(event: &notify::Result<notify::Event>, name: Option<&OsStr>) -> bool {
    let Ok(event) = event else {
        return true;
    };
    if event.need_rescan() {
        return true;
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    // Comparing names rather than whole paths also catches either end of a rename,
    // and doesn't trip over how the path was spelled when it was given.
    name.is_none_or(|name| {
        event
            .paths
            .iter()
            .any(|path| path.file_name() == Some(name))
    })
}

/// The file at `path`, opened, and what it looks like; `None` if nothing is there.
fn open(path: &Path) -> io::Result<Option<(File, Snapshot)>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let seen = snapshot(&file)?;
    Ok(Some((file, seen)))
}

fn snapshot(file: &File) -> io::Result<Snapshot> {
    let meta = file.metadata()?;
    let mut prefix = vec![0; meta.len().min(PREFIX_LEN as u64) as usize];
    let mut filled = 0;
    while filled < prefix.len() {
        match file.read_at(&mut prefix[filled..], filled as u64) {
            // Cut short since the stat: the prefix is what is left.
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    prefix.truncate(filled);
    Ok(Snapshot {
        inode: meta.ino(),
        size: meta.len(),
        prefix,
    })
}
