//! Reading the timestamps of the fixture logs in `tests/fixtures`, one per preset,
//! and dating the ones that only show a time of day.

use std::path::Path;

use jiff::Timestamp;
use jiff::civil::{DateTime, Time};
use jiff::tz::TimeZone;
use vilog::format::{Anchor, Format, LogTime, resolve_timestamps, strip_ansi};

mod common;

use common::{built_in, fixture, fixtures, lines};

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap_or_else(|err| panic!("{name}: {err}"))
}

/// Every entry's timestamp span in the fixture `name` and what `preset` reads from it
/// in `zone`.
fn timestamps(
    preset: &'static Format,
    name: &str,
    zone: &TimeZone,
) -> Vec<(String, Option<LogTime>)> {
    let parser = preset.entry_parser().unwrap();
    let bytes = fixture(name);
    lines(&bytes)
        .into_iter()
        .filter_map(|line| {
            let line = strip_ansi(line);
            let head = parser.parse(&line)?;
            let span = &line[head.timestamp?];
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
    let in_warsaw = timestamps(built_in("spring3"), "spring3.log", &zone("Europe/Warsaw"));
    for other in [TimeZone::UTC, zone("America/New_York")] {
        assert_eq!(
            timestamps(built_in("spring3"), "spring3.log", &other),
            in_warsaw,
            "{other:?}"
        );
    }
}

#[test]
fn a_fixture_without_offsets_reads_in_the_given_zone() {
    let in_utc = timestamps(built_in("logback"), "logback.log", &TimeZone::UTC);
    let (span, first) = &in_utc[0];
    assert_eq!(span, "2026-10-07 09:14:02.118");
    let expected = "2026-10-07T09:14:02.118Z".parse().unwrap();
    assert_eq!(*first, Some(LogTime::Instant(expected)));
}

/// The fixture's entry timestamps as `resolve_timestamps` dates them in `zone`, the
/// anchor taken from `path` and `mtime`, each with the span it was read from.
fn dated(
    preset: &'static Format,
    name: &str,
    path: &str,
    mtime: &str,
    zone: &TimeZone,
) -> Vec<(String, Timestamp)> {
    let (spans, times): (Vec<String>, Vec<Option<LogTime>>) =
        timestamps(preset, name, zone).into_iter().unzip();
    let mtime: Timestamp = mtime.parse().unwrap();
    let anchor = Anchor::for_file(Path::new(path), mtime, zone);
    let resolved = resolve_timestamps(&times, anchor, zone);
    assert_eq!(resolved.len(), spans.len(), "{name}");
    spans
        .into_iter()
        .zip(resolved)
        .map(|(span, resolved)| {
            let resolved = resolved.unwrap_or_else(|| panic!("{name}: {span} has no timestamp"));
            (span, resolved)
        })
        .collect()
}

/// Asserts that every entry before midnight in `dated` is on `day_before` and every
/// one after it on `day`, at the time of day its span shows.
fn assert_dated_across_midnight(
    dated: &[(String, Timestamp)],
    day_before: &str,
    day: &str,
    zone: &TimeZone,
) {
    for (span, resolved) in dated {
        let date = if span.starts_with("23:") {
            day_before
        } else {
            day
        };
        assert_eq!(
            wall_clock(LogTime::Instant(*resolved), zone),
            format!("{date} {}", normalized(span)),
            "{span}"
        );
    }
}

#[test]
fn dates_the_logback_basic_fixture_across_midnight_from_its_mtime() {
    let warsaw = zone("Europe/Warsaw");
    let dated = dated(
        built_in("logback-basic"),
        "logback-basic.log",
        "/var/log/2026-01-01/logback-basic.log",
        "2026-10-07T22:10:00Z",
        &warsaw,
    );
    assert_eq!(dated.len(), 22);
    assert_dated_across_midnight(&dated, "2026-10-07", "2026-10-08", &warsaw);
}

#[test]
fn dates_the_wildfly_console_fixture_across_midnight_from_its_file_name() {
    let warsaw = zone("Europe/Warsaw");
    let dated = dated(
        built_in("wildfly-console"),
        "wildfly-console.log",
        "/var/log/wildfly/console.log.2026-10-08",
        "2026-10-09T05:00:00Z",
        &warsaw,
    );
    assert_eq!(dated.len(), 24);
    assert_dated_across_midnight(&dated, "2026-10-07", "2026-10-08", &warsaw);
}
