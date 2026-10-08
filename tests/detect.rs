//! Format detection on the fixture logs in `tests/fixtures`, one per preset.

use jlv::format::{Preset, detect};

mod common;

use common::{fixture, fixtures, lines};

/// What detection makes of a file with these bytes, split into lines as the viewer
/// splits them.
fn detect_bytes(bytes: &[u8]) -> Option<Preset> {
    detect(lines(bytes))
}

#[test]
fn every_fixture_is_detected_as_its_own_preset() {
    for (preset, name) in fixtures() {
        assert_eq!(detect_bytes(&fixture(name)), Some(preset), "{name}");
    }
}

#[test]
fn every_fixture_with_crlf_line_endings_is_detected_as_its_own_preset() {
    for (preset, name) in fixtures() {
        let text = String::from_utf8(fixture(name)).expect("fixtures are UTF-8");
        let crlf = text.replace('\n', "\r\n");
        assert_eq!(detect_bytes(crlf.as_bytes()), Some(preset), "{name}");
    }
}

#[test]
fn every_fixture_in_colour_is_detected_as_its_own_preset() {
    for (preset, name) in fixtures() {
        let bytes = fixture(name);
        let coloured: Vec<Vec<u8>> = lines(&bytes)
            .into_iter()
            .map(|line| [b"\x1b[1;36m".as_slice(), line, b"\x1b[0m"].concat())
            .collect();
        let detection = detect(coloured.iter().map(Vec::as_slice));
        assert_eq!(detection, Some(preset), "{name}");
    }
}

#[test]
fn no_fixture_is_recognized_after_200_lines_of_jvm_startup_noise() {
    let noise =
        "OpenJDK 64-Bit Server VM warning: Sharing is only supported for boot loader classes\n"
            .repeat(200);
    for (_, name) in fixtures() {
        let bytes = [noise.as_bytes(), &fixture(name)].concat();
        let detection = detect_bytes(&bytes);
        assert_eq!(detection, None, "{name}");
    }
}
