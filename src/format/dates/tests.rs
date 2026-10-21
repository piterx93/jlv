use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{self, TimeZone};
use jiff::{SignedDuration, Timestamp};
use proptest::prelude::*;

use super::*;

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap_or_else(|err| panic!("{name}: {err}"))
}

fn warsaw() -> TimeZone {
    zone("Europe/Warsaw")
}

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

/// A time of day written as `HH:MM:SS.fffffffff`.
fn time(text: &str) -> Time {
    text.parse().unwrap()
}

/// An instant written in RFC 3339, e.g. `2026-10-07T10:00:00.123Z`.
fn instant(rfc3339: &str) -> Timestamp {
    rfc3339.parse().unwrap()
}

fn named(date_text: &str) -> Anchor {
    Anchor::Named(date(date_text))
}

/// A local mtime written as `yyyy-MM-ddTHH:mm:ss`.
fn modified(date_time: &str) -> Anchor {
    Anchor::Modified(date_time.parse().unwrap())
}

fn time_of_day(text: &str) -> Option<LogTime> {
    Some(LogTime::TimeOfDay(time(text)))
}

fn instant_entry(rfc3339: &str) -> Option<LogTime> {
    Some(LogTime::Instant(instant(rfc3339)))
}

/// The timestamps in UTC of entries with the times of day `times` (`None` for an
/// entry without one), from `anchor`.
fn resolve_utc_from(times: &[Option<&str>], anchor: Anchor) -> Vec<Option<Timestamp>> {
    let times: Vec<_> = times
        .iter()
        .map(|time| time.and_then(time_of_day))
        .collect();
    resolve_timestamps(&times, anchor, &TimeZone::UTC)
}

/// [`resolve_utc_from`] with the date `anchor` in the file name.
fn resolve_utc(times: &[Option<&str>], anchor: &str) -> Vec<Option<Timestamp>> {
    resolve_utc_from(times, named(anchor))
}

/// `rfc3339` instants as [`resolve_timestamps`] gives them, `None` as is.
fn expected(rfc3339: &[Option<&str>]) -> Vec<Option<Timestamp>> {
    rfc3339.iter().map(|text| text.map(instant)).collect()
}

/// When the file was last written, unless a test says otherwise.
const MTIME: &str = "2026-10-08T09:00:00Z";

fn mtime_anchor() -> Anchor {
    modified("2026-10-08T09:00:00")
}

fn anchor_of(path: &str) -> Anchor {
    Anchor::for_file(Path::new(path), instant(MTIME), &TimeZone::UTC)
}

// --- the anchor ---

#[test]
fn takes_the_date_from_a_rotated_file_name() {
    assert_eq!(anchor_of("server.log.2026-10-06"), named("2026-10-06"));
}

#[test]
fn takes_the_date_from_the_middle_of_a_compressed_file_name() {
    assert_eq!(anchor_of("app-2026-10-06.log.gz"), named("2026-10-06"));
}

#[test]
fn a_date_in_the_directories_does_not_count() {
    assert_eq!(anchor_of("/var/log/2026-01-01/server.log"), mtime_anchor());
}

#[test]
fn a_date_in_the_file_name_wins_over_one_in_the_directories() {
    assert_eq!(
        anchor_of("/var/log/2026-01-01/server.log.2026-10-06"),
        named("2026-10-06")
    );
}

#[test]
fn of_several_dates_in_the_file_name_the_last_one_counts() {
    assert_eq!(
        anchor_of("app-2026-10-01-to-2026-10-06.log"),
        named("2026-10-06")
    );
    // The last one, not the latest.
    assert_eq!(
        anchor_of("export-2026-10-06.log.2026-09-30"),
        named("2026-09-30")
    );
}

#[test]
fn an_impossible_date_in_the_file_name_is_ignored() {
    for name in [
        "server.log.2026-02-30",
        "server.log.2026-13-01",
        "server.log.2026-10-00",
        "server.log.2026-02-29",
    ] {
        assert_eq!(anchor_of(name), mtime_anchor(), "{name}");
    }
}

#[test]
fn an_impossible_date_does_not_hide_an_earlier_valid_one() {
    assert_eq!(
        anchor_of("app-2026-10-06.log.2026-02-30"),
        named("2026-10-06")
    );
}

#[test]
fn february_29_is_a_date_in_a_leap_year() {
    assert_eq!(anchor_of("server.log.2028-02-29"), named("2028-02-29"));
}

