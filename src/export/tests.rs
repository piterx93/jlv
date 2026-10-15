use std::io::{self, Write};
use std::ops::Range;

use proptest::collection::vec;
use proptest::prelude::*;

use super::*;

/// What `write_entries` writes for `entries` of `bytes`.
fn written(bytes: &[u8], entries: impl IntoIterator<Item = Range<usize>>) -> Vec<u8> {
    let mut out = Vec::new();
    write_entries(&mut out, bytes, entries).unwrap();
    out
}

/// What `write_entries` writes for `bytes` as one entry.
fn written_whole(bytes: &[u8]) -> Vec<u8> {
    written(bytes, Some(0..bytes.len()))
}

#[test]
fn strips_ansi_escapes_from_every_line_of_an_entry() {
    let entry = b"\x1b[31mERROR\x1b[0m boom\n\x1b[2m\tat App.main\x1b[0m\n";
    assert_eq!(written_whole(entry), b"ERROR boom\n\tat App.main\n");
}

#[test]
fn keeps_crlf_line_endings() {
    let entry = b"\x1b[31mERROR\x1b[0m boom\r\n\tat App.main\r\n";
    assert_eq!(written_whole(entry), b"ERROR boom\r\n\tat App.main\r\n");
}

#[test]
fn writes_only_the_given_entries_in_order() {
    let bytes = b"one\ntwo\nthree\nfour";
    assert_eq!(written(bytes, [4..8, 14..18]), b"two\nfour");
}

/// A disk that fills up after `room` bytes.
struct FullAfter {
    room: usize,
}

impl Write for FullAfter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.room == 0 {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        let n = buf.len().min(self.room);
        self.room -= n;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn returns_the_error_of_a_full_disk() {
    let bytes = b"one\ntwo\nthree\n";
    let err = write_entries(&mut FullAfter { room: 5 }, bytes, [0..4, 4..8, 8..14]).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::StorageFull);
}

/// Bytes with plenty of line endings and escape sequences, cut into entries of one
/// or more lines, some of which are left out.
fn entries() -> impl Strategy<Value = (Vec<u8>, Vec<Range<usize>>)> {
    let byte = prop_oneof![
        Just(b'\n'),
        Just(b'\r'),
        Just(0x1b),
        Just(b'['),
        Just(b']'),
        Just(b'm'),
        Just(0x07),
        any::<u8>(),
    ];
    let flags = || vec(any::<bool>(), 200);
    (vec(byte, 0..200), flags(), flags()).prop_map(|(bytes, splits, kept)| {
        let mut starts = vec![0];
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' && i + 1 < bytes.len() && splits[i] {
                starts.push(i + 1);
            }
        }
        let ends = starts.iter().skip(1).copied().chain([bytes.len()]);
        let ranges = starts
            .iter()
            .copied()
            .zip(ends)
            .zip(kept)
            .filter_map(|((start, end), kept)| (kept && start < end).then_some(start..end))
            .collect();
        (bytes, ranges)
    })
}

proptest! {
    #[test]
    fn writes_text_without_escapes_unchanged((mut bytes, entries) in entries()) {
        for b in bytes.iter_mut().filter(|b| **b == 0x1b) {
            *b = b'x';
        }
        let expected: Vec<u8> = entries.iter().flat_map(|r| bytes[r.clone()].to_vec()).collect();
        prop_assert_eq!(written(&bytes, entries.clone()), expected);
    }

    #[test]
    fn writes_no_escape_and_every_line_break((bytes, entries) in entries()) {
        let out = written(&bytes, entries.clone());
        let breaks = entries.iter().flat_map(|r| &bytes[r.clone()]).filter(|&&b| b == b'\n').count();
        prop_assert!(!out.contains(&0x1b));
        prop_assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), breaks);
    }
}
