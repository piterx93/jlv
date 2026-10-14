//! Where the parts of the screen go.

use ratatui::layout::{Constraint, Layout, Rect};

use super::{MIN_HEIGHT, MIN_WIDTH};

/// The fewest lines the details pane takes, borders included, and the fewest the rows
/// keep.
const MIN_PANE: u16 = 5;

/// The areas of the screen, in the order they are stacked.
#[derive(Debug, Clone, Copy)]
pub(super) struct Screen {
    pub header: Rect,
    pub filters: Option<Rect>,
    pub rows: Rect,
    pub details: Option<Rect>,
    pub footer: Rect,
    /// Whether the area is the minimum size; below it only a notice is shown.
    pub fits: bool,
}

impl Screen {
    /// Splits `area`. The filter bar and the details pane need a screen of at least
    /// the minimum size.
    pub fn new(area: Rect, has_filters: bool, has_details: bool) -> Self {
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let fits = area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT;
        let (filters, body) = if has_filters && fits {
            let [bar, body] =
                Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(body);
            (Some(bar), body)
        } else {
            (None, body)
        };
        let (rows, details) = if has_details && fits {
            let height = (area.height / 3)
                .min(body.height.saturating_sub(MIN_PANE))
                .max(MIN_PANE);
            let [rows, details] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(height)]).areas(body);
            (rows, Some(details))
        } else {
            (body, None)
        };
        Screen {
            header,
            filters,
            rows,
            details,
            footer,
            fits,
        }
    }
}