#[test]
fn a_date_needs_two_digit_month_and_day() {
    for name in ["server.log.2026-10-6", "server.log.2026-1-06"] {
        assert_eq!(anchor_of(name), mtime_anchor(), "{name}");
    }
}

#[test]
fn without_a_date_in_the_file_name_the_anchor_is_the_mtime_in_the_zone() {
    let mtime = instant("2026-10-06T22:30:00.250Z");
    let cases = [
        (warsaw(), "2026-10-07T00:30:00.250"),
        (TimeZone::UTC, "2026-10-06T22:30:00.250"),
        (zone("America/New_York"), "2026-10-06T18:30:00.250"),
        (TimeZone::fixed(tz::offset(-23)), "2026-10-05T23:30:00.250"),
    ];
    for (zone, expected) in cases {
        assert_eq!(
            Anchor::for_file(Path::new("server.log"), mtime, &zone),
            modified(expected),
            "{zone:?}"
        );
    }
}

#[test]
fn a_path_without_a_file_name_falls_back_to_the_mtime() {
    for path in ["", "/", "/var/log/2026-01-01/.."] {
        assert_eq!(anchor_of(path), mtime_anchor(), "{path:?}");
    }
}

#[test]
fn a_date_in_a_file_name_that_is_not_utf8_still_counts() {
    let path = Path::new(OsStr::from_bytes(b"app-\xff\xfe.log.2026-10-06"));
    assert_eq!(
        Anchor::for_file(path, instant(MTIME), &TimeZone::UTC),
        named("2026-10-06")
    );
}

// --- the rule: midnights between two entries ---

/// Asserts `midnights_between` for each `(earlier, later)` pair.
fn assert_midnights(expected: i8, pairs: &[(&str, &str)]) {
    for &(earlier, later) in pairs {
        assert_eq!(
            midnights_between(time(earlier), time(later)),
            expected,
            "{earlier} -> {later}"
        );
    }
}

#[test]
fn a_step_back_of_more_than_2_hours_is_a_midnight() {
    assert_midnights(
        1,
        &[
            ("23:59:59.999", "00:00:00.000"),
            ("02:00:00.000000001", "00:00:00"),
            ("23:59:59.997", "00:00:00.001"),
            // A quiet night.
            ("18:00:00", "08:00:00"),
            ("12:00:00", "00:00:00"),
        ],
    );
}

#[test]
fn a_step_forward_of_more_than_23_hours_59_minutes_is_a_late_line_from_before_midnight() {
    assert_midnights(
        -1,
        &[
            ("00:00:00", "23:59:00.000000001"),
            ("00:00:00.001", "23:59:59.998"),
            ("00:00:00", "23:59:59.999999999"),
        ],
    );
}

#[test]
fn anything_in_between_is_the_same_day() {
    assert_midnights(
        0,
        &[
            // A step back of exactly 2 h.
            ("02:00:00", "00:00:00"),
            // Out-of-order logging.
            ("00:00:00.003", "00:00:00.001"),
            // The repeated hour of a DST fall-back.
            ("02:59:59", "02:00:00"),
            ("10:00:00", "10:00:00"),
            // A quiet stretch within the day.
            ("01:00:00", "20:00:00"),
            // A step forward of exactly 23 h 59 min.
            ("00:00:00", "23:59:00"),
        ],
    );
}

// --- dating from the last entry back ---

#[test]
fn a_named_anchor_is_the_date_of_the_last_entry() {
    for last in ["00:00:00", "12:00:00", "23:59:59.999"] {
        let mut dates = DatesBackward::new(named("2026-10-07"));
        assert_eq!(dates.date(time(last)), date("2026-10-07"), "{last}");
    }
}

#[test]
fn a_modified_anchor_dates_the_last_entry_as_an_entry_after_it() {
    let cases = [
        // Flushed just after midnight.
        ("2026-10-08T00:00:01", "23:59:58", "2026-10-07"),
        ("2026-10-08T09:00:00", "08:59:59.999", "2026-10-08"),
        // Touched the next morning.
        ("2026-10-08T09:00:00", "18:00:00", "2026-10-07"),
        // Nothing is written after the mtime, so this is no late line.
        ("2026-10-08T23:59:30", "00:00:00", "2026-10-08"),
    ];
    for (mtime, last, expected) in cases {
        let mut dates = DatesBackward::new(modified(mtime));
        assert_eq!(dates.date(time(last)), date(expected), "{mtime} {last}");
    }
}

