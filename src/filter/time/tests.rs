use jiff::Timestamp;
use jiff::tz::{self, TimeZone};
use proptest::prelude::*;

use super::*;

/// The timestamp of the last entry in most examples.
const LAST: &str = "2026-10-07 18:00:00";
/// The wall clock in the examples, the morning after.
const NOW: &str = "2026-10-08 09:00:00";

/// `date_time` in UTC.
fn utc(date_time: &str) -> Timestamp {
    format!("{date_time}Z").parse().unwrap()
}

fn spec(spec: &str) -> TimeSpec {
    TimeSpec::parse(spec).unwrap()
}

fn filter(written: &str, base: Base) -> TimeFilter {
    TimeFilter {
        spec: spec(written),
        base,
    }
}

/// What `written` covers in UTC, counted from the file, its last entry at `last`.
fn range_at(written: &str, last: &str) -> TimeRange {
    filter(written, Base::File).range(utc(last), utc(NOW), &TimeZone::UTC)
}

/// What `written` covers in UTC, counted from the file, its last entry at [`LAST`].
fn range(written: &str) -> TimeRange {
    range_at(written, LAST)
}

/// From `start` on, up to but not including `end`, both in UTC.
fn between(start: &str, end: Option<&str>) -> TimeRange {
    TimeRange {
        start: utc(start),
        end: end.map(utc),
    }
}

// --- the last N ---

#[test]
fn the_last_minutes_hours_and_days_count_back_from_the_last_entry_with_no_end() {
    for (written, start) in [
        ("1m", "2026-10-07 17:59:00"),
        ("10m", "2026-10-07 17:50:00"),
        ("60m", "2026-10-07 17:00:00"),
        ("24h", "2026-10-06 18:00:00"),
        ("2d", "2026-10-05 18:00:00"),
        ("99999d", "1752-12-23 18:00:00"),
    ] {
        assert_eq!(range(written), between(start, None), "{written}");
    }
}

#[test]
fn a_window_reaching_past_the_earliest_time_starts_there() {
    assert_eq!(
        range("99999999d"),
        TimeRange {
            start: Timestamp::MIN,
            end: None
        }
    );
}

#[test]
fn a_range_starting_after_the_latest_time_there_is_starts_there() {
    assert_eq!(
        range("9999-12-31 12:00.."),
        TimeRange {
            start: Timestamp::MAX,
            end: None
        }
    );
}

#[test]
fn a_range_starting_before_the_earliest_time_there_is_starts_there() {
    // The earliest time there is falls late on its day, so midnight that day is before it.
    let range = filter("00:00-", Base::File).range(Timestamp::MIN, utc(NOW), &TimeZone::UTC);
    assert_eq!(
        range,
        TimeRange {
            start: Timestamp::MIN,
            end: None
        }
    );
}

#[test]
fn the_last_n_on_the_wall_clock_counts_back_from_now() {
    let range = filter("10m", Base::Now).range(utc(LAST), utc(NOW), &TimeZone::UTC);
    assert_eq!(range, between("2026-10-08 08:50:00", None));
}

// --- ranges ---

#[test]
fn a_minute_range_ends_with_the_last_instant_of_its_end_minute() {
    assert_eq!(
        range("12:00-12:30"),
        between("2026-10-07 12:00:00", Some("2026-10-07 12:31:00"))
    );
}

#[test]
fn a_second_range_ends_with_the_last_instant_of_its_end_second() {
    assert_eq!(
        range("12:00:15-12:00:45"),
        between("2026-10-07 12:00:15", Some("2026-10-07 12:00:46"))
    );
}

#[test]
fn a_range_without_an_end_is_open() {
    for (written, start) in [
        ("12:00-", "2026-10-07 12:00:00"),
        ("2026-10-01 12:00..", "2026-10-01 12:00:00"),
    ] {
        assert_eq!(range(written), between(start, None), "{written}");
    }
}

#[test]
fn a_range_with_dates_keeps_them() {
    assert_eq!(
        range("2026-10-01 12:00..2026-10-02 13:00"),
        between("2026-10-01 12:00:00", Some("2026-10-02 13:01:00"))
    );
}

