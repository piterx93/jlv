//! The heap the entry index takes per entry, PRD §13's ~32 bytes: what its `Vec`
//! allocates, room for `capacity` records. And how it gets there: growing rarely and
//! by large steps, so that a big file's records aren't copied over and over. An
//! allocator counts the growths the polling thread makes.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::Write;
use std::mem;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use vilog::format::Anchor;
use vilog::index::{Indexer, Record, Settings};

mod common;

use common::built_in;

#[global_allocator]
static HEAP: Counting = Counting;

thread_local! {
    /// Whether this thread's allocations are counted: only while it polls.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    /// The allocations and growing reallocations counted so far.
    static GROWTHS: Cell<usize> = const { Cell::new(0) };
    /// The smallest share of its new size a counted reallocation added.
    static SMALLEST_STEP: Cell<f64> = const { Cell::new(1.0) };
}

fn counting() -> bool {
    COUNTING.try_with(Cell::get).unwrap_or(false)
}

fn count(old: usize, new: usize) {
    GROWTHS.set(GROWTHS.get() + 1);
    let step = (new - old) as f64 / new as f64;
    SMALLEST_STEP.set(SMALLEST_STEP.get().min(step));
}

struct Counting;

// SAFETY: every call goes to `System` as it came; the counter only watches.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if counting() {
            count(0, layout.size());
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        unsafe { System.dealloc(block, layout) }
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if counting() && size > layout.size() {
            count(layout.size(), size);
        }
        unsafe { System.realloc(block, layout, size) }
    }
}

/// Just past a size where growing by doubling leaves almost half of the room unused.
const ENTRIES: usize = 330_000;

/// A Logback log of `ENTRIES` one-line entries of about 150 bytes, a few bytes
/// shorter towards the end, so that the end holds a few more entries per byte
/// than the start.
fn log() -> Vec<u8> {
    let mut log = Vec::new();
    for n in 0..ENTRIES {
        let (seconds, millis) = (n / 1000, n % 1000);
        writeln!(
            log,
            "2026-10-07 {:02}:{:02}:{:02}.{millis:03} [http-nio-8080-exec-3] INFO  \
             com.example.order.OrderService - Order {n} created{:padding$}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            "",
            padding = 40 - n * 4 / ENTRIES,
        )
        .unwrap();
    }
    log
}

/// The index of [`log`] and how it grew while polled.
struct Grown {
    records: Vec<Record>,
    growths: usize,
    smallest_step: f64,
}

fn index() -> Grown {
    let zone = TimeZone::UTC;
    let settings = Settings {
        format: built_in("logback"),
        anchor: Anchor::for_file(Path::new("app.log"), Timestamp::UNIX_EPOCH, &zone),
        zone,
    };
    let mut indexer = Indexer::spawn(Arc::new(log()), settings);
    GROWTHS.set(0);
    SMALLEST_STEP.set(1.0);
    let mut records = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        COUNTING.set(true);
        let progress = indexer.poll(&mut records);
        COUNTING.set(false);
        if progress.is_complete() {
            break;
        }
        assert!(Instant::now() < deadline, "indexing took over 30 s");
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(records.len(), ENTRIES);
    Grown {
        records,
        growths: GROWTHS.get(),
        smallest_step: SMALLEST_STEP.get(),
    }
}

#[test]
fn index_takes_at_most_36_bytes_per_entry() {
    let records = index().records;
    let allocated = records.capacity() * mem::size_of::<Record>();
    let per_entry = allocated as f64 / ENTRIES as f64;
    assert!(per_entry <= 36.0, "{per_entry:.1} B per entry");
}

#[test]
fn index_grows_no_more_often_than_doubling_would() {
    let growths = index().growths;
    assert!(growths <= ENTRIES.ilog2() as usize, "{growths} growths");
}

#[test]
fn each_growth_adds_at_least_a_sixty_fourth_of_the_index() {
    let step = index().smallest_step;
    assert!(step >= 1.0 / 64.0, "a growth added {step:.4} of the index");
}
