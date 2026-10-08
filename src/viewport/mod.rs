//! Which rows are on screen and which one is selected. Pure state: it only knows how
//! many rows there are and how many fit on the screen, not what the rows contain.

#[cfg(test)]
mod tests;

/// A window of `height` screen lines over `total` rows, with one selected row.
///
/// After every operation:
/// - there is a selection exactly when `total > 0`, and it is below `total`;
/// - the selected row is on screen: `top <= selected < top + height`;
/// - the screen doesn't run past the last row (`top + height <= total`) unless all
///   rows fit (`total < height`), in which case `top == 0`.
///
/// With `height == 0` nothing is on screen; `top` then follows the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewport {
    total: usize,
    height: usize,
    top: usize,
    // Kept at 0 while there are no rows, so growing from empty selects the first row.
    selected: usize,
}

impl Viewport {
    /// A viewport scrolled to the top with the first row selected.
    pub fn new(total: usize, height: usize) -> Self {
        Self {
            total,
            height,
            top: 0,
            selected: 0,
        }
    }

    /// Number of rows.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Number of screen lines.
    pub fn height(&self) -> usize {
        self.height
    }

    /// The row shown on the first screen line.
    pub fn top(&self) -> usize {
        self.top
    }

    /// The selected row, `None` when there are no rows.
    pub fn selected(&self) -> Option<usize> {
        (self.total > 0).then_some(self.selected)
    }

    /// Moves the selection up by `n` rows, stopping at the first row. The screen
    /// scrolls only as far as needed to show the selection.
    pub fn up(&mut self, n: usize) {
        self.select(self.selected.saturating_sub(n), self.top);
    }

    /// Moves the selection down by `n` rows, stopping at the last row. The screen
    /// scrolls only as far as needed to show the selection.
    pub fn down(&mut self, n: usize) {
        self.select(self.selected.saturating_add(n), self.top);
    }

    /// Moves the selection up by a page of `height` rows (at least one), stopping at
    /// the first row. The screen scrolls by the same amount, stopping at the first
    /// row, so the selection keeps its screen line unless the screen hits the start.
    pub fn page_up(&mut self) {
        self.scroll_up(self.lines());
    }

    /// Moves the selection down by a page of `height` rows (at least one), stopping
    /// at the last row. The screen scrolls by the same amount, stopping where the last
    /// row is on the last line, so the selection keeps its screen line unless the
    /// screen hits the end.
    pub fn page_down(&mut self) {
        self.scroll_down(self.lines());
    }

    /// Like [`page_up`](Self::page_up) with a page of `height / 2` rows (at least one).
    pub fn half_page_up(&mut self) {
        self.scroll_up(self.half_page());
    }

    /// Like [`page_down`](Self::page_down) with a page of `height / 2` rows (at least
    /// one).
    pub fn half_page_down(&mut self) {
        self.scroll_down(self.half_page());
    }

    /// Selects the first row and shows the first screen.
    pub fn go_top(&mut self) {
        self.select(0, 0);
    }

    /// Selects the last row and shows the last full screen.
    pub fn go_bottom(&mut self) {
        self.select(usize::MAX, usize::MAX);
    }

    /// Selects `row` (the last row if `row` is past the end). A row already on screen
    /// is selected without scrolling; any other row is put on the middle screen line
    /// (the upper one of two for an even height), as far as the ends allow.
    pub fn goto(&mut self, row: usize) {
        let row = row.min(self.last_row());
        let top = if (self.top..self.top.saturating_add(self.lines())).contains(&row) {
            self.top
        } else {
            row.saturating_sub((self.lines() - 1) / 2)
        };
        self.select(row, top);
    }

    /// Changes the number of screen lines. The selection stays; the screen scrolls
    /// only as far as needed to show it and to not run past the last row.
    pub fn resize(&mut self, height: usize) {
        self.height = height;
        self.select(self.selected, self.top);
    }

    /// Changes the number of rows, e.g. when the file grows or is truncated. The
    /// selection stays unless its row is gone, then the new last row is selected; with
    /// no rows before, the first row is selected. The screen scrolls only as far as
    /// needed to show the selection and to not run past the last row.
    pub fn set_total(&mut self, total: usize) {
        self.total = total;
        self.select(self.selected, self.top);
    }

    fn scroll_up(&mut self, page: usize) {
        self.select(
            self.selected.saturating_sub(page),
            self.top.saturating_sub(page),
        );
    }

    fn scroll_down(&mut self, page: usize) {
        self.select(
            self.selected.saturating_add(page),
            self.top.saturating_add(page),
        );
    }

    /// Selects `row` and scrolls to `top`, each moved only as far as the invariants
    /// require.
    fn select(&mut self, row: usize, top: usize) {
        self.selected = row.min(self.last_row());
        let lowest = (self.selected + 1).saturating_sub(self.lines());
        let highest = self.selected.min(self.total.saturating_sub(self.lines()));
        // `clamp` can't panic: `selected < total`, or both are 0, so `lowest <= highest`.
        self.top = top.clamp(lowest, highest);
    }

    /// The last row, or 0 when there are none.
    fn last_row(&self) -> usize {
        self.total.saturating_sub(1)
    }

    /// The height to scroll by: a zero-line screen scrolls like a one-line one, which
    /// keeps `top` on the selection.
    fn lines(&self) -> usize {
        self.height.max(1)
    }

    fn half_page(&self) -> usize {
        (self.height / 2).max(1)
    }
}
