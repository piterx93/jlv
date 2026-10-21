//! The entry index against grouping the same lines one by one with `EntryGrouper`,
//! on the fixture logs and on generated ones, indexed whole and in chunks, and the
//! tail read from the end against the end of the index.

use std::ops::Range;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use proptest::prelude::*;
use vilog::format::{
    Anchor, BUILT_IN, Entry, EntryGrouper, Format, Level, resolve_timestamps, strip_ansi,
};
use vilog::index::{Chunk, Indexer, Record, Settings, tail};

mod common;

use common::{built_in, fixture, fixtures, lines};

/// What the index says about an entry but its line number, which the tail doesn't
/// know.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Indexed {
    bytes: Range<u64>,
    timestamp: Option<Timestamp>,
    level: Option<Level>,
    class: Option<Range<usize>>,
    has_exception: bool,
}

fn indexed(records: &[Record]) -> Vec<Indexed> {
    records
        .iter()
        .map(|record| Indexed {
            bytes: record.bytes(),
            timestamp: record.timestamp(),
            level: record.level(),
            class: record.class(),
            has_exception: record.has_exception(),
        })
        .collect()
}

/// The lines of `bytes` without line endings, each with the offset it starts at.
fn lines_at(bytes: &[u8]) -> Vec<(usize, &[u8])> {
    let mut start = 0;
    bytes
        .split_inclusive(|&byte| byte == b'\n')
        .map(|piece| {
            let line = match piece.strip_suffix(b"\n") {
                Some(line) => line.strip_suffix(b"\r").unwrap_or(line),
                None => piece,
            };
            let at = start;
            start += piece.len();
            (at, line)
        })
        .collect()
}

/// What the index of `bytes` has to say: the entries `EntryGrouper` finds when it is
/// fed the lines one by one, their levels as `Level::parse` reads them, their
/// classes if they end by byte 65 535 and their timestamps as `resolve_timestamps`
/// gives them, to the microsecond.
fn line_by_line(bytes: &[u8], settings: &Settings) -> Vec<Indexed> {
    let lines = lines_at(bytes);
    let mut grouper = EntryGrouper::new(settings.format);
    let mut entries: Vec<Entry> = lines
        .iter()
        .filter_map(|(_, line)| grouper.push(line))
        .collect();
    entries.extend(grouper.open_entry().cloned());

    let parser = settings.format.entry_parser();
    let first_lines: Vec<Vec<u8>> = entries
        .iter()
        .map(|entry| strip_ansi(lines[entry.lines.start].1).into_owned())
        .collect();
    let times: Vec<_> = entries
        .iter()
        .zip(&first_lines)
        .map(|(entry, line)| {
            let head = entry.head.as_ref()?;
            parser?.timestamp(&line[head.timestamp.clone()], &settings.zone)
        })
        .collect();
    let timestamps = resolve_timestamps(&times, settings.anchor, &settings.zone);

    entries
        .iter()
        .zip(&first_lines)
        .zip(timestamps)
        .map(|((entry, line), timestamp)| {
            let start = lines[entry.lines.start].0;
            let end = lines
                .get(entry.lines.end)
                .map_or(bytes.len(), |(at, _)| *at);
            let head = entry.head.as_ref();
            Indexed {
                bytes: start as u64..end as u64,
                timestamp: timestamp.map(|timestamp| {
                    Timestamp::from_microsecond(timestamp.as_microsecond()).unwrap()
                }),
                level: head.and_then(|head| Level::parse(&line[head.level.clone()])),
                class: head
                    .map(|head| head.class.clone())
                    .filter(|class| class.end <= 65_535),
                has_exception: entry.has_exception,
            }
        })
        .collect()
}

fn index(bytes: &[u8], settings: &Settings) -> Vec<Record> {
    Chunk::index(bytes, 0, 0..bytes.len(), settings).finish(settings)
}

/// `bytes` indexed in chunks that end at `cuts`, which are in order, and joined.
fn chunked(bytes: &[u8], cuts: &[usize], settings: &Settings) -> Vec<Record> {
    let mut start = 0;
    let mut whole: Option<Chunk> = None;
    for end in cuts.iter().copied().chain([bytes.len()]) {
        let chunk = Chunk::index(bytes, 0, start..end, settings);
        match &mut whole {
            Some(whole) => whole.join(chunk),
            None => whole = Some(chunk),
        }
        start = end;
    }
    whole.unwrap().finish(settings)
}

