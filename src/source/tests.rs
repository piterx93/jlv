use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use tempfile::TempDir;

use super::*;

const CAP: usize = 64 * 1024;

fn file_with(dir: &TempDir, content: &[u8]) -> PathBuf {
    let path = dir.path().join("app.log");
    fs::write(&path, content).unwrap();
    path
}

fn mapped(path: &Path) -> Source {
    Source::map(File::open(path).unwrap()).unwrap()
}

/// The lines of a text input, as text, for readable failures.
fn lines(text: &str) -> Vec<&str> {
    let index = LineIndex::new(text.as_bytes());
    (0..index.len())
        .map(|i| std::str::from_utf8(index.line(text.as_bytes(), i)).unwrap())
        .collect()
}

fn byte_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let index = LineIndex::new(bytes);
    (0..index.len()).map(|i| index.line(bytes, i)).collect()
}

fn a_times(n: usize) -> String {
    "a".repeat(n)
}

fn cut(kept: &str) -> String {
    format!("{kept}…")
}

// --- mapping ---

#[test]
fn mapped_source_holds_the_file_content() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"12:00:00.123 INFO Started\n12:00:01.000 WARN Slow\n");
    assert_eq!(
        mapped(&path).bytes(),
        b"12:00:00.123 INFO Started\n12:00:01.000 WARN Slow\n"
    );
}

#[test]
fn zero_length_file_maps_to_no_bytes() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"");
    assert_eq!(mapped(&path).bytes(), b"");
}

#[test]
fn binary_content_is_mapped_verbatim() {
    let dir = TempDir::new().unwrap();
    let content = b"\0\xff\xfe\r\n\x1b[31mred\x1b[0m\xc3\n\0";
    let path = file_with(&dir, content);
    assert_eq!(mapped(&path).bytes(), content);
}

#[test]
fn file_spanning_many_pages_is_mapped_completely() {
    let dir = TempDir::new().unwrap();
    let content: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let path = file_with(&dir, &content);
    assert_eq!(mapped(&path).bytes(), content.as_slice());
}

#[test]
fn mapping_outlives_the_deleted_file() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"first\nsecond\n");
    let source = mapped(&path);
    fs::remove_file(&path).unwrap();
    assert_eq!(source.bytes(), b"first\nsecond\n");
}

#[test]
fn line_starts_index_into_the_mapped_file() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"INFO one\r\nWARN two\nERROR three");
    let source = mapped(&path);
    let index = LineIndex::new(source.bytes());
    assert_eq!(index.starts(), [0, 10, 19]);
    assert_eq!(&source.bytes()[19..], b"ERROR three");
}

// --- line splitting ---

#[test]
fn empty_input_has_no_lines() {
    let index = LineIndex::new(b"");
    assert_eq!(index.len(), 0);
    assert!(index.is_empty());
    assert_eq!(index.starts(), [0usize; 0]);
}

#[test]
fn lone_newline_is_one_empty_line() {
    assert_eq!(lines("\n"), [""]);
}

#[test]
fn text_without_newline_is_one_line() {
    assert_eq!(lines("a"), ["a"]);
}

#[test]
fn trailing_newline_does_not_start_another_line() {
    assert_eq!(lines("a\n"), ["a"]);
}

#[test]
fn input_with_lines_is_not_empty() {
    assert!(!LineIndex::new(b"\n").is_empty());
}

#[test]
fn last_line_without_newline_still_counts() {
    assert_eq!(lines("first\nsecond"), ["first", "second"]);
}

#[test]
fn consecutive_newlines_give_empty_lines() {
    assert_eq!(lines("a\n\n\nb\n"), ["a", "", "", "b"]);
}

#[test]
fn crlf_endings_are_not_part_of_the_line() {
    assert_eq!(lines("a\r\nb\r\n"), ["a", "b"]);
}

#[test]
fn mixed_line_endings_are_both_understood() {
    assert_eq!(lines("a\r\nb\nc\r\nd"), ["a", "b", "c", "d"]);
}

#[test]
fn lone_cr_in_the_middle_is_not_a_separator() {
    assert_eq!(lines("a\rb\nc"), ["a\rb", "c"]);
}

#[test]
fn only_one_cr_before_the_newline_is_dropped() {
    assert_eq!(lines("a\r\r\n"), ["a\r"]);
}

