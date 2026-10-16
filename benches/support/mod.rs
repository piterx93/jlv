//! What the benchmarks share: the log they run on and ways to index it.
//!
//! The log is `JLV_BENCH_LOG` if that is set, written there first if no file is
//! there yet and kept afterwards, so that several runs share one big log. Otherwise
//! it is written to the temp dir and removed when the benchmark ends. Either way it
//! is a Logback log of about `JLV_BENCH_SIZE` bytes (`2G`, `64M`, …; 32M if unset).

use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use jlv::app::App;
use jlv::format::{Anchor, Preset, detect};
use jlv::index::{Indexer, Record, Settings};
use jlv::source::{self, Source};
use tempfile::TempPath;

pub mod generator;

use generator::{Spec, generate, parse_size};

const DEFAULT_SIZE: &str = "32M";

pub struct BenchLog {
    path: PathBuf,
    /// Removes a log of our own when dropped.
    _temp: Option<TempPath>,
}

impl BenchLog {
    /// Finds or writes the log as the module docs say. Panics on a bad size or a log
    /// that can't be written: a benchmark can't run without one.
    pub fn get() -> BenchLog {
        let size = env::var("JLV_BENCH_SIZE").unwrap_or_else(|_| DEFAULT_SIZE.to_owned());
        let size = parse_size(&size).unwrap_or_else(|err| panic!("JLV_BENCH_SIZE: {err}"));
        let (path, temp) = match env::var_os("JLV_BENCH_LOG") {
            Some(path) => (PathBuf::from(path), None),
            None => {
                let temp = tempfile::Builder::new()
                    .prefix("jlv-bench-")
                    .suffix(".log")
                    .tempfile()
                    .expect("a temp file for the log")
                    .into_temp_path();
                (temp.to_path_buf(), Some(temp))
            }
        };
        if temp.is_some() || !path.exists() {
            let spec = Spec {
                preset: Preset::Logback,
                size,
                seed: 1,
            };
            eprintln!("writing {} bytes of log to {}", size, path.display());
            generate(&path, spec).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        }
        BenchLog { path, _temp: temp }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The log's size, e.g. `2.0 GiB`, to tell runs on different logs apart.
    pub fn size_label(&self) -> String {
        let size = self.open().bytes().len() as f64;
        if size >= f64::from(1 << 30) {
            format!("{:.1} GiB", size / f64::from(1 << 30))
        } else {
            format!("{:.0} MiB", size / f64::from(1 << 20))
        }
    }

    /// Maps the log, panicking with its path if it can't.
    pub fn open(&self) -> Source {
        source::open(&self.path).unwrap_or_else(|err| panic!("{}: {err}", self.path.display()))
    }

    /// A viewer over the log, set up as the binary sets it up, its preset detected
    /// and timestamps read in UTC.
    pub fn viewer(&self, follow: bool) -> App {
        App::new(
            &self.path,
            self.open(),
            None,
            TimeZone::UTC,
            Timestamp::now(),
            follow,
        )
    }
}

/// How the viewer would read `source`, opened from `path`.
pub fn settings(path: &Path, source: &Source) -> Settings {
    let zone = TimeZone::UTC;
    Settings {
        preset: detect(source::lines(source.bytes())).unwrap_or(Preset::Raw),
        anchor: Anchor::for_file(path, Timestamp::now(), &zone),
        zone,
    }
}

/// The index of all of `source`, built on background threads as the viewer builds it.
pub fn index(source: &Arc<Source>, settings: &Settings) -> Vec<Record> {
    let mut indexer = Indexer::spawn(Arc::clone(source), settings.clone());
    let mut records = Vec::new();
    while !indexer.poll(&mut records).is_complete() {
        thread::sleep(Duration::from_millis(1));
    }
    records
}

/// Polls `app` until `done` says so, panicking after a minute.
pub fn poll_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    app.poll_index();
    while !done(app) {
        assert!(Instant::now() < deadline, "the viewer took over a minute");
        thread::sleep(Duration::from_millis(1));
        app.poll_index();
    }
}
