//! The rows between the header and the footer: each entry's first line, cut into the
//! columns of its preset, or whole under `Raw`, and the continuation lines of the
//! unfolded entries.

use std::borrow::Cow;
use std::iter;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{Theme, abbreviate, level_style, printable};
use crate::app::{App, ShownLine};
use crate::format::{EntryParser, Level, strip_ansi};
use crate::source;

/// The width of the `▶` marker of the selected row.
const MARKER: usize = 1;
/// The narrowest the line numbers are, so that most files never widen them.
const MIN_NUMBER_WIDTH: usize = 4;
const LEVEL_WIDTH: usize = 5;
const THREAD_WIDTH: usize = 15;
const MIN_CLASS_WIDTH: usize = 24;
const MAX_CLASS_WIDTH: usize = 40;
/// The narrowest screen whose timestamps show the date.
const DATE_FROM_WIDTH: usize = 100;
/// Timestamp formats and the width of what they print.
const TIME: (&str, usize) = ("%H:%M:%S%.3f", 12);
const DATE_TIME: (&str, usize) = ("%Y-%m-%d %H:%M:%S%.3f", 23);

/// Draws the rows on screen into `body`, reading only their entries. Sets the app's
/// screen to the rows that fit and keeps its scroll from running past the widest
/// message on screen.
pub(super) fn render(frame: &mut Frame, app: &mut App, theme: Theme, body: Rect) {
    let columns = Columns::new(app, usize::from(body.width));
    let height = usize::from(body.height);
    if app.wraps() {
        fit_wrapped(app, &columns, theme, height);
    } else {
        app.resize(height);
    }

    let viewport = app.viewport();
    let top = viewport.top();
    let shown = top..viewport.total().min(top + height);
    let rows: Vec<Row> = shown.map(|i| columns.row(app, i, theme)).collect();
    // Wrapped, every message is seen whole.
    let overflow = if app.wraps() {
        0
    } else {
        let widest = rows.iter().map(|row| row.text.width()).max();
        widest.unwrap_or(0).saturating_sub(columns.message)
    };
    app.limit_scroll(overflow);

    let selected = app.viewport().selected();
    let lines = (top..).zip(&rows).flat_map(|(i, row)| {
        let selected = selected == Some(i);
        let style = if selected {
            theme.paint(Style::new().reversed())
        } else {
            Style::new()
        };
        let lines = row.lines(&columns, app, selected, theme);
        lines.into_iter().map(move |line| line.style(style))
    });
    for (line, area) in lines.zip(body.rows()) {
        frame.render_widget(line, area);
    }
}

/// Wrapped rows take as many screen lines as their messages need, so how many rows
/// fit depends on which rows they are. Gives the app's screen the rows that fit from
/// its top, or those that fit ending with the selection when it lies below them, or
/// ending with the last row when they reach it, so that the screen doesn't run past
/// the last row.
fn fit_wrapped(app: &mut App, columns: &Columns, theme: Theme, height: usize) {
    let fitting = |rows: &mut dyn Iterator<Item = usize>| {
        let mut used = 0;
        rows.take_while(|&i| {
            // A row taller than the screen shows its first lines.
            used += columns.row(app, i, theme).height(columns).min(height);
            used <= height
        })
        .count()
    };
    let viewport = app.viewport();
    let (top, total) = (viewport.top(), viewport.total());
    let mut rows = fitting(&mut (top..total));
    if top + rows == total {
        rows = fitting(&mut (0..total).rev());
    } else if let Some(selected) = viewport.selected()
        && selected >= top + rows
    {
        rows = fitting(&mut (0..=selected).rev());
    }
    app.resize(rows);
}

/// How wide the columns are on a screen of some width, in terminal columns.
struct Columns {
    /// The width of the line numbers, `None` while they are hidden.
    numbers: Option<usize>,
    /// The width of the line numbers with the space after them and the marker.
    gutter: usize,
    /// `None` under `Raw`, which has only the message column.
    table: Option<Table>,
    /// The width of the columns between the gutter and the message.
    lead: usize,
    /// The width of the message column.
    message: usize,
}

/// The columns before the message, under a preset that has them.
struct Table {
    parser: EntryParser,
    timestamp_format: &'static str,
    timestamp: usize,
    /// `None` while the thread column is hidden.
    thread: Option<usize>,
    class: usize,
}