#[test]
fn dates_each_entry_from_the_one_after_it() {
    let mut dates = DatesBackward::new(named("2026-10-07"));
    let fed_backwards = [
        ("00:00:00.002", "2026-10-07"),
        ("23:59:59.998", "2026-10-06"),
        ("00:00:00.001", "2026-10-07"),
        ("23:59:59.997", "2026-10-06"),
        ("08:00:00", "2026-10-06"),
        ("18:00:00", "2026-10-05"),
    ];
    for (time_text, expected) in fed_backwards {
        assert_eq!(dates.date(time(time_text)), date(expected), "{time_text}");
    }
}

// --- resolving a whole log ---

#[test]
fn an_empty_log_has_no_timestamps() {
    assert_eq!(resolve_utc(&[], "2026-10-07"), expected(&[]));
}

#[test]
fn a_single_entry_gets_the_anchor_date() {
    assert_eq!(
        resolve_utc(&[Some("12:00:00.123")], "2026-10-07"),
        expected(&[Some("2026-10-07T12:00:00.123Z")])
    );
}

#[test]
fn entries_without_a_midnight_between_them_keep_the_anchor_date() {
    assert_eq!(
        resolve_utc(
            &[
                Some("01:00:00"),
                Some("20:00:00"),
                Some("20:00:00"),
                Some("23:59:00"),
            ],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-07T01:00:00Z"),
            Some("2026-10-07T20:00:00Z"),
            Some("2026-10-07T20:00:00Z"),
            Some("2026-10-07T23:59:00Z"),
        ])
    );
}

#[test]
fn entries_before_a_midnight_crossing_get_the_day_before() {
    assert_eq!(
        resolve_utc(
            &[
                Some("22:00:00.000"),
                Some("23:58:41.007"),
                Some("00:00:00.004"),
                Some("00:05:00.000"),
            ],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-06T22:00:00Z"),
            Some("2026-10-06T23:58:41.007Z"),
            Some("2026-10-07T00:00:00.004Z"),
            Some("2026-10-07T00:05:00Z"),
        ])
    );
}

#[test]
fn every_midnight_crossing_steps_back_one_more_day() {
    assert_eq!(
        resolve_utc(
            &[
                Some("22:00:00.000"),
                Some("02:00:00.000"),
                Some("23:00:00.000"),
                Some("01:00:00.000"),
                Some("23:30:00.000"),
                Some("00:30:00.000"),
            ],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-04T22:00:00Z"),
            Some("2026-10-05T02:00:00Z"),
            Some("2026-10-05T23:00:00Z"),
            Some("2026-10-06T01:00:00Z"),
            Some("2026-10-06T23:30:00Z"),
            Some("2026-10-07T00:30:00Z"),
        ])
    );
}

#[test]
fn a_quiet_night_is_a_midnight() {
    assert_eq!(
        resolve_utc(&[Some("18:00:00"), Some("08:00:00")], "2026-10-07"),
        expected(&[Some("2026-10-06T18:00:00Z"), Some("2026-10-07T08:00:00Z")])
    );
}

#[test]
fn crossing_midnight_steps_back_across_month_and_year_ends() {
    assert_eq!(
        resolve_utc(&[Some("23:00:00.000"), Some("01:00:00.000")], "2026-01-01"),
        expected(&[Some("2025-12-31T23:00:00Z"), Some("2026-01-01T01:00:00Z")])
    );
    assert_eq!(
        resolve_utc(&[Some("23:00:00.000"), Some("01:00:00.000")], "2028-03-01"),
        expected(&[Some("2028-02-29T23:00:00Z"), Some("2028-03-01T01:00:00Z")])
    );
}

#[test]
fn the_last_millisecond_before_midnight_is_on_the_day_before_the_first_after_it() {
    let resolved = resolve_utc(&[Some("23:59:59.999"), Some("00:00:00.000")], "2026-10-07");
    assert_eq!(
        resolved,
        expected(&[
            Some("2026-10-06T23:59:59.999Z"),
            Some("2026-10-07T00:00:00Z")
        ])
    );
}

