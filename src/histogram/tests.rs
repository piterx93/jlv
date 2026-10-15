use jiff::tz::{self, TimeZone};
use jiff::{SignedDuration, Timestamp};
use proptest::collection::vec;
use proptest::prelude::*;

use super::*;
use crate::filter::TimeRange;
use crate::format::Level;

fn at(time: &str) -> Timestamp {
    time.parse().unwrap()
}

fn utc() -> TimeZone {
    TimeZone::UTC
}

fn minutes(n: i64) -> SignedDuration {
    SignedDuration::from_mins(n)
}

/// A counter that took in `entries`, given as UTC times and levels.
fn counted(entries: &[(&str, Option<Level>)]) -> Counter {
    let mut counter = Counter::default();
    for &(time, level) in entries {
        counter.add(at(time), level);
    }
    counter
}

/// The histogram of `entries` in at most 60 buckets in UTC.
fn histogram(entries: &[(&str, Option<Level>)]) -> Histogram {
    counted(entries).histogram(60, &utc()).unwrap()
}

// --- bucket size ---

#[test]
fn one_second_buckets_while_the_seconds_fit_the_width() {
    let size = bucket_size(
        at("2026-10-07T10:00:00Z"),
        at("2026-10-07T10:00:59.999Z"),
        60,
        &utc(),
    );
    assert_eq!(size, SignedDuration::from_secs(1));
}

#[test]
fn bucket_size_is_the_smallest_that_fits_the_width() {
    // 1 m buckets would take 120 columns.
    let size = bucket_size(
        at("2026-10-07T10:00:00Z"),
        at("2026-10-07T11:59:00Z"),
        60,
        &utc(),
    );
    assert_eq!(size, minutes(5));
}

#[test]
fn a_range_across_a_bucket_edge_needs_a_bucket_on_either_side() {
    // Every size up to 5 m puts the two ends in buckets of their own.
    let size = bucket_size(
        at("2026-10-07T10:04:59Z"),
        at("2026-10-07T10:05:00Z"),
        1,
        &utc(),
    );
    assert_eq!(size, minutes(10));
}

#[test]
fn thirty_minute_buckets_fit_an_hour_into_two_columns() {
    let size = bucket_size(
        at("2026-10-07T10:00:00Z"),
        at("2026-10-07T10:59:59Z"),
        2,
        &utc(),
    );
    assert_eq!(size, minutes(30));
}

#[test]
fn the_zone_clock_decides_which_buckets_the_ends_fall_in() {
    // 12:00 and 17:00 at +02:00 share a 6 h bucket; 10:00 and 15:00 in UTC don't.
    let size = bucket_size(
        at("2026-10-07T10:00:00Z"),
        at("2026-10-07T15:00:00Z"),
        1,
        &TimeZone::fixed(tz::offset(2)),
    );
    assert_eq!(size, SignedDuration::from_hours(6));
}

#[test]
fn ends_on_either_side_of_midnight_on_the_zone_clock_need_merged_days() {
    // 23:30 and 00:30 at +02:00, on days 20733 and 20734 after the epoch, which only
    // share a bucket of 3 days.
    let size = bucket_size(
        at("2026-10-07T21:30:00Z"),
        at("2026-10-07T22:30:00Z"),
        1,
        &TimeZone::fixed(tz::offset(2)),
    );
    assert_eq!(size, SignedDuration::from_hours(3 * 24));
}

#[test]
fn days_merge_when_even_one_day_buckets_outnumber_the_width() {
    let size = bucket_size(
        at("2020-01-01T00:00:00Z"),
        at("2026-01-01T00:00:00Z"),
        60,
        &utc(),
    );
    assert_eq!(size, SignedDuration::from_hours(37 * 24));
}

#[test]
fn days_merge_into_the_fewest_days_that_fit() {
    // 200 days in 100 columns.
    let size = bucket_size(
        at("2020-01-01T00:00:00Z"),
        at("2020-07-18T00:00:00Z"),
        100,
        &utc(),
    );
    assert_eq!(size, SignedDuration::from_hours(2 * 24));
}

#[test]
fn buckets_start_at_a_multiple_of_their_size_on_the_zone_clock() {
    let zone = TimeZone::fixed(tz::offset(2));
    let counter = counted(&[
        ("2026-10-07T23:00:00Z", Some(Level::Warn)),
        ("2026-11-30T12:00:00Z", Some(Level::Warn)),
    ]);
    let histogram = counter.histogram(60, &zone).unwrap();
    assert_eq!(histogram.size, SignedDuration::from_hours(24));
    // Midnight at +02:00, which is 2026-10-08 there.
    assert_eq!(histogram.start, at("2026-10-07T22:00:00Z"));
}

// --- counting ---

