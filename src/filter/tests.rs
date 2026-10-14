use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use proptest::prelude::*;

use super::*;
use crate::format::{Anchor, Level, Preset};
use crate::index::{self, Chunk, Settings};
use crate::matcher::{self, Matcher};

/// How long evaluating a test log of a few entries may take before the test fails.
/// It takes milliseconds; the deadline is only there so that an engine that never
/// finishes fails the test quickly.
const DEADLINE: Duration = Duration::from_secs(2);

const LEVELS: [Level; 6] = [
    Level::Trace,
    Level::Debug,
    Level::Info,
    Level::Warn,
    Level::Error,
    Level::Fatal,
];

fn settings(preset: Preset) -> Settings {
    let zone = TimeZone::UTC;
    let mtime: Timestamp = "2026-10-08T12:00:00Z".parse().unwrap();
    Settings {
        preset,
        anchor: Anchor::Modified(zone.to_datetime(mtime)),
        zone,
    }
}

/// `log` and its index, shared as the viewer shares them with the engine.
fn indexed(preset: Preset, log: &[u8]) -> Arc<RwLock<Indexed<Vec<u8>>>> {
    let settings = settings(preset);
    let records = Chunk::index(log, 0, 0..log.len(), &settings).finish(&settings);
    Arc::new(RwLock::new(Indexed {
        records,
        bytes: Arc::new(log.to_vec()),
    }))
}

