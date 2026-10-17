//! Guessing a file's preset from its first lines.

use std::borrow::Cow;

use super::{Preset, is_exception_line, strip_ansi};

#[cfg(test)]
mod tests;

const SAMPLE_LINES: usize = 200;
const MAX_LINES_READ: usize = 2000;

/// Guesses the preset of a file from `lines`, the file's lines in order, without line
/// endings. `None` if no preset fits.
///
/// The sample is the first 200 lines that aren't blank (empty, or only spaces and tabs)
/// once ANSI escape codes are stripped; no line past it is read. Blank lines don't
/// count toward the 200, but at most 2000 lines are read in all, so a file of blank
/// lines isn't read to its end. Continuation lines in the sample, those that start
/// with a space or a tab or are an exception line (see [`is_exception_line`]), are
/// left out. Each preset but `Raw` scores the share of the remaining lines that start
/// an entry in it. The best score wins if it is at least 60%, ties going to the preset
/// that comes first in [`Preset::ALL`]. Otherwise, and when no line remains, it is
/// `None`.
pub fn detect<'a>(lines: impl IntoIterator<Item = &'a [u8]>) -> Option<Preset> {
    // `take` stops pulling once it has enough, so the rest of the file is never read.
    let scored: Vec<Cow<[u8]>> = lines
        .into_iter()
        .take(MAX_LINES_READ)
        .map(strip_ansi)
        .filter(|line| !is_blank(line))
        .take(SAMPLE_LINES)
        .filter(|line| !is_continuation(line))
        .collect();
    Preset::ALL
        .into_iter()
        .filter_map(|preset| Some((preset, preset.entry_parser()?)))
        .map(|(preset, parser)| {
            let entries = scored.iter().filter(|line| parser.parse(line).is_some());
            (preset, entries.count())
        })
        // `max_by_key` keeps the last of equal maximums; reversed, that is the first
        // in `ALL`.
        .rev()
        .max_by_key(|&(_, entries)| entries)
        // 60% in integers, so that exactly 60% isn't lost to rounding.
        .filter(|&(_, entries)| !scored.is_empty() && entries * 5 >= scored.len() * 3)
        .map(|(preset, _)| preset)
}

fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|byte| matches!(byte, b' ' | b'\t'))
}

fn is_continuation(line: &[u8]) -> bool {
    matches!(line.first(), Some(b' ' | b'\t')) || is_exception_line(line)
}
