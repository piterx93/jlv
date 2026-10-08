//! Turning the entries' timestamps into instants: a date for the formats that only
//! write the time of day, and a timestamp for entries that have none.

use std::cmp::Ordering;
use std::path::Path;

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use crate::format::LogTime;
use crate::format::time::{date, local_to_instant};

#[cfg(test)]
mod tests;

/// Where dating a time-only log starts from: what is known about when its last entry
/// was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    /// A date in the file name: the date of the last entry.
    Named(Date),
    /// The file's mtime as local date and time: a virtual entry after the last one.
    /// Nothing is written after it, so the last entry is on its date or the day
    /// before, never after.
    Modified(DateTime),
}

impl Anchor {
    /// The anchor of the file at `path`: the last valid `YYYY-MM-DD` in the file name
    /// (`server.log.2026-10-06`, `app-2026-10-06.log.gz`), else `mtime` in `zone`.
    /// Only the file name counts, not the directories above it, and an impossible
    /// date such as `2026-02-30` is no date.
    pub fn for_file(path: &Path, mtime: Timestamp, zone: &TimeZone) -> Anchor {
        path.file_name()
            .and_then(|name| last_date_in(name.as_encoded_bytes()))
            .map_or_else(|| Anchor::Modified(zone.to_datetime(mtime)), Anchor::Named)
    }
}

fn last_date_in(name: &[u8]) -> Option<Date> {
    const LEN: usize = "YYYY-MM-DD".len();
    name.windows(LEN).rev().find_map(date)
}

/// How many midnights lie between an entry at `earlier` and the next one at `later`,
/// so that `later` is on the date of `earlier` plus this many days:
///
/// - `1` when `earlier` is more than 2 h after `later`: midnight was crossed. A
///   smaller step back is out-of-order logging or the repeated hour of a DST
///   fall-back.
/// - `-1` when `earlier` is more than 23 h 59 min before `later`: `later` was stamped
///   just before midnight by a late thread and written after `earlier`, which was
///   stamped just after it. Out-of-order logging is assumed to stay under a minute.
/// - `0` otherwise.
pub fn midnights_between(earlier: Time, later: Time) -> i8 {
    let step = later.duration_since(earlier);
    if step < -LONGEST_STEP_BACK {
        1
    } else if step > LONGEST_STEP_FORWARD {
        -1
    } else {
        0
    }
}

const LONGEST_STEP_BACK: SignedDuration = SignedDuration::from_hours(2);
const LONGEST_STEP_FORWARD: SignedDuration = SignedDuration::from_mins(23 * 60 + 59);

/// Dates the times of day of a log from its last entry back, one entry at a time.
/// Entries without a time of day are not fed to it.
#[derive(Debug, Clone)]
pub struct DatesBackward {
    later: Later,
}

/// What the next entry fed to [`DatesBackward`] is dated from.
#[derive(Debug, Clone)]
enum Later {
    Anchor(Anchor),
    Entry(DateTime),
}

impl DatesBackward {
    pub fn new(anchor: Anchor) -> Self {
        DatesBackward {
            later: Later::Anchor(anchor),
        }
    }

    /// The date of the entry at `time`, which comes just before the one fed last (for
    /// the first call: the last entry of the log).
    ///
    /// - After [`Anchor::Named`], the last entry is on that date.
    /// - After [`Anchor::Modified`], the last entry is dated as if the mtime were an
    ///   entry after it, except that it is never a late line: the last entry is on
    ///   the mtime's date or the day before.
    /// - Every other entry is dated from the one after it by [`midnights_between`].
    pub fn date(&mut self, time: Time) -> Date {
        let date = match self.later {
            Later::Anchor(Anchor::Named(date)) => date,
            Later::Anchor(Anchor::Modified(mtime)) => {
                // Nothing is written after the mtime, so it is never a late line.
                back_by(mtime.date(), midnights_between(time, mtime.time()).max(0))
            }
            Later::Entry(later) => back_by(later.date(), midnights_between(time, later.time())),
        };
        self.later = Later::Entry(date.to_datetime(time));
        date
    }
}

/// `date` moved back by `midnights` as [`midnights_between`] counts them, which is
/// at most one either way. Where that would leave jiff's range it stays: a log that
/// far out has no right date anyway.
fn back_by(date: Date, midnights: i8) -> Date {
    let shifted = match midnights.cmp(&0) {
        Ordering::Greater => date.yesterday(),
        Ordering::Less => date.tomorrow(),
        Ordering::Equal => Ok(date),
    };
    shifted.unwrap_or(date)
}

/// Every entry's timestamp as an instant, `times` in file order.
///
/// - An `Instant` stays as it is.
/// - A `TimeOfDay` gets a date from [`DatesBackward`] started at `anchor`. Date and
///   time are local time in `zone`, read as [`EntryParser::timestamp`] reads a
///   timestamp without offset.
/// - `None` takes the timestamp of the entry before it; the entries before the first
///   timestamp stay `None`.
///
/// [`EntryParser::timestamp`]: crate::format::EntryParser::timestamp
pub fn resolve_timestamps(
    times: &[Option<LogTime>],
    anchor: Anchor,
    zone: &TimeZone,
) -> Vec<Option<Timestamp>> {
    let mut dates = DatesBackward::new(anchor);
    let mut resolved = vec![None; times.len()];
    for (time, slot) in times.iter().zip(&mut resolved).rev() {
        *slot = match *time {
            Some(LogTime::Instant(instant)) => Some(instant),
            Some(LogTime::TimeOfDay(time)) => {
                local_to_instant(dates.date(time).to_datetime(time), zone)
            }
            None => None,
        };
    }
    let mut previous = None;
    for slot in &mut resolved {
        *slot = slot.or(previous);
        previous = *slot;
    }
    resolved
}