#[test]
fn a_few_milliseconds_out_of_order_just_after_midnight_keep_the_day() {
    assert_eq!(
        resolve_utc(
            &[
                Some("23:59:59.990"),
                Some("00:00:00.003"),
                Some("00:00:00.001"),
                Some("00:00:00.005"),
            ],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-06T23:59:59.990Z"),
            Some("2026-10-07T00:00:00.003Z"),
            Some("2026-10-07T00:00:00.001Z"),
            Some("2026-10-07T00:00:00.005Z"),
        ])
    );
}

#[test]
fn a_few_milliseconds_out_of_order_just_before_midnight_keep_the_day() {
    assert_eq!(
        resolve_utc(
            &[
                Some("23:59:59.995"),
                Some("23:59:59.993"),
                Some("00:00:00.001"),
            ],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-06T23:59:59.995Z"),
            Some("2026-10-06T23:59:59.993Z"),
            Some("2026-10-07T00:00:00.001Z"),
        ])
    );
}

#[test]
fn a_late_line_from_just_before_midnight_does_not_move_the_lines_around_it() {
    // The third line was stamped before midnight but written after the second.
    assert_eq!(
        resolve_utc(
            &[
                Some("23:59:59.997"),
                Some("00:00:00.001"),
                Some("23:59:59.998"),
                Some("00:00:00.002"),
            ],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-06T23:59:59.997Z"),
            Some("2026-10-07T00:00:00.001Z"),
            Some("2026-10-06T23:59:59.998Z"),
            Some("2026-10-07T00:00:00.002Z"),
        ])
    );
}

#[test]
fn a_late_line_from_before_midnight_at_the_end_puts_the_line_before_it_on_the_next_day() {
    // A date in the file name is the date of the last entry, so the entry after
    // midnight is a day later.
    assert_eq!(
        resolve_utc(&[Some("00:00:00.001"), Some("23:59:59.998")], "2026-10-07"),
        expected(&[
            Some("2026-10-08T00:00:00.001Z"),
            Some("2026-10-07T23:59:59.998Z"),
        ])
    );
}

#[test]
fn a_whole_day_without_entries_goes_unnoticed_so_the_entries_before_it_come_out_a_day_late() {
    // Written at 23:00 on the 5th and at 01:00 on the 7th, with nothing on the 6th.
    // The two times of day can't tell one night from two.
    assert_eq!(
        resolve_utc(&[Some("23:00:00.000"), Some("01:00:00.000")], "2026-10-07"),
        expected(&[Some("2026-10-06T23:00:00Z"), Some("2026-10-07T01:00:00Z")])
    );
}

#[test]
fn a_log_flushed_just_after_midnight_is_dated_from_the_mtime() {
    assert_eq!(
        resolve_utc_from(
            &[Some("23:59:57"), Some("23:59:58")],
            modified("2026-10-08T00:00:01")
        ),
        expected(&[Some("2026-10-07T23:59:57Z"), Some("2026-10-07T23:59:58Z")])
    );
}

// --- zones ---

#[test]
fn date_and_time_of_day_are_local_time_in_the_zone() {
    let times = [time_of_day("23:30:00.000"), time_of_day("00:30:00.000")];
    assert_eq!(
        resolve_timestamps(&times, named("2026-10-07"), &warsaw()),
        vec![
            Some(instant("2026-10-06T21:30:00Z")),
            Some(instant("2026-10-06T22:30:00Z")),
        ]
    );
    assert_eq!(
        resolve_timestamps(&times, named("2026-10-07"), &zone("America/New_York")),
        vec![
            Some(instant("2026-10-07T03:30:00Z")),
            Some(instant("2026-10-07T04:30:00Z")),
        ]
    );
}

#[test]
fn the_repeated_hour_of_a_dst_fall_back_keeps_the_day() {
    // Warsaw falls back from 03:00 to 02:00 on 2026-10-25. Both times are ambiguous
    // and read as the earlier instant, as in `EntryParser::timestamp`.
    let times = [time_of_day("02:59:59"), time_of_day("02:00:00")];
    assert_eq!(
        resolve_timestamps(&times, named("2026-10-25"), &warsaw()),
        vec![
            Some(instant("2026-10-25T00:59:59Z")),
            Some(instant("2026-10-25T00:00:00Z")),
        ]
    );
}

#[test]
fn a_time_of_day_in_a_dst_gap_is_read_with_the_offset_before_the_gap() {
    // Warsaw jumps from 02:00 to 03:00 on 2026-03-29, as in `EntryParser::timestamp`.
    assert_eq!(
        resolve_timestamps(
            &[time_of_day("02:30:00.000")],
            named("2026-03-29"),
            &warsaw()
        ),
        vec![Some(instant("2026-03-29T01:30:00Z"))]
    );
}

