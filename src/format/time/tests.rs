use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{self, Offset, TimeZone};
use jiff::{Timestamp, Zoned};
use proptest::prelude::*;

use super::*;
use crate::format::{EntryParser, Format};
use crate::test_support::{built_in, text_formats};

/// The formats whose timestamp has a date but no offset.
const LOCAL_DATE_PRESETS: [&str; 3] = ["logback", "spring2", "wildfly"];

const TIME_ONLY_PRESETS: [&str; 2] = ["logback-basic", "wildfly-console"];

fn parser(preset: &'static Format) -> EntryParser {
    preset
        .entry_parser()
        .unwrap_or_else(|| panic!("{preset:?} has no entry parser"))
}

fn parse(preset: &'static Format, span: &str, zone: &TimeZone) -> Option<LogTime> {
    parser(preset).timestamp(span.as_bytes(), zone)
}

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap_or_else(|err| panic!("{name}: {err}"))
}

fn warsaw() -> TimeZone {
    zone("Europe/Warsaw")
}

/// An instant written in RFC 3339, e.g. `2026-10-07T10:00:00.123Z`.
fn instant(rfc3339: &str) -> Option<LogTime> {
    let instant: Timestamp = rfc3339.parse().unwrap();
    Some(LogTime::Instant(instant))
}

/// A time of day written as `HH:MM:SS.fffffffff`.
fn time_of_day(text: &str) -> Option<LogTime> {
    let time: Time = text.parse().unwrap();
    Some(LogTime::TimeOfDay(time))
}

