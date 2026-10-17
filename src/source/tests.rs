use std::fs;
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use flate2::Compression;
use flate2::write::GzEncoder;
use jiff::Timestamp;

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
fn text_lines(text: &str) -> Vec<&str> {
    lines(text.as_bytes())
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect()
}

fn byte_lines(bytes: &[u8]) -> Vec<&[u8]> {
    lines(bytes).collect()
}

/// Where `line`, a slice of `bytes`, starts in it.
fn offset_in(bytes: &[u8], line: &[u8]) -> usize {
    line.as_ptr() as usize - bytes.as_ptr() as usize
}

fn a_times(n: usize) -> String {
    "a".repeat(n)
}

fn cut(kept: &str) -> String {
    format!("{kept}…")
}

// --- mapping ---

/// Sets the modification time of the file at `path` to `rfc3339`.
fn set_mtime(path: &Path, rfc3339: &str) {
    let mtime: Timestamp = rfc3339.parse().unwrap();
    let file = File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::from(mtime)).unwrap();
}

#[test]
fn source_knows_when_the_file_was_modified() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"one\n");
    set_mtime(&path, "2026-10-06T21:15:30Z");
    assert_eq!(
        mapped(&path).modified(),
        Some("2026-10-06T21:15:30Z".parse().unwrap())
    );
}

#[test]
fn empty_source_knows_when_the_file_was_modified() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"");
    set_mtime(&path, "2026-10-06T21:15:30Z");
    assert_eq!(
        mapped(&path).modified(),
        Some("2026-10-06T21:15:30Z".parse().unwrap())
    );
}

/// A file rotated or touched after it was opened doesn't change what the viewer
/// dates its entries from.
#[test]
fn modification_time_is_the_one_from_when_the_file_was_mapped() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"one\n");
    set_mtime(&path, "2026-10-06T21:15:30Z");
    let source = mapped(&path);
    set_mtime(&path, "2026-10-08T09:00:00Z");
    assert_eq!(
        source.modified(),
        Some("2026-10-06T21:15:30Z".parse().unwrap())
    );
}

#[test]
fn opened_source_knows_when_the_file_was_modified() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"one\n");
    set_mtime(&path, "2026-10-06T21:15:30Z");
    assert_eq!(
        open(&path).unwrap().modified(),
        Some("2026-10-06T21:15:30Z".parse().unwrap())
    );
}

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

/// Content over many pages, also of 16 KiB ones, with no zero byte in it.
fn pages_of_text() -> Vec<u8> {
    (0..300_000u32).map(|i| b'a' + (i % 26) as u8).collect()
}

fn truncate(path: &Path, len: u64) {
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(len)
        .unwrap();
}

#[test]
fn bytes_past_a_truncation_read_as_zeros() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &pages_of_text());
    let source = mapped(&path);
    truncate(&path, 0);
    // the last byte first, so the fault is in the middle of a page
    assert_eq!(source.bytes()[299_999], 0);
    assert!(source.bytes().iter().all(|&b| b == 0));
    assert_eq!(source.bytes().len(), 300_000);
}

#[test]
fn bytes_before_a_truncation_are_kept() {
    let dir = TempDir::new().unwrap();
    let content = pages_of_text();
    let path = file_with(&dir, &content);
    let source = mapped(&path);
    truncate(&path, 100_000);
    let (kept, cut) = source.bytes().split_at(100_000);
    assert_eq!(kept, &content[..100_000]);
    assert!(cut.iter().all(|&b| b == 0));
}

#[test]
fn truncation_under_a_reader_on_another_thread_reads_as_zeros() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &pages_of_text());
    let source = mapped(&path);
    truncate(&path, 0);
    let zeros = thread::scope(|s| {
        s.spawn(|| source.bytes().iter().filter(|&&b| b == 0).count())
            .join()
            .unwrap()
    });
    assert_eq!(zeros, 300_000);
}

#[test]
fn every_live_mapping_survives_a_truncation() {
    let dir = TempDir::new().unwrap();
    let first = file_with(&dir, &pages_of_text());
    let second = dir.path().join("second.log");
    fs::write(&second, pages_of_text()).unwrap();
    drop(mapped(&first));
    let sources = [mapped(&first), mapped(&second)];
    truncate(&first, 0);
    truncate(&second, 0);
    for source in &sources {
        assert!(source.bytes().iter().all(|&b| b == 0));
    }
}

