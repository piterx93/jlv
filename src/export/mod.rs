//! Writing entries of a log to a file, as `F8` / `e` exports what the filters show.

use std::io::{self, Write};
use std::ops::Range;

use crate::format::strip_ansi_into;

#[cfg(test)]
mod tests;

/// Writes the entries of `bytes` at the byte ranges `entries` to `out`, in the order
/// given, streamed as they are read. Every line keeps its line ending (`\n` or
/// `\r\n`) and loses its ANSI escape codes, stripped line by line as
/// [`strip_ansi`](crate::format::strip_ansi) strips them. Each range starts at the
/// start of a line and ends at the start of a line or at the end of `bytes`.
///
/// The first error from `out` ends the export and is returned.
pub fn write_entries(
    out: &mut impl Write,
    bytes: &[u8],
    entries: impl IntoIterator<Item = Range<usize>>,
) -> io::Result<()> {
    let mut buffer = Vec::new();
    for range in entries {
        for line in bytes[range].split_inclusive(|&b| b == b'\n') {
            let (text, ending) = split_ending(line);
            out.write_all(strip_ansi_into(text, &mut buffer))?;
            out.write_all(ending)?;
        }
    }
    Ok(())
}

/// `line` split before its `\n` or `\r\n`, if it has one.
fn split_ending(line: &[u8]) -> (&[u8], &[u8]) {
    let Some(without_lf) = line.strip_suffix(b"\n") else {
        return (line, b"");
    };
    let text_len = without_lf
        .strip_suffix(b"\r")
        .map_or(without_lf.len(), <[u8]>::len);
    line.split_at(text_len)
}
