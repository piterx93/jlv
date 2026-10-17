//! Exporting what the filters show, end to end: a log file on disk, the viewer over
//! it, and the file the export writes.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use jiff::tz::TimeZone;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vilog::app::App;
use vilog::filter::{Filters, Levels};
use vilog::format::{Level, Preset};
use vilog::source;

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

#[test]
fn export_writes_the_filtered_entries_as_in_the_file_without_escapes() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("app.log");
    fs::write(&path, fixture("export.log")).unwrap();
    let mut app = App::new(
        &path,
        source::open(&path).unwrap(),
        Some(Preset::Logback),
        TimeZone::UTC,
        "2026-10-08T12:00:00Z".parse().unwrap(),
        false,
    );
    app.set_current_dir(dir.path());
    app.set_filters(Filters {
        levels: Levels::only(&[Level::Warn, Level::Error]),
        ..Filters::default()
    });
    for code in [KeyCode::Char('e'), KeyCode::Enter] {
        app.key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    // The export waits for the index and the filter, which the polls take in.
    let deadline = Instant::now() + Duration::from_secs(1);
    while app.notice() == Some("export waits for the filters") {
        assert!(Instant::now() < deadline, "exporting took longer than 1 s");
        app.poll_index();
        thread::sleep(Duration::from_millis(1));
    }
    let exported = fs::read(dir.path().join("app.log.filtered.log")).unwrap();
    assert_eq!(exported, fixture("export.filtered.log"));
}