#[test]
fn cr_at_the_very_end_without_newline_is_kept() {
    assert_eq!(lines("a\nb\r"), ["a", "b\r"]);
}

#[test]
fn crlf_alone_is_one_empty_line() {
    assert_eq!(lines("\r\n"), [""]);
}

#[test]
fn line_starts_are_byte_offsets() {
    let index = LineIndex::new(b"ab\r\ncd\n\nx");
    assert_eq!(index.starts(), [0, 4, 7, 8]);
}

#[test]
fn line_starts_count_bytes_not_characters() {
    let index = LineIndex::new("é😀\nü\n".as_bytes());
    assert_eq!(index.starts(), [0, 7]);
}

#[test]
fn nul_and_invalid_bytes_are_ordinary_line_content() {
    let bytes = b"\0\xff\n\xfe\xc3\r\n\x80";
    assert_eq!(
        byte_lines(bytes),
        [&b"\0\xff"[..], &b"\xfe\xc3"[..], &b"\x80"[..]]
    );
}

#[test]
fn any_line_can_be_read_without_the_ones_before() {
    let text = "one\ntwo\r\nthree\nfour";
    let index = LineIndex::new(text.as_bytes());
    assert_eq!(index.line(text.as_bytes(), 3), b"four");
    assert_eq!(index.line(text.as_bytes(), 1), b"two");
}

#[test]
fn many_lines_are_all_indexed() {
    let text: String = (0..10_000).map(|i| format!("line {i}\n")).collect();
    let index = LineIndex::new(text.as_bytes());
    assert_eq!(index.len(), 10_000);
    assert_eq!(index.line(text.as_bytes(), 9_999), b"line 9999");
}

/// What the lines must rejoin to: the input without the `\r` before each `\n` and
/// without one trailing `\n`.
fn rejoined_expectation(bytes: &[u8]) -> Vec<u8> {
    let mut expected: Vec<u8> = bytes
        .iter()
        .enumerate()
        .filter(|&(i, &b)| !(b == b'\r' && bytes.get(i + 1) == Some(&b'\n')))
        .map(|(_, &b)| b)
        .collect();
    if expected.last() == Some(&b'\n') {
        expected.pop();
    }
    expected
}

/// Arbitrary bytes, with line endings common enough to matter.
fn log_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        prop_oneof![
            3 => any::<u8>(),
            1 => Just(b'\n'),
            1 => Just(b'\r'),
        ],
        0..256,
    )
}

proptest! {
    #[test]
    fn line_index_never_panics_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..1024)
    ) {
        let index = LineIndex::new(&bytes);
        for i in 0..index.len() {
            index.line(&bytes, i);
        }
    }

    #[test]
    fn rejoined_lines_give_back_the_input(bytes in log_bytes()) {
        let joined = byte_lines(&bytes).join(&b'\n');
        prop_assert_eq!(joined, rejoined_expectation(&bytes));
    }

    #[test]
    fn every_line_sits_at_its_start_offset(bytes in log_bytes()) {
        let index = LineIndex::new(&bytes);
        prop_assert_eq!(index.starts().len(), index.len());
        for (i, &start) in index.starts().iter().enumerate() {
            let line = index.line(&bytes, i);
            prop_assert!(bytes[start..].starts_with(line));
            prop_assert!(!line.contains(&b'\n'));
            prop_assert!(start == 0 || bytes[start - 1] == b'\n');
        }
    }

    #[test]
    fn every_newline_ends_a_line(bytes in log_bytes()) {
        let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
        let unterminated = usize::from(!bytes.is_empty() && bytes.last() != Some(&b'\n'));
        prop_assert_eq!(LineIndex::new(&bytes).len(), newlines + unterminated);
    }
}

// --- display ---

#[test]
fn valid_utf8_is_shown_unchanged() {
    let text = "12:00:00.123 [main] INFO  c.e.Zażółć -- 日本 😀 done";
    assert_eq!(display(text.as_bytes()), text);
}

#[test]
fn empty_line_is_empty_text() {
    assert_eq!(display(b""), "");
}

#[test]
fn invalid_utf8_becomes_replacement_characters() {
    assert_eq!(display(b"a\xffb\xc3"), "a\u{FFFD}b\u{FFFD}");
}

