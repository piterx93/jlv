//! The histogram row.

use jiff::{SignedDuration, tz::TimeZone};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::{MIN_HEIGHT, MIN_WIDTH, Theme};
use crate::app::{App, Focus};
use crate::histogram::Histogram;

/// The fewest columns at the end of the row that name the scale or the selected bucket.
const MIN_LABEL_WIDTH: u16 = 20;
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
    let label_width = label_width(app, area.width, &zone)?;
    let histogram = app.histogram(usize::from(area.width.saturating_sub(label_width)))?;
    let max = histogram.counts.iter().copied().max().unwrap_or(0);
    let label = match histogram.counts.get(selected) {
        Some(&count) if focused => bucket_label(histogram, selected, count, &zone),
        _ => max_label(histogram, max),
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

/// The label column: wide enough for the longest label its buckets could show. More
/// room for the label means fewer, larger buckets, so the counts and with them the
/// labels can grow; the width settles once a label fits.
fn label_width(app: &mut App, width: u16, zone: &TimeZone) -> Option<u16> {
    if let Some(settled) = app.settled_label_width(width) {
        return Some(settled);
    }
    let mut label_width = MIN_LABEL_WIDTH;
    loop {
        let histogram = app.histogram(usize::from(width.saturating_sub(label_width)))?;
        let max = histogram.counts.iter().copied().max().unwrap_or(0);
        let focused = bucket_label(histogram, 0, max, zone);
        let longest = focused.len().max(max_label(histogram, max).len());
        let needed = u16::try_from(longest + 1).unwrap_or(u16::MAX);
        if needed <= label_width || needed >= width {
            let settled = label_width.max(needed.min(width));
            app.settle_label_width(width, settled);
            return Some(settled);
        }
        label_width = needed;
    }
}

fn bucket_label(histogram: &Histogram, bucket: usize, count: u32, zone: &TimeZone) -> String {
    let start = zone.to_datetime(histogram.range(bucket).start);
    format!(
        "{count} at {}",
        start.strftime(start_format(histogram, zone))
    )
}

fn max_label(histogram: &Histogram, max: u32) -> String {
    format!("max {max}/{}", size_label(histogram.size))
}

/// The date shows whenever the first and last bucket start on different dates.
fn start_format(histogram: &Histogram, zone: &TimeZone) -> &'static str {
    let date = |i: usize| zone.to_datetime(histogram.range(i).start).date();
    if histogram.size.as_secs() >= 86_400 {
        "%Y-%m-%d"
    } else if date(0) == date(histogram.counts.len() - 1) {
        "%H:%M:%S"
    } else {
        "%m-%d %H:%M:%S"
    }
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
