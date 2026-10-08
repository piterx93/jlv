//! Helpers shared by the integration tests.

use std::fs;
use std::path::Path;

use jlv::format::Preset;
use jlv::source::LineIndex;

/// Every preset but `Raw`, with its fixture in `tests/fixtures`.
pub const FIXTURES: [(Preset, &str); 6] = [
    (Preset::Logback, "logback.log"),
    (Preset::LogbackBasic, "logback-basic.log"),
    (Preset::Spring3, "spring3.log"),
    (Preset::Spring2, "spring2.log"),
    (Preset::Wildfly, "wildfly.log"),
    (Preset::WildflyConsole, "wildfly-console.log"),
];

/// The bytes of the fixture `name`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// The lines of `bytes` as in the file, without line endings.
pub fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    let index = LineIndex::new(bytes);
    (0..index.len()).map(|i| index.line(bytes, i)).collect()
}
