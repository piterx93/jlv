//! The histogram row.

use jiff::SignedDuration;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::{MIN_HEIGHT, MIN_WIDTH, Theme};
use crate::app::{App, Focus};

/// The columns at the end of the row that name the scale or the selected bucket.
const LABEL_WIDTH: u16 = 20;
const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// The row for a screen of `area` and the number of buckets it shows, `None` while
/// there is no histogram to show.
pub(super) fn row(app: &mut App, area: Rect, theme: Theme) -> Option<(Line<'static>, u16)> {
    let fits = area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT;
    if !fits || !app.shows_histogram() {
        return None;
    }
    let focused = app.focus() == Focus::Histogram;
    let selected = app.histogram_bucket();
    let zone = app.settings().zone.clone();
    let histogram = app.histogram(usize::from(area.width.saturating_sub(LABEL_WIDTH)))?;
    let max = histogram.counts.iter().copied().max().unwrap_or(0);
    let label = match histogram.counts.get(selected) {
        Some(&count) if focused => {
            let start = zone.to_datetime(histogram.range(selected).start);
            let format = if histogram.size.as_secs() < 86_400 {
                "%H:%M:%S"
            } else {
                "%Y-%m-%d"
            };
            format!("{count} at {}", start.strftime(format))
        }
        _ => format!("max {max}/{}", size_label(histogram.size)),
    };
    let reversed = theme.paint(Style::new().reversed());
    let mut spans: Vec<Span> = histogram
        .counts
        .iter()
        .enumerate()
        .map(|(i, &count)| {
            let style = if focused && i == selected {
                reversed
            } else {
                Style::new()
            };
            Span::styled(bar(count, max).to_string(), style)
        })
        .collect();
    // Fewer buckets than `area.width`, so this fits.
    let buckets = u16::try_from(spans.len()).unwrap_or(u16::MAX);
    let pad = usize::from(area.width).saturating_sub(spans.len() + label.chars().count());
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::raw(label));
    Some((Line::from(spans), buckets))
}

/// The bar for `count` in eighths of `max`, rounded up; blank for 0.
fn bar(count: u32, max: u32) -> char {
    if count == 0 {
        return ' ';
    }
    let eighths = (u64::from(count) * 8).div_ceil(u64::from(max));
    BARS[eighths as usize - 1]
}

fn size_label(size: SignedDuration) -> String {
    let secs = size.as_secs();
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86_400 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
}
