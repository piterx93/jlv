//! Gzip-compressed logs, opened through the same `source::open` as plain ones.

use std::fs;
use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;
use jlv::format::detect;
use jlv::source::{self, Origin};
use tempfile::TempDir;

mod common;

use common::{fixture, fixtures, lines};

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn every_gzipped_fixture_opens_as_its_log_and_is_detected_as_its_preset() {
    let dir = TempDir::new().unwrap();
    for (preset, name) in fixtures() {
        let log = fixture(name);
        let path = dir.path().join(format!("{name}.gz"));
        fs::write(&path, gzip(&log)).unwrap();
        let source = source::open(&path).unwrap();
        assert_eq!(source.origin(), Origin::Gzip, "{name}");
        assert_eq!(source.bytes(), log, "{name}");
        assert_eq!(detect(lines(source.bytes())), Some(preset), "{name}");
    }
}