fn one_byte_chunks(bytes: &[u8], settings: &Settings) -> Vec<Record> {
    let cuts: Vec<usize> = (1..bytes.len()).collect();
    chunked(bytes, &cuts, settings)
}

fn warsaw() -> TimeZone {
    TimeZone::get("Europe/Warsaw").unwrap()
}

/// The fixtures were written in Warsaw; the time-only ones end on 2026-10-08.
fn fixture_settings(preset: &'static Format) -> Settings {
    Settings {
        format: preset,
        zone: warsaw(),
        anchor: Anchor::Named("2026-10-08".parse().unwrap()),
    }
}

fn instant(rfc3339: &str) -> Timestamp {
    rfc3339.parse().unwrap()
}

// --- fixtures ---

#[test]
fn index_of_every_fixture_matches_its_lines_grouped_one_by_one() {
    for (preset, name) in fixtures() {
        let bytes = fixture(name);
        let settings = fixture_settings(preset);
        assert_eq!(
            indexed(&index(&bytes, &settings)),
            line_by_line(&bytes, &settings),
            "{name}"
        );
    }
}

#[test]
fn raw_index_of_every_fixture_has_an_entry_per_line() {
    for (_, name) in fixtures() {
        let bytes = fixture(name);
        let settings = fixture_settings(built_in("raw"));
        let records = index(&bytes, &settings);
        assert_eq!(records.len(), lines(&bytes).len(), "{name}");
        assert_eq!(indexed(&records), line_by_line(&bytes, &settings), "{name}");
    }
}

#[test]
fn every_fixture_in_one_byte_chunks_gives_the_same_index() {
    for (preset, name) in fixtures() {
        let bytes = fixture(name);
        let settings = fixture_settings(preset);
        assert_eq!(
            one_byte_chunks(&bytes, &settings),
            index(&bytes, &settings),
            "{name}"
        );
    }
}

/// Every line ending is `\r\n`, so some one-byte chunk ends between every `\r` and
/// its `\n`.
#[test]
fn every_fixture_with_crlf_endings_in_one_byte_chunks_gives_the_same_index() {
    for (preset, name) in fixtures() {
        let bytes: Vec<u8> = String::from_utf8(fixture(name))
            .unwrap()
            .replace('\n', "\r\n")
            .into_bytes();
        let settings = fixture_settings(preset);
        let records = one_byte_chunks(&bytes, &settings);
        assert_eq!(records, index(&bytes, &settings), "{name}");
        assert_eq!(indexed(&records), line_by_line(&bytes, &settings), "{name}");
    }
}

#[test]
fn logback_basic_fixture_is_dated_across_midnight() {
    let records = index(
        &fixture("logback-basic.log"),
        &fixture_settings(built_in("logback-basic")),
    );
    // 23:58:41.007 and 00:00:05.001 in Warsaw.
    assert_eq!(
        records.first().unwrap().timestamp(),
        Some(instant("2026-10-07T21:58:41.007Z"))
    );
    assert_eq!(
        records.last().unwrap().timestamp(),
        Some(instant("2026-10-07T22:00:05.001Z"))
    );
}

#[test]
fn wildfly_console_fixture_is_dated_across_midnight() {
    let records = index(
        &fixture("wildfly-console.log"),
        &fixture_settings(built_in("wildfly-console")),
    );
    // The headless entry, then 23:59:56,102 to 00:02:00,145 in Warsaw.
    assert_eq!(records[0].timestamp(), None);
    assert_eq!(
        records[1].timestamp(),
        Some(instant("2026-10-07T21:59:56.102Z"))
    );
    assert_eq!(
        records.last().unwrap().timestamp(),
        Some(instant("2026-10-07T22:02:00.145Z"))
    );
}