#[test]
fn an_ambiguous_time_of_day_in_a_dst_fold_is_the_earlier_instant() {
    assert_eq!(
        resolve_timestamps(
            &[time_of_day("02:30:00.000")],
            named("2026-10-25"),
            &warsaw()
        ),
        vec![Some(instant("2026-10-25T00:30:00Z"))]
    );
}

// --- entries without a timestamp ---

#[test]
fn entries_without_any_timestamp_stay_without_one() {
    assert_eq!(resolve_utc(&[None], "2026-10-07"), expected(&[None]));
    assert_eq!(
        resolve_utc(&[None, None, None], "2026-10-07"),
        expected(&[None, None, None])
    );
}

#[test]
fn an_entry_without_a_timestamp_takes_the_one_before_it() {
    assert_eq!(
        resolve_utc(
            &[Some("23:59:00.000"), None, Some("00:01:00.000"), None, None],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-06T23:59:00Z"),
            Some("2026-10-06T23:59:00Z"),
            Some("2026-10-07T00:01:00Z"),
            Some("2026-10-07T00:01:00Z"),
            Some("2026-10-07T00:01:00Z"),
        ])
    );
}

#[test]
fn entries_before_the_first_timestamp_stay_without_one() {
    assert_eq!(
        resolve_utc(&[None, None, Some("10:00:00.000"), None], "2026-10-07"),
        expected(&[
            None,
            None,
            Some("2026-10-07T10:00:00Z"),
            Some("2026-10-07T10:00:00Z"),
        ])
    );
}

#[test]
fn the_anchor_goes_to_the_last_entry_with_a_time_of_day() {
    assert_eq!(
        resolve_utc(
            &[Some("23:00:00.000"), Some("01:00:00.000"), None, None],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-06T23:00:00Z"),
            Some("2026-10-07T01:00:00Z"),
            Some("2026-10-07T01:00:00Z"),
            Some("2026-10-07T01:00:00Z"),
        ])
    );
}

#[test]
fn entries_without_a_timestamp_do_not_hide_a_midnight_crossing() {
    assert_eq!(
        resolve_utc(
            &[Some("23:00:00.000"), None, None, Some("01:00:00.000")],
            "2026-10-07"
        ),
        expected(&[
            Some("2026-10-06T23:00:00Z"),
            Some("2026-10-06T23:00:00Z"),
            Some("2026-10-06T23:00:00Z"),
            Some("2026-10-07T01:00:00Z"),
        ])
    );
}

// --- full dates ---

#[test]
fn instants_stay_as_they_are_whatever_the_anchor_and_zone() {
    let times = [
        instant_entry("2026-10-07T12:00:00.123Z"),
        instant_entry("2026-10-05T23:00:00Z"),
        instant_entry("2026-10-07T12:00:00.121Z"),
    ];
    for anchor in [named("2001-01-01"), modified("2001-01-01T00:00:00")] {
        for zone in [TimeZone::UTC, warsaw()] {
            assert_eq!(
                resolve_timestamps(&times, anchor, &zone),
                vec![
                    Some(instant("2026-10-07T12:00:00.123Z")),
                    Some(instant("2026-10-05T23:00:00Z")),
                    Some(instant("2026-10-07T12:00:00.121Z")),
                ],
                "{anchor:?} {zone:?}"
            );
        }
    }
}

#[test]
fn an_entry_without_a_timestamp_takes_the_instant_before_it() {
    let times = [
        None,
        instant_entry("2026-10-07T12:00:00Z"),
        None,
        instant_entry("2026-10-07T12:00:05Z"),
        None,
    ];
    assert_eq!(
        resolve_timestamps(&times, named("2026-10-07"), &TimeZone::UTC),
        expected(&[
            None,
            Some("2026-10-07T12:00:00Z"),
            Some("2026-10-07T12:00:00Z"),
            Some("2026-10-07T12:00:05Z"),
            Some("2026-10-07T12:00:05Z"),
        ])
    );
}