#[test]
fn a_single_time_is_the_minute_or_second_it_names() {
    for (written, start, end) in [
        ("12:30", "2026-10-07 12:30:00", "2026-10-07 12:31:00"),
        ("12:30:15", "2026-10-07 12:30:15", "2026-10-07 12:30:16"),
        (
            "2026-10-01 12:30",
            "2026-10-01 12:30:00",
            "2026-10-01 12:31:00",
        ),
    ] {
        assert_eq!(range(written), between(start, Some(end)), "{written}");
    }
}

#[test]
fn a_range_without_a_date_is_on_the_date_the_last_entry_has_in_the_zone() {
    // 23:30 UTC is already the next day at +02:00.
    let zone = TimeZone::fixed(tz::offset(2));
    let range =
        filter("12:00-12:30", Base::File).range(utc("2026-10-07 23:30:00"), utc(NOW), &zone);
    assert_eq!(
        range,
        between("2026-10-08 10:00:00", Some("2026-10-08 10:31:00"))
    );
}

#[test]
fn a_range_without_a_date_is_on_the_date_of_the_last_entry_on_the_wall_clock_too() {
    let range = filter("12:00-12:30", Base::Now).range(utc(LAST), utc(NOW), &TimeZone::UTC);
    assert_eq!(
        range,
        between("2026-10-07 12:00:00", Some("2026-10-07 12:31:00"))
    );
}

#[test]
fn a_range_crossing_midnight_ends_the_day_after_a_last_entry_before_midnight() {
    assert_eq!(
        range_at("23:50-00:10", "2026-10-07 23:55:00"),
        between("2026-10-07 23:50:00", Some("2026-10-08 00:11:00"))
    );
}

#[test]
fn a_range_crossing_midnight_starts_the_day_before_a_last_entry_past_midnight() {
    assert_eq!(
        range_at("23:50-00:10", "2026-10-08 00:05:00"),
        between("2026-10-07 23:50:00", Some("2026-10-08 00:11:00"))
    );
}

#[test]
fn a_range_crossing_midnight_starts_on_the_date_of_a_last_entry_right_at_its_start() {
    assert_eq!(
        range_at("23:50-00:10", "2026-10-07 23:50:00"),
        between("2026-10-07 23:50:00", Some("2026-10-08 00:11:00"))
    );
}

#[test]
fn a_range_whose_end_takes_in_its_start_does_not_cross_midnight() {
    for (written, start) in [
        ("12:00-12:00", "2026-10-07 12:00:00"),
        ("12:00:30-12:00", "2026-10-07 12:00:30"),
    ] {
        assert_eq!(
            range(written),
            between(start, Some("2026-10-07 12:01:00")),
            "{written}"
        );
    }
}

#[test]
fn a_range_with_dates_ending_before_its_start_is_an_error() {
    assert!(TimeSpec::parse("2026-10-07 13:00..2026-10-07 12:00").is_err());
    // The end minute takes in the start.
    assert_eq!(
        range("2026-10-07 12:00:30..2026-10-07 12:00"),
        between("2026-10-07 12:00:30", Some("2026-10-07 12:01:00"))
    );
}

// --- reading ---

#[test]
fn whitespace_around_the_spec_and_its_parts_is_ignored() {
    for (spaced, written) in [
        (" 10m ", "10m"),
        ("12:00 - 12:30", "12:00-12:30"),
        (" 12:00 -", "12:00-"),
        (
            "2026-10-07 12:00 .. 2026-10-07 13:00",
            "2026-10-07 12:00..2026-10-07 13:00",
        ),
    ] {
        assert_eq!(TimeSpec::parse(spaced), Ok(spec(written)), "{spaced:?}");
    }
}

#[test]
fn malformed_specs_are_errors() {
    for written in [
        "",
        "  ",
        "soon",
        "10",
        "m",
        "0m",
        "-5m",
        "5s",
        "10x",
        "1.5h",
        "10m-20m",
        "9:00-",
        "24:00-",
        "12:60-",
        "12:00:60-",
        "12:0-13:00",
        "12-13",
        "-12:00",
        "12:00-13",
        "12:00-12:30-13:00",
        "12:00:15.500-",
        "12:00..13:00",
        "2026-10-07 12:00..13:00",
        "2026-10-07 12:00-2026-10-07 13:00",
        "2026-10-07..2026-10-08",
        "2026-13-01 12:00..",
        "2026-02-30 12:00..",
    ] {
        assert!(TimeSpec::parse(written).is_err(), "{written:?}");
    }
}