#[test]
fn a_dropped_mapping_gives_its_place_to_the_next() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &pages_of_text());
    for _ in 0..300 {
        drop(mapped(&path));
    }
    let source = mapped(&path);
    truncate(&path, 0);
    assert!(source.bytes().iter().all(|&b| b == 0));
}

#[test]
fn an_empty_mapping_takes_no_place_from_another() {
    let dir = TempDir::new().unwrap();
    let empty = dir.path().join("empty.log");
    fs::write(&empty, b"").unwrap();
    let path = file_with(&dir, &pages_of_text());
    // What a file truncated between `Source::map`'s size check and the mapping gives.
    // SAFETY: nothing changes the empty file.
    let nothing = Guarded::new(unsafe { Mmap::map(&File::open(&empty).unwrap()).unwrap() });
    let source = mapped(&path);
    drop(nothing);
    truncate(&path, 0);
    assert!(source.bytes().iter().all(|&b| b == 0));
}

/// Faults past the end of a mapping the guard doesn't know. Run in a process of its own
/// by [`a_fault_outside_every_mapping_still_kills_the_process`].
#[test]
#[ignore = "dies of SIGBUS on purpose"]
fn fault_outside_every_mapping() {
    if env::var_os("JLV_FAULT_OUTSIDE").is_none() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let _guarded = mapped(&file_with(&dir, b"installs the handler\n"));
    let other = dir.path().join("other.log");
    fs::write(&other, pages_of_text()).unwrap();
    // SAFETY: none, the truncation below is the point.
    let unguarded = unsafe { Mmap::map(&File::open(&other).unwrap()).unwrap() };
    truncate(&other, 0);
    std::hint::black_box(unguarded[299_999]);
}

#[test]
fn a_fault_outside_every_mapping_still_kills_the_process() {
    let mut child = Command::new(env::current_exe().unwrap())
        .args(["--exact", "source::tests::fault_outside_every_mapping"])
        .args(["--ignored", "--quiet"])
        .env("JLV_FAULT_OUTSIDE", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("the fault is handled over and over instead of killing the process");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.signal(), Some(libc::SIGBUS));
}

#[test]
fn source_as_bytes_is_the_mapped_content() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"INFO one\r\nWARN two\nERROR three");
    let source = mapped(&path);
    let bytes: &[u8] = source.as_ref();
    assert_eq!(bytes, b"INFO one\r\nWARN two\nERROR three");
}

#[test]
fn lines_of_the_mapped_file_are_slices_of_it() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, b"INFO one\r\nWARN two\nERROR three");
    let source = mapped(&path);
    let starts: Vec<usize> = lines(source.bytes())
        .map(|line| offset_in(source.bytes(), line))
        .collect();
    assert_eq!(starts, [0, 10, 19]);
}

// --- line splitting ---

#[test]
fn empty_input_has_no_lines() {
    assert_eq!(lines(b"").next(), None);
}

#[test]
fn lone_newline_is_one_empty_line() {
    assert_eq!(text_lines("\n"), [""]);
}

#[test]
fn text_without_newline_is_one_line() {
    assert_eq!(text_lines("a"), ["a"]);
}

#[test]
fn trailing_newline_does_not_start_another_line() {
    assert_eq!(text_lines("a\n"), ["a"]);
}

#[test]
fn last_line_without_newline_still_counts() {
    assert_eq!(text_lines("first\nsecond"), ["first", "second"]);
}

#[test]
fn consecutive_newlines_give_empty_lines() {
    assert_eq!(text_lines("a\n\n\nb\n"), ["a", "", "", "b"]);
}

#[test]
fn crlf_endings_are_not_part_of_the_line() {
    assert_eq!(text_lines("a\r\nb\r\n"), ["a", "b"]);
}

#[test]
fn mixed_line_endings_are_both_understood() {
    assert_eq!(text_lines("a\r\nb\nc\r\nd"), ["a", "b", "c", "d"]);
}

#[test]
fn lone_cr_in_the_middle_is_not_a_separator() {
    assert_eq!(text_lines("a\rb\nc"), ["a\rb", "c"]);
}

#[test]
fn only_one_cr_before_the_newline_is_dropped() {
    assert_eq!(text_lines("a\r\r\n"), ["a\r"]);
}