#[test]
fn nul_bytes_are_kept() {
    assert_eq!(display(b"a\0b"), "a\0b");
}

#[test]
fn line_of_exactly_64_kib_is_not_cut() {
    let line = a_times(CAP);
    assert_eq!(display(line.as_bytes()), line);
}

#[test]
fn line_one_byte_over_64_kib_is_cut_and_marked() {
    let line = a_times(CAP + 1);
    assert_eq!(display(line.as_bytes()), cut(&a_times(CAP)));
}

#[test]
fn huge_line_is_cut_at_64_kib() {
    let line = a_times(4 * 1024 * 1024);
    assert_eq!(display(line.as_bytes()), cut(&a_times(CAP)));
}

#[test]
fn multi_byte_character_ending_at_the_cap_is_not_cut() {
    let line = format!("{}é", a_times(CAP - 2));
    assert_eq!(display(line.as_bytes()), line);
}

#[test]
fn multi_byte_character_ending_at_the_cap_is_kept_when_more_follows() {
    let line = format!("{}é and more", a_times(CAP - 2));
    assert_eq!(
        display(line.as_bytes()),
        cut(&format!("{}é", a_times(CAP - 2)))
    );
}

#[test]
fn cap_inside_a_multi_byte_character_drops_the_whole_character() {
    for ch in ['é', '€', '😀'] {
        let width = ch.len_utf8();
        // the cap falls after 1..width bytes of `ch`
        for inside in 1..width {
            let before = CAP - inside;
            let line = format!("{}{ch}tail", a_times(before));
            assert_eq!(
                display(line.as_bytes()),
                cut(&a_times(before)),
                "{ch:?} cut after {inside} of its {width} bytes"
            );
        }
    }
}

#[test]
fn invalid_byte_just_before_the_cap_is_shown_as_replacement() {
    let mut line = a_times(CAP - 1).into_bytes();
    line.extend_from_slice(b"\xffz");
    assert_eq!(
        display(&line),
        cut(&format!("{}\u{FFFD}", a_times(CAP - 1)))
    );
}

#[test]
fn line_of_invalid_bytes_is_capped_too() {
    let line = vec![0xff; CAP + 10];
    assert_eq!(display(&line), cut(&"\u{FFFD}".repeat(CAP)));
}

/// `before` bytes of `a` followed by a short tail, so the tail straddles the cap.
fn around_the_cap<T: Strategy>(tail: T) -> impl Strategy<Value = (usize, T::Value)> {
    (CAP - 8..=CAP, tail)
}

proptest! {
    #[test]
    fn short_lines_are_decoded_lossily(
        bytes in prop::collection::vec(any::<u8>(), 0..1024)
    ) {
        prop_assert_eq!(display(&bytes), String::from_utf8_lossy(&bytes));
    }

    #[test]
    fn long_valid_text_keeps_every_character_that_fits(
        (before, tail) in around_the_cap(prop::collection::vec(any::<char>(), 1..8))
    ) {
        let line: String = a_times(before).chars().chain(tail).collect();
        let mut fitting = String::new();
        for ch in line.chars() {
            if fitting.len() + ch.len_utf8() > CAP {
                break;
            }
            fitting.push(ch);
        }
        let expected = if fitting.len() == line.len() { fitting } else { cut(&fitting) };
        prop_assert_eq!(display(line.as_bytes()), expected);
    }

    #[test]
    fn long_lines_of_any_bytes_are_cut_without_inventing_characters(
        (before, tail) in around_the_cap(prop::collection::vec(any::<u8>(), 0..12))
    ) {
        let mut line = a_times(before).into_bytes();
        line.extend_from_slice(&tail);
        let shown = display(&line);
        if line.len() <= CAP {
            prop_assert_eq!(shown, String::from_utf8_lossy(&line));
        } else {
            let kept = shown.strip_suffix('…').expect("a cut line ends with …");
            prop_assert!(String::from_utf8_lossy(&line).starts_with(kept));
            // at most the 3 bytes of a split character are dropped before the cap
            prop_assert!(
                (CAP - 3..=CAP).any(|k| String::from_utf8_lossy(&line[..k]) == kept),
                "kept {} chars", kept.chars().count()
            );
        }
    }
}
