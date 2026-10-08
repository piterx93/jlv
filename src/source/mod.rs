//! The log file as bytes: opening it, mapping it into memory, splitting it into lines
//! and turning a line into text that is safe to show.

use std::borrow::Cow;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::iter;
use std::path::Path;

use memchr::memchr_iter;
use memmap2::Mmap;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum OpenError {
    Io(io::Error),
    Directory,
    NotRegularFile,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => f.write_str(&describe(err)),
            Self::Directory => f.write_str("is a directory"),
            Self::NotRegularFile => f.write_str("not a regular file"),
        }
    }
}

impl From<io::Error> for OpenError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Opens a log file, following symlinks, and maps it. The type is checked before
/// opening because opening a FIFO for reading blocks until something writes to it.
pub fn open(path: &Path) -> Result<Source, OpenError> {
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() {
        return Err(OpenError::Directory);
    }
    if !metadata.is_file() {
        return Err(OpenError::NotRegularFile);
    }
    Ok(Source::map(File::open(path)?)?)
}

/// A log file mapped into memory.
#[derive(Debug)]
pub struct Source {
    // `None` for an empty file: a mapping of zero bytes is an error on Linux.
    map: Option<Mmap>,
}

impl Source {
    /// Maps the whole file. A zero-length file gives an empty source.
    pub fn map(file: File) -> io::Result<Self> {
        if file.metadata()?.len() == 0 {
            return Ok(Self { map: None });
        }
        // SAFETY: the mapping is only sound while nobody changes the file, and nothing
        // can promise that for a log file. Appends are harmless, since bytes past the
        // mapped length are never read. Rewrites in place change bytes under a shared
        // slice; every reader treats them as arbitrary bytes, so the worst outcome is
        // garbled text. Truncation is the real hazard: touching a page past the new
        // end raises SIGBUS, which is not handled yet.
        let map = unsafe { Mmap::map(&file)? };
        Ok(Self { map: Some(map) })
    }

    /// The file's content as it was when it was mapped.
    pub fn bytes(&self) -> &[u8] {
        self.map.as_deref().unwrap_or_default()
    }
}

/// Where each line of a byte buffer starts. `\n` ends a line, a `\r` right before it is
/// not part of the line, and a last line without `\n` still counts. The buffer is not
/// stored: pass the same bytes to [`LineIndex::line`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(bytes: &[u8]) -> Self {
        // A line starts at 0 and after every `\n`, unless that is the end of the buffer.
        let starts = iter::once(0)
            .chain(memchr_iter(b'\n', bytes).map(|newline| newline + 1))
            .filter(|&start| start < bytes.len())
            .collect();
        Self { starts }
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// The byte offset of every line's first byte, in order.
    pub fn starts(&self) -> &[usize] {
        &self.starts
    }

    /// Line `i` without its line ending. Panics if `i >= self.len()`.
    pub fn line<'a>(&self, bytes: &'a [u8], i: usize) -> &'a [u8] {
        let end = self.starts.get(i + 1).copied().unwrap_or(bytes.len());
        let line = &bytes[self.starts[i]..end];
        match line.strip_suffix(b"\n") {
            Some(line) => line.strip_suffix(b"\r").unwrap_or(line),
            // only the last line can lack a `\n`, and then a `\r` is content
            None => line,
        }
    }
}

/// Bytes of a line that [`display`] decodes; the rest is cut off.
const DISPLAY_CAP: usize = 64 * 1024;

/// A line as text: invalid UTF-8 becomes U+FFFD, and a line longer than 64 KiB is cut
/// before the first character that does not fit and ends with `…`.
pub fn display(line: &[u8]) -> Cow<'_, str> {
    if line.len() <= DISPLAY_CAP {
        return String::from_utf8_lossy(line);
    }
    let mut text = String::with_capacity(DISPLAY_CAP + '…'.len_utf8());
    let mut used = 0;
    // Only the last chunk has no invalid bytes, and it cannot fit because the line is
    // longer than the cap. So every chunk that gets past the break has an invalid
    // sequence, and the loop always ends at the break, never after the last chunk.
    for chunk in line.utf8_chunks() {
        let valid = chunk.valid();
        let fits = floor_char_boundary(valid, DISPLAY_CAP - used);
        text.push_str(&valid[..fits]);
        used += fits;
        let invalid = chunk.invalid().len();
        if fits < valid.len() || used + invalid > DISPLAY_CAP {
            break;
        }
        text.push(char::REPLACEMENT_CHARACTER);
        used += invalid;
    }
    text.push('…');
    Cow::Owned(text)
}

/// The longest prefix of `text` that is at most `max` bytes and ends on a character
/// boundary. Same as the unstable `str::floor_char_boundary`.
fn floor_char_boundary(text: &str, max: usize) -> usize {
    // 0 is always a boundary, so the fallback is never used.
    (0..=max.min(text.len()))
        .rev()
        .find(|&end| text.is_char_boundary(end))
        .unwrap_or(0)
}

/// Turns `No such file or directory (os error 2)` into `no such file or directory`,
/// the way `cat` and friends phrase it.
fn describe(err: &io::Error) -> String {
    let text = err.to_string();
    let text = text
        .split_once(" (os error ")
        .map_or(text.as_str(), |(reason, _)| reason);
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}
