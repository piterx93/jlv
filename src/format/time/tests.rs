use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{self, Offset, TimeZone};
use jiff::{Timestamp, Zoned};
use proptest::prelude::*;

use super::*;
use crate::test_support::text_formats;

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap_or_else(|err| panic!("{name}: {err}"))
}

fn warsaw() -> TimeZone {
    zone("Europe/Warsaw")
}

fn read_str(span: &str, zone: &TimeZone) -> Option<LogTime> {
    read(span.as_bytes(), zone)
}

/// An instant written in RFC 3339, e.g. `2026-10-07T10:00:00.123Z`.
fn instant(rfc3339: &str) -> Option<LogTime> {
    Some(LogTime::Instant(at(rfc3339)))
}

fn at(rfc3339: &str) -> Timestamp {
    rfc3339.parse().unwrap()
}

/// A time of day written as `HH:MM:SS.fffffffff`.
fn time_of_day(text: &str) -> Option<LogTime> {
    let time: Time = text.parse().unwrap();
    Some(LogTime::TimeOfDay(time))
}

// --- the shapes, with every example of the PRD's format tables ---

#[test]
fn reads_every_time_only_example() {
    let cases = [
        ("logback-basic", "12:00:00.123", "12:00:00.123"),
        ("wildfly-console", "12:00:00,123", "12:00:00.123"),
        ("serilog-console", "12:00:00", "12:00:00"),
    ];
    for (source, span, expected) in cases {
        assert_eq!(
            read_str(span, &warsaw()),
            time_of_day(expected),
            "{source}: {span}"
        );
    }
}

#[test]
fn reads_every_example_without_an_offset_as_local_time() {
    // Warsaw is two hours ahead of UTC on 2026-10-07.
    let cases = [
        (
            "logback, spring2",
            "2026-10-07 12:00:00.123",
            "10:00:00.123",
        ),
        ("wildfly, python", "2026-10-07 12:00:00,123", "10:00:00.123"),
        ("nlog", "2026-10-07 12:00:00.1234", "10:00:00.1234"),
        ("go-log", "2026/10/07 12:00:00", "10:00:00"),
        (
            "go-log, microseconds",
            "2026/10/07 12:00:00.000000",
            "10:00:00",
        ),
        (
            "T before the time",
            "2026-10-07T12:00:00.123",
            "10:00:00.123",
        ),
    ];
    for (source, span, expected) in cases {
        assert_eq!(
            read_str(span, &warsaw()),
            instant(&format!("2026-10-07T{expected}Z")),
            "{source}: {span}"
        );
    }
}

#[test]
fn reads_every_example_with_an_offset_whatever_the_zone() {
    let cases = [
        (
            "spring3, slog",
            "2026-10-07T12:00:00.123+02:00",
            "10:00:00.123",
        ),
        ("gunicorn", "2026-10-07 12:00:00 +0200", "10:00:00"),
        ("serilog", "2026-10-07 12:00:00.123 +02:00", "10:00:00.123"),
        (
            "zap-console",
            "2026-10-07T12:00:00.123+0200",
            "10:00:00.123",
        ),
        ("logrus", "2026-10-07T12:00:00+02:00", "10:00:00"),
        (
            "docker",
            "2026-10-07T10:00:00.123456789Z",
            "10:00:00.123456789",
        ),
        ("hours only", "2026-10-07T12:00:00.123+02", "10:00:00.123"),
        ("negative", "2026/10/07 05:00:00,5 -05", "10:00:00.5"),
    ];
    for zone in [TimeZone::UTC, warsaw(), zone("America/New_York")] {
        for (source, span, expected) in cases {
            assert_eq!(
                read_str(span, &zone),
                instant(&format!("2026-10-07T{expected}Z")),
                "{source}: {span} in {zone:?}"
            );
        }
    }
}

#[test]
fn reads_epoch_time_in_the_unit_its_digit_count_gives() {
    let cases = [
        ("0", "1970-01-01T00:00:00Z"),
        ("1791367200", "2026-10-07T10:00:00Z"),
        ("1791367200.5", "2026-10-07T10:00:00.5Z"),
        ("1791367200.123456789", "2026-10-07T10:00:00.123456789Z"),
        ("99999999999", "5138-11-16T09:46:39Z"),
        ("1791367200123", "2026-10-07T10:00:00.123Z"),
        ("1791367200123456", "2026-10-07T10:00:00.123456Z"),
        ("1791367200123456789", "2026-10-07T10:00:00.123456789Z"),
    ];
    for (number, expected) in cases {
        assert_eq!(epoch(number.as_bytes()), Some(at(expected)), "{number}");
    }
}

