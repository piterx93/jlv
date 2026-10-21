//! Helpers shared by the integration tests.

// Each test crate uses only some of them.
#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use vilog::format::{BUILT_IN, Format, Kind};
use vilog::source;

/// The built-in format `id` names.
pub fn built_in(id: &str) -> &'static Format {
    Format::by_id(id).unwrap_or_else(|| panic!("no built-in format {id}"))
}

/// Every built-in text format, in table order, with the name of its fixture in
/// `tests/fixtures`: `<id>.log`. A format without one fails the first test that reads
/// it. The JSON fixtures are in `tests/fixtures/json`, one per logging library.
pub fn fixtures() -> impl Iterator<Item = (&'static Format, &'static str)> {
    static FIXTURES: LazyLock<Vec<(&Format, String)>> = LazyLock::new(|| {
        BUILT_IN
            .iter()
            .filter(|format| matches!(format.kind, Kind::Regex(_)))
            .map(|format| (format, format!("{}.log", format.id)))
            .collect()
    });
    FIXTURES
        .iter()
        .map(|(format, name)| (*format, name.as_str()))
}

/// The bytes of the fixture `name`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// The lines of `bytes` as in the file, without line endings.
pub fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    source::lines(bytes).collect()
}