#[test]
fn indexer_indexes_every_fixture_as_its_lines_group() {
    // Each fixture takes milliseconds, even in a debug build.
    let deadline = Instant::now() + Duration::from_secs(1);
    for (preset, name) in fixtures() {
        let bytes = fixture(name);
        let settings = fixture_settings(preset);
        let expected = line_by_line(&bytes, &settings);
        let mut indexer = Indexer::spawn(Arc::new(bytes), settings);
        let mut records = Vec::new();
        while !indexer.poll(&mut records).is_complete() {
            assert!(Instant::now() < deadline, "{name}: not complete");
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(indexed(&records), expected, "{name}");
    }
}

// --- generated logs ---

/// An entry start of one of the presets at `hour:minute:second`. Hours from 24 up
/// don't exist, which leaves the entry without a timestamp of its own.
fn entry_start(preset: usize, hour: u8, minute: u8, second: u8) -> String {
    let time = format!("{hour:02}:{minute:02}:{second:02}");
    let line = match preset {
        0 => "2026-10-07 TIME.123 [main] INFO  com.example.App - Started",
        1 => "TIME.123 [main] WARN  com.example.App -- Slow",
        2 => {
            "2026-10-07TTIME.123+02:00 ERROR 12345 --- [myapp] [           main] o.s.b.SpringApplication                  : Failed"
        }
        3 => {
            "2026-10-07 TIME.123 DEBUG 12345 --- [           main] o.s.b.SpringApplication                  : Ready"
        }
        4 => {
            "2026-10-07 TIME,123 INFO  [org.jboss.as.server] (Controller Boot Thread) WFLYSRV0025: started"
        }
        _ => "TIME,123 SEVERE [org.jboss.as] (MSC service thread 1-2) WFLYSRV0049: starting",
    };
    line.replace("TIME", &time)
}

/// A line without its ending: entry starts of every preset, some coloured, stack
/// trace lines, wrapped and blank lines, and a few arbitrary bytes.
fn log_line() -> impl Strategy<Value = Vec<u8>> {
    let start = (0..6usize, 0..26u8, 0..60u8, 0..60u8, any::<bool>()).prop_map(
        |(preset, hour, minute, second, coloured)| {
            let line = entry_start(preset, hour, minute, second);
            let line = if coloured {
                format!("\x1b[32m{line}\x1b[0m")
            } else {
                line
            };
            line.into_bytes()
        },
    );
    let continuation = prop::sample::select(vec![
        "\tat com.example.App.main(App.java:10)",
        "Caused by: java.io.IOException: gone",
        "java.lang.IllegalStateException: boom",
        "\t... 5 more",
        "  and the message goes on",
        "",
    ])
    .prop_map(|line| line.as_bytes().to_vec());
    prop_oneof![
        4 => start,
        4 => continuation,
        1 => prop::collection::vec(any::<u8>(), 0..12),
    ]
}

/// A log of up to 40 lines ending in `\n` or `\r\n`, the last maybe in neither.
fn any_log() -> impl Strategy<Value = Vec<u8>> {
    let ending = prop::sample::select(vec![&b"\n"[..], &b"\r\n"[..]]);
    let lines = prop::collection::vec((log_line(), ending), 0..40);
    (lines, any::<bool>()).prop_map(|(lines, unterminated)| {
        let mut bytes = Vec::new();
        let count = lines.len();
        for (i, (line, ending)) in lines.into_iter().enumerate() {
            bytes.extend(line);
            if !(unterminated && i + 1 == count) {
                bytes.extend(ending);
            }
        }
        bytes
    })
}

/// A log and positions to cut it at, in order.
fn log_and_cuts() -> impl Strategy<Value = (Vec<u8>, Vec<usize>)> {
    any_log().prop_flat_map(|bytes| {
        let len = bytes.len();
        let cuts = prop::collection::vec(0..=len, 0..12).prop_map(|mut cuts| {
            cuts.sort_unstable();
            cuts
        });
        (Just(bytes), cuts)
    })
}

fn any_preset() -> impl Strategy<Value = &'static Format> {
    prop::sample::select(BUILT_IN.iter().collect::<Vec<_>>())
}

fn any_settings() -> impl Strategy<Value = Settings> {
    let zone = prop::sample::select(vec!["UTC", "Europe/Warsaw", "America/New_York"]);
    let anchor = prop_oneof![
        Just(Anchor::Named("2026-10-08".parse().unwrap())),
        Just(Anchor::Modified("2026-10-08T00:30:00".parse().unwrap())),
    ];
    (any_preset(), zone, anchor).prop_map(|(preset, zone, anchor)| Settings {
        format: preset,
        zone: TimeZone::get(zone).unwrap(),
        anchor,
    })
}

