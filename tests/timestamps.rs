//! Reading the timestamps of the fixture logs in `tests/fixtures`, one per preset.

use jiff::civil::{DateTime, Time};
use jiff::tz::TimeZone;
use jlv::format::{LogTime, Preset, strip_ansi};

mod common;

use common::{fixture, fixtures, lines};

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap_or_else(|err| panic!("{name}: {err}"))
}

/// Every entry's timestamp span in the fixture `name` and what `preset` reads from it
/// in `zone`.
fn timestamps(preset: Preset, name: &str, zone: &TimeZone) -> Vec<(String, Option<LogTime>)> {
    let parser = preset.entry_parser().unwrap();
    let bytes = fixture(name);
    lines(&bytes)
        .into_iter()
        .filter_map(|line| {
            let line = strip_ansi(line);
            let head = parser.parse(&line)?;
            let span = &line[head.timestamp];
            let text = String::from_utf8(span.to_vec()).unwrap();
            Some((text, parser.timestamp(span, zone)))
        })
        .collect()
}

fn write_time(time: Time) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        time.hour(),
        time.minute(),
        time.second(),
        time.millisecond()
    )
}

fn write_date_time(date_time: DateTime) -> String {
    let date = date_time.date();
    format!(
        "{:04}-{:02}-{:02} {}",
        date.year(),
        date.month(),
        date.day(),
        write_time(date_time.time())
    )
}

/// What the parsed timestamp shows on a wall clock in `zone`, written like the
/// fixtures write their timestamps but always with `.` and a space before the time.
fn wall_clock(time: LogTime, zone: &TimeZone) -> String {
    match time {
        LogTime::Instant(instant) => write_date_time(instant.to_zoned(zone.clone()).datetime()),
        LogTime::TimeOfDay(time) => write_time(time),
    }
}

/// `span` written like [`wall_clock`] does: Spring 3's `T` and offset and WildFly's
/// `,` are gone.
fn normalized(span: &str) -> String {
    let date_time_len = "2026-10-07 09:14:02.118".len();
    span.replace('T', " ")
        .replace(',', ".")
        .chars()
        .take(date_time_len)
        .collect()
}

#[test]
fn every_fixture_timestamp_reads_as_the_time_it_shows() {
    // The fixtures are written in Warsaw in October, where Spring 3's `+02:00` is
    // local time too.
    let warsaw = zone("Europe/Warsaw");
    for (preset, name) in fixtures() {
        let has_date = preset.entry_parser().unwrap().has_date();
        let timestamps = timestamps(preset, name, &warsaw);
        assert!(!timestamps.is_empty(), "{name}");
        for (span, parsed) in timestamps {
            let parsed = parsed.unwrap_or_else(|| panic!("{name}: {span} doesn't parse"));
            assert_eq!(
                matches!(parsed, LogTime::Instant(_)),
                has_date,
                "{name}: {span}"
            );
            assert_eq!(wall_clock(parsed, &warsaw), normalized(&span), "{name}");
        }
    }
}

#[test]
fn the_spring3_fixture_reads_the_same_in_any_zone() {
    let in_warsaw = timestamps(Preset::Spring3, "spring3.log", &zone("Europe/Warsaw"));
    for other in [TimeZone::UTC, zone("America/New_York")] {
        assert_eq!(
            timestamps(Preset::Spring3, "spring3.log", &other),
            in_warsaw,
            "{other:?}"
        );
    }
}

#[test]
fn a_fixture_without_offsets_reads_in_the_given_zone() {
    let in_utc = timestamps(Preset::Logback, "logback.log", &TimeZone::UTC);
    let (span, first) = &in_utc[0];
    assert_eq!(span, "2026-10-07 09:14:02.118");
    let expected = "2026-10-07T09:14:02.118Z".parse().unwrap();
    assert_eq!(*first, Some(LogTime::Instant(expected)));
}
