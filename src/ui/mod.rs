//! Drawing an [`App`]: a header line, the file's lines and a footer with key hints.

use std::borrow::Cow;
use std::ffi::OsStr;
use std::iter;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::UnicodeWidthChar;

use crate::app::{App, ClassPanel, LevelPanel, Panel, TimePanel};
use crate::filter::{TimeFilter, TimeSpec};
use crate::format::Level;
use crate::source::Origin;

mod abbrev;
mod table;

pub use abbrev::abbreviate;

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
const NO_MATCHES: &str = "no entries match the current filters";
const KEY_HINTS: &str = "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit";
const LEVEL_PANEL_HINTS: &str = "j/k Move  Space/1-6 Toggle  Enter Apply  Esc Cancel";
const CLASS_PANEL_HINTS: &str = "↑/↓ Line (↓ at the end adds one)  Enter Apply  Esc Cancel";
const TIME_PANEL_HINTS: &str = "↑/↓ Preset  Tab File/now  Enter Apply  Esc Cancel";
const TAB_STOP: usize = 4;

/// Draws the whole screen: the header (`jlv ─ <file name> ─ <size> ─ <preset>`, the
/// preset the index reads the file in, then the follow status, then
/// ` ─ format not recognized, showing raw` while `Raw` is shown because detection
/// found no preset, then ` ─ indexing <percent>%` until the index is complete), the
/// rows on screen with `▶` in front of the selected one, and the key hints in the
/// footer.
///
/// While [`App::shows_line_numbers`], each row starts with its line number in the
/// file, in dark gray, right-aligned as wide as the last line number of the entries
/// needs (at least 4) and followed by a space; blank while it isn't known.
///
/// The follow status is ` ─ READ-ONLY` for a gzip file, which is never followed,
/// then ` ─ truncated .gz` if it ended early or was corrupt ([`App::origin`]). For a
/// plain file it is ` ─ WAITING FOR FILE` while the file is gone from its path,
/// otherwise ` ─ FOLLOW ●` while the app follows the file and ` ─ PAUSED +N new`
/// while it doesn't and N new entries came in ([`App::new_entries`]); nothing while
/// it doesn't and none came.
///
/// The row that marks a truncation shows `── file truncated ──` after the gutter.
///
/// Under `Raw` a row is the entry's first line. Under the other presets it is a table
/// row, columns separated by a space: the timestamp in the app's zone
/// (`%Y-%m-%d %H:%M:%S%.3f`, `%H:%M:%S%.3f` below 100 columns), the level (5 wide,
/// in [`level_style`]), the thread if shown (15 wide, a longer name keeps its end
/// after `…`), the class abbreviated to fit (30% of the width, from 24 to 40 columns,
/// cut with `…` if even the abbreviation is too wide) and the message in the rest.
/// The headless entry has only a message, its first line. A folded entry's message
/// ends with ` [+N]`, N being the lines it hides; an unfolded entry's continuation
/// lines are rows of their own, in the message column. Messages are cut at the
/// right edge, or wrapped onto the following lines of the message column; scrolled
/// right, they stop once the longest message on screen ends at the right edge.
/// The lines in between follow the frame's height, so this resizes `app`.
///
/// Below 80×24 the lines are replaced by `terminal too small (80×24 needed)` and `app`
/// keeps its size, so a briefly small terminal doesn't move the view. They are
/// replaced by `no entries match the current filters` once the filters' results are
/// in and no entry passes ([`App::nothing_matches`]).
///
/// While the level panel ([`App::panel`]) is open it is drawn over the middle of
/// the body, a box 16 columns wide and 8 high titled ` Levels `: one line per level,
/// TRACE to FATAL, as ` ▶[x] 1 TRACE ` (`▶` on the cursor's line, `[ ]` for a level
/// the panel hides, then the key that toggles it and the name). The footer then
/// shows the panel's keys, `j/k Move  Space/1-6 Toggle  Enter Apply  Esc Cancel`.
///
/// The class panel is drawn the same way, a box 44 columns wide and 10 high titled
/// ` Classes `: one line per rule line of the panel, as ` ▶com.myco.*` (`▶` on the
/// cursor's line). The footer then shows `↑/↓ Line (↓ at the end adds one)  Enter
/// Apply  Esc Cancel`, or, while the panel has an [error](crate::app::ClassPanel::error),
/// why its rules don't parse.
///
/// The time panel is drawn the same way, a box 40 columns wide and 4 high titled
/// ` Time `: the spec as typed, as ` ▶10m`, and below it the filter it makes, as
/// `  last 10m (file)`, blank while it makes none. The footer then shows `↑/↓
/// Preset  Tab File/now  Enter Apply  Esc Cancel`, or the panel's
/// [error](crate::app::TimePanel::error).
///
/// While the app has a [notice](App::notice), the header ends with ` ─ ` and it.
pub fn render(frame: &mut Frame, app: &mut App, theme: Theme) {
    let area = frame.area();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let progress = app.progress();
    let indexing = if progress.is_complete() {
        String::new()
    } else {
        format!(" ─ indexing {}%", progress.percent())
    };
    let notice = if app.format_unrecognized() {
        " ─ format not recognized, showing raw"
    } else {
        ""
    };
    let key_notice = app
        .notice()
        .map_or(String::new(), |notice| format!(" ─ {notice}"));
    let title = format!(
        "jlv ─ {} ─ {} ─ {}{}{notice}{indexing}{key_notice}",
        printable(app.file_name()),
        human_size(app.size()),
        app.settings().preset,
        follow_status(app),
    );
    let bar = Style::new().reversed();
    frame.render_widget(Line::from(title).style(theme.paint(bar.bold())), header);

    let fits = area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT;
    if !fits {
        frame.render_widget(Line::from(TOO_SMALL), body);
    } else if app.nothing_matches() {
        frame.render_widget(Line::from(NO_MATCHES), body);
    } else {
        table::render(frame, app, theme, body);
    }
    let hints = match app.panel() {
        None => KEY_HINTS.into(),
        Some(Panel::Levels(panel)) => {
            if fits {
                render_level_panel(frame, panel, body);
            }
            LEVEL_PANEL_HINTS.into()
        }
        Some(Panel::Classes(panel)) => {
            if fits {
                render_class_panel(frame, panel, body);
            }
            panel_hints(panel.error.as_deref(), CLASS_PANEL_HINTS)
        }
        Some(Panel::Time(panel)) => {
            if fits {
                render_time_panel(frame, panel, body);
            }
            panel_hints(panel.error.as_deref(), TIME_PANEL_HINTS)
        }
    };
    frame.render_widget(Line::from(hints).style(theme.paint(bar)), footer);
}