proptest! {
    #[test]
    fn any_chunking_gives_the_same_index(
        (bytes, cuts) in log_and_cuts(),
        settings in any_settings(),
    ) {
        prop_assert_eq!(chunked(&bytes, &cuts, &settings), index(&bytes, &settings));
    }

    #[test]
    fn one_byte_chunks_give_the_same_index(
        bytes in any_log(),
        settings in any_settings(),
    ) {
        prop_assert_eq!(one_byte_chunks(&bytes, &settings), index(&bytes, &settings));
    }

    #[test]
    fn tail_is_the_end_of_the_index(
        bytes in prop_oneof![any_log(), prop::collection::vec(any::<u8>(), 0..300)],
        count in 0..50usize,
        window in 1..=256usize,
        settings in any_settings(),
    ) {
        let records = index(&bytes, &settings);
        let end = &records[records.len().saturating_sub(count)..];
        let tail = tail(&bytes, count, window, bytes.len(), &settings);
        prop_assert_eq!(tail.as_deref().map(indexed), Some(indexed(end)));
    }

    #[test]
    fn tail_within_a_limit_is_the_end_of_the_index_or_none(
        bytes in any_log(),
        count in 0..50usize,
        window in 1..=256usize,
        limit in 0..2000usize,
        settings in any_settings(),
    ) {
        let records = index(&bytes, &settings);
        let end = &records[records.len().saturating_sub(count)..];
        match tail(&bytes, count, window, limit, &settings) {
            Some(tail) => prop_assert_eq!(indexed(&tail), indexed(end)),
            None => prop_assert!(limit < bytes.len(), "None within the whole file"),
        }
    }

    #[test]
    fn entries_are_numbered_by_the_line_they_start_on(
        bytes in any_log(),
        settings in any_settings(),
    ) {
        for record in index(&bytes, &settings) {
            let before = &bytes[..record.bytes().start as usize];
            let newlines = before.iter().filter(|&&byte| byte == b'\n').count() as u64;
            prop_assert_eq!(record.line(), Some(newlines + 1));
        }
    }

    #[test]
    fn index_matches_the_lines_grouped_one_by_one(
        bytes in any_log(),
        settings in any_settings(),
    ) {
        prop_assert_eq!(indexed(&index(&bytes, &settings)), line_by_line(&bytes, &settings));
    }
}

// --- the tail ---

/// An entry whose timestamp can't be read takes the one of the entry before it, even
/// when that entry lies before the window first read.
#[test]
fn tail_reads_back_to_the_timestamp_its_first_entry_takes() {
    let lines =
        [(12, 0), (25, 1), (12, 2)].map(|(hour, second)| entry_start(0, hour, 0, second) + "\n");
    let bytes = lines.concat().into_bytes();
    let settings = fixture_settings(built_in("logback"));
    let window = lines[1].len() + lines[2].len();
    assert_eq!(
        indexed(&tail(&bytes, 2, window, bytes.len(), &settings).unwrap()),
        indexed(&index(&bytes, &settings)[1..])
    );
}

/// The lines before the tail aren't read, so their count is unknown.
#[test]
fn tail_entries_have_no_line_numbers() {
    let bytes = b"first\nsecond\nthird\n";
    let records = tail(bytes, 1, 6, bytes.len(), &fixture_settings(built_in("raw"))).unwrap();
    assert_eq!(records.iter().map(Record::line).collect::<Vec<_>>(), [None]);
}

/// Bytes from the end the tails of large files may read: blank lines this long take
/// milliseconds to index in a debug build.
const LIMIT: usize = 64 << 10;

/// 32 MiB of blank lines, which take seconds to index in a debug build.
fn blank_lines() -> Vec<u8> {
    vec![b'\n'; 32 << 20]
}

/// Every entry after 32 MiB of blank lines, under a preset with dates and a time-only
/// one.
#[test]
fn tail_reads_only_the_end_of_the_file() {
    for (example, preset) in [(0, built_in("logback")), (1, built_in("logback-basic"))] {
        let entries: String = (0..3)
            .map(|second| entry_start(example, 12, 0, second) + "\n")
            .collect();
        let len = entries.len() as u64 / 3;
        let mut bytes = blank_lines();
        let blank = bytes.len() as u64;
        bytes.extend_from_slice(entries.as_bytes());

        let started = Instant::now();
        let records = tail(&bytes, 3, 4096, LIMIT, &fixture_settings(preset)).unwrap();
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "{}: {elapsed:?}",
            preset.id
        );
        let ranges: Vec<_> = records.iter().map(Record::bytes).collect();
        let expected: Vec<_> = (0..3)
            .map(|i| blank + i * len..blank + (i + 1) * len)
            .collect();
        assert_eq!(ranges, expected, "{}", preset.id);
    }
}

/// The blank lines are one entry without a head, whose start lies far past the limit.
#[test]
fn tail_gives_up_at_the_limit() {
    let bytes = blank_lines();
    let started = Instant::now();
    let records = tail(
        &bytes,
        3,
        4096,
        LIMIT,
        &fixture_settings(built_in("logback")),
    );
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert_eq!(records, None);
}