/// `date_time` (`yyyy-MM-dd HH:mm:ss.SSS`, no offset) as each preset with a date
/// writes it. Spring 3 always adds an offset, so it gets `Z`.
fn date_spans(date_time: &str) -> [(&'static Format, String); 4] {
    let with_t = date_time.replacen(' ', "T", 1);
    [
        (built_in("logback"), date_time.to_string()),
        (built_in("spring3"), format!("{with_t}Z")),
        (built_in("spring2"), date_time.to_string()),
        (built_in("wildfly"), date_time.replace('.', ",")),
    ]
}

// --- dates, offsets and zones ---

#[test]
fn a_timestamp_without_offset_is_local_time_in_the_given_zone() {
    for preset in LOCAL_DATE_PRESETS.map(built_in) {
        assert_eq!(
            parse(preset, "2026-10-07 12:00:00.123", &warsaw()),
            instant("2026-10-07T10:00:00.123Z"),
            "{preset:?}"
        );
    }
}

#[test]
fn the_same_wall_clock_time_is_another_instant_in_another_zone() {
    let span = "2026-10-07 12:00:00.123";
    let cases = [
        (TimeZone::UTC, "2026-10-07T12:00:00.123Z"),
        (zone("America/New_York"), "2026-10-07T16:00:00.123Z"),
        (TimeZone::fixed(tz::offset(9)), "2026-10-07T03:00:00.123Z"),
    ];
    for (zone, expected) in cases {
        assert_eq!(
            parse(built_in("logback"), span, &zone),
            instant(expected),
            "{zone:?}"
        );
    }
}

#[test]
fn an_explicit_offset_wins_over_the_given_zone() {
    let span = "2026-10-07T12:00:00.123+02:00";
    for zone in [TimeZone::UTC, warsaw(), zone("America/New_York")] {
        assert_eq!(
            parse(built_in("spring3"), span, &zone),
            instant("2026-10-07T10:00:00.123Z"),
            "{zone:?}"
        );
    }
}

#[test]
fn z_means_utc_whatever_the_given_zone() {
    let span = "2026-10-07T12:00:00.123Z";
    for zone in [TimeZone::UTC, warsaw(), zone("America/New_York")] {
        assert_eq!(
            parse(built_in("spring3"), span, &zone),
            instant("2026-10-07T12:00:00.123Z"),
            "{zone:?}"
        );
    }
}

#[test]
fn reads_negative_offsets_and_offsets_with_minutes() {
    let cases = [
        ("2026-10-07T12:00:00.123-05:00", "2026-10-07T17:00:00.123Z"),
        ("2026-10-07T12:00:00.123-03:30", "2026-10-07T15:30:00.123Z"),
        ("2026-10-07T12:00:00.123+05:45", "2026-10-07T06:15:00.123Z"),
    ];
    for (span, expected) in cases {
        assert_eq!(
            parse(built_in("spring3"), span, &warsaw()),
            instant(expected),
            "{span}"
        );
    }
}

#[test]
fn an_offset_crossing_midnight_moves_the_date() {
    assert_eq!(
        parse(
            built_in("spring3"),
            "2026-10-07T00:30:00.000+02:00",
            &TimeZone::UTC
        ),
        instant("2026-10-06T22:30:00Z")
    );
}

#[test]
fn a_wall_clock_time_in_a_dst_gap_is_read_with_the_offset_before_the_gap() {
    // Warsaw jumps from 02:00 to 03:00 on 2026-03-29: 02:30 doesn't exist and
    // `compatible` takes it as 03:30 summer time.
    for preset in LOCAL_DATE_PRESETS.map(built_in) {
        assert_eq!(
            parse(preset, "2026-03-29 02:30:00.000", &warsaw()),
            instant("2026-03-29T01:30:00Z"),
            "{preset:?}"
        );
    }
}

#[test]
fn an_ambiguous_wall_clock_time_in_a_dst_fold_is_the_earlier_instant() {
    // Warsaw falls back from 03:00 to 02:00 on 2026-10-25: 02:30 happens twice and
    // `compatible` takes the first, still summer time.
    for preset in LOCAL_DATE_PRESETS.map(built_in) {
        assert_eq!(
            parse(preset, "2026-10-25 02:30:00.000", &warsaw()),
            instant("2026-10-25T00:30:00Z"),
            "{preset:?}"
        );
    }
}

#[test]
fn an_explicit_offset_picks_the_later_of_two_ambiguous_wall_clock_times() {
    assert_eq!(
        parse(
            built_in("spring3"),
            "2026-10-25T02:30:00.000+01:00",
            &warsaw()
        ),
        instant("2026-10-25T01:30:00Z")
    );
}

// --- the time of day ---

#[test]
fn time_only_presets_give_a_time_of_day() {
    assert_eq!(
        parse(built_in("logback-basic"), "23:59:58.996", &warsaw()),
        time_of_day("23:59:58.996")
    );
    assert_eq!(
        parse(built_in("wildfly-console"), "00:00:01,338", &warsaw()),
        time_of_day("00:00:01.338")
    );
}

#[test]
fn a_time_of_day_does_not_depend_on_the_zone() {
    for preset in TIME_ONLY_PRESETS.map(built_in) {
        for zone in [TimeZone::UTC, warsaw(), TimeZone::fixed(tz::offset(-11))] {
            assert_eq!(
                parse(preset, "12:00:00.123", &zone),
                time_of_day("12:00:00.123"),
                "{preset:?} {zone:?}"
            );
        }
    }
}

// --- fractions of a second ---

#[test]
fn a_comma_before_the_fraction_works_like_a_dot() {
    for preset in LOCAL_DATE_PRESETS.map(built_in) {
        assert_eq!(
            parse(preset, "2026-10-07 12:00:00,123", &TimeZone::UTC),
            instant("2026-10-07T12:00:00.123Z"),
            "{preset:?}"
        );
    }
    assert_eq!(
        parse(
            built_in("spring3"),
            "2026-10-07T12:00:00,123Z",
            &TimeZone::UTC
        ),
        instant("2026-10-07T12:00:00.123Z")
    );
    for preset in TIME_ONLY_PRESETS.map(built_in) {
        assert_eq!(
            parse(preset, "12:00:00,123", &TimeZone::UTC),
            time_of_day("12:00:00.123"),
            "{preset:?}"
        );
    }
}

#[test]
fn a_fraction_of_one_to_nine_digits_is_a_decimal_fraction_of_a_second() {
    let cases = [
        (".5", "0.5"),
        (",05", "0.05"),
        (".123", "0.123"),
        (".1234", "0.1234"),
        (".000001", "0.000001"),
        (".12345678", "0.12345678"),
        (".123456789", "0.123456789"),
        (".000000001", "0.000000001"),
        (".999999999", "0.999999999"),
    ];
    for (fraction, seconds) in cases {
        let date_span = format!("2026-10-07 12:00:00{fraction}");
        let expected = format!("2026-10-07T12:00:0{seconds}Z");
        assert_eq!(
            parse(built_in("logback"), &date_span, &TimeZone::UTC),
            instant(&expected),
            "{date_span}"
        );
        let time_span = format!("12:00:00{fraction}");
        let expected = format!("12:00:0{seconds}");
        assert_eq!(
            parse(built_in("logback-basic"), &time_span, &TimeZone::UTC),
            time_of_day(&expected),
            "{time_span}"
        );
    }
}

#[test]
fn a_fraction_of_more_than_nine_digits_is_none() {
    assert_eq!(
        parse(
            built_in("logback"),
            "2026-10-07 12:00:00.1234567890",
            &TimeZone::UTC
        ),
        None
    );
    assert_eq!(
        parse(
            built_in("logback-basic"),
            "12:00:00.1234567890",
            &TimeZone::UTC
        ),
        None
    );
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
        for (preset, span) in date_spans(&format!("{date} 12:00:00.123")) {
            assert_eq!(parse(preset, &span, &warsaw()), None, "{preset:?} {span}");
        }
    }
}

