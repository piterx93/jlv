//! Drawing an [`App`]: a header line, the file's lines and a footer with key hints.

use std::borrow::Cow;
use std::ffi::OsStr;
use std::iter;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

use crate::app::App;

#[cfg(test)]
mod tests;

/// Whether the screen may use colors and text attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Styled,
    /// No colors, no bold, no reverse video: every cell in the terminal's default style.
    Plain,
}

impl Theme {
    /// The theme for the value of the `NO_COLOR` environment variable: any non-empty
    /// value turns styling off (<https://no-color.org>).
    pub fn from_no_color(value: Option<&OsStr>) -> Self {
        match value {
            Some(value) if !value.is_empty() => Self::Plain,
            _ => Self::Styled,
        }
    }

    /// `style` if this theme allows styling, the terminal's default style otherwise.
    fn paint(self, style: Style) -> Style {
        match self {
            Self::Styled => style,
            Self::Plain => Style::new(),
        }
    }
}

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;
const TOO_SMALL: &str = "terminal too small (80×24 needed)";
const KEY_HINTS: &str = "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit";
const TAB_STOP: usize = 4;

/// Draws the whole screen: the header (`jlv ─ <file name> ─ <size> ─ Raw`), the lines
/// on screen with `▶` in front of the selected one, and the key hints in the footer.
/// The lines in between follow the frame's height, so this resizes `app`.
///
/// Below 80×24 the lines are replaced by `terminal too small (80×24 needed)` and `app`
/// keeps its size, so a briefly small terminal doesn't move the view.
pub fn render(frame: &mut Frame, app: &mut App, theme: Theme) {
    let area = frame.area();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let title = format!(
        "jlv ─ {} ─ {} ─ Raw",
        printable(app.file_name()),
        human_size(app.size())
    );
    let bar = Style::new().reversed();
    frame.render_widget(Line::from(title).style(theme.paint(bar.bold())), header);
    frame.render_widget(Line::from(KEY_HINTS).style(theme.paint(bar)), footer);

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(Line::from(TOO_SMALL), body);
    } else {
        app.resize(usize::from(body.height));
        render_lines(frame, app, theme, body);
    }
}

/// Draws the lines on screen into `body`, one per row, reading only those lines.
fn render_lines(frame: &mut Frame, app: &App, theme: Theme, body: Rect) {
    let viewport = app.viewport();
    for (i, row) in (viewport.top()..viewport.total()).zip(body.rows()) {
        let selected = viewport.selected() == Some(i);
        let (gutter, style) = if selected {
            ("▶", theme.paint(Style::new().reversed()))
        } else {
            (" ", Style::new())
        };
        let text = printable(&app.line(i)).into_owned();
        frame.render_widget(
            Line::from(vec![gutter.into(), text.into()]).style(style),
            row,
        );
    }
}

/// A byte count for people, 1024-based with one decimal: `0 B`, `1023 B`, `1.0 KB`,
/// `1.8 GB`, up to `EB`. Rounds to the nearest tenth; a value that rounds to 1024 is
/// shown in the next unit.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // At most 6, as u64::MAX is just under 16 EB, so it always indexes UNITS.
    let mut power = bytes.ilog(1024);
    let scale = 1024u128.pow(power);
    // u128, so that multiplying by 10 can't overflow.
    let mut tenths = (u128::from(bytes) * 10 + scale / 2) / scale;
    if tenths == 10 * 1024 {
        power += 1;
        tenths = 10;
    }
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[power as usize])
}

/// Text that is safe to put on the screen: tabs expand to the next 4-column tab stop
/// (wide characters count as two columns) and every other control character, C0, DEL
/// or C1, becomes `�`. Text without either is returned as is.
pub fn printable(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return Cow::Borrowed(text);
    }
    let mut shown = String::with_capacity(text.len());
    let mut column: usize = 0;
    for c in text.chars() {
        if c == '\t' {
            let stop = (column + 1).next_multiple_of(TAB_STOP);
            shown.extend(iter::repeat_n(' ', stop - column));
            column = stop;
        } else {
            let c = if c.is_control() {
                char::REPLACEMENT_CHARACTER
            } else {
                c
            };
            shown.push(c);
            column += c.width().unwrap_or(0);
        }
    }
    Cow::Owned(shown)
}