#[test]
fn counts_warn_error_and_fatal_per_bucket() {
    let mut entries: Vec<_> = Level::ALL
        .iter()
        .map(|&level| ("2026-10-07T10:00:00Z", Some(level)))
        .collect();
    entries.push(("2026-10-07T10:00:00Z", None));
    entries.push(("2026-10-07T10:00:02Z", Some(Level::Warn)));
    assert_eq!(histogram(&entries).counts, [3, 0, 1]);
}

#[test]
fn entries_below_warn_widen_the_histogram() {
    let histogram = histogram(&[
        ("2026-10-07T10:00:00Z", Some(Level::Info)),
        ("2026-10-07T10:00:03Z", Some(Level::Error)),
        ("2026-10-07T10:00:05Z", Some(Level::Debug)),
    ]);
    assert_eq!(histogram.start, at("2026-10-07T10:00:00Z"));
    assert_eq!(histogram.counts, [0, 0, 0, 1, 0, 0]);
}

#[test]
fn entries_may_come_in_any_order() {
    let early = ("2026-10-07T10:00:00Z", Some(Level::Info));
    let late = ("2026-10-07T10:00:02Z", Some(Level::Warn));
    assert_eq!(histogram(&[late, early]), histogram(&[early, late]));
}

#[test]
fn entries_sharing_one_timestamp_make_one_bucket() {
    let time = "2026-10-07T10:00:00.500Z";
    let histogram = histogram(&[
        (time, Some(Level::Warn)),
        (time, Some(Level::Info)),
        (time, Some(Level::Error)),
        (time, Some(Level::Warn)),
    ]);
    assert_eq!(
        histogram,
        Histogram {
            start: at("2026-10-07T10:00:00Z"),
            size: SignedDuration::from_secs(1),
            counts: vec![3],
        }
    );
}

#[test]
fn a_single_entry_makes_one_bucket_of_a_second() {
    let histogram = histogram(&[("2026-10-07T10:00:07.250Z", Some(Level::Fatal))]);
    assert_eq!(
        histogram,
        Histogram {
            start: at("2026-10-07T10:00:07Z"),
            size: SignedDuration::from_secs(1),
            counts: vec![1],
        }
    );
}

#[test]
fn removing_entries_takes_them_out_of_their_buckets() {
    let mut counter = counted(&[
        ("2026-10-07T10:00:00Z", Some(Level::Warn)),
        ("2026-10-07T10:00:02Z", Some(Level::Error)),
        ("2026-10-07T10:00:02Z", Some(Level::Error)),
    ]);
    counter.remove(at("2026-10-07T10:00:00Z"), Some(Level::Warn));
    counter.remove(at("2026-10-07T10:00:02Z"), Some(Level::Error));
    let histogram = counter.histogram(60, &utc()).unwrap();
    assert_eq!(histogram.counts, [0, 0, 1]);
}

#[test]
fn no_entries_make_no_histogram() {
    assert_eq!(Counter::default().histogram(60, &utc()), None);
}

#[test]
fn a_bucket_covers_its_start_up_to_the_next_bucket() {
    let histogram = Histogram {
        start: at("2026-10-07T10:00:00Z"),
        size: minutes(5),
        counts: vec![0, 4, 1],
    };
    assert_eq!(
        histogram.range(2),
        TimeRange {
            start: at("2026-10-07T10:10:00Z"),
            end: Some(at("2026-10-07T10:15:00Z")),
        }
    );
}

// --- properties ---

/// Entries as milliseconds after 2026-01-01 over some three years, and a level, `None`
/// for the last index.
fn entries() -> impl Strategy<Value = Vec<(i64, Option<Level>)>> {
    let level = (0..=Level::ALL.len()).prop_map(|i| Level::ALL.get(i).copied());
    vec((0..100_000_000_000i64, level), 1..200)
}

fn counter_of(entries: &[(i64, Option<Level>)]) -> Counter {
    let base = at("2026-01-01T00:00:00Z");
    let mut counter = Counter::default();
    for &(millis, level) in entries {
        counter.add(base + SignedDuration::from_millis(millis), level);
    }
    counter
}

proptest! {
    #[test]
    fn counts_sum_to_the_warn_error_and_fatal_entries(entries in entries(), width in 0..200usize) {
        let histogram = counter_of(&entries).histogram(width, &utc()).unwrap();
        let warnings = entries
            .iter()
            .filter(|(_, level)| level.is_some_and(|level| level >= Level::Warn))
            .count();
        prop_assert_eq!(histogram.counts.iter().map(|&n| n as usize).sum::<usize>(), warnings);
    }

    #[test]
    fn buckets_never_outnumber_the_width(entries in entries(), width in 0..200usize) {
        let histogram = counter_of(&entries).histogram(width, &utc()).unwrap();
        prop_assert!(!histogram.counts.is_empty());
        prop_assert!(histogram.counts.len() <= width.max(1));
    }
}