#[test]
fn february_29_is_a_date_in_a_leap_year() {
    for (preset, span) in date_spans("2028-02-29 12:00:00.123") {
        assert_eq!(
            parse(preset, &span, &TimeZone::UTC),
            instant("2028-02-29T12:00:00.123Z"),
            "{preset:?}"
        );
    }
}

#[test]
fn an_impossible_time_is_none() {
    let times = [
        "25:00:00.000",
        "24:00:00.000",
        "12:60:00.000",
        "12:00:60.000",
        "12:00:61.000",
    ];
    for time in times {
        for (preset, span) in date_spans(&format!("2026-10-07 {time}")) {
            assert_eq!(parse(preset, &span, &warsaw()), None, "{preset:?} {span}");
        }
        for preset in TIME_ONLY_PRESETS.map(built_in) {
            assert_eq!(parse(preset, time, &warsaw()), None, "{preset:?} {time}");
        }
    }
}

#[test]
fn an_impossible_offset_is_none() {
    assert_eq!(
        parse(
            built_in("spring3"),
            "2026-10-07T12:00:00.123+02:60",
            &warsaw()
        ),
        None
    );
}

#[test]
fn an_instant_beyond_what_jiff_can_represent_is_none() {
    assert_eq!(
        parse(
            built_in("logback"),
            "9999-12-31 23:59:59.999",
            &TimeZone::UTC
        ),
        None
    );
    assert_eq!(
        parse(
            built_in("spring3"),
            "9999-12-31T23:59:59.999-05:00",
            &TimeZone::UTC
        ),
        None
    );
}

#[test]
fn a_span_that_is_not_a_whole_timestamp_is_none() {
    let spans = [
        "",
        "INFO",
        "2026-10-07",
        "2026-10-07 12:0",
        "2026-10-07 12:00:00",
        "2026-10-07 12:00:00.",
        "2026-10-07 12:00:00.123x",
        " 2026-10-07 12:00:00.123",
        "2026-+1-07 12:00:00.123",
        "2026-10-07 12:+1:00.123",
        "2026-10-07 12:00:00.+12",
        "2026/10/07 12:00:00.123",
        "2026-10-07T12:00:00.123",
        "2026-10-07 12:00:00.123+02:00",
    ];
    for span in spans {
        assert_eq!(
            parse(built_in("logback"), span, &TimeZone::UTC),
            None,
            "{span:?}"
        );
    }
}

#[test]
fn a_time_only_span_that_is_not_a_whole_time_is_none() {
    let spans = [
        "",
        "12:00",
        "12:00:00",
        "12:00:00,",
        "12:00:00,123 ",
        "1:00:00,123",
        "+1:00:00,123",
        "12-00-00,123",
    ];
    for span in spans {
        assert_eq!(
            parse(built_in("wildfly-console"), span, &TimeZone::UTC),
            None,
            "{span:?}"
        );
    }
}

#[test]
fn a_spring3_timestamp_with_a_broken_separator_or_offset_is_none() {
    let spans = [
        "2026-10-07 12:00:00.123+02:00",
        "2026-10-07T12:00:00.123",
        "2026-10-07T12:00:00.123+02",
        "2026-10-07T12:00:00.123+0200",
        "2026-10-07T12:00:00.123+02:00 ",
        "2026-10-07T12:00:00.123 02:00",
        "2026-10-07T12:00:00.123ZZ",
    ];
    for span in spans {
        assert_eq!(
            parse(built_in("spring3"), span, &TimeZone::UTC),
            None,
            "{span:?}"
        );
    }
}

// --- properties ---

fn structured() -> impl Strategy<Value = &'static Format> {
    prop::sample::select(text_formats())
}