/// The first instant jiff knows, `-009999-01-02T01:59:59Z`, falls on a different
/// local day and time in every zone. A time of day dated to that day but less than
/// a second before it has no instant, as one further out has none.
#[test]
fn a_time_of_day_just_before_the_first_instant_has_no_timestamp() {
    for (time, anchor, zone) in [
        (
            "16:59:58.000000001",
            "-009999-01-01T00:00:00",
            TimeZone::fixed(tz::offset(-9)),
        ),
        // New York's local mean time, -04:56:02.
        (
            "21:03:56.000000001",
            "-009999-01-01T00:00:00",
            zone("America/New_York"),
        ),
        (
            "02:59:58.5",
            "-009999-01-03T03:00:00",
            TimeZone::fixed(tz::offset(25)),
        ),
    ] {
        assert_eq!(
            resolve_timestamps(&[time_of_day(time)], modified(anchor), &zone),
            [None],
            "{time} from {anchor} in {zone:?}"
        );
    }
}

/// `-009999-01-03T03:59:58` is the first instant in the zone furthest east, and
/// the earliest local time that is an instant in every zone.
#[test]
fn the_earliest_local_time_in_every_zone_is_the_first_instant_furthest_east() {
    let furthest_east =
        TimeZone::fixed(tz::Offset::from_seconds(25 * 3600 + 59 * 60 + 59).unwrap());
    let times = [time_of_day("03:59:58")];
    let resolved = resolve_timestamps(&times, modified("-009999-01-03T03:59:58"), &furthest_east);
    assert_eq!(resolved, [Some(Timestamp::MIN)]);
}

#[test]
fn dating_forward_to_just_before_the_first_instant_gives_no_timestamp() {
    let zone = TimeZone::fixed(tz::offset(-9));
    let mut dates = DatesForward::new(zone.to_datetime(Timestamp::MIN));
    assert_eq!(dates.timestamp(time("16:59:58.5"), &zone), None);
}

// --- properties ---

fn zones() -> impl Strategy<Value = TimeZone> {
    prop_oneof![
        Just(TimeZone::UTC),
        Just(warsaw()),
        Just(zone("America/New_York")),
        (-25i8..=25).prop_map(|hours| TimeZone::fixed(tz::offset(hours))),
    ]
}

fn any_time() -> impl Strategy<Value = Time> {
    (0i8..24, 0i8..60, 0i8..60, 0i32..1_000_000_000)
        .prop_map(|(hour, minute, second, nanos)| Time::new(hour, minute, second, nanos).unwrap())
}

/// Any date jiff knows, its first and last day included.
fn any_date() -> impl Strategy<Value = Date> {
    prop_oneof![
        Just(Date::MIN),
        Just(Date::MAX),
        (-9999i16..=9999, 1i8..=12, 1i8..=28)
            .prop_map(|(year, month, day)| Date::new(year, month, day).unwrap()),
    ]
}

/// A date far enough from both ends of jiff's range that stepping a day back or
/// forward for every entry, and any zone offset, stays inside it.
fn ordinary_date() -> impl Strategy<Value = Date> {
    (1000i16..=9000, 1i8..=12, 1i8..=28)
        .prop_map(|(year, month, day)| Date::new(year, month, day).unwrap())
}

fn anchor_on(date: impl Strategy<Value = Date>) -> impl Strategy<Value = Anchor> {
    (date, any_time(), any::<bool>()).prop_map(|(date, time, is_named)| {
        if is_named {
            Anchor::Named(date)
        } else {
            Anchor::Modified(date.to_datetime(time))
        }
    })
}

fn any_anchor() -> impl Strategy<Value = Anchor> {
    anchor_on(any_date())
}

fn ordinary_anchor() -> impl Strategy<Value = Anchor> {
    anchor_on(ordinary_date())
}

fn any_timestamp() -> impl Strategy<Value = Timestamp> {
    (Timestamp::MIN.as_second()..=Timestamp::MAX.as_second())
        .prop_map(|second| Timestamp::from_second(second).unwrap())
}

fn any_log_time() -> impl Strategy<Value = Option<LogTime>> {
    prop_oneof![
        Just(None),
        any_time().prop_map(|time| Some(LogTime::TimeOfDay(time))),
        any_timestamp().prop_map(|instant| Some(LogTime::Instant(instant))),
    ]
}