impl Table {
    /// The width of the columns before the message, each with the space after it.
    fn width(&self) -> usize {
        let thread = self.thread.map_or(0, |thread| thread + 1);
        self.timestamp + 1 + LEVEL_WIDTH + 1 + thread + self.class + 1
    }
}

impl Columns {
    fn new(app: &App, width: usize) -> Self {
        let table = app.settings().preset.entry_parser().map(|parser| {
            let (timestamp_format, timestamp) = if width < DATE_FROM_WIDTH {
                TIME
            } else {
                DATE_TIME
            };
            Table {
                parser,
                timestamp_format,
                timestamp,
                thread: app.shows_thread().then_some(THREAD_WIDTH),
                class: (width * 3 / 10).clamp(MIN_CLASS_WIDTH, MAX_CLASS_WIDTH),
            }
        });
        let lead = table.as_ref().map_or(0, Table::width);
        // As wide as the file's last line needs, rather than the lines on screen, so
        // that the columns don't move while scrolling.
        let numbers = app.shows_line_numbers().then(|| {
            let digits = app
                .last_line_number()
                .map_or(0, |last| last.to_string().len());
            digits.max(MIN_NUMBER_WIDTH)
        });
        let gutter = numbers.map_or(0, |numbers| numbers + 1) + MARKER;
        Columns {
            numbers,
            gutter,
            table,
            lead,
            message: width.saturating_sub(gutter + lead),
        }
    }

    /// Row `i` of `app`, its text made safe to show.
    fn row(&self, app: &App, i: usize, theme: Theme) -> Row {
        let shown = app.row(i);
        let mut row = match shown {
            Some(shown) => self.entry_row(app, shown, theme),
            None => Row::new(Vec::new(), app.line(i).into_owned(), 0),
        };
        row.number = shown.and_then(|shown| shown.number);
        row
    }

    fn entry_row(&self, app: &App, shown: ShownLine, theme: Theme) -> Row {
        // `Raw` doesn't look into lines, so escape codes are shown, made harmless.
        let Some(table) = &self.table else {
            return Row::new(Vec::new(), text(shown.line), shown.hidden);
        };
        let line = strip_ansi(shown.line);
        let head = shown.first.then(|| table.parser.parse(&line)).flatten();
        let Some(head) = head else {
            // Continuation lines and the headless entry have nothing but a message.
            let blank = Span::raw(" ".repeat(self.lead));
            return Row::new(vec![blank], text(&line), shown.hidden);
        };
        let record = shown.record;
        let timestamp = record.timestamp().map_or_else(String::new, |timestamp| {
            let local = app.settings().zone.to_datetime(timestamp);
            local.strftime(table.timestamp_format).to_string()
        });
        let level = record.level().map_or_else(
            || text(&line[head.level]),
            |level| level_name(level).to_owned(),
        );
        let mut cells = vec![
            Span::raw(cell(&timestamp, table.timestamp) + " "),
            Span::styled(
                cell(&level, LEVEL_WIDTH),
                theme.paint(level_style(record.level())),
            ),
            Span::raw(" "),
        ];
        if let Some(width) = table.thread {
            let thread = text(&line[head.thread]);
            cells.push(Span::raw(cell_keeping_end(&thread, width) + " "));
        }
        let class = text(&line[head.class]);
        let class = cell(&abbreviate(&class, table.class), table.class);
        cells.push(Span::raw(class + " "));
        Row::new(cells, text(&line[head.message]), shown.hidden)
    }
}

/// One row's text: the cells before the message, each padded to its column with the
/// space after it, and the message followed by the marker of the lines folded away.
struct Row {
    cells: Vec<Span<'static>>,
    text: String,
    /// Where the marker starts in `text`.
    marker: usize,
    /// The line's number in the file, if known.
    number: Option<u64>,
}

impl Row {
    /// The row of `message` after `cells`, marked ` [+hidden]` unless no lines are
    /// hidden.
    fn new(cells: Vec<Span<'static>>, message: String, hidden: usize) -> Self {
        let marker = message.len();
        let mut text = message;
        if hidden > 0 {
            text += &format!(" [+{hidden}]");
        }
        Row {
            cells,
            text,
            marker,
            number: None,
        }
    }

