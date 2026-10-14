//! What the mouse points at: the parts of the last screen drawn that take clicks and
//! the wheel.

use ratatui::layout::{Position, Rect};

#[cfg(test)]
mod tests;

/// Where the last screen drew what the mouse can act on. Empty (the default) before
/// the first screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HitMap {
    /// The area of the rows, between the header (and the filter bar and histogram
    /// row) and the details pane or the footer.
    pub rows: Rect,
    /// The row of the viewport drawn on each screen line of `rows`, from the top. A
    /// wrapped row is on each of its lines. Fewer entries than `rows.height` when the
    /// rows end above its bottom; empty when none were drawn.
    pub lines: Vec<usize>,
    /// The histogram's buckets on the histogram row: bucket `i` in column `x + i`,
    /// as wide as there are buckets. `None` without a histogram row.
    pub buckets: Option<Rect>,
    /// The details pane, borders included. `None` while it isn't shown.
    pub details: Option<Rect>,
}

/// What is under a cell of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A row of the viewport, as [`Viewport::selected`](crate::viewport::Viewport::selected)
    /// counts them.
    Row(usize),
    /// A bucket of the histogram, counted from the left.
    Bucket(usize),
    /// The details pane.
    Details,
}

impl HitMap {
    /// What was drawn at `column`, `row` of the screen; `None` for the header, the
    /// filter bar, the footer, the histogram's label, the lines below the last row and
    /// anything outside the screen this map was made for.
    pub fn target(&self, column: u16, row: u16) -> Option<Target> {
        let at = Position::new(column, row);
        if let Some(buckets) = self.buckets.filter(|b| b.contains(at)) {
            return Some(Target::Bucket(usize::from(column - buckets.x)));
        }
        if self.details.is_some_and(|d| d.contains(at)) {
            return Some(Target::Details);
        }
        if self.rows.contains(at) {
            let line = usize::from(row - self.rows.y);
            return self.lines.get(line).map(|&row| Target::Row(row));
        }
        None
    }
}
