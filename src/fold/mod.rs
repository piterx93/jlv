//! Which lines of the entries are rows on screen. Pure state: it only knows how many
//! continuation lines each entry has and which entries are unfolded, not what the
//! lines contain.
//!
//! An entry's first line is always a row. Its continuation lines (a stack trace, say)
//! are rows only while it is unfolded; folded, it shows how many lines it hides.
//! Entries start folded. Folding or unfolding every entry takes constant time, however
//! many entries there are.

#[cfg(test)]
mod tests;

/// The fold state of a list of entries, and the rows it makes of them.
///
/// Every entry is in the global mode, folded or unfolded, except the exceptions, which
/// are in the other one. Switching the mode clears the exceptions, so it doesn't
/// visit the entries.
#[derive(Debug, Clone, Default)]
pub struct Folds {
    /// For each entry, the continuation lines of all entries up to and including it.
    line_ends: Vec<usize>,
    unfolded: bool,
    /// The entries not in the global mode, in ascending order.
    exceptions: Vec<usize>,
    /// For each exception, the continuation lines of all exceptions up to and
    /// including it.
    exception_ends: Vec<usize>,
}

impl Folds {
    /// Adds an entry after the last one, with `continuation_lines` lines after its
    /// first. It is folded, or unfolded if [`unfold_all`](Self::unfold_all) was
    /// applied after the last [`fold_all`](Self::fold_all).
    pub fn push(&mut self, continuation_lines: usize) {
        let before = self.line_ends.last().copied().unwrap_or(0);
        self.line_ends.push(before + continuation_lines);
    }

    /// Keeps the first `entries` entries, folded or unfolded as they are, and drops
    /// the rest. Entries pushed from now on are folded or unfolded as the last
    /// [`fold_all`](Self::fold_all) or [`unfold_all`](Self::unfold_all) says, as
    /// before. Keeps every entry if there are no more than `entries`.
    pub fn truncate(&mut self, entries: usize) {
        self.line_ends.truncate(entries);
        // `exception_ends` sums the exceptions from the first, so its prefix still fits.
        let kept = self.exceptions.partition_point(|&e| e < entries);
        self.exceptions.truncate(kept);
        self.exception_ends.truncate(kept);
    }

    /// How many entries there are.
    pub fn entries(&self) -> usize {
        self.line_ends.len()
    }

    /// How many rows the entries take: one per entry, plus the continuation lines of
    /// the unfolded ones.
    pub fn row_count(&self) -> usize {
        self.row_of(self.line_ends.len(), 0)
    }

    /// `(entry, line)` of `row`: which entry it shows, and which of its lines, 0 for
    /// the first. Panics if `row` is not below [`row_count`](Self::row_count).
    pub fn line_at(&self, row: usize) -> (usize, usize) {
        assert!(row < self.row_count(), "row {row} of {}", self.row_count());
        // The last entry whose first row is at or before `row`; entry 0 starts at row 0.
        let (mut low, mut high) = (0, self.line_ends.len());
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if self.row_of(middle, 0) <= row {
                low = middle;
            } else {
                high = middle;
            }
        }
        (low, row - self.row_of(low, 0))
    }

    /// The row showing line `line` of entry `entry`, the inverse of
    /// [`line_at`](Self::line_at). The line must be shown: the first line of any
    /// entry, or any line of an unfolded one.
    pub fn row_of(&self, entry: usize, line: usize) -> usize {
        let exceptions = self.exceptions.partition_point(|&e| e < entry);
        let exception_lines = exceptions
            .checked_sub(1)
            .map_or(0, |last| self.exception_ends[last]);
        let shown_lines = if self.unfolded {
            self.lines_before(entry) - exception_lines
        } else {
            exception_lines
        };
        entry + shown_lines + line
    }

    /// How many lines of `entry` are hidden: its continuation lines while it is
    /// folded, otherwise 0.
    pub fn hidden_lines(&self, entry: usize) -> usize {
        let excepted = self.exceptions.binary_search(&entry).is_ok();
        if self.unfolded != excepted {
            0
        } else {
            self.continuation_lines(entry)
        }
    }

    /// Unfolds `entry` if it is folded, folds it if it is not. An entry without
    /// continuation lines stays as it is.
    pub fn toggle(&mut self, entry: usize) {
        let lines = self.continuation_lines(entry);
        if lines == 0 {
            return;
        }
        // Exceptions are toggled by hand, a few at a time, so shifting the ones after
        // is cheap.
        match self.exceptions.binary_search(&entry) {
            Ok(at) => {
                self.exceptions.remove(at);
                self.exception_ends.remove(at);
                for end in &mut self.exception_ends[at..] {
                    *end -= lines;
                }
            }
            Err(at) => {
                let before = at.checked_sub(1).map_or(0, |i| self.exception_ends[i]);
                self.exceptions.insert(at, entry);
                self.exception_ends.insert(at, before);
                for end in &mut self.exception_ends[at..] {
                    *end += lines;
                }
            }
        }
    }

    /// No entries, in the mode of `self`: the entries pushed fold as they would here.
    pub fn empty_like(&self) -> Folds {
        Folds {
            line_ends: Vec::new(),
            unfolded: self.unfolded,
            exceptions: Vec::new(),
            exception_ends: Vec::new(),
        }
    }

    /// Folds every entry, and the entries pushed from now on.
    pub fn fold_all(&mut self) {
        self.set_mode(false);
    }

    /// Unfolds every entry, and the entries pushed from now on.
    pub fn unfold_all(&mut self) {
        self.set_mode(true);
    }

    fn set_mode(&mut self, unfolded: bool) {
        self.unfolded = unfolded;
        // `usize` needs no drop, so clearing takes constant time.
        self.exceptions.clear();
        self.exception_ends.clear();
    }

    /// The continuation lines of the entries before `entry`.
    fn lines_before(&self, entry: usize) -> usize {
        entry.checked_sub(1).map_or(0, |last| self.line_ends[last])
    }

    /// The lines of `entry` after its first, folded or not.
    pub fn continuation_lines(&self, entry: usize) -> usize {
        self.lines_before(entry + 1) - self.lines_before(entry)
    }
}