    /// How many screen lines the row takes when wrapped.
    fn height(&self, columns: &Columns) -> usize {
        wrap(&self.text, columns.message).len()
    }

    /// The row's screen lines: one with the message scrolled as far as the app says
    /// and cut where the marker still fits, or, wrapped, as many as the message and
    /// the marker need, the cells only on the first. The gutter of the first has the
    /// line number, if shown, and `▶` if the row is `selected`.
    fn lines(&self, columns: &Columns, app: &App, selected: bool, theme: Theme) -> Vec<Line<'_>> {
        let parts = if app.wraps() {
            let parts = wrap(&self.text, columns.message);
            parts.into_iter().map(Cow::Borrowed).collect()
        } else {
            let (message, marker) = self.text.split_at(self.marker);
            let width = columns.message.saturating_sub(marker.width());
            let kept = take_columns(skip_columns(message, app.scroll()).chars(), width);
            vec![Cow::Owned(kept + marker)]
        };
        let mut parts = parts.into_iter().map(Span::raw);
        let number = columns.numbers.map(|width| {
            let number = self
                .number
                .map_or_else(String::new, |number| number.to_string());
            let style = theme.paint(Style::new().fg(Color::DarkGray));
            Span::styled(format!("{number:>width$} "), style)
        });
        let marker = Span::raw(if selected { "▶" } else { " " });
        let cells = self.cells.iter().cloned();
        let first = number
            .into_iter()
            .chain([marker])
            .chain(cells)
            .chain(parts.next());
        let indent = " ".repeat(columns.gutter + columns.lead);
        let rest = parts.map(|part| Line::from(vec![Span::raw(indent.clone()), part]));
        iter::once(Line::from_iter(first)).chain(rest).collect()
    }
}

/// `bytes` as text that is safe to show.
fn text(bytes: &[u8]) -> String {
    printable(&source::display(bytes)).into_owned()
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Trace => "TRACE",
        Level::Debug => "DEBUG",
        Level::Info => "INFO",
        Level::Warn => "WARN",
        Level::Error => "ERROR",
        Level::Fatal => "FATAL",
    }
}

/// `text` in a column `width` wide: padded with spaces, or cut on the right with `…`
/// where it doesn't fit.
fn cell(text: &str, width: usize) -> String {
    if text.width() <= width {
        return pad(text, width);
    }
    let kept = take_columns(text.chars(), width.saturating_sub(1));
    pad(&format!("{kept}…"), width)
}

/// [`cell`] cutting on the left instead, which keeps the end of `text`.
fn cell_keeping_end(text: &str, width: usize) -> String {
    if text.width() <= width {
        return pad(text, width);
    }
    let kept: String = take_columns(text.chars().rev(), width.saturating_sub(1));
    let kept: String = kept.chars().rev().collect();
    pad(&format!("…{kept}"), width)
}

/// The first of `chars` that fit into `width` columns.
fn take_columns(chars: impl Iterator<Item = char>, width: usize) -> String {
    let mut used = 0;
    chars
        .take_while(|c| {
            used += c.width().unwrap_or(0);
            used <= width
        })
        .collect()
}

/// `text` followed by the spaces that make it `width` columns wide.
fn pad(text: &str, width: usize) -> String {
    let spaces = width.saturating_sub(text.width());
    format!("{text}{}", " ".repeat(spaces))
}

/// `text` without its first `columns` columns. A wide character cut in two goes.
fn skip_columns(text: &str, columns: usize) -> &str {
    let mut skipped = 0;
    for (at, c) in text.char_indices() {
        if skipped >= columns {
            return &text[at..];
        }
        skipped += c.width().unwrap_or(0);
    }
    ""
}

/// `text` cut into lines of at most `width` columns, at least one. A character wider
/// than `width` gets a line of its own.
fn wrap(text: &str, width: usize) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (at, c) in text.char_indices() {
        let columns = c.width().unwrap_or(0);
        if used + columns > width && at > start {
            lines.push(&text[start..at]);
            (start, used) = (at, 0);
        }
        used += columns;
    }
    lines.push(&text[start..]);
    lines
}
