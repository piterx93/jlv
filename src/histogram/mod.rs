//! The histogram of warnings and errors over time (PRD §7.1): the WARN, ERROR and
//! FATAL entries counted per time bucket.
//!
//! Buckets are aligned to their size on the clock of a zone: a bucket of 5 minutes
//! starts at a multiple of 5 minutes after 1970-01-01 00:00 as that clock reads it,
//! the zone's offset being the one it has at the earliest entry. So 1 m buckets start
//! on the minute, 1 d buckets at midnight.
//!
//! A [`Counter`] takes in entries one at a time, in any order, so that new entries
//! only cost what it takes to add them. The buckets are worked out from it for the
//! width there is to show them in.

use std::collections::BTreeMap;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use crate::filter::TimeRange;
use crate::format::Level;

#[cfg(test)]
mod tests;

/// The bucket sizes, smallest first: 1s 5s 10s 30s 1m 5m 10m 30m 1h 6h 1d.
pub const SIZES: [SignedDuration; 11] = [
    SignedDuration::from_secs(1),
    SignedDuration::from_secs(5),
    SignedDuration::from_secs(10),
    SignedDuration::from_secs(30),
    SignedDuration::from_secs(60),
    SignedDuration::from_secs(5 * 60),
    SignedDuration::from_secs(10 * 60),
    SignedDuration::from_secs(30 * 60),
    SignedDuration::from_secs(3600),
    SignedDuration::from_secs(6 * 3600),
    SignedDuration::from_secs(86_400),
];

/// The size of the buckets that show the time from `first` to `last` in at most
/// `width` aligned buckets: the smallest of [`SIZES`] that does, or, when even 1 d
/// buckets are too many, the smallest whole number of days that does. The buckets
/// that hold `first` and `last` and every one between them count, so a range that
/// straddles a bucket's edge needs two. A width of 0 counts as 1.
pub fn bucket_size(
    first: Timestamp,
    last: Timestamp,
    width: usize,
    zone: &TimeZone,
) -> SignedDuration {
    let width = i64::try_from(width.max(1)).unwrap_or(i64::MAX);
    let offset = offset_at(first, zone);
    let (first, last) = (seconds(first) + offset, seconds(last) + offset);
    let buckets = |size: i64| last.div_euclid(size) - first.div_euclid(size) + 1;
    if let Some(size) = SIZES.iter().find(|size| buckets(size.as_secs()) <= width) {
        return *size;
    }
    // `n` buckets of `k` days span more than `(n - 1) * k` days, so no `k` up to
    // that many days divided by the width fits.
    let days = (last - first) / DAY;
    let fits = |k: &i64| buckets(k * DAY) <= width;
    let k = (days / width).max(1);
    let k = (k..).find(fits).unwrap_or(k);
    SignedDuration::from_secs(k * DAY)
}

const DAY: i64 = 86_400;

/// Whole seconds since the epoch, rounded down.
fn seconds(at: Timestamp) -> i64 {
    at.as_microsecond().div_euclid(1_000_000)
}

fn offset_at(at: Timestamp, zone: &TimeZone) -> i64 {
    i64::from(zone.to_offset(at).seconds())
}

/// WARN, ERROR and FATAL entries per time bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    /// Where the first bucket starts.
    pub start: Timestamp,
    /// How long every bucket is, as [`bucket_size`] says.
    pub size: SignedDuration,
    /// Per bucket, from the first: the WARN, ERROR and FATAL entries in it. The
    /// buckets run from the one holding the earliest entry to the one holding the
    /// latest, so there is at least one.
    pub counts: Vec<u32>,
}

impl Histogram {
    /// What bucket `bucket` covers: from its start, inclusive, to the start of the
    /// bucket after it, exclusive.
    pub fn range(&self, bucket: usize) -> TimeRange {
        let edge = |n: usize| {
            let n = i32::try_from(n).unwrap_or(i32::MAX);
            let after = self.size.saturating_mul(n);
            self.start.checked_add(after).unwrap_or(Timestamp::MAX)
        };
        TimeRange {
            start: edge(bucket),
            end: Some(edge(bucket + 1)),
        }
    }
}

/// Entries taken in so far, ready to be put into buckets.
#[derive(Debug, Clone, Default)]
pub struct Counter {
    /// The earliest and the latest entry, whatever their level.
    span: Option<(Timestamp, Timestamp)>,
    /// Per second, the WARN, ERROR and FATAL entries in it. Only seconds that have
    /// one are kept, so its size follows the warnings, not the file.
    per_second: BTreeMap<i64, u32>,
}

impl Counter {
    /// Takes in an entry with a timestamp. Entries of every level, and without one,
    /// widen the time the histogram spans; only WARN, ERROR and FATAL are counted.
    pub fn add(&mut self, at: Timestamp, level: Option<Level>) {
        self.span = Some(match self.span {
            Some((first, last)) => (first.min(at), last.max(at)),
            None => (at, at),
        });
        if level.is_some_and(|level| level >= Level::Warn) {
            let count = self.per_second.entry(seconds(at)).or_default();
            *count = count.saturating_add(1);
        }
    }

    /// Takes out an entry taken in with [`add`](Self::add), which the index read again.
    /// The span it widened stays; that is harmless.
    pub fn remove(&mut self, at: Timestamp, level: Option<Level>) {
        let second = seconds(at);
        if !level.is_some_and(|level| level >= Level::Warn) {
            return;
        }
        if let Some(count) = self.per_second.get_mut(&second) {
            *count -= 1;
            if *count == 0 {
                self.per_second.remove(&second);
            }
        }
    }

    /// Whether no entry with a timestamp was taken in.
    pub fn is_empty(&self) -> bool {
        self.span.is_none()
    }

    /// The histogram of the entries taken in, in at most `width` buckets aligned on
    /// the clock of `zone` (see [`bucket_size`]), from the earliest entry to the
    /// latest. `None` before the first entry.
    pub fn histogram(&self, width: usize, zone: &TimeZone) -> Option<Histogram> {
        let (first, last) = self.span?;
        let size = bucket_size(first, last, width, zone);
        let (size_secs, offset) = (size.as_secs(), offset_at(first, zone));
        let bucket_of = |second: i64| (second + offset).div_euclid(size_secs);
        let first_bucket = bucket_of(seconds(first));
        let len = bucket_of(seconds(last)) - first_bucket + 1;
        let mut counts = vec![0; usize::try_from(len).ok()?];
        for (&second, &count) in &self.per_second {
            counts[usize::try_from(bucket_of(second) - first_bucket).ok()?] += count;
        }
        let start = Timestamp::from_second(first_bucket * size_secs - offset).ok()?;
        Some(Histogram {
            start,
            size,
            counts,
        })
    }
}