fn zones() -> impl Strategy<Value = TimeZone> {
    prop_oneof![
        Just(TimeZone::UTC),
        Just(warsaw()),
        Just(zone("America/New_York")),
        (-25i8..=25).prop_map(|hours| TimeZone::fixed(tz::offset(hours))),
    ]
}

/// Bytes shaped like any preset's timestamp, with any digits in it.
fn timestamp_shaped() -> impl Strategy<Value = String> {
    prop_oneof![
        r"[0-9]{4}-[0-9]{2}-[0-9]{2}[ T][0-9]{2}:[0-9]{2}:[0-9]{2}[.,][0-9]{1,9}(Z|[+-][0-9]{2}:[0-9]{2})?",
        r"[0-9]{2}:[0-9]{2}:[0-9]{2}[.,][0-9]{1,9}",
    ]
}

/// A valid date and time, the fraction written with `digits` digits.
fn date_time() -> impl Strategy<Value = (DateTime, usize)> {
    (
        (1i16..=9998, 1i8..=12, 1i8..=31),
        (0i8..24, 0i8..60, 0i8..60, 0i32..1_000_000_000),
        1usize..=9,
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

/// `time` as `HH:mm:ss` and a fraction of `digits` digits after `separator`.
fn write_time(time: Time, separator: char, digits: usize) -> String {
    let nanos = format!("{:09}", time.subsec_nanosecond());
    format!(
        "{:02}:{:02}:{:02}{separator}{}",
        time.hour(),
        time.minute(),
        time.second(),
        &nanos[..digits]
    )
}

fn write_date(date: Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}

fn write_offset(offset: Offset) -> String {
    let seconds = offset.seconds();
    let sign = if seconds < 0 { '-' } else { '+' };
    let minutes = seconds.abs() / 60;
    format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

proptest! {
    #[test]
    fn never_panics_on_arbitrary_bytes(
        preset in structured(),
        zone in zones(),
        bytes in prop::collection::vec(any::<u8>(), 0..64),
    ) {
        parser(preset).timestamp(&bytes, &zone);
    }

    #[test]
    fn never_panics_on_timestamp_shaped_bytes_with_any_digits(
        preset in structured(),
        zone in zones(),
        span in timestamp_shaped(),
    ) {
        parser(preset).timestamp(span.as_bytes(), &zone);
    }

    #[test]
    fn reads_back_any_local_date_and_time(
        preset in prop::sample::select(LOCAL_DATE_PRESETS.map(built_in).to_vec()),
        zone in zones(),
        (date_time, digits) in date_time(),
        separator in prop::sample::select(vec!['.', ',']),
    ) {
        let span = format!(
            "{} {}",
            write_date(date_time.date()),
            write_time(date_time.time(), separator, digits)
        );
        let expected = date_time.to_zoned(zone.clone()).ok().map(|zoned: Zoned| zoned.timestamp());
        prop_assume!(expected.is_some());
        prop_assert_eq!(
            parse(preset, &span, &zone),
            expected.map(LogTime::Instant),
            "{}", span
        );
    }

    #[test]
    fn reads_back_any_date_and_time_with_an_offset(
        zone in zones(),
        (date_time, digits) in date_time(),
        separator in prop::sample::select(vec!['.', ',']),
        offset_minutes in -(18 * 60)..=(18 * 60i32),
        utc in any::<bool>(),
    ) {
        let offset = if utc {
            Offset::UTC
        } else {
            Offset::from_seconds(offset_minutes * 60).unwrap()
        };
        let written_offset = if utc { "Z".to_string() } else { write_offset(offset) };
        let span = format!(
            "{}T{}{written_offset}",
            write_date(date_time.date()),
            write_time(date_time.time(), separator, digits)
        );
        let expected = offset.to_timestamp(date_time).ok();
        prop_assume!(expected.is_some());
        prop_assert_eq!(
            parse(built_in("spring3"), &span, &zone),
            expected.map(LogTime::Instant),
            "{}", span
        );
    }

    #[test]
    fn reads_back_any_time_of_day(
        preset in prop::sample::select(TIME_ONLY_PRESETS.map(built_in).to_vec()),
        zone in zones(),
        (date_time, digits) in date_time(),
        separator in prop::sample::select(vec!['.', ',']),
    ) {
        let span = write_time(date_time.time(), separator, digits);
        prop_assert_eq!(
            parse(preset, &span, &zone),
            Some(LogTime::TimeOfDay(date_time.time())),
            "{}", span
        );
    }
}