/// Polls `engine` until it has evaluated all `total` entries, handing every result
/// it publishes on the way to `check`. Fails the test after [`DEADLINE`].
fn finish(engine: &mut Engine, total: usize, mut check: impl FnMut(&Filtered)) -> Filtered {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let filtered = engine.poll();
        check(filtered);
        if filtered.evaluated == total {
            return filtered.clone();
        }
        assert!(
            Instant::now() < deadline,
            "filtering took longer than {DEADLINE:?}: {filtered:?}"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

/// What `filters` find in `log`, once all of it is evaluated.
fn filter(preset: Preset, log: &str, filters: Filters) -> Filtered {
    let indexed = indexed(preset, log.as_bytes());
    let total = indexed.read().unwrap().records.len();
    let mut engine = Engine::spawn(indexed, preset, filters, Options { chunk: 2 });
    finish(&mut engine, total, |_| {})
}

fn text(pattern: &str) -> Option<Matcher> {
    Some(Matcher::new(pattern, matcher::Options::default()).unwrap())
}

fn counts(pairs: &[(Level, u32)]) -> [u32; 6] {
    let mut counts = [0; 6];
    for &(level, count) in pairs {
        counts[level as usize] = count;
    }
    counts
}

#[test]
fn an_empty_index_matches_nothing() {
    assert_eq!(
        filter(Preset::Logback, "", Filters::default()),
        Filtered::default()
    );
}

#[test]
fn counters_ignore_the_level_filter() {
    let log = "\
2026-10-07 12:00:00.000 [main] INFO  com.myco.Order - created
2026-10-07 12:00:01.000 [main] WARN  com.myco.Order - slow
2026-10-07 12:00:02.000 [main] ERROR com.myco.Order - failed
";
    let mut filters = Filters::default();
    for level in LEVELS {
        filters.levels.set(level, false);
    }
    assert_eq!(
        filter(Preset::Logback, log, filters),
        Filtered {
            evaluated: 3,
            entries: vec![],
            counts: counts(&[(Level::Info, 1), (Level::Warn, 1), (Level::Error, 1)]),
        }
    );
}

/// The levels `levels` shows, from least to most severe.
fn shown(levels: Levels) -> Vec<Level> {
    Level::ALL
        .into_iter()
        .filter(|&level| levels.is_on(level))
        .collect()
}

#[test]
fn toggling_a_level_hides_only_that_level() {
    let mut levels = Levels::default();
    levels.toggle(Level::Warn);
    assert_eq!(
        shown(levels),
        [
            Level::Trace,
            Level::Debug,
            Level::Info,
            Level::Error,
            Level::Fatal
        ]
    );
}

#[test]
fn toggling_a_level_twice_shows_it_again() {
    let mut levels = Levels::default();
    levels.toggle(Level::Warn);
    levels.toggle(Level::Warn);
    assert_eq!(levels, Levels::default());
}

#[test]
fn only_shows_just_the_levels_it_names() {
    let levels = Levels::only(&[Level::Warn, Level::Error]);
    assert_eq!(shown(levels), [Level::Warn, Level::Error]);
}

/// A headless line, an INFO entry and an entry whose level jlv doesn't know.
const WITHOUT_LEVELS: &str = "\
started
2026-10-07 12:00:00.000 [main] INFO  com.myco.Order - created
2026-10-07 12:00:01.000 [main] SEVERE com.myco.Order - failed
";

#[test]
fn with_every_level_off_no_entry_matches_not_even_one_without_a_level() {
    let filters = Filters {
        levels: Levels::only(&[]),
        ..Filters::default()
    };
    let found = filter(Preset::Logback, WITHOUT_LEVELS, filters);
    assert_eq!(found.entries, [] as [u32; 0]);
}

#[test]
fn entries_without_a_level_are_hidden_while_any_level_is_off() {
    let mut filters = Filters::default();
    filters.levels.toggle(Level::Trace);
    let found = filter(Preset::Logback, WITHOUT_LEVELS, filters);
    assert_eq!(found.entries, [1]);
}

#[test]
fn raw_entries_are_matched_over_the_whole_line() {
    let filters = Filters {
        text: text("beta"),
        ..Filters::default()
    };
    let found = filter(Preset::Raw, "alpha beta\ngamma\n", filters);
    assert_eq!(found.entries, [0]);
}

#[test]
fn escape_codes_are_stripped_before_matching() {
    let log = "\
2026-10-07 12:00:00.000 [main] \x1b[31mERROR\x1b[0m \x1b[36mcom.myco.Order\x1b[0m - pay\x1b[1mment\x1b[0m failed
\t\x1b[2mat com.myco.Boom\x1b[0m
2026-10-07 12:00:01.000 [main] INFO  org.hibernate.SQL - select
";
    let class = Filters {
        classes: Classes {
            include: vec!["com.myco.Order".to_string()],
            exclude: vec![],
        },
        ..Filters::default()
    };
    let message = Filters {
        text: text("payment"),
        ..Filters::default()
    };
    let continuation = Filters {
        text: text("Boom"),
        ..Filters::default()
    };
    for filters in [class, message, continuation] {
        let found = filter(Preset::Logback, log, filters.clone());
        assert_eq!(found.entries, [0], "{filters:?}");
    }
}

#[test]
fn continuation_lines_appended_to_the_last_entry_are_matched() {
    let log = "2026-10-07 12:00:00.000 [main] ERROR com.myco.Order - failed\n";
    let more = "\tat com.myco.Boom\n";
    let filters = Filters {
        text: text("Boom"),
        ..Filters::default()
    };
    let indexed = indexed(Preset::Logback, log.as_bytes());
    let mut engine = Engine::spawn(
        Arc::clone(&indexed),
        Preset::Logback,
        filters,
        Options { chunk: 1 },
    );
    assert_eq!(finish(&mut engine, 1, |_| {}).entries, [] as [u32; 0]);
    append(&indexed, Preset::Logback, format!("{log}{more}").as_bytes());
    engine.reread(0);
    assert_eq!(finish(&mut engine, 1, |_| {}).entries, [0]);
}

#[test]
fn results_of_entries_read_again_are_never_published_stale() {
    let line = |second: u8, message: &str| {
        format!("2026-10-07 12:00:0{second}.000 [main] INFO  com.myco.Order - {message}\n")
    };
    let before = [line(0, "one"), line(1, "boom"), line(2, "boom")].concat();
    let after = [line(0, "one"), line(1, "two"), line(2, "boom")].concat();
    let shared = indexed(Preset::Logback, before.as_bytes());
    let mut engine = Engine::spawn(
        Arc::clone(&shared),
        Preset::Logback,
        Filters {
            text: text("boom"),
            ..Filters::default()
        },
        Options { chunk: 1 },
    );
    let found = finish(&mut engine, 3, |_| {});
    assert_eq!(found.entries, [1, 2]);
    assert_eq!(found.counts, counts(&[(Level::Info, 2)]));
    let rewritten = Arc::into_inner(indexed(Preset::Logback, after.as_bytes())).unwrap();
    *shared.write().unwrap() = rewritten.into_inner().unwrap();
    engine.reread(1);
    // From here on, the second and third entries count only as they read now.
    let found = finish(&mut engine, 3, |partial| {
        let third_in = partial.evaluated == 3;
        let expected = Filtered {
            evaluated: partial.evaluated,
            entries: if third_in { vec![2] } else { vec![] },
            counts: counts(&[(Level::Info, u32::from(third_in))]),
        };
        assert_eq!(*partial, expected);
    });
    assert_eq!(found.entries, [2]);
}

/// Brings `indexed` up to date with `log`, the whole file after more was appended,
/// as the viewer does.
fn append(indexed: &RwLock<Indexed<Vec<u8>>>, preset: Preset, log: &[u8]) -> usize {
    let mut indexed = indexed.write().unwrap();
    let known = indexed.records.len();
    index::append(&mut indexed.records, log, &settings(preset));
    indexed.bytes = Arc::new(log.to_vec());
    known
}

// A model of the log and the filters, and a naive filter over it.

const WORDS: [&str; 4] = ["alpha", "beta", "myco", "Order"];
const CLASSES: [&str; 3] = ["com.myco.Order", "com.myco.pay.Client", "org.hibernate.SQL"];
const RULES: [&str; 5] = ["myco", "Order", "hibernate", "pay", "Client"];

/// An entry: a first line with columns, or none for the lines before the first one,
/// and continuation lines, each holding one word.
#[derive(Debug, Clone)]
struct Entry {
    head: Option<Head>,
    lines: Vec<&'static str>,
}

#[derive(Debug, Clone)]
struct Head {
    second: u8,
    /// `None` is written as `SEVERE`, a level word jlv doesn't know.
    level: Option<Level>,
    class: &'static str,
    message: &'static str,
}

#[derive(Debug, Clone)]
struct Model {
    levels: [bool; 6],
    include: Vec<&'static str>,
    exclude: Vec<&'static str>,
    /// Seconds after 12:00:00, the end exclusive.
    time: Option<(u8, Option<u8>)>,
    text: Option<&'static str>,
}

fn at_second(second: u8) -> Timestamp {
    format!("2026-10-07T12:00:{second:02}Z").parse().unwrap()
}

impl Model {
    fn filters(&self) -> Filters {
        let mut levels = Levels::default();
        for (level, &on) in LEVELS.iter().zip(&self.levels) {
            levels.set(*level, on);
        }
        Filters {
            levels,
            classes: Classes {
                include: self.include.iter().map(|rule| rule.to_string()).collect(),
                exclude: self.exclude.iter().map(|rule| rule.to_string()).collect(),
            },
            time: self.time.map(|(start, end)| TimeRange {
                start: at_second(start),
                end: end.map(at_second),
            }),
            text: self.text.and_then(text),
        }
    }

    fn passes_level(&self, entry: &Entry) -> bool {
        match entry.head.as_ref().and_then(|head| head.level) {
            Some(level) => self.levels[level as usize],
            None => self.levels.iter().all(|&on| on),
        }
    }

    fn passes_others(&self, entry: &Entry) -> bool {
        let head = entry.head.as_ref();
        let class = head.map(|head| head.class);
        let class_passes = !self
            .exclude
            .iter()
            .any(|rule| class.is_some_and(|class| class.contains(rule)))
            && (self.include.is_empty()
                || self
                    .include
                    .iter()
                    .any(|rule| class.is_some_and(|class| class.contains(rule))));
        let time_passes = self.time.is_none_or(|(start, end)| {
            head.is_some_and(|head| head.second >= start && end.is_none_or(|end| head.second < end))
        });
        let text_passes = self.text.is_none_or(|word| {
            head.is_some_and(|head| head.message.contains(word))
                || entry.lines.iter().any(|line| line.contains(word))
        });
        class_passes && time_passes && text_passes
    }

    fn naive(&self, entries: &[Entry]) -> Filtered {
        let mut filtered = Filtered {
            evaluated: entries.len(),
            ..Filtered::default()
        };
        for (i, entry) in entries.iter().enumerate() {
            if !self.passes_others(entry) {
                continue;
            }
            if let Some(level) = entry.head.as_ref().and_then(|head| head.level) {
                filtered.counts[level as usize] += 1;
            }
            if self.passes_level(entry) {
                filtered.entries.push(i as u32);
            }
        }
        filtered
    }
}

fn render(entries: &[Entry]) -> String {
    let mut log = String::new();
    for entry in entries {
        if let Some(head) = &entry.head {
            let level = head.level.map_or("SEVERE", |level| {
                ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL"][level as usize]
            });
            log.push_str(&format!(
                "2026-10-07 12:00:{:02}.000 [main] {level:<5} {} - {}\n",
                head.second, head.class, head.message
            ));
        }
        for line in &entry.lines {
            log.push_str(&format!("\tat {line}\n"));
        }
    }
    log
}

fn word() -> impl Strategy<Value = &'static str> {
    prop::sample::select(&WORDS[..])
}

fn head() -> impl Strategy<Value = Head> {
    (
        0..6u8,
        prop::option::weighted(0.8, prop::sample::select(&LEVELS[..])),
        prop::sample::select(&CLASSES[..]),
        word(),
    )
        .prop_map(|(second, level, class, message)| Head {
            second,
            level,
            class,
            message,
        })
}

/// Entries as a file has them: only the first can be without a head, and it then
/// has at least one line.
fn entries() -> impl Strategy<Value = Vec<Entry>> {
    let lines = || prop::collection::vec(word(), 0..3);
    (
        prop::option::of(prop::collection::vec(word(), 1..3)),
        prop::collection::vec((head(), lines()), 0..12),
    )
        .prop_map(|(lead, entries)| {
            let lead = lead.map(|lines| Entry { head: None, lines });
            lead.into_iter()
                .chain(entries.into_iter().map(|(head, lines)| Entry {
                    head: Some(head),
                    lines,
                }))
                .collect()
        })
}

fn model() -> impl Strategy<Value = Model> {
    let rules = || prop::collection::vec(prop::sample::select(&RULES[..]), 0..3);
    (
        prop::array::uniform6(prop::bool::weighted(0.8)),
        rules(),
        rules(),
        prop::option::of((0..7u8, prop::option::of(0..7u8))),
        prop::option::of(word()),
    )
        .prop_map(|(levels, include, exclude, time, text)| Model {
            levels,
            include,
            exclude,
            time,
            text,
        })
}

proptest! {
    // A case that never finishes costs a whole deadline, so shrinking one is cut short.
    #![proptest_config(ProptestConfig {
        max_shrink_time: 5_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn results_match_a_naive_filter_after_a_change_of_filters(
        entries in entries(),
        first in model(),
        second in model(),
        chunk in 1..5usize,
        polls in 0..4usize,
    ) {
        let indexed = indexed(Preset::Logback, render(&entries).as_bytes());
        let mut engine = Engine::spawn(
            indexed,
            Preset::Logback,
            first.filters(),
            Options { chunk },
        );
        for _ in 0..polls {
            let partial = engine.poll();
            assert_eq!(*partial, first.naive(&entries[..partial.evaluated]));
        }
        engine.set_filters(second.filters());
        let found = finish(&mut engine, entries.len(), |partial| {
            assert_eq!(*partial, second.naive(&entries[..partial.evaluated]));
        });
        prop_assert_eq!(found, second.naive(&entries));
    }

    #[test]
    fn results_match_a_naive_filter_after_an_append(
        entries in entries(),
        model in model(),
        chunk in 1..5usize,
        polls in 0..4usize,
        cut in any::<prop::sample::Index>(),
    ) {
        let log = render(&entries);
        let cut = cut.index(log.len() + 1);
        let indexed = indexed(Preset::Logback, &log.as_bytes()[..cut]);
        let mut engine = Engine::spawn(
            Arc::clone(&indexed),
            Preset::Logback,
            model.filters(),
            Options { chunk },
        );
        for _ in 0..polls {
            engine.poll();
        }
        let known = append(&indexed, Preset::Logback, log.as_bytes());
        engine.reread(known.saturating_sub(1));
        let found = finish(&mut engine, entries.len(), |_| {});
        prop_assert_eq!(found, model.naive(&entries));
    }
}