#[test]
fn cr_at_the_very_end_without_newline_is_kept() {
    assert_eq!(text_lines("a\nb\r"), ["a", "b\r"]);
}

#[test]
fn crlf_alone_is_one_empty_line() {
    assert_eq!(text_lines("\r\n"), [""]);
}

#[test]
fn lines_start_after_each_newline() {
    let bytes = b"ab\r\ncd\n\nx";
    let starts: Vec<usize> = lines(bytes).map(|line| offset_in(bytes, line)).collect();
    assert_eq!(starts, [0, 4, 7, 8]);
}

#[test]
fn lines_run_out_after_the_last_line() {
    let mut lines = lines(b"one\ntwo\n");
    assert_eq!(lines.next(), Some(&b"one"[..]));
    assert_eq!(lines.next(), Some(&b"two"[..]));
    assert_eq!(lines.next(), None);
    assert_eq!(lines.next(), None);
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
fn many_lines_are_all_read() {
    let text: String = (0..10_000).map(|i| format!("line {i}\n")).collect();
    assert_eq!(lines(text.as_bytes()).count(), 10_000);
    assert_eq!(lines(text.as_bytes()).last(), Some(&b"line 9999"[..]));
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
    fn lines_never_panic_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..1024)
    ) {
        lines(&bytes).for_each(drop);
    }

    #[test]
    fn rejoined_lines_give_back_the_input(bytes in log_bytes()) {
        let joined = byte_lines(&bytes).join(&b'\n');
        prop_assert_eq!(joined, rejoined_expectation(&bytes));
    }

    #[test]
    fn every_line_starts_at_the_start_or_after_a_newline(bytes in log_bytes()) {
        for line in lines(&bytes) {
            let start = offset_in(&bytes, line);
            prop_assert!(start < bytes.len());
            prop_assert!(!line.contains(&b'\n'));
            prop_assert!(start == 0 || bytes[start - 1] == b'\n');
        }
    }

    #[test]
    fn every_newline_ends_a_line(bytes in log_bytes()) {
        let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
        let unterminated = usize::from(!bytes.is_empty() && bytes.last() != Some(&b'\n'));
        prop_assert_eq!(lines(&bytes).count(), newlines + unterminated);
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

// --- gzip ---

fn gzip_at(level: Compression, bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), level);
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    gzip_at(Compression::fast(), bytes)
}

/// `n` lines of log text, each a little different.
fn log_text(n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| {
            format!(
                "12:00:{:02}.{i:03} [main] INFO  com.example.App -- request {i}\n",
                i % 60
            )
        })
        .collect::<String>()
        .into_bytes()
}

/// Opens `path` with `temp` as the temp dir, ignoring the progress.
fn open_with_temp(path: &Path, temp: &TempDir) -> Source {
    open_in(path, temp.path(), |_| {}).unwrap()
}

fn is_empty_dir(dir: &TempDir) -> bool {
    fs::read_dir(dir.path()).unwrap().next().is_none()
}

#[test]
fn gzip_content_is_detected_without_the_extension() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &gzip(&log_text(3)));
    assert_eq!(open_with_temp(&path, &dir).origin(), Origin::Gzip);
}

#[test]
fn plain_file_named_gz_opens_as_plain() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app.log.gz");
    fs::write(&path, log_text(3)).unwrap();
    let source = open_with_temp(&path, &dir);
    assert_eq!(source.origin(), Origin::Plain);
    assert_eq!(source.bytes(), log_text(3));
}

#[test]
fn members_of_a_multi_member_gzip_follow_each_other() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &[gzip(b"first\n"), gzip(b"second\n")].concat());
    let source = open_with_temp(&path, &dir);
    assert_eq!(source.bytes(), b"first\nsecond\n");
    assert_eq!(source.origin(), Origin::Gzip);
}

#[test]
fn gzip_of_nothing_opens_to_no_bytes() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &gzip(b""));
    let source = open_with_temp(&path, &dir);
    assert_eq!(source.bytes(), b"");
    assert_eq!(source.origin(), Origin::Gzip);
}

#[test]
fn truncated_gzip_keeps_what_was_decompressed() {
    let dir = TempDir::new().unwrap();
    let text = log_text(200);
    let compressed = gzip(&text);
    let path = file_with(&dir, &compressed[..compressed.len() - 20]);
    let source = open_with_temp(&path, &dir);
    assert_eq!(source.origin(), Origin::TruncatedGzip);
    assert!(!source.bytes().is_empty());
    assert!(text.starts_with(source.bytes()));
}