/// [`any_log_time`], and the times of day in `zone` less than a second outside
/// jiff's range, where a log dated at either end of it lands.
fn any_log_time_in(zone: &TimeZone) -> impl Strategy<Value = Option<LogTime>> + use<> {
    let first = zone.to_datetime(Timestamp::MIN).time();
    let last = zone.to_datetime(Timestamp::MAX).time();
    let outside = (1i64..1_000_000_000, any::<bool>()).prop_map(move |(nanos, before)| {
        let step = SignedDuration::from_nanos(nanos);
        let time = if before {
            first.wrapping_sub(step)
        } else {
            last.wrapping_add(step)
        };
        Some(LogTime::TimeOfDay(time))
    });
    prop_oneof![any_log_time(), outside]
}

/// An anchor on the local date of the first or last instant in `zone`, the days a
/// log dated at either end of jiff's range is on.
fn anchor_on_the_local_ends(zone: &TimeZone) -> impl Strategy<Value = Anchor> + use<> {
    let first = zone.to_datetime(Timestamp::MIN).date();
    let last = zone.to_datetime(Timestamp::MAX).date();
    anchor_on(prop_oneof![Just(first), Just(last)])
}

fn times_of_day() -> impl Strategy<Value = Vec<Option<LogTime>>> {
    prop::collection::vec(
        prop::option::weighted(0.8, any_time().prop_map(LogTime::TimeOfDay)),
        0..64,
    )
}

fn instants() -> impl Strategy<Value = Vec<Option<LogTime>>> {
    prop::collection::vec(
        prop::option::weighted(0.8, any_timestamp().prop_map(LogTime::Instant)),
        0..64,
    )
}

/// The resolved timestamps of the entries that have a time, in order.
fn known(times: &[Option<LogTime>], resolved: &[Option<Timestamp>]) -> Vec<Option<Timestamp>> {
    times
        .iter()
        .zip(resolved)
        .filter(|(time, _)| time.is_some())
        .map(|(_, resolved)| *resolved)
        .collect()
}

fn utc(date_time: DateTime) -> Timestamp {
    date_time.to_zoned(TimeZone::UTC).unwrap().timestamp()
}

