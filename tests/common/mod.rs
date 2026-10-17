//! Helpers shared by the integration tests.

use std::fs;
use std::path::Path;

use jlv::format::Preset;
use jlv::source;

/// Every preset but `Raw`, with its fixture in `tests/fixtures`, in the order of
/// [`Preset::ALL`].
pub fn fixtures() -> impl Iterator<Item = (Preset, &'static str)> {
    Preset::ALL
        .into_iter()
        .filter_map(|preset| fixture_name(preset).map(|name| (preset, name)))
}

/// The fixture of `preset`. A new preset doesn't compile until it has one.
fn fixture_name(preset: Preset) -> Option<&'static str> {
    let name = match preset {
        Preset::Logback => "logback.log",
        Preset::LogbackBasic => "logback-basic.log",
        Preset::Spring3 => "spring3.log",
        Preset::Spring2 => "spring2.log",
        Preset::Wildfly => "wildfly.log",
        Preset::WildflyConsole => "wildfly-console.log",
        Preset::Raw => return None,
    };
    Some(name)
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
