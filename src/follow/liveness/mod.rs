//! How recently the followed file changed, as the dot in the header shows it.

use std::time::Duration;

const QUIET_AFTER: Duration = Duration::from_secs(10);
const IDLE_AFTER: Duration = Duration::from_secs(300);

#[cfg(test)]
mod tests;

/// How recently the file changed: it grew, was cut short or another file took its
/// place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// Changed less than 10 s ago: a green `●`.
    Live,
    /// Changed from 10 s to less than 5 min ago: a yellow `◐`.
    Quiet,
    /// Unchanged for 5 min or more, a gzip file, or a file gone from its path: a grey
    /// `○`.
    Idle,
}

impl Liveness {
    /// The liveness of a file whose last change was `elapsed` ago.
    pub fn since(elapsed: Duration) -> Self {
        if elapsed < QUIET_AFTER {
            Self::Live
        } else if elapsed < IDLE_AFTER {
            Self::Quiet
        } else {
            Self::Idle
        }
    }

    /// How much longer after `elapsed` the liveness of a file whose last change was
    /// `elapsed` ago stays the same, if nothing changes; `None` once it is
    /// [`Idle`](Self::Idle), which lasts.
    pub fn changes_in(elapsed: Duration) -> Option<Duration> {
        match Self::since(elapsed) {
            Self::Live => Some(QUIET_AFTER - elapsed),
            Self::Quiet => Some(IDLE_AFTER - elapsed),
            Self::Idle => None,
        }
    }
}
