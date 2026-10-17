//! Reading a mapped file that is truncated while it is read.
//!
//! Touching a page of a mapping past the end of its file raises SIGBUS, and a log file
//! is shortened under its readers sooner or later: `copytruncate`, `> app.log`. The
//! handler installed here maps zero-filled pages over the rest of the mapping instead,
//! so the reader that hit the fault carries on reading zeros, garbled text at worst,
//! until the watcher sees the truncation and the file is mapped again. A fault outside
//! every registered mapping goes to the handler that was there before.

use std::ffi::{c_int, c_void};
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Once, OnceLock};

use memmap2::Mmap;

/// A mapping whose faults past the end of the file read as zeros for as long as it
/// lives.
#[derive(Debug)]
pub(super) struct Guarded {
    map: Mmap,
    slot: Option<&'static Slot>,
}

impl Guarded {
    pub(super) fn new(map: Mmap) -> Self {
        install();
        let slot = register(map.as_ptr() as usize, map.len());
        Self { map, slot }
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.map
    }
}

impl Drop for Guarded {
    fn drop(&mut self) {
        // Runs before the map is unmapped, so the handler never sees a mapping after
        // it is unmapped.
        if let Some(slot) = self.slot {
            slot.start.store(0, Ordering::Release);
            slot.len.store(0, Ordering::Release);
        }
    }
}

/// One mapping the handler knows: `len` is non-zero while the slot is taken, `start`
/// while the mapping is guarded.
#[derive(Debug)]
struct Slot {
    start: AtomicUsize,
    len: AtomicUsize,
}

/// vilog maps one file at a time, two while one replaces the other; the tests map many
/// at once. A mapping that finds no free slot is not guarded.
static SLOTS: [Slot; 256] = [const {
    Slot {
        start: AtomicUsize::new(0),
        len: AtomicUsize::new(0),
    }
}; 256];

static PAGE: AtomicUsize = AtomicUsize::new(0);

static PREVIOUS: OnceLock<libc::sigaction> = OnceLock::new();

fn register(start: usize, len: usize) -> Option<&'static Slot> {
    // A file truncated to nothing just before it was mapped maps as empty, and a zero
    // length would leave the slot free for the next mapping. No byte of it is read.
    if len == 0 {
        return None;
    }
    let slot = SLOTS.iter().find(|slot| {
        slot.len
            .compare_exchange(0, len, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    })?;
    // `len` is stored first, so the handler never sees a start with a stale length.
    slot.start.store(start, Ordering::Release);
    Some(slot)
}

/// The registered mapping that holds `addr`, as its start and length.
fn mapping_at(addr: usize) -> Option<(usize, usize)> {
    SLOTS.iter().find_map(|slot| {
        let start = slot.start.load(Ordering::Acquire);
        let len = slot.len.load(Ordering::Relaxed);
        (start != 0 && (start..start + len).contains(&addr)).then_some((start, len))
    })
}

fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // SAFETY: sysconf and sigaction are given valid arguments, and the handler only
        // makes calls that are safe in a signal handler.
        unsafe {
            PAGE.store(
                libc::sysconf(libc::_SC_PAGESIZE) as usize,
                Ordering::Relaxed,
            );
            let mut previous: libc::sigaction = mem::zeroed();
            libc::sigaction(libc::SIGBUS, ptr::null(), &mut previous);
            PREVIOUS.get_or_init(|| previous);
            let mut action: libc::sigaction = mem::zeroed();
            action.sa_sigaction = on_sigbus as *const () as libc::sighandler_t;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(libc::SIGBUS, &action, ptr::null_mut());
        }
    });
}

extern "C" fn on_sigbus(signal: c_int, info: *mut libc::siginfo_t, _: *mut c_void) {
    // SAFETY: the kernel hands a valid siginfo for SIGBUS, whose address is the fault's.
    let addr = unsafe { (*info).si_addr() } as usize;
    if let Some((start, len)) = mapping_at(addr) {
        let from = addr & !(PAGE.load(Ordering::Relaxed) - 1);
        // SAFETY: the faulting thread borrows the mapping's bytes, so it can't be dropped
        // and unmapped meanwhile. Its pages are only ever read, and zeros are as good as
        // any other bytes a rewrite could leave there.
        let zeros = unsafe {
            libc::mmap(
                from as *mut c_void,
                start + len - from,
                libc::PROT_READ,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED,
                -1,
                0,
            )
        };
        if zeros != libc::MAP_FAILED {
            return;
        }
    }
    // Not ours, or the zeros couldn't be mapped: once the previous handler is back, the
    // access faults again and that handler, or the default, deals with it.
    if let Some(previous) = PREVIOUS.get() {
        // SAFETY: `previous` is what sigaction reported for SIGBUS.
        unsafe { libc::sigaction(signal, previous, ptr::null_mut()) };
    }
}