#[test]
fn every_text_format_reads_its_timestamps_with_the_one_reader() {
    let spans = [
        "12:00:00",
        "2026/10/07 12:00:00,5",
        "2026-10-07T12:00:00.123",
        "2026-10-07 12:00:00 +0200",
        "2026-10-07T12:00:00.123Z",
    ];
    for format in text_formats() {
        let parser = format.entry_parser().unwrap();
        for span in spans {
            assert_eq!(
                parser.timestamp(span.as_bytes(), &warsaw()),
                read_str(span, &warsaw()),
                "{format:?}: {span}"
            );
        }
    }
}

// --- zones ---

#[test]
fn a_wall_clock_time_in_a_dst_gap_is_read_with_the_offset_before_the_gap() {
    // Warsaw jumps from 02:00 to 03:00 on 2026-03-29: 02:30 doesn't exist and
    // `compatible` takes it as 03:30 summer time.
    assert_eq!(
        read_str("2026-03-29 02:30:00", &warsaw()),
        instant("2026-03-29T01:30:00Z")
    );
}

#[test]
fn an_ambiguous_wall_clock_time_in_a_dst_fold_is_the_earlier_instant() {
    // Warsaw falls back from 03:00 to 02:00 on 2026-10-25: 02:30 happens twice and
    // `compatible` takes the first, still summer time.
    assert_eq!(
        read_str("2026-10-25 02:30:00", &warsaw()),
        instant("2026-10-25T00:30:00Z")
    );
}

// --- edge cases ---

#[test]
fn a_leap_second_is_read_as_second_59() {
    assert_eq!(
        read_str("2016-12-31T23:59:60.123Z", &warsaw()),
        instant("2016-12-31T23:59:59.123Z")
    );
    assert_eq!(
        read_str("2016-12-31 23:59:60", &TimeZone::UTC),
        instant("2016-12-31T23:59:59Z")
    );
    assert_eq!(read_str("23:59:60,5", &warsaw()), time_of_day("23:59:59.5"));
}

#[test]
fn reads_the_largest_offsets_in_use() {
    let cases = [
        ("2026-10-07T12:00:00+14:00", "2026-10-06T22:00:00Z"),
        ("2026-10-07T12:00:00-12:00", "2026-10-08T00:00:00Z"),
    ];
    for (span, expected) in cases {
        assert_eq!(read_str(span, &warsaw()), instant(expected), "{span}");
    }
}

#[test]
fn a_fraction_of_ten_digits_is_none() {
    let spans = [
        "2026-10-07 12:00:00.1234567890",
        "2026-10-07T12:00:00,1234567890Z",
        "12:00:00.1234567890",
    ];
    for span in spans {
        assert_eq!(read_str(span, &TimeZone::UTC), None, "{span}");
    }
}

#[test]
fn a_number_with_another_count_of_integer_digits_is_not_epoch_time() {
    let numbers = [
        "179136720012",
        "17913672001234",
        "179136720012345",
        "17913672001234567",
        "179136720012345678",
        "17913672001234567890",
    ];
    for number in numbers {
        assert_eq!(epoch(number.as_bytes()), None, "{number}");
    }
}

#[test]
fn a_malformed_number_is_not_epoch_time() {
    let numbers = [
        "",
        ".",
        "1.",
        ".5",
        "-1",
        "+1",
        "1e9",
        "1.5e9",
        " 1",
        "1 ",
        "1_000",
        "1791367200..5",
        "1791367200.1234567890",
        "1791367200123.5",
        "\"1791367200\"",
    ];
    for number in numbers {
        assert_eq!(epoch(number.as_bytes()), None, "{number:?}");
    }
}

// --- what isn't a timestamp ---

#[test]
fn an_impossible_date_is_none() {
    let dates = [
        "2026-02-30",
        "2026-02-29",
        "2026-04-31",
        "2026-13-45",
        "2026-13-01",
        "2026-00-10",
        "2026-10-00",
        "2026-10-32",
    ];
    for date in dates {
        for span in [format!("{date} 12:00:00"), format!("{date}T12:00:00Z")] {
            assert_eq!(read_str(&span, &warsaw()), None, "{span}");
        }
    }
}

#[test]
fn february_29_is_a_date_in_a_leap_year() {
    assert_eq!(
        read_str("2028/02/29 12:00:00", &TimeZone::UTC),
        instant("2028-02-29T12:00:00Z")
    );
}

#[test]
fn an_impossible_time_is_none() {
    let times = ["25:00:00", "24:00:00", "12:60:00", "12:00:61"];
    for time in times {
        for span in [
            time.to_string(),
            format!("2026-10-07 {time}"),
            format!("2026-10-07T{time}Z"),
        ] {
            assert_eq!(read_str(&span, &warsaw()), None, "{span}");
        }
    }
}

