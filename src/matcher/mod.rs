//! Finds a pattern in a line of text. Search and the text filter share it: search
//! runs it over every line of an entry, the text filter over the message of the
//! first line and the continuation lines.
//!
//! A line is bytes, so that a line that isn't valid UTF-8 can be searched too. The
//! pattern is plain text or a regex, either of them case-sensitive or not. Only
//! non-empty matches count: a regex that matches the empty string finds nothing
//! where it matches nothing else, and an empty pattern finds nothing at all, which
//! callers take as "no search" or "no filter".

use std::ops::Range;

#[cfg(test)]
mod tests;

/// How the pattern is read. The default is plain text, case-sensitive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// The pattern is a regex (`regex::bytes` syntax) instead of plain text.
    pub regex: bool,
    /// Letters match in either case, non-ASCII ones included (`Ärger` matches `ärger`).
    pub ignore_case: bool,
}

/// A compiled pattern.
#[derive(Debug, Clone)]
pub struct Matcher {
    regex: regex::bytes::Regex,
    pattern: String,
    options: Options,
}

impl Matcher {
    /// Compiles `pattern`. Fails only for an invalid regex, with a one-line message
    /// for the prompt to show; plain text is always valid.
    pub fn new(pattern: &str, options: Options) -> Result<Self, String> {
        let source = if options.regex {
            pattern.to_owned()
        } else {
            regex::escape(pattern)
        };
        let regex = regex::bytes::RegexBuilder::new(&source)
            .case_insensitive(options.ignore_case)
            .build()
            .map_err(|error| format!("invalid regex: {}", last_line(&error.to_string())))?;
        Ok(Self {
            regex,
            pattern: pattern.to_owned(),
            options,
        })
    }

    /// The byte ranges of the matches in `line`, a single line without its line
    /// break, in order and not overlapping, none of them empty. `^` and `$` match at
    /// the start and end of the line.
    pub fn find(&self, line: &[u8]) -> Vec<Range<usize>> {
        self.matches(line).collect()
    }

    /// The pattern as given to [`new`](Self::new).
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// The options given to [`new`](Self::new).
    pub fn options(&self) -> Options {
        self.options
    }

    /// Whether [`find`](Self::find) would find anything in `line`.
    pub fn is_match(&self, line: &[u8]) -> bool {
        self.matches(line).next().is_some()
    }

    fn matches<'a>(&'a self, line: &'a [u8]) -> impl Iterator<Item = Range<usize>> + 'a {
        self.regex
            .find_iter(line)
            .map(|found| found.range())
            .filter(|range| !range.is_empty())
    }
}

/// A regex syntax error spans several lines, quoting the pattern with a caret under
/// the culprit; the prompt has room for the last one, which says what is wrong.
fn last_line(message: &str) -> &str {
    let last = message.lines().last().unwrap_or(message);
    last.strip_prefix("error: ").unwrap_or(last)
}