proptest! {
    #[test]
    fn resolving_never_panics_on_any_mix_of_timestamps(
        (zone, times, anchor) in zones().prop_flat_map(|zone| {
            let times = prop::collection::vec(any_log_time_in(&zone), 0..64);
            let anchor = prop_oneof![any_anchor(), anchor_on_the_local_ends(&zone)];
            (Just(zone), times, anchor)
        }),
    ) {
        resolve_timestamps(&times, anchor, &zone);
    }

    #[test]
    fn dating_backwards_never_panics(
        times in prop::collection::vec(any_time(), 0..64),
        anchor in any_anchor(),
    ) {
        let mut dates = DatesBackward::new(anchor);
        for time in times {
            dates.date(time);
        }
    }

    #[test]
    fn every_entry_keeps_its_time_of_day_and_the_last_gets_a_named_anchor_date(
        times in times_of_day(),
        anchor in ordinary_date(),
    ) {
        let resolved = resolve_timestamps(&times, Anchor::Named(anchor), &TimeZone::UTC);
        let mut last_date = None;
        for (time, resolved) in times.iter().zip(&resolved) {
            if let Some(LogTime::TimeOfDay(time)) = time {
                let resolved = resolved.expect("an entry with a time has a timestamp");
                let civil = resolved.to_zoned(TimeZone::UTC).datetime();
                prop_assert_eq!(civil.time(), *time);
                last_date = Some(civil.date());
            }
        }
        if let Some(last_date) = last_date {
            prop_assert_eq!(last_date, anchor);
        }
    }

    #[test]
    fn consecutive_entries_step_back_at_most_2_hours_and_forward_at_most_23_hours_59_minutes(
        times in times_of_day(),
        anchor in ordinary_anchor(),
    ) {
        let resolved = resolve_timestamps(&times, anchor, &TimeZone::UTC);
        let known: Vec<Timestamp> = known(&times, &resolved).into_iter().flatten().collect();
        let longest_forward = SignedDuration::from_hours(23) + SignedDuration::from_mins(59);
        for pair in known.windows(2) {
            let step = pair[1].duration_since(pair[0]);
            prop_assert!(
                step >= SignedDuration::from_hours(-2) && step <= longest_forward,
                "{} -> {}", pair[0], pair[1]
            );
        }
    }

    #[test]
    fn the_last_entry_is_on_the_mtime_date_or_the_day_before_and_at_most_2_hours_after_it(
        times in prop::collection::vec(any_time(), 1..64),
        mtime in (ordinary_date(), any_time()).prop_map(|(date, time)| date.to_datetime(time)),
    ) {
        let entries: Vec<_> = times.iter().map(|&time| Some(LogTime::TimeOfDay(time))).collect();
        let resolved = resolve_timestamps(&entries, Anchor::Modified(mtime), &TimeZone::UTC);
        let last = resolved.last().copied().flatten().expect("the last entry has a timestamp");
        let last_date = last.to_zoned(TimeZone::UTC).date();
        prop_assert!(
            last_date == mtime.date() || last_date == mtime.date().yesterday().unwrap(),
            "{} with mtime {}", last, mtime
        );
        prop_assert!(utc(mtime).duration_since(last) >= SignedDuration::from_hours(-2));
    }

    #[test]
    fn resolving_dates_times_of_day_as_dating_backwards_does(
        times in prop::collection::vec(any_time(), 0..64),
        anchor in ordinary_anchor(),
    ) {
        let entries: Vec<_> = times.iter().map(|&time| Some(LogTime::TimeOfDay(time))).collect();
        let resolved = resolve_timestamps(&entries, anchor, &TimeZone::UTC);
        let mut dates = DatesBackward::new(anchor);
        let mut stepped: Vec<_> = times
            .iter()
            .rev()
            .map(|&time| Some(utc(dates.date(time).to_datetime(time))))
            .collect();
        stepped.reverse();
        prop_assert_eq!(resolved, stepped);
    }

    #[test]
    fn an_entry_without_a_timestamp_has_the_one_before_it(
        times in prop_oneof![times_of_day(), instants()],
        anchor in ordinary_anchor(),
        zone in zones(),
    ) {
        let resolved = resolve_timestamps(&times, anchor, &zone);
        for i in 0..times.len() {
            if times[i].is_none() {
                let before = if i == 0 { None } else { resolved[i - 1] };
                prop_assert_eq!(resolved[i], before, "entry {}", i);
            }
        }
    }

    #[test]
    fn every_entry_has_a_timestamp_but_those_before_the_first_one(
        times in prop_oneof![
            times_of_day(),
            instants(),
            prop::collection::vec(any_log_time(), 0..64),
        ],
        anchor in ordinary_anchor(),
        zone in zones(),
    ) {
        let resolved = resolve_timestamps(&times, anchor, &zone);
        prop_assert_eq!(resolved.len(), times.len());
        let first = times.iter().position(Option::is_some).unwrap_or(times.len());
        for (i, resolved) in resolved.iter().enumerate() {
            prop_assert_eq!(resolved.is_some(), i >= first, "entry {}", i);
        }
    }

    #[test]
    fn entries_without_a_timestamp_do_not_change_the_others(
        times in times_of_day(),
        anchor in ordinary_anchor(),
        zone in zones(),
    ) {
        let resolved = resolve_timestamps(&times, anchor, &zone);
        let only_known: Vec<_> = times.iter().copied().filter(Option::is_some).collect();
        prop_assert_eq!(
            known(&times, &resolved),
            resolve_timestamps(&only_known, anchor, &zone)
        );
    }

    #[test]
    fn instants_come_out_as_they_went_in(
        times in instants(),
        anchor in any_anchor(),
        zone in zones(),
    ) {
        let resolved = resolve_timestamps(&times, anchor, &zone);
        for (time, resolved) in times.iter().zip(&resolved) {
            if let Some(LogTime::Instant(instant)) = time {
                prop_assert_eq!(*resolved, Some(*instant));
            }
        }
    }

    #[test]
    fn the_anchor_never_panics_on_any_path(
        bytes in prop::collection::vec(any::<u8>(), 0..64),
        mtime in any_timestamp(),
        zone in zones(),
    ) {
        Anchor::for_file(Path::new(OsStr::from_bytes(&bytes)), mtime, &zone);
    }

    #[test]
    fn a_valid_date_at_the_end_of_any_file_name_is_the_anchor(
        name in any::<String>(),
        anchor in (0i16..=9999, 1i8..=12, 1i8..=31)
            .prop_filter_map("no such day", |(y, m, d)| Date::new(y, m, d).ok()),
        mtime in any_timestamp(),
        zone in zones(),
    ) {
        let path = format!(
            "{name}.{:04}-{:02}-{:02}",
            anchor.year(),
            anchor.month(),
            anchor.day()
        );
        prop_assert_eq!(
            Anchor::for_file(Path::new(&path), mtime, &zone),
            Anchor::Named(anchor)
        );
    }
}