fn render_level_panel(frame: &mut Frame, panel: &LevelPanel, body: Rect) {
    let lines: Vec<Line> = Level::ALL
        .iter()
        .zip(1..)
        .map(|(&level, key)| {
            let cursor = if level == panel.cursor { '▶' } else { ' ' };
            let check = if panel.levels.is_on(level) { 'x' } else { ' ' };
            Line::from(format!(" {cursor}[{check}] {key} {:<5} ", level.name()))
        })
        .collect();
    let area = body.centered(Constraint::Length(16), Constraint::Length(8));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(Block::bordered().title(" Levels ")),
        area,
    );
}

fn render_class_panel(frame: &mut Frame, panel: &ClassPanel, body: Rect) {
    let lines: Vec<Line> = panel
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let cursor = if i == panel.cursor { '▶' } else { ' ' };
            Line::from(format!(" {cursor}{}", printable(line)))
        })
        .collect();
    let area = body.centered(Constraint::Length(44), Constraint::Length(10));
    // Keeps the cursor's line in view once there are more lines than the box holds.
    let scroll = panel
        .cursor
        .saturating_sub(usize::from(area.height.saturating_sub(3)));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(Block::bordered().title(" Classes "))
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        area,
    );
}

/// The footer of a panel: why what it edits doesn't apply, if it doesn't, else its
/// key hints.
fn panel_hints<'a>(error: Option<&'a str>, hints: &'static str) -> Cow<'a, str> {
    error.map_or(hints.into(), printable)
}

fn render_time_panel(frame: &mut Frame, panel: &TimePanel, body: Rect) {
    // What the spec makes, once it makes something.
    let filter = TimeSpec::parse(&panel.spec).map(|spec| {
        TimeFilter {
            spec,
            base: panel.base,
        }
        .to_string()
    });
    let lines = vec![
        Line::from(format!(" ▶{}", printable(&panel.spec))),
        Line::from(format!("  {}", filter.unwrap_or_default())),
    ];
    let area = body.centered(Constraint::Length(40), Constraint::Length(4));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(Block::bordered().title(" Time ")),
        area,
    );
}

/// The follow status of the header, with the ` ─ ` before it; empty while paused
/// with no new entries.
fn follow_status(app: &App) -> Cow<'static, str> {
    match app.origin() {
        Origin::Gzip => " ─ READ-ONLY".into(),
        Origin::TruncatedGzip => " ─ READ-ONLY ─ truncated .gz".into(),
        Origin::Plain => plain_follow_status(app),
    }
}

fn plain_follow_status(app: &App) -> Cow<'static, str> {
    if app.waiting_for_file() {
        " ─ WAITING FOR FILE".into()
    } else if app.following() {
        " ─ FOLLOW ●".into()
    } else {
        match app.new_entries() {
            0 => "".into(),
            n => format!(" ─ PAUSED +{n} new").into(),
        }
    }
}

/// The style of a level in the level column (PRD §6.4): TRACE dark gray, DEBUG cyan,
/// INFO green, WARN yellow, ERROR red, FATAL bold white on red, and the terminal's
/// default style for a level jlv doesn't know.
pub fn level_style(level: Option<Level>) -> Style {
    let style = Style::new();
    match level {
        Some(Level::Trace) => style.fg(Color::DarkGray),
        Some(Level::Debug) => style.fg(Color::Cyan),
        Some(Level::Info) => style.fg(Color::Green),
        Some(Level::Warn) => style.fg(Color::Yellow),
        Some(Level::Error) => style.fg(Color::Red),
        Some(Level::Fatal) => style.fg(Color::White).bg(Color::Red).bold(),
        None => style,
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