/// The checksum is only checked at the end, after all the content came out.
#[test]
fn gzip_with_a_wrong_checksum_keeps_its_content_and_counts_as_truncated() {
    let dir = TempDir::new().unwrap();
    let mut compressed = gzip(&log_text(3));
    // the trailer is the CRC-32 of the content, then its length, 4 bytes each
    let crc = compressed.len() - 8;
    compressed[crc] ^= 0xff;
    let path = file_with(&dir, &compressed);
    let source = open_with_temp(&path, &dir);
    assert_eq!(source.origin(), Origin::TruncatedGzip);
    assert_eq!(source.bytes(), log_text(3));
}

/// A time-only log in a gzip file is dated from when the gzip file was written, not
/// from when it was decompressed.
#[test]
fn gzip_source_has_the_modification_time_of_the_compressed_file() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &gzip(&log_text(3)));
    set_mtime(&path, "2026-10-06T21:15:30Z");
    assert_eq!(
        open_with_temp(&path, &dir).modified(),
        Some("2026-10-06T21:15:30Z".parse().unwrap())
    );
}

/// Stored without compression, the file is as big as its content, big enough to be
/// read in several steps.
#[test]
fn progress_counts_the_compressed_bytes_read_up_to_the_whole_file() {
    let dir = TempDir::new().unwrap();
    let compressed = gzip_at(Compression::none(), &log_text(16_000));
    let path = file_with(&dir, &compressed);
    let mut reports = Vec::new();
    open_in(&path, dir.path(), |read| reports.push(read)).unwrap();
    let whole = compressed.len() as u64;
    assert!(
        reports.iter().any(|&read| 0 < read && read < whole),
        "nothing between 0 and {whole}: {reports:?}"
    );
    assert!(reports.is_sorted(), "{reports:?}");
    assert_eq!(reports.last(), Some(&whole));
}

#[test]
fn temp_file_is_removed_when_the_source_is_dropped() {
    let dir = TempDir::new().unwrap();
    let temp = TempDir::new().unwrap();
    let path = file_with(&dir, &gzip(&log_text(3)));
    drop(open_with_temp(&path, &temp));
    assert!(is_empty_dir(&temp));
}

#[test]
fn temp_file_is_removed_when_a_panic_unwinds_through_decompression() {
    let dir = TempDir::new().unwrap();
    let temp = TempDir::new().unwrap();
    let compressed = gzip(&log_text(3));
    let path = file_with(&dir, &compressed);
    let whole = compressed.len() as u64;
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        open_in(&path, temp.path(), |read| {
            if read == whole {
                panic!("interrupted after reading the whole file");
            }
        })
    }));
    let payload = result.expect_err("progress never reached the whole file");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"interrupted after reading the whole file")
    );
    assert!(is_empty_dir(&temp));
}

#[test]
fn unusable_temp_dir_is_an_error_naming_it() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &gzip(&log_text(3)));
    let missing = dir.path().join("missing");
    let err = open_in(&path, &missing, |_| {}).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "cannot decompress into {}: no such file or directory",
            missing.display()
        )
    );
}

#[test]
fn plain_file_opens_without_the_temp_dir() {
    let dir = TempDir::new().unwrap();
    let path = file_with(&dir, &log_text(3));
    let source = open_in(&path, &dir.path().join("missing"), |_| {}).unwrap();
    assert_eq!(source.bytes(), log_text(3));
}

proptest! {
    #[test]
    fn any_gzipped_content_opens_as_that_content(
        content in prop::collection::vec(any::<u8>(), 0..2048)
    ) {
        let dir = TempDir::new().unwrap();
        let path = file_with(&dir, &gzip(&content));
        let source = open_with_temp(&path, &dir);
        prop_assert_eq!(source.bytes(), content.as_slice());
    }

    #[test]
    fn anything_after_the_gzip_magic_opens_without_an_error(
        rest in prop::collection::vec(any::<u8>(), 0..512)
    ) {
        let dir = TempDir::new().unwrap();
        let path = file_with(&dir, &[b"\x1f\x8b".as_slice(), &rest].concat());
        let opened = open_in(&path, dir.path(), |_| ());
        prop_assert!(opened.is_ok(), "{:?}", opened.err());
    }
}