#[test]
fn an_error_quotes_the_spec() {
    let error = TimeSpec::parse("soon").unwrap_err();
    assert!(error.contains("'soon'"), "{error}");
}

// --- showing ---

#[test]
fn specs_show_as_they_are_read() {
    for written in [
        "10m",
        "24h",
        "2d",
        "12:00-12:30",
        "12:00:15-12:00:45",
        "12:00-",
        "23:50-00:10",
        "12:30",
        "2026-10-07 12:00..2026-10-07 13:00",
        "2026-10-07 12:00:15..",
        "2026-10-07 12:30",
    ] {
        assert_eq!(spec(written).to_string(), written);
    }
}

#[test]
fn the_last_n_shows_how_long_and_its_base() {
    assert_eq!(filter("10m", Base::File).to_string(), "last 10m (file)");
    assert_eq!(filter("24h", Base::Now).to_string(), "last 24h (now)");
}

#[test]
fn a_range_shows_as_written_whatever_the_base() {
    for base in [Base::File, Base::Now] {
        assert_eq!(filter("12:00 - 12:30", base).to_string(), "12:00-12:30");
    }
}

// --- covering ---

#[test]
fn covering_spans_the_whole_seconds_from_its_start_up_to_its_end() {
    let spec = TimeSpec::covering(
        utc("2026-10-07 10:10:00.400"),
        utc("2026-10-07 10:15:00"),
        &TimeZone::UTC,
    );
    let filter = TimeFilter {
        spec,
        base: Base::File,
    };
    assert_eq!(
        filter.range(utc(LAST), utc(NOW), &TimeZone::UTC),
        between("2026-10-07 10:10:00", Some("2026-10-07 10:15:00"))
    );
}

#[test]
fn covering_shows_its_times_on_the_zone_clock() {
    let zone = TimeZone::fixed(tz::offset(2));
    let spec = TimeSpec::covering(
        utc("2026-10-07 08:10:00"),
        utc("2026-10-07 08:15:00"),
        &zone,
    );
    assert_eq!(spec.to_string(), "2026-10-07 10:10:00..2026-10-07 10:14:59");
}

// --- anything ---

fn spec_like() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<String>(),
        "[0-9]{1,25}[mhd]",
        "[0-9: .-]{0,24}",
        "[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}(:[0-9]{2})?\\.\\.([0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2})?",
    ]
}

fn any_timestamp() -> impl Strategy<Value = Timestamp> {
    let seconds = Timestamp::MIN.as_second()..=Timestamp::MAX.as_second();
    prop_oneof![
        Just(Timestamp::MIN),
        Just(Timestamp::MAX),
        seconds.prop_map(|second| Timestamp::from_second(second).unwrap()),
    ]
}

proptest! {
    #[test]
    fn reading_dating_and_showing_any_spec_never_panics(
        written in spec_like(),
        last in any_timestamp(),
        now in any_timestamp(),
        hours in -12i8..=14,
    ) {
        if let Ok(spec) = TimeSpec::parse(&written) {
            let zone = TimeZone::fixed(tz::offset(hours));
            for base in [Base::File, Base::Now] {
                let filter = TimeFilter { spec: spec.clone(), base };
                filter.range(last, now, &zone);
                let _ = filter.to_string();
            }
        }
    }

    #[test]
    fn a_spec_starts_where_its_range_counted_from_the_file_starts(
        written in spec_like(),
        last in any_timestamp(),
        now in any_timestamp(),
        hours in -12i8..=14,
    ) {
        if let Ok(spec) = TimeSpec::parse(&written) {
            let zone = TimeZone::fixed(tz::offset(hours));
            let range = TimeFilter { spec: spec.clone(), base: Base::File }.range(last, now, &zone);
            prop_assert_eq!(spec.start(last, &zone), range.start);
        }
    }
}
