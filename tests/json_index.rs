//! The index of every JSON fixture, read with the mapping chosen from the file itself:
//! each entry's level, timestamp, logger and exception.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use vilog::format::{Anchor, Level, Mapping};
use vilog::index::{Chunk, Record, Settings};

mod common;

use common::{built_in, fixture, lines};

/// The fixture `json/<name>` and its records, indexed with the mapping chosen from
/// it. A timestamp without an offset is UTC.
fn indexed(name: &str) -> (Vec<u8>, Vec<Record>) {
    let bytes = fixture(&format!("json/{name}"));
    let settings = Settings {
        format: built_in("json"),
        zone: TimeZone::UTC,
        anchor: Anchor::Named("2026-03-14".parse().unwrap()),
        mapping: Mapping::choose(lines(&bytes)),
    };
    let records = Chunk::index(&bytes, 0, 0..bytes.len(), &settings).finish(&settings);
    (bytes, records)
}

const T: Option<Level> = Some(Level::Trace);
const D: Option<Level> = Some(Level::Debug);
const I: Option<Level> = Some(Level::Info);
const W: Option<Level> = Some(Level::Warn);
const E: Option<Level> = Some(Level::Error);
const F: Option<Level> = Some(Level::Fatal);

/// Strings go through the aliases, numbers are on the pino scale, and a CLEF line
/// without `@l` is Information. zap's `dpanic` and winston's `http` are no level of
/// PRD §6.6.
#[test]
fn levels_of_every_json_fixture_come_from_its_level_key() {
    let expected: [(&str, &[Option<Level>]); 11] = [
        ("zap.log", &[I, D, I, W, E, I, None, I]),
        ("slog.log", &[I, D, I, W, E, I, I]),
        ("pino.log", &[I, D, I, W, E, T, F, I]),
        ("winston.log", &[I, D, None, W, E, I, T, I]),
        ("structlog.log", &[I, D, I, W, E, I, D, F]),
        ("python-json-logger.log", &[I, D, I, W, E, I, F, I]),
        ("serilog-clef.log", &[I, D, I, W, E, I, T, F]),
        ("ms-json-console.log", &[I, D, I, W, E, I, T, F]),
        ("logstash-logback.log", &[I, D, I, W, E, I, T, I]),
        ("ecs.log", &[I, D, I, W, E, I, T, F]),
        ("docker-json-file.log", &[None; 8]),
    ];
    for (name, levels) in expected {
        let (_, records) = indexed(name);
        let read: Vec<_> = records.iter().map(Record::level).collect();
        assert_eq!(read, levels, "{name}");
    }
}

/// Strings in every shape of PRD §6.7, epoch seconds with a fraction (zap) and epoch
/// milliseconds (pino), kept to the microsecond. Each entry has a timestamp of its
/// own, later than the one before.
#[test]
fn timestamps_of_every_json_fixture_come_from_its_timestamp_key() {
    let expected = [
        ("zap.log", "2026-03-14T09:12:01.104213Z"),
        ("slog.log", "2026-03-14T08:12:01.104213Z"),
        ("pino.log", "2026-03-14T10:30:00Z"),
        ("winston.log", "2026-03-14T09:12:01.104Z"),
        ("structlog.log", "2026-03-14T09:12:01.104213Z"),
        ("python-json-logger.log", "2026-03-14T09:12:01.104Z"),
        ("serilog-clef.log", "2026-03-14T09:12:01.104213Z"),
        ("ms-json-console.log", "2026-03-14T09:12:01.104Z"),
        ("logstash-logback.log", "2026-03-14T08:12:01.104Z"),
        ("ecs.log", "2026-03-14T08:12:01.104Z"),
        ("docker-json-file.log", "2026-03-14T09:12:01.104213Z"),
    ];
    for (name, first) in expected {
        let (_, records) = indexed(name);
        let read: Vec<Option<Timestamp>> = records.iter().map(Record::timestamp).collect();
        assert_eq!(read[0], Some(first.parse().unwrap()), "{name}");
        assert!(
            read.windows(2).all(|pair| pair[0] < pair[1]),
            "{name}: {read:?}"
        );
    }
}

/// The logger span is the value between the quotes, as written in the line.
#[test]
fn logger_of_every_json_fixture_comes_from_its_logger_key() {
    let expected = [
        ("zap.log", Some("orders")),
        ("slog.log", None),
        ("pino.log", Some("api")),
        ("winston.log", None),
        ("structlog.log", Some("app.main")),
        ("python-json-logger.log", Some("app.main")),
        ("serilog-clef.log", Some("Microsoft.Hosting.Lifetime")),
        ("ms-json-console.log", Some("Microsoft.Hosting.Lifetime")),
        (
            "logstash-logback.log",
            Some("com.acme.orders.OrderApplication"),
        ),
        ("ecs.log", Some("com.acme.orders.OrderApplication")),
        ("docker-json-file.log", None),
    ];
    for (name, logger) in expected {
        let (bytes, records) = indexed(name);
        let first = lines(&bytes)[0];
        let class = records[0]
            .class()
            .map(|class| str::from_utf8(&first[class]).unwrap());
        assert_eq!(class, logger, "{name}");
    }
}

/// Each fixture with an exception has it on its fifth line, under its library's key;
/// slog and Docker have none.
#[test]
fn exceptions_of_every_json_fixture_come_from_its_exception_field() {
    let expected: [(&str, &[usize]); 11] = [
        ("zap.log", &[4]),
        ("slog.log", &[]),
        ("pino.log", &[4]),
        ("winston.log", &[4]),
        ("structlog.log", &[4]),
        ("python-json-logger.log", &[4]),
        ("serilog-clef.log", &[4]),
        ("ms-json-console.log", &[4]),
        ("logstash-logback.log", &[4]),
        ("ecs.log", &[4]),
        ("docker-json-file.log", &[]),
    ];
    for (name, entries) in expected {
        let (_, records) = indexed(name);
        let with_exception: Vec<usize> = records
            .iter()
            .enumerate()
            .filter(|(_, record)| record.has_exception())
            .map(|(entry, _)| entry)
            .collect();
        assert_eq!(with_exception, entries, "{name}");
    }
}