#[test]
fn an_impossible_offset_is_none() {
    for span in ["2026-10-07T12:00:00+02:60", "2026-10-07 12:00:00 -0260"] {
        assert_eq!(read_str(span, &warsaw()), None, "{span}");
    }
}

#[test]
fn an_instant_beyond_what_jiff_can_represent_is_none() {
    for span in ["9999-12-31 23:59:59.999", "9999-12-31T23:59:59.999-05:00"] {
        assert_eq!(read_str(span, &TimeZone::UTC), None, "{span}");
    }
}

#[test]
fn a_span_that_is_not_a_whole_timestamp_is_none() {
    let spans = [
        "",
        "INFO",
        "1791367200",
        "2026-10-07",
        "2026-10-07 12:00",
        "12:00",
        "2026-10-07 12:0",
        "1:00:00",
        "12-00-00",
        "26-10-07 12:00:00",
        "20261007T120000Z",
        "2026-10-07 12:00:00.",
        "12:00:00,",
        "2026-10-07 12:00:00.123x",
        " 2026-10-07 12:00:00.123",
        "2026-10-07 12:00:00.123 ",
        "2026-10-07T12:00:00Z ",
        "2026-+1-07 12:00:00",
        "2026-10-07 12:+1:00",
        "2026-10-07 12:00:00.+12",
        "2026/10-07 12:00:00",
        "2026-10/07 12:00:00",
        "2026-10-07_12:00:00",
        "2026-10-07  12:00:00",
        "2026-10-07T12:00:00  +02:00",
        "2026-10-07T12:00:00 02:00",
        "2026-10-07T12:00:00+2",
        "2026-10-07T12:00:00+02:0",
        "2026-10-07T12:00:00+020",
        "2026-10-07T12:00:00+02:00:00",
        "2026-10-07T12:00:00ZZ",
        "2026-10-07T12:00:00+02:00Z",
        "12:00:00Z",
        "12:00:00+02:00",
    ];
    for span in spans {
        assert_eq!(read_str(span, &TimeZone::UTC), None, "{span:?}");
    }
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

/// Bytes shaped like any timestamp, with any digits in it.
fn timestamp_shaped() -> impl Strategy<Value = String> {
    prop_oneof![
        r"[0-9]{4}[-/][0-9]{2}[-/][0-9]{2}[ T][0-9]{2}:[0-9]{2}:[0-9]{2}([.,][0-9]{0,10})? ?(Z|[+-][0-9]{2}(:?[0-9]{2})?)?",
        r"[0-9]{2}:[0-9]{2}:[0-9]{2}([.,][0-9]{0,10})?",
    ]
}

/// A valid date and time, the fraction written with `digits` digits, none if 0.
fn date_time() -> impl Strategy<Value = (DateTime, usize)> {
    (
        (1i16..=9998, 1i8..=12, 1i8..=31),
        (0i8..24, 0i8..60, 0i8..60, 0i32..1_000_000_000),
        0usize..=9,
    )
        .prop_filter_map(
            "no such day",
            |((year, month, day), (hour, minute, second, nanos), digits)| {
                let date = Date::new(year, month, day).ok()?;
                let time = Time::new(hour, minute, second, truncate(nanos, digits)).ok()?;
                Some((date.to_datetime(time), digits))
            },
        )
}

/// `nanos` with only its first `digits` decimal digits.
fn truncate(nanos: i32, digits: usize) -> i32 {
    let unit = 10i32.pow(9 - digits as u32);
    nanos / unit * unit
}

fn fraction_separator() -> impl Strategy<Value = char> {
    prop::sample::select(vec!['.', ','])
}

/// `time` as `HH:mm:ss`, then a fraction of `digits` digits after `separator`.
fn write_time(time: Time, separator: char, digits: usize) -> String {
    let hms = format!(
        "{:02}:{:02}:{:02}",
        time.hour(),
        time.minute(),
        time.second()
    );
    if digits == 0 {
        return hms;
    }
    let nanos = format!("{:09}", time.subsec_nanosecond());
    format!("{hms}{separator}{}", &nanos[..digits])
}

/// A valid date and time and how it is written, in any shape without an offset.
fn date_and_time() -> impl Strategy<Value = (DateTime, String)> {
    (
        date_time(),
        fraction_separator(),
        prop::sample::select(vec!['-', '/']),
        prop::sample::select(vec!['T', ' ']),
    )
        .prop_map(
            |((date_time, digits), fraction, date_separator, before_time)| {
                let date = date_time.date();
                let written = format!(
                    "{:04}{date_separator}{:02}{date_separator}{:02}{before_time}{}",
                    date.year(),
                    date.month(),
                    date.day(),
                    write_time(date_time.time(), fraction, digits)
                );
                (date_time, written)
            },
        )
}

/// An offset and how it is written: `Z`, `±hh:mm`, `±hhmm` or `±hh`.
fn offset() -> impl Strategy<Value = (Offset, String)> {
    let sign = |negative: bool| if negative { '-' } else { '+' };
    prop_oneof![
        Just((Offset::UTC, "Z".to_string())),
        (-(18 * 60)..=(18 * 60i32), any::<bool>()).prop_map(move |(minutes, colon)| {
            let colon = if colon { ":" } else { "" };
            let written = format!(
                "{}{:02}{colon}{:02}",
                sign(minutes < 0),
                minutes.abs() / 60,
                minutes.abs() % 60
            );
            (Offset::from_seconds(minutes * 60).unwrap(), written)
        }),
        (-18i32..=18).prop_map(move |hours| {
            let written = format!("{}{:02}", sign(hours < 0), hours.abs());
            (Offset::from_seconds(hours * 3600).unwrap(), written)
        }),
    ]
}

/// A number of epoch time and the instant it stands for.
fn epoch_number() -> impl Strategy<Value = (String, Timestamp)> {
    let from_nanos = |nanos: i128| Timestamp::from_nanosecond(nanos).unwrap();
    prop_oneof![
        (0i128..100_000_000_000, 0i32..1_000_000_000, 0usize..=9).prop_map(
            move |(seconds, nanos, digits)| {
                let nanos = truncate(nanos, digits);
                let written = if digits == 0 {
                    seconds.to_string()
                } else {
                    format!("{seconds}.{}", &format!("{nanos:09}")[..digits])
                };
                let instant = from_nanos(seconds * 1_000_000_000 + i128::from(nanos));
                (written, instant)
            }
        ),
        (1_000_000_000_000i128..10_000_000_000_000)
            .prop_map(move |millis| (millis.to_string(), from_nanos(millis * 1_000_000))),
        (1_000_000_000_000_000i128..10_000_000_000_000_000)
            .prop_map(move |micros| (micros.to_string(), from_nanos(micros * 1_000))),
        (1_000_000_000_000_000_000i128..10_000_000_000_000_000_000)
            .prop_map(move |nanos| (nanos.to_string(), from_nanos(nanos))),
    ]
}

proptest! {
    #[test]
    fn never_panics_on_arbitrary_bytes(
        zone in zones(),
        bytes in prop::collection::vec(any::<u8>(), 0..64),
    ) {
        read(&bytes, &zone);
        epoch(&bytes);
    }

    #[test]
    fn never_panics_on_timestamp_shaped_bytes_with_any_digits(
        zone in zones(),
        span in timestamp_shaped(),
    ) {
        read(span.as_bytes(), &zone);
    }

    #[test]
    fn never_panics_on_any_digits(number in r"[0-9]{0,40}(\.[0-9]{0,20})?") {
        epoch(number.as_bytes());
    }

    #[test]
    fn reads_back_any_local_date_and_time(
        zone in zones(),
        (date_time, span) in date_and_time(),
    ) {
        let expected = date_time.to_zoned(zone.clone()).ok().map(|zoned: Zoned| zoned.timestamp());
        prop_assume!(expected.is_some());
        prop_assert_eq!(read_str(&span, &zone), expected.map(LogTime::Instant), "{}", span);
    }

    #[test]
    fn reads_back_any_date_and_time_with_an_offset(
        zone in zones(),
        (date_time, written) in date_and_time(),
        (offset, written_offset) in offset(),
        space in any::<bool>(),
    ) {
        let space = if space { " " } else { "" };
        let span = format!("{written}{space}{written_offset}");
        let expected = offset.to_timestamp(date_time).ok();
        prop_assume!(expected.is_some());
        prop_assert_eq!(read_str(&span, &zone), expected.map(LogTime::Instant), "{}", span);
    }

    #[test]
    fn reads_back_any_time_of_day(
        zone in zones(),
        (date_time, digits) in date_time(),
        separator in fraction_separator(),
    ) {
        let span = write_time(date_time.time(), separator, digits);
        prop_assert_eq!(
            read_str(&span, &zone),
            Some(LogTime::TimeOfDay(date_time.time())),
            "{}", span
        );
    }

    #[test]
    fn reads_back_any_epoch_number((number, expected) in epoch_number()) {
        prop_assert_eq!(epoch(number.as_bytes()), Some(expected), "{}", number);
    }
}
