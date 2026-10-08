//! Grouping lines into entries: an entry is a line that starts one, followed by the
//! lines that don't (stack traces, wrapped messages, blank lines).

use std::ops::Range;
use std::sync::LazyLock;

use regex::bytes::Regex;

use super::{EntryHead, EntryParser, Preset, strip_ansi};

#[cfg(test)]
mod tests;

/// One log entry: its first line and the continuation lines after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The entry's lines, as numbers of the lines fed to the [`EntryGrouper`], from 0.
    /// Never empty: lines `lines.start + 1..lines.end` are the continuation lines.
    pub lines: Range<usize>,
    /// The columns of the first line, with spans into that line after ANSI stripping.
    /// `None` for the headless entry made of the lines before the first entry start,
    /// and for every entry under `Raw`.
    pub head: Option<EntryHead>,
    /// Whether a continuation line is an exception line (see [`is_exception_line`]).
    /// The first line of an entry with a head starts with its timestamp and is not
    /// checked. A headless entry is the tail of one never seen (a file that
    /// starts mid-trace), so its first line is checked too. Under `Raw` no entry has
    /// an exception.
    pub has_exception: bool,
}

/// Groups lines into entries as they come, in file order. The last entry stays open
/// until a line starts the next one, since more continuation lines may follow.
#[derive(Debug)]
pub struct EntryGrouper {
    parser: Option<EntryParser>,
    open: Option<Entry>,
}

impl EntryGrouper {
    /// A grouper for `preset`'s entry-start lines. Under `Raw` every line is an entry.
    pub fn new(preset: Preset) -> Self {
        EntryGrouper {
            parser: preset.entry_parser(),
            open: None,
        }
    }

    /// Takes the next line, as in the file but without its line ending; ANSI escape
    /// codes are stripped before it is looked at. If the line starts a new entry, the
    /// entry it closes is returned.
    pub fn push(&mut self, line: &[u8]) -> Option<Entry> {
        // Under `Raw` the line's content doesn't matter, so it isn't even stripped.
        let Some(parser) = self.parser else {
            return self.start(None, false);
        };
        let line = strip_ansi(line);
        if let Some(head) = parser.parse(&line) {
            return self.start(Some(head), false);
        }
        match &mut self.open {
            Some(entry) => {
                entry.lines.end += 1;
                // Once flagged, the rest of a long stack trace needn't be matched.
                entry.has_exception = entry.has_exception || is_exception_line(&line);
                None
            }
            None => self.start(None, is_exception_line(&line)),
        }
    }

    /// The last entry, which later lines may still extend. `None` before any line.
    pub fn open_entry(&self) -> Option<&Entry> {
        self.open.as_ref()
    }

    /// Opens an entry at the next line and returns the one it closes.
    fn start(&mut self, head: Option<EntryHead>, has_exception: bool) -> Option<Entry> {
        let first = self.open.as_ref().map_or(0, |entry| entry.lines.end);
        self.open.replace(Entry {
            lines: first..first + 1,
            head,
            has_exception,
        })
    }
}

/// Whether `line` looks like part of a Java stack trace: a frame (`\tat …`), a cause
/// (`Caused by: …`), omitted frames (`\t... 5 more`, `\t... 3 common frames omitted`)
/// or an exception header (`java.lang.IllegalStateException: …`). `line` has no line
/// ending and is already ANSI-stripped.
pub fn is_exception_line(line: &[u8]) -> bool {
    // Anchored, so matching stops at the first byte no alternative can take: a long
    // line costs no more than its leading whitespace and first word. Unlike the preset
    // patterns it keeps Unicode on, so `\w` takes non-ASCII Java identifiers.
    static EXCEPTION_LINE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
            r"^(?:\s+at ",
            r"|Caused by: ",
            r"|\s+\.\.\. \d+ (?:more|common frames omitted)",
            r"|[\w.$]+(?:Exception|Error)(?:: |$))",
        ))
        .expect("the exception pattern is a valid regex")
    });
    EXCEPTION_LINE.is_match(line)
}
