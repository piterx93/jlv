//! Indexing, filtering and exception and bookmark jumps over a generated log (PRD §13). See
//! `docs/benchmarks.md`; `VILOG_BENCH_SIZE=2G` makes it the PRD's 2 GB.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, Throughput};
use vilog::app::Action;
use vilog::filter::{Engine, Filters, Indexed, Levels, Options, Reader};
use vilog::format::Level;
use vilog::matcher::{self, Matcher};

#[expect(dead_code, reason = "the probe lines appended to the log are for nfr")]
mod support;

use support::BenchLog;

/// What the viewer evaluates per chunk (`FILTER_CHUNK` in the app).
const FILTER_CHUNK: usize = 1024;

/// Bookmarks on entries the jump benchmark's filter hides.
const BOOKMARKS: usize = 5;

fn main() {
    let log = BenchLog::get();
    let mut criterion = Criterion::default().configure_from_args();
    indexing(&mut criterion, &log);
    filtering(&mut criterion, &log);
    jumps(&mut criterion, &log);
    criterion.final_summary();
}

fn indexing(c: &mut Criterion, log: &BenchLog) {
    let source = Arc::new(log.open());
    let settings = support::settings(log.path(), &source);
    let mut group = c.benchmark_group("index");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(source.bytes().len() as u64));
    group.bench_function(log.size_label(), |b| {
        b.iter_with_large_drop(|| support::index(&source, &settings));
    });
    group.finish();
}

/// A full re-filter: a new generation of filters over the complete index, until
/// every entry is evaluated.
fn filtering(c: &mut Criterion, log: &BenchLog) {
    let source = Arc::new(log.open());
    let settings = support::settings(log.path(), &source);
    let size = source.bytes().len() as u64;
    let records = support::index(&source, &settings);
    let total = records.len();
    let indexed = Arc::new(RwLock::new(Indexed {
        records,
        bytes: source,
    }));
    let options = Options {
        chunk: FILTER_CHUNK,
    };
    let reader = Reader::new(settings.format, settings.mapping);
    let mut engine = Engine::spawn(indexed, reader, Filters::default(), options);
    let text = |pattern, regex| {
        let options = matcher::Options {
            regex,
            ignore_case: false,
        };
        Matcher::new(pattern, options).expect("a valid pattern")
    };
    let cases = [
        (
            "level",
            Filters {
                levels: Levels::only(&[Level::Error]),
                ..Filters::default()
            },
        ),
        (
            "text",
            Filters {
                text: Some(text("timeout", false)),
                ..Filters::default()
            },
        ),
        (
            "regex",
            Filters {
                text: Some(text(r"order \d+ (created|failed)", true)),
                ..Filters::default()
            },
        ),
    ];
    let mut group = c.benchmark_group("re-filter");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(size));
    for (name, filters) in cases {
        group.bench_function(BenchmarkId::new(name, log.size_label()), |b| {
            b.iter(|| {
                engine.set_filters(filters.clone());
                let deadline = Instant::now() + Duration::from_secs(60);
                let evaluated = engine.poll_until(total, deadline).evaluated;
                assert_eq!(evaluated, total, "the filter took over a minute");
            });
        });
    }
    group.finish();
}

/// `]` and `'` while the filters hide every entry with an exception and every
/// bookmark: each jump has to look at every shown entry to find none.
fn jumps(c: &mut Criterion, log: &BenchLog) {
    let mut app = log.viewer(false);
    app.resize(50);
    support::poll_until(&mut app, |app| app.progress().is_complete());
    for _ in 0..BOOKMARKS {
        app.apply(Action::NextException);
        app.apply(Action::ToggleBookmark);
        assert_eq!(app.notice(), Some("bookmarked"));
    }
    // Exceptions only follow warnings and errors.
    app.set_filters(Filters {
        levels: Levels::only(&[Level::Trace, Level::Debug, Level::Info]),
        ..Filters::default()
    });
    support::poll_until(&mut app, |app| !app.filtering());
    let cases = [
        ("exception jump", Action::NextException, "no exceptions"),
        ("bookmark jump", Action::NextBookmark, "no bookmarks"),
    ];
    for (name, action, notice) in cases {
        app.apply(action);
        assert_eq!(app.notice(), Some(notice));
        let mut group = c.benchmark_group(name);
        group.bench_function(BenchmarkId::new("no hit", log.size_label()), |b| {
            b.iter(|| app.apply(action));
        });
        group.finish();
    }
}
