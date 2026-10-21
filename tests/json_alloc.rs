//! The JSON key scanner allocates nothing (PRD §14): an allocator counts what the
//! scanning thread allocates.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use vilog::format::json;

#[global_allocator]
static HEAP: Counting = Counting;

thread_local! {
    /// Allocations this thread has made so far.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn allocations() -> usize {
    ALLOCATIONS.try_with(Cell::get).unwrap_or(0)
}

struct Counting;

// SAFETY: every call goes to `System` as it came; the counter only watches.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        unsafe { System.dealloc(block, layout) }
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        unsafe { System.realloc(block, layout, size) }
    }
}

#[test]
fn scanning_a_line_allocates_nothing() {
    let line = br#" { "time" : "2026-10-07T12:00:00Z", "msg":"a \"quoted\" \\ text", "tags":["a",{"b":[1,2]}], "level":30 } "#;
    let before = allocations();
    let keys = json::fields(line).count();
    let level = json::find(line, b"level");
    let after = allocations();
    assert_eq!(after - before, 0);
    assert_eq!(keys, 4);
    assert!(level.is_some());
}
