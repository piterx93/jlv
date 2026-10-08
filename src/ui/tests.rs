use std::ffi::{OsStr, OsString};
use std::ops::RangeInclusive;
use std::os::unix::ffi::OsStringExt;

use insta::assert_snapshot;
use proptest::prelude::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Cell;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::Color;
use unicode_width::UnicodeWidthStr;

use super::*;
use crate::app::{Action, action};
use crate::format::Preset;
use crate::test_support::{
    EXAMPLES, app, app_in, char_key, detected_app, key, named_app, numbered_log, row, unindexed_app,
};

fn draw(terminal: &mut Terminal<TestBackend>, app: &mut App, theme: Theme) {
    terminal.draw(|frame| render(frame, app, theme)).unwrap();
}

fn drawn(app: &mut App, width: u16, height: u16, theme: Theme) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    draw(&mut terminal, app, theme);
    terminal
}

/// The 80×24 terminal after pressing `keys`, drawn after each.
fn terminal_after(app: &mut App, keys: &[KeyEvent]) -> Terminal<TestBackend> {
    let mut terminal = drawn(app, 80, 24, Theme::Styled);
    for &key in keys {
        app.apply(action(key).unwrap());
        draw(&mut terminal, app, Theme::Styled);
    }
    terminal
}

/// The 80×24 screen after pressing `keys`, as text.
fn screen_after(app: &mut App, keys: &[KeyEvent]) -> String {
    terminal_after(app, keys).backend().to_string()
}

/// Screen lines `lines` of the 80×24 screen after pressing `keys`, as [`row`] shows
/// them, one per line.
fn lines_after(app: &mut App, keys: &[KeyEvent], lines: RangeInclusive<u16>) -> String {
    let terminal = terminal_after(app, keys);
    let rows: Vec<String> = lines.map(|y| row(&terminal, y)).collect();
    rows.join("\n")
}

/// How `numbered_log` shows line `n` (1-based) on screen, `▶` when it is selected.
fn shown(n: usize, selected: bool) -> String {
    let gutter = if selected { '▶' } else { ' ' };
    format!("{gutter}12:00:00.{n:03} [main] INFO  com.example.App - step {n}")
}

fn hundred_lines() -> App {
    app(numbered_log(100).as_bytes())
}

// --- whole screens ---

#[test]
fn first_screen_shows_the_top_of_the_file() {
    assert_snapshot!(screen_after(&mut hundred_lines(), &[]), @r#"
    "jlv ─ app.log ─ 5.1 KB ─ Raw                                                    "
    "▶12:00:00.001 [main] INFO  com.example.App - step 1                             "
    " 12:00:00.002 [main] INFO  com.example.App - step 2                             "
    " 12:00:00.003 [main] INFO  com.example.App - step 3                             "
    " 12:00:00.004 [main] INFO  com.example.App - step 4                             "
    " 12:00:00.005 [main] INFO  com.example.App - step 5                             "
    " 12:00:00.006 [main] INFO  com.example.App - step 6                             "
    " 12:00:00.007 [main] INFO  com.example.App - step 7                             "
    " 12:00:00.008 [main] INFO  com.example.App - step 8                             "
    " 12:00:00.009 [main] INFO  com.example.App - step 9                             "
    " 12:00:00.010 [main] INFO  com.example.App - step 10                            "
    " 12:00:00.011 [main] INFO  com.example.App - step 11                            "
    " 12:00:00.012 [main] INFO  com.example.App - step 12                            "
    " 12:00:00.013 [main] INFO  com.example.App - step 13                            "
    " 12:00:00.014 [main] INFO  com.example.App - step 14                            "
    " 12:00:00.015 [main] INFO  com.example.App - step 15                            "
    " 12:00:00.016 [main] INFO  com.example.App - step 16                            "
    " 12:00:00.017 [main] INFO  com.example.App - step 17                            "
    " 12:00:00.018 [main] INFO  com.example.App - step 18                            "
    " 12:00:00.019 [main] INFO  com.example.App - step 19                            "
    " 12:00:00.020 [main] INFO  com.example.App - step 20                            "
    " 12:00:00.021 [main] INFO  com.example.App - step 21                            "
    " 12:00:00.022 [main] INFO  com.example.App - step 22                            "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn capital_g_shows_the_last_screen() {
    let out = screen_after(&mut hundred_lines(), &[char_key('G')]);
    assert_snapshot!(out, @r#"
    "jlv ─ app.log ─ 5.1 KB ─ Raw                                                    "
    " 12:00:00.079 [main] INFO  com.example.App - step 79                            "
    " 12:00:00.080 [main] INFO  com.example.App - step 80                            "
    " 12:00:00.081 [main] INFO  com.example.App - step 81                            "
    " 12:00:00.082 [main] INFO  com.example.App - step 82                            "
    " 12:00:00.083 [main] INFO  com.example.App - step 83                            "
    " 12:00:00.084 [main] INFO  com.example.App - step 84                            "
    " 12:00:00.085 [main] INFO  com.example.App - step 85                            "
    " 12:00:00.086 [main] INFO  com.example.App - step 86                            "
    " 12:00:00.087 [main] INFO  com.example.App - step 87                            "
    " 12:00:00.088 [main] INFO  com.example.App - step 88                            "
    " 12:00:00.089 [main] INFO  com.example.App - step 89                            "
    " 12:00:00.090 [main] INFO  com.example.App - step 90                            "
    " 12:00:00.091 [main] INFO  com.example.App - step 91                            "
    " 12:00:00.092 [main] INFO  com.example.App - step 92                            "
    " 12:00:00.093 [main] INFO  com.example.App - step 93                            "
    " 12:00:00.094 [main] INFO  com.example.App - step 94                            "
    " 12:00:00.095 [main] INFO  com.example.App - step 95                            "
    " 12:00:00.096 [main] INFO  com.example.App - step 96                            "
    " 12:00:00.097 [main] INFO  com.example.App - step 97                            "
    " 12:00:00.098 [main] INFO  com.example.App - step 98                            "
    " 12:00:00.099 [main] INFO  com.example.App - step 99                            "
    "▶12:00:00.100 [main] INFO  com.example.App - step 100                           "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn page_down_shows_the_next_screen() {
    let out = screen_after(&mut hundred_lines(), &[key(KeyCode::PageDown)]);
    assert_snapshot!(out, @r#"
    "jlv ─ app.log ─ 5.1 KB ─ Raw                                                    "
    "▶12:00:00.023 [main] INFO  com.example.App - step 23                            "
    " 12:00:00.024 [main] INFO  com.example.App - step 24                            "
    " 12:00:00.025 [main] INFO  com.example.App - step 25                            "
    " 12:00:00.026 [main] INFO  com.example.App - step 26                            "
    " 12:00:00.027 [main] INFO  com.example.App - step 27                            "
    " 12:00:00.028 [main] INFO  com.example.App - step 28                            "
    " 12:00:00.029 [main] INFO  com.example.App - step 29                            "
    " 12:00:00.030 [main] INFO  com.example.App - step 30                            "
    " 12:00:00.031 [main] INFO  com.example.App - step 31                            "
    " 12:00:00.032 [main] INFO  com.example.App - step 32                            "
    " 12:00:00.033 [main] INFO  com.example.App - step 33                            "
    " 12:00:00.034 [main] INFO  com.example.App - step 34                            "
    " 12:00:00.035 [main] INFO  com.example.App - step 35                            "
    " 12:00:00.036 [main] INFO  com.example.App - step 36                            "
    " 12:00:00.037 [main] INFO  com.example.App - step 37                            "
    " 12:00:00.038 [main] INFO  com.example.App - step 38                            "
    " 12:00:00.039 [main] INFO  com.example.App - step 39                            "
    " 12:00:00.040 [main] INFO  com.example.App - step 40                            "
    " 12:00:00.041 [main] INFO  com.example.App - step 41                            "
    " 12:00:00.042 [main] INFO  com.example.App - step 42                            "
    " 12:00:00.043 [main] INFO  com.example.App - step 43                            "
    " 12:00:00.044 [main] INFO  com.example.App - step 44                            "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn empty_file_shows_an_empty_body() {
    let mut app = named_app("/var/log/empty.log", b"");
    assert_snapshot!(screen_after(&mut app, &[]), @r#"
    "jlv ─ empty.log ─ 0 B ─ Raw                                                     "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn header_shows_the_indexing_progress_until_the_index_is_complete() {
    let mut app = unindexed_app(Preset::Raw, numbered_log(100).as_bytes());
    assert_snapshot!(screen_after(&mut app, &[]), @r#"
    "jlv ─ app.log ─ 5.1 KB ─ Raw ─ indexing 0%                                      "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn header_names_the_preset_the_file_is_read_in() {
    let mut app = app_in(
        Preset::Logback,
        b"2026-10-07 12:00:00.123 [main] INFO  com.example.App - Started\n",
    );
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(row(&terminal, 0), "jlv ─ app.log ─ 63 B ─ Logback");
}

#[test]
fn empty_file_shows_no_indexing_progress() {
    let mut app = unindexed_app(Preset::Raw, b"");
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(row(&terminal, 0), "jlv ─ app.log ─ 0 B ─ Raw");
}

#[test]
fn too_small_terminal_says_so_instead_of_showing_lines() {
    let terminal = drawn(&mut hundred_lines(), 60, 9, Theme::Styled);
    assert_snapshot!(terminal.backend().to_string(), @r#"
    "jlv ─ app.log ─ 5.1 KB ─ Raw                                "
    "terminal too small (80×24 needed)                           "
    "                                                            "
    "                                                            "
    "                                                            "
    "                                                            "
    "                                                            "
    "                                                            "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom"
    "#);
}

/// The buffer's debug view lists every change of style: the header in bold reverse
/// video, the selected line and the footer in reverse video, the rest plain.
#[test]
fn styled_theme_marks_header_selection_and_footer() {
    let terminal = drawn(&mut hundred_lines(), 80, 24, Theme::Styled);
    assert_snapshot!(format!("{:?}", terminal.backend().buffer()), @r#"
    Buffer {
        area: Rect { x: 0, y: 0, width: 80, height: 24 },
        content: [
            "jlv ─ app.log ─ 5.1 KB ─ Raw                                                    ",
            "▶12:00:00.001 [main] INFO  com.example.App - step 1                             ",
            " 12:00:00.002 [main] INFO  com.example.App - step 2                             ",
            " 12:00:00.003 [main] INFO  com.example.App - step 3                             ",
            " 12:00:00.004 [main] INFO  com.example.App - step 4                             ",
            " 12:00:00.005 [main] INFO  com.example.App - step 5                             ",
            " 12:00:00.006 [main] INFO  com.example.App - step 6                             ",
            " 12:00:00.007 [main] INFO  com.example.App - step 7                             ",
            " 12:00:00.008 [main] INFO  com.example.App - step 8                             ",
            " 12:00:00.009 [main] INFO  com.example.App - step 9                             ",
            " 12:00:00.010 [main] INFO  com.example.App - step 10                            ",
            " 12:00:00.011 [main] INFO  com.example.App - step 11                            ",
            " 12:00:00.012 [main] INFO  com.example.App - step 12                            ",
            " 12:00:00.013 [main] INFO  com.example.App - step 13                            ",
            " 12:00:00.014 [main] INFO  com.example.App - step 14                            ",
            " 12:00:00.015 [main] INFO  com.example.App - step 15                            ",
            " 12:00:00.016 [main] INFO  com.example.App - step 16                            ",
            " 12:00:00.017 [main] INFO  com.example.App - step 17                            ",
            " 12:00:00.018 [main] INFO  com.example.App - step 18                            ",
            " 12:00:00.019 [main] INFO  com.example.App - step 19                            ",
            " 12:00:00.020 [main] INFO  com.example.App - step 20                            ",
            " 12:00:00.021 [main] INFO  com.example.App - step 21                            ",
            " 12:00:00.022 [main] INFO  com.example.App - step 22                            ",
            "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            ",
        ],
        styles: [
            x: 0, y: 0, fg: Reset, bg: Reset, underline: Reset, modifier: BOLD | REVERSED,
            x: 0, y: 1, fg: Reset, bg: Reset, underline: Reset, modifier: REVERSED,
            x: 0, y: 2, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
            x: 0, y: 23, fg: Reset, bg: Reset, underline: Reset, modifier: REVERSED,
        ]
    }
    "#);
}

/// The buffer's debug view lists every change of style; a single entry at 0,0 means
/// the whole screen is in the terminal's default style.
#[test]
fn plain_theme_draws_the_screen_without_any_style() {
    let terminal = drawn(&mut hundred_lines(), 80, 24, Theme::Plain);
    assert_snapshot!(format!("{:?}", terminal.backend().buffer()), @r#"
    Buffer {
        area: Rect { x: 0, y: 0, width: 80, height: 24 },
        content: [
            "jlv ─ app.log ─ 5.1 KB ─ Raw                                                    ",
            "▶12:00:00.001 [main] INFO  com.example.App - step 1                             ",
            " 12:00:00.002 [main] INFO  com.example.App - step 2                             ",
            " 12:00:00.003 [main] INFO  com.example.App - step 3                             ",
            " 12:00:00.004 [main] INFO  com.example.App - step 4                             ",
            " 12:00:00.005 [main] INFO  com.example.App - step 5                             ",
            " 12:00:00.006 [main] INFO  com.example.App - step 6                             ",
            " 12:00:00.007 [main] INFO  com.example.App - step 7                             ",
            " 12:00:00.008 [main] INFO  com.example.App - step 8                             ",
            " 12:00:00.009 [main] INFO  com.example.App - step 9                             ",
            " 12:00:00.010 [main] INFO  com.example.App - step 10                            ",
            " 12:00:00.011 [main] INFO  com.example.App - step 11                            ",
            " 12:00:00.012 [main] INFO  com.example.App - step 12                            ",
            " 12:00:00.013 [main] INFO  com.example.App - step 13                            ",
            " 12:00:00.014 [main] INFO  com.example.App - step 14                            ",
            " 12:00:00.015 [main] INFO  com.example.App - step 15                            ",
            " 12:00:00.016 [main] INFO  com.example.App - step 16                            ",
            " 12:00:00.017 [main] INFO  com.example.App - step 17                            ",
            " 12:00:00.018 [main] INFO  com.example.App - step 18                            ",
            " 12:00:00.019 [main] INFO  com.example.App - step 19                            ",
            " 12:00:00.020 [main] INFO  com.example.App - step 20                            ",
            " 12:00:00.021 [main] INFO  com.example.App - step 21                            ",
            " 12:00:00.022 [main] INFO  com.example.App - step 22                            ",
            "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            ",
        ],
        styles: [
            x: 0, y: 0, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
        ]
    }
    "#);
}

// --- the table ---

/// An entry of every level, one of them with a stack trace, a thread name longer than
/// the thread column and a class name that is abbreviated to fit.
const TABLE_LOG: &str = concat!(
    "2026-10-07 12:00:00.100 [main] TRACE com.example.App - Tick\n",
    "2026-10-07 12:00:00.200 [main] DEBUG com.example.App - Config loaded\n",
    "2026-10-07 12:00:00.300 [http-nio-8080-exec-3] INFO  com.example.order.OrderService - Order 42 created\n",
    "2026-10-07 12:00:01.000 [main] WARN  com.example.App - Retry 1/3\n",
    "2026-10-07 12:00:02.000 [main] ERROR com.example.App - Failed\n",
    "java.lang.IllegalStateException: boom\n",
    "\tat com.example.App.main(App.java:10)\n",
    "2026-10-07 12:00:03.000 [main] FATAL com.example.App - Giving up\n",
);

fn table() -> App {
    app_in(Preset::Logback, TABLE_LOG.as_bytes())
}

/// The buffer's debug view lists every change of style; a single entry at 0,0 means
/// the whole screen is in the terminal's default style.
#[test]
fn plain_theme_draws_the_table_without_any_style() {
    let terminal = drawn(&mut table(), 80, 24, Theme::Plain);
    assert_snapshot!(format!("{:?}", terminal.backend().buffer()), @r#"
    Buffer {
        area: Rect { x: 0, y: 0, width: 80, height: 24 },
        content: [
            "jlv ─ app.log ─ 500 B ─ Logback                                                 ",
            "▶12:00:00.100 TRACE com.example.App          Tick                               ",
            " 12:00:00.200 DEBUG com.example.App          Config loaded                      ",
            " 12:00:00.300 INFO  c.e.order.OrderService   Order 42 created                   ",
            " 12:00:01.000 WARN  com.example.App          Retry 1/3                          ",
            " 12:00:02.000 ERROR com.example.App          Failed [+2]                        ",
            " 12:00:03.000 FATAL com.example.App          Giving up                          ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "                                                                                ",
            "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            ",
        ],
        styles: [
            x: 0, y: 0, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
        ]
    }
    "#);
}

#[test]
fn levels_have_the_colours_of_their_severity() {
    let styles = [
        (Some(Level::Trace), Style::new().fg(Color::DarkGray)),
        (Some(Level::Debug), Style::new().fg(Color::Cyan)),
        (Some(Level::Info), Style::new().fg(Color::Green)),
        (Some(Level::Warn), Style::new().fg(Color::Yellow)),
        (Some(Level::Error), Style::new().fg(Color::Red)),
        (
            Some(Level::Fatal),
            Style::new().fg(Color::White).bg(Color::Red).bold(),
        ),
        (None, Style::new()),
    ];
    for (level, style) in styles {
        assert_eq!(level_style(level), style, "{level:?}");
    }
}

// Where the columns start at 80 columns, after the `▶` gutter.
const TIMESTAMP_COLUMN: u16 = 1;
const LEVEL_COLUMN: u16 = 14;
const MESSAGE_COLUMN: u16 = 45;

#[test]
fn styled_table_colours_only_the_level_column() {
    let terminal = drawn(&mut table(), 80, 24, Theme::Styled);
    let buffer = terminal.backend().buffer();
    let levels = [
        Level::Trace,
        Level::Debug,
        Level::Info,
        Level::Warn,
        Level::Error,
        Level::Fatal,
    ];
    for (y, level) in (1..).zip(levels) {
        let style = level_style(Some(level));
        let cell = &buffer[(LEVEL_COLUMN, y)];
        assert_eq!(
            (cell.fg, cell.bg),
            (
                style.fg.unwrap_or(Color::Reset),
                style.bg.unwrap_or(Color::Reset)
            ),
            "{level:?}"
        );
        assert!(cell.modifier.contains(style.add_modifier), "{level:?}");
        for x in [TIMESTAMP_COLUMN, MESSAGE_COLUMN] {
            assert_eq!(buffer[(x, y)].fg, Color::Reset, "column {x} of {level:?}");
        }
    }
}

#[test]
fn capital_t_shows_the_thread_column() {
    let out = lines_after(&mut table(), &[char_key('T')], 1..=6);
    assert_snapshot!(out, @r"
    ▶12:00:00.100 TRACE main            com.example.App          Tick
     12:00:00.200 DEBUG main            com.example.App          Config loaded
     12:00:00.300 INFO  …io-8080-exec-3 c.e.order.OrderService   Order 42 created
     12:00:01.000 WARN  main            com.example.App          Retry 1/3
     12:00:02.000 ERROR main            com.example.App          Failed [+2]
     12:00:03.000 FATAL main            com.example.App          Giving up
    ");
}

#[test]
fn capital_t_twice_hides_the_thread_column_again() {
    let toggled = screen_after(&mut table(), &[char_key('T'), char_key('T')]);
    assert_eq!(toggled, screen_after(&mut table(), &[]));
}

#[test]
fn timestamps_show_the_date_from_100_columns() {
    let terminal = drawn(&mut table(), 100, 24, Theme::Styled);
    assert_eq!(
        row(&terminal, 1),
        "▶2026-10-07 12:00:00.100 TRACE com.example.App                Tick"
    );
}

#[test]
fn class_column_grows_to_at_most_40_columns() {
    let terminal = drawn(&mut table(), 200, 24, Theme::Styled);
    assert_eq!(
        row(&terminal, 1),
        "▶2026-10-07 12:00:00.100 TRACE com.example.App                          Tick"
    );
}

#[test]
fn level_column_names_the_level_however_the_line_spells_it() {
    let mut app = app_in(
        Preset::Logback,
        b"2026-10-07 12:00:00.100 [main] warning com.example.App - Retry\n",
    );
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(
        row(&terminal, 1),
        "▶12:00:00.100 WARN  com.example.App          Retry"
    );
}

// --- long messages ---

/// A message of 62 columns, longer than the 35 of the message column at 80 columns,
/// and a short one.
const LONG_MESSAGE_LOG: &str = concat!(
    "2026-10-07 12:00:00.100 [main] INFO  com.example.App - ",
    "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\n",
    "2026-10-07 12:00:00.200 [main] WARN  com.example.App - short message\n",
);

fn long_messages() -> App {
    app_in(Preset::Logback, LONG_MESSAGE_LOG.as_bytes())
}

#[test]
fn right_arrow_scrolls_the_messages_by_8_columns() {
    let out = lines_after(&mut long_messages(), &[key(KeyCode::Right)], 1..=2);
    assert_snapshot!(out, @r"
    ▶12:00:00.100 INFO  com.example.App          89abcdefghijklmnopqrstuvwxyzABCDEFG
     12:00:00.200 WARN  com.example.App          ssage
    ");
}

#[test]
fn scrolling_right_stops_once_the_longest_message_on_screen_ends_at_the_edge() {
    let right = key(KeyCode::Right);
    let out = lines_after(&mut long_messages(), &[right; 4], 1..=1);
    assert_eq!(
        out,
        "▶12:00:00.100 INFO  com.example.App          rstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
    );
}

#[test]
fn scrolling_left_after_the_end_moves_back_8_columns_from_there() {
    let right = key(KeyCode::Right);
    let keys = [right, right, right, right, key(KeyCode::Left)];
    let out = lines_after(&mut long_messages(), &keys, 1..=1);
    assert_eq!(
        out,
        "▶12:00:00.100 INFO  com.example.App          jklmnopqrstuvwxyzABCDEFGHIJKLMNOPQR"
    );
}

#[test]
fn raw_lines_scroll_like_messages() {
    let line = "0123456789".repeat(10);
    let out = lines_after(&mut app(line.as_bytes()), &[key(KeyCode::Right)], 1..=1);
    assert_eq!(out, format!("▶{}", &line[8..87]));
}

#[test]
fn w_wraps_long_messages_onto_the_next_lines_of_the_message_column() {
    let out = lines_after(&mut long_messages(), &[char_key('w')], 1..=3);
    assert_snapshot!(out, @r"
    ▶12:00:00.100 INFO  com.example.App          0123456789abcdefghijklmnopqrstuvwxy
                                                 zABCDEFGHIJKLMNOPQRSTUVWXYZ
     12:00:00.200 WARN  com.example.App          short message
    ");
}

#[test]
fn w_twice_cuts_long_messages_again() {
    let toggled = screen_after(&mut long_messages(), &[char_key('w'), char_key('w')]);
    assert_eq!(toggled, screen_after(&mut long_messages(), &[]));
}

/// 30 entries of two screen lines each at 80 columns, 11 of them on the 22 lines of
/// the body.
fn two_line_entries() -> App {
    let log: String = (1..=30)
        .map(|i| {
            let padding = "x".repeat(40);
            format!(
                "2026-10-07 12:00:00.{i:03} [main] INFO  com.example.App - step {i:02} {padding}\n"
            )
        })
        .collect();
    app_in(Preset::Logback, log.as_bytes())
}

/// The first 13 columns of screen lines `lines` after `w` and then `j` `downs` times.
fn wrapped_starts_after_moving_down(downs: usize, lines: [u16; 3]) -> [String; 3] {
    let mut keys = vec![char_key('w')];
    keys.extend(iter::repeat_n(char_key('j'), downs));
    let terminal = terminal_after(&mut two_line_entries(), &keys);
    lines.map(|y| row(&terminal, y).chars().take(13).collect())
}

#[test]
fn moving_down_within_a_screen_of_wrapped_entries_does_not_scroll() {
    assert_eq!(
        wrapped_starts_after_moving_down(1, [1, 2, 3]),
        [" 12:00:00.001", " ".repeat(13).as_str(), "▶12:00:00.002"]
    );
}

#[test]
fn moving_down_through_wrapped_entries_keeps_the_selected_one_on_screen() {
    assert_eq!(
        wrapped_starts_after_moving_down(11, [1, 21, 22]),
        [" 12:00:00.002", "▶12:00:00.012", " ".repeat(13).as_str()]
    );
}

/// 40 entries, the first 20 of two screen lines at 80 columns and the rest of one,
/// or the other way round when `two_lines_first` is false.
fn mixed_height_entries(two_lines_first: bool) -> App {
    let log: String = (1..=40)
        .map(|i| {
            let padding = if (i <= 20) == two_lines_first {
                "x".repeat(40)
            } else {
                String::new()
            };
            format!(
                "2026-10-07 12:00:00.{i:03} [main] INFO  com.example.App - step {i:02} {padding}\n"
            )
        })
        .collect();
    app_in(Preset::Logback, log.as_bytes())
}

/// The first 13 columns of screen lines `lines` after pressing `keys`.
fn starts_after(app: &mut App, keys: &[KeyEvent], lines: [u16; 3]) -> [String; 3] {
    let terminal = terminal_after(app, keys);
    lines.map(|y| row(&terminal, y).chars().take(13).collect())
}

#[test]
fn bottom_of_wrapped_entries_fills_the_body_up_to_the_last_line() {
    // The last screen holds the last 20 entries and the two-line one before them.
    let keys = [char_key('w'), char_key('G')];
    assert_eq!(
        starts_after(&mut mixed_height_entries(true), &keys, [1, 3, 22]),
        [" 12:00:00.020", " 12:00:00.021", "▶12:00:00.040"]
    );
}

#[test]
fn moving_down_through_the_first_screen_of_wrapped_entries_does_not_scroll() {
    // The first screen holds the 20 one-line entries and the first two-line one.
    let mut keys = vec![char_key('w')];
    keys.extend([char_key('j'); 11]);
    assert_eq!(
        starts_after(&mut mixed_height_entries(false), &keys, [1, 12, 21]),
        [" 12:00:00.001", "▶12:00:00.012", " 12:00:00.021"]
    );
}

#[test]
fn wrapped_messages_continue_under_the_message_column_next_to_the_thread() {
    let keys = [char_key('T'), char_key('w')];
    let out = lines_after(&mut long_messages(), &keys, 1..=5);
    assert_snapshot!(out, @r"
    ▶12:00:00.100 INFO  main            com.example.App          0123456789abcdefghi
                                                                 jklmnopqrstuvwxyzAB
                                                                 CDEFGHIJKLMNOPQRSTU
                                                                 VWXYZ
     12:00:00.200 WARN  main            com.example.App          short message
    ");
}

// --- format detection ---

#[test]
fn unrecognized_format_is_shown_raw_with_a_notice_in_the_header() {
    assert_snapshot!(screen_after(&mut detected_app(b"hello\nworld\n"), &[]), @r#"
    "jlv ─ app.log ─ 12 B ─ Raw ─ format not recognized, showing raw                 "
    "▶hello                                                                          "
    " world                                                                          "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "                                                                                "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn cycled_preset_replaces_the_notice_in_the_header() {
    let mut app = detected_app(b"hello\nworld\n");
    let terminal = terminal_after(&mut app, &[char_key('P')]);
    assert_eq!(
        row(&terminal, 0),
        "jlv ─ app.log ─ 12 B ─ Logback ─ indexing 0%"
    );
}

// --- folding ---

/// Entry `i`, counted from 1, says `step i` and has `frames[i - 1]` stack frames.
fn entries_with_frames(frames: &[usize]) -> App {
    let log: String = (1..)
        .zip(frames)
        .map(|(i, &n)| {
            let frames: String = (1..=n)
                .map(|f| format!("\tat App.step(App.java:{f})\n"))
                .collect();
            format!("2026-10-07 12:00:00.{i:03} [main] INFO  com.example.App - step {i}\n{frames}")
        })
        .collect();
    app_in(Preset::Logback, log.as_bytes())
}

fn traces() -> App {
    entries_with_frames(&[0, 2, 1, 0])
}

#[test]
fn space_unfolds_the_selected_entry_under_its_message_column() {
    let out = lines_after(&mut traces(), &[char_key('j'), char_key(' ')], 1..=6);
    assert_snapshot!(out, @r"
     12:00:00.001 INFO  com.example.App          step 1
    ▶12:00:00.002 INFO  com.example.App          step 2
                                                     at App.step(App.java:1)
                                                     at App.step(App.java:2)
     12:00:00.003 INFO  com.example.App          step 3 [+1]
     12:00:00.004 INFO  com.example.App          step 4
    ");
}

#[test]
fn capital_z_unfolds_every_entry() {
    let out = lines_after(&mut traces(), &[char_key('Z')], 1..=7);
    assert_snapshot!(out, @r"
    ▶12:00:00.001 INFO  com.example.App          step 1
     12:00:00.002 INFO  com.example.App          step 2
                                                     at App.step(App.java:1)
                                                     at App.step(App.java:2)
     12:00:00.003 INFO  com.example.App          step 3
                                                     at App.step(App.java:1)
     12:00:00.004 INFO  com.example.App          step 4
    ");
}

#[test]
fn z_folds_every_entry_again() {
    let keys = [char_key('Z'), char_key('z')];
    assert_eq!(
        screen_after(&mut traces(), &keys),
        screen_after(&mut traces(), &[])
    );
}

#[test]
fn space_on_a_single_line_entry_changes_nothing() {
    let toggled = screen_after(&mut traces(), &[char_key(' ')]);
    assert_eq!(toggled, screen_after(&mut traces(), &[]));
}

/// 30 entries of three lines, more than the 22 lines of the body once unfolded.
fn thirty_traces() -> App {
    entries_with_frames(&[2; 30])
}

#[test]
fn unfolding_all_keeps_the_selected_entry_on_its_screen_line() {
    let mut keys = vec![char_key('j'); 5];
    keys.push(char_key('Z'));
    let out = lines_after(&mut thirty_traces(), &keys, 5..=7);
    assert_snapshot!(out, @r"
                                                     at App.step(App.java:2)
    ▶12:00:00.006 INFO  com.example.App          step 6
                                                     at App.step(App.java:1)
    ");
}

#[test]
fn folding_from_a_stack_frame_selects_its_entry_on_the_same_screen_line() {
    // `j` 40 times selects the first frame of entry 14 on the last line.
    let mut keys = vec![char_key('Z')];
    keys.extend([char_key('j'); 40]);
    keys.push(char_key(' '));
    let out = lines_after(&mut thirty_traces(), &keys, 21..=22);
    assert_snapshot!(out, @r"
                                                     at App.step(App.java:2)
    ▶12:00:00.014 INFO  com.example.App          step 14 [+2]
    ");
}

#[test]
fn folding_the_last_entry_at_the_bottom_keeps_it_on_the_bottom_line() {
    let keys = [char_key('Z'), char_key('G'), char_key(' ')];
    let out = lines_after(&mut thirty_traces(), &keys, 21..=22);
    assert_snapshot!(out, @r"
                                                     at App.step(App.java:2)
    ▶12:00:00.030 INFO  com.example.App          step 30 [+2]
    ");
}

/// 30 single-line entries and a last one of 31 lines, taller than the body.
fn tall_last_entry() -> App {
    let mut frames = vec![0; 30];
    frames.push(30);
    entries_with_frames(&frames)
}

#[test]
fn unfolding_all_at_the_bottom_keeps_an_entry_taller_than_the_screen_on_the_bottom_line() {
    let out = lines_after(
        &mut tall_last_entry(),
        &[char_key('G'), char_key('Z')],
        21..=22,
    );
    assert_snapshot!(out, @r"
     12:00:00.030 INFO  com.example.App          step 30
    ▶12:00:00.031 INFO  com.example.App          step 31
    ");
}

#[test]
fn bottom_after_unfolding_all_selects_the_last_frame_of_an_entry_taller_than_the_screen() {
    let keys = [char_key('G'), char_key('Z'), char_key('G')];
    let out = lines_after(&mut tall_last_entry(), &keys, 21..=22);
    assert_snapshot!(out, @r"
                                                     at App.step(App.java:29)
    ▶                                                at App.step(App.java:30)
    ");
}

#[test]
fn unfolding_all_keeps_a_selected_stack_frame_on_its_screen_line() {
    // Entry 20 unfolded on a scrolled screen, its first frame selected on line 13.
    let mut keys = vec![char_key('G')];
    keys.extend([char_key('k'); 10]);
    keys.extend([char_key(' '), char_key('j'), char_key('Z')]);
    let out = lines_after(&mut thirty_traces(), &keys, 12..=13);
    assert_snapshot!(out, @r"
     12:00:00.020 INFO  com.example.App          step 20
    ▶                                                at App.step(App.java:1)
    ");
}

#[test]
fn wrapped_bottom_of_an_unfolded_entry_taller_than_the_screen_shows_its_last_frames() {
    let mut frames = vec![0; 30];
    frames.push(100);
    let keys = [char_key('w'), char_key('G'), char_key('Z'), char_key('G')];
    let terminal = terminal_after(&mut entries_with_frames(&frames), &keys);
    let frame = |n: usize| format!("    at App.step(App.java:{n})");
    assert_eq!(
        [row(&terminal, 1), row(&terminal, 22)],
        [
            format!(" {:44}{}", "", frame(79)),
            format!("▶{:44}{}", "", frame(100))
        ]
    );
}

// --- selection ---

#[test]
fn moving_down_moves_the_marker_without_scrolling() {
    let mut app = hundred_lines();
    let mut terminal = drawn(&mut app, 80, 24, Theme::Styled);
    app.apply(Action::Down);
    draw(&mut terminal, &mut app, Theme::Styled);
    assert_eq!(
        [row(&terminal, 1), row(&terminal, 2)],
        [shown(1, false), shown(2, true)]
    );
}

#[test]
fn styled_theme_highlights_the_selected_line() {
    let mut app = hundred_lines();
    let mut terminal = drawn(&mut app, 80, 24, Theme::Styled);
    app.apply(Action::Down);
    draw(&mut terminal, &mut app, Theme::Styled);
    let style = |y: u16| terminal.backend().buffer()[(1, y)].style();
    assert_ne!(style(2), style(1), "selected line looks like the one above");
    assert_ne!(style(2), style(3), "selected line looks like the one below");
}

// --- files shorter than the screen ---

#[test]
fn file_shorter_than_the_screen_leaves_the_rest_of_the_body_blank() {
    let terminal = drawn(&mut app(b"one\ntwo\nthree\n"), 80, 24, Theme::Styled);
    let body: Vec<String> = (1..=22).map(|y| row(&terminal, y)).collect();
    let mut expected = vec!["▶one".to_string(), " two".into(), " three".into()];
    expected.resize(22, String::new());
    assert_eq!(body, expected);
}

#[test]
fn bottom_of_a_file_shorter_than_the_screen_does_not_scroll() {
    let mut app = app(b"one\ntwo\nthree\n");
    let mut terminal = drawn(&mut app, 80, 24, Theme::Styled);
    app.apply(Action::Bottom);
    draw(&mut terminal, &mut app, Theme::Styled);
    assert_eq!(
        [row(&terminal, 1), row(&terminal, 3)],
        [" one".to_string(), "▶three".to_string()]
    );
}

// --- what a line looks like ---

#[test]
fn stack_trace_lines_start_with_four_columns_for_the_tab() {
    let mut app =
        app(b"java.lang.IllegalStateException: boom\n\tat com.example.App.main(App.java:10)\n");
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(
        row(&terminal, 2),
        "     at com.example.App.main(App.java:10)"
    );
}

#[test]
fn escape_sequences_are_shown_as_replacement_characters() {
    let mut app = app(b"\x1b[31mred\x1b[0m and a bell\x07\n");
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(
        row(&terminal, 1),
        "▶\u{FFFD}[31mred\u{FFFD}[0m and a bell\u{FFFD}"
    );
}

#[test]
fn wide_characters_take_two_columns_up_to_the_tab_stop() {
    let mut app = app("日本語のログ\tok\n".as_bytes());
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(row(&terminal, 1), "▶日本語のログ    ok");
}

#[test]
fn wide_character_that_does_not_fit_on_the_line_is_left_out() {
    let mut app = app("日".repeat(40).as_bytes());
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(row(&terminal, 1), format!("▶{}", "日".repeat(39)));
}

#[test]
fn long_line_is_cut_at_the_right_edge() {
    let mut app = app("a".repeat(100).as_bytes());
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(row(&terminal, 1), format!("▶{}", "a".repeat(79)));
}

#[test]
fn windows_line_endings_are_not_shown() {
    let terminal = drawn(&mut app(b"one\r\ntwo\r\n"), 80, 24, Theme::Styled);
    assert_eq!([row(&terminal, 1), row(&terminal, 2)], ["▶one", " two"]);
}

// --- header ---

#[test]
fn control_characters_in_the_file_name_are_replaced() {
    let mut app = named_app("/var/log/a\x1bb.log", b"x\n");
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    assert_eq!(row(&terminal, 0), "jlv ─ a\u{FFFD}b.log ─ 2 B ─ Raw");
}

// --- terminal size ---

#[test]
fn one_column_short_of_80_is_too_small() {
    let terminal = drawn(&mut hundred_lines(), 79, 24, Theme::Styled);
    assert_eq!(row(&terminal, 1), "terminal too small (80×24 needed)");
}

#[test]
fn one_line_short_of_24_is_too_small() {
    let terminal = drawn(&mut hundred_lines(), 80, 23, Theme::Styled);
    assert_eq!(row(&terminal, 1), "terminal too small (80×24 needed)");
}

#[test]
fn body_fills_the_lines_between_header_and_footer() {
    let mut app = hundred_lines();
    drawn(&mut app, 80, 30, Theme::Styled);
    assert_eq!(app.viewport().height(), 28);
}

#[test]
fn growing_the_terminal_at_the_bottom_fills_it_with_earlier_lines() {
    let mut app = hundred_lines();
    let mut terminal = drawn(&mut app, 80, 24, Theme::Styled);
    app.apply(Action::Bottom);
    draw(&mut terminal, &mut app, Theme::Styled);
    terminal.backend_mut().resize(80, 30);
    draw(&mut terminal, &mut app, Theme::Styled);
    assert_eq!(
        [row(&terminal, 1), row(&terminal, 28)],
        [shown(73, false), shown(100, true)]
    );
}

#[test]
fn shrinking_the_terminal_at_the_bottom_keeps_the_last_line_selected_on_screen() {
    let mut app = hundred_lines();
    let mut terminal = drawn(&mut app, 80, 30, Theme::Styled);
    app.apply(Action::Bottom);
    draw(&mut terminal, &mut app, Theme::Styled);
    terminal.backend_mut().resize(80, 24);
    draw(&mut terminal, &mut app, Theme::Styled);
    assert_eq!(
        [row(&terminal, 1), row(&terminal, 22)],
        [shown(79, false), shown(100, true)]
    );
}

#[test]
fn briefly_too_small_terminal_keeps_the_scroll_position() {
    let mut app = hundred_lines();
    let mut terminal = drawn(&mut app, 80, 24, Theme::Styled);
    for _ in 0..30 {
        app.apply(Action::Down);
    }
    draw(&mut terminal, &mut app, Theme::Styled);
    let before = (app.viewport().top(), app.viewport().selected());
    terminal.backend_mut().resize(60, 9);
    draw(&mut terminal, &mut app, Theme::Styled);
    terminal.backend_mut().resize(80, 24);
    draw(&mut terminal, &mut app, Theme::Styled);
    assert_eq!((app.viewport().top(), app.viewport().selected()), before);
}

// --- NO_COLOR ---

#[test]
fn unset_no_color_allows_styling() {
    assert_eq!(Theme::from_no_color(None), Theme::Styled);
}

#[test]
fn empty_no_color_allows_styling() {
    assert_eq!(Theme::from_no_color(Some(OsStr::new(""))), Theme::Styled);
}

#[test]
fn no_color_set_to_1_turns_styling_off() {
    assert_eq!(Theme::from_no_color(Some(OsStr::new("1"))), Theme::Plain);
}

#[test]
fn no_color_turns_styling_off_whatever_its_value() {
    assert_eq!(Theme::from_no_color(Some(OsStr::new("0"))), Theme::Plain);
    assert_eq!(
        Theme::from_no_color(Some(OsStr::new("false"))),
        Theme::Plain
    );
}

#[test]
fn non_utf8_no_color_turns_styling_off() {
    let value = OsString::from_vec(vec![0xff]);
    assert_eq!(Theme::from_no_color(Some(&value)), Theme::Plain);
}

// --- sizes ---

#[test]
fn zero_bytes() {
    assert_eq!(human_size(0), "0 B");
}

#[test]
fn single_byte() {
    assert_eq!(human_size(1), "1 B");
}

#[test]
fn largest_size_in_bytes() {
    assert_eq!(human_size(1023), "1023 B");
}

#[test]
fn one_kilobyte_is_1024_bytes() {
    assert_eq!(human_size(1024), "1.0 KB");
}

#[test]
fn kilobytes_show_one_decimal() {
    assert_eq!(human_size(1536), "1.5 KB");
}

#[test]
fn tenths_round_down_below_the_half() {
    // 1996 B = 1.949 KB
    assert_eq!(human_size(1996), "1.9 KB");
}

#[test]
fn tenths_round_up_from_above_the_half() {
    // 1997 B = 1.950 2 KB
    assert_eq!(human_size(1997), "2.0 KB");
}

#[test]
fn ten_kilobytes_and_more_keep_the_decimal() {
    assert_eq!(human_size(10 * 1024), "10.0 KB");
}

#[test]
fn largest_size_in_kilobytes() {
    // 1 048 524 B = 1023.949 KB
    assert_eq!(human_size(1_048_524), "1023.9 KB");
}

#[test]
fn size_that_rounds_to_1024_kilobytes_is_a_megabyte() {
    // 1 048 525 B = 1023.950 2 KB
    assert_eq!(human_size(1_048_525), "1.0 MB");
}

#[test]
fn one_byte_short_of_a_megabyte_is_a_megabyte() {
    assert_eq!(human_size(1024 * 1024 - 1), "1.0 MB");
}

#[test]
fn megabytes_show_one_decimal() {
    // 5.5 × 1024² = 5 767 168
    assert_eq!(human_size(5_767_168), "5.5 MB");
}

#[test]
fn whole_kilobytes_just_below_a_megabyte_stay_in_kilobytes() {
    assert_eq!(human_size(1023 * 1024), "1023.0 KB");
}

#[test]
fn largest_size_in_gigabytes() {
    // 1023.9 × 1024³ is about 1 099 404 820 070
    assert_eq!(human_size(1_099_404_820_070), "1023.9 GB");
}

#[test]
fn gigabytes() {
    // 1.8 × 1024³ = 1 932 735 283.2
    assert_eq!(human_size(1_932_735_283), "1.8 GB");
}

#[test]
fn terabytes() {
    assert_eq!(human_size(1 << 40), "1.0 TB");
}

#[test]
fn petabytes() {
    assert_eq!(human_size(1 << 50), "1.0 PB");
}

#[test]
fn exabytes() {
    assert_eq!(human_size(1 << 60), "1.0 EB");
}

#[test]
fn largest_size() {
    assert_eq!(human_size(u64::MAX), "16.0 EB");
}

/// The size `human_size` stands for, in bytes, to compare two of them.
fn bytes_meant(size: &str) -> f64 {
    let (number, unit) = size.split_once(' ').unwrap();
    let power = ["B", "KB", "MB", "GB", "TB", "PB", "EB"]
        .iter()
        .position(|&u| u == unit)
        .unwrap();
    number.parse::<f64>().unwrap() * 1024f64.powi(power as i32)
}

/// Byte counts spread evenly over the units: the bit length is picked first, so plain
/// `any::<u64>()` doesn't land in EB nearly every time.
fn byte_count() -> impl Strategy<Value = u64> {
    (0u32..64, any::<u64>()).prop_map(|(shift, bits)| bits >> shift)
}

proptest! {
    #[test]
    fn size_is_whole_bytes_or_one_decimal_below_1024_of_a_unit(bytes in byte_count()) {
        let size = human_size(bytes);
        prop_assert!(is_well_formed_size(&size), "{} gave {:?}", bytes, size);
    }

    #[test]
    fn bigger_files_never_show_a_smaller_size(
        small in byte_count(),
        step in prop_oneof![0u64..4096, byte_count()],
    ) {
        let big = small.saturating_add(step);
        prop_assert!(
            bytes_meant(&human_size(small)) <= bytes_meant(&human_size(big)),
            "{} shows as {:?}, {} as {:?}", small, human_size(small), big, human_size(big)
        );
    }
}

/// `<0–1023> B` or `<1–1023>.<digit> <unit>`.
fn is_well_formed_size(size: &str) -> bool {
    let Some((number, unit)) = size.split_once(' ') else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let no_leading_zero = |s: &str| s == "0" || !s.starts_with('0');
    match number.split_once('.') {
        None => {
            unit == "B"
                && digits(number)
                && no_leading_zero(number)
                && number.parse::<u32>().unwrap() < 1024
        }
        Some((whole, tenth)) => {
            ["KB", "MB", "GB", "TB", "PB", "EB"].contains(&unit)
                && digits(whole)
                && no_leading_zero(whole)
                && tenth.len() == 1
                && digits(tenth)
                && (1..1024).contains(&whole.parse::<u32>().unwrap())
        }
    }
}

// --- printable text ---

#[test]
fn text_without_tabs_or_control_characters_is_unchanged() {
    let text = "12:00:00.123 [main] INFO  Zürich 日本 – done";
    assert!(matches!(printable(text), Cow::Borrowed(t) if t == text));
}

#[test]
fn leading_tab_becomes_four_spaces() {
    assert_eq!(printable("\tat com.example.App"), "    at com.example.App");
}

#[test]
fn tab_moves_to_the_next_multiple_of_four_columns() {
    assert_eq!(printable("ab\tc"), "ab  c");
}

#[test]
fn tab_right_after_three_columns_is_one_space() {
    assert_eq!(printable("key\tvalue"), "key value");
}

#[test]
fn tab_on_a_tab_stop_is_four_spaces() {
    assert_eq!(printable("abcd\te"), "abcd    e");
}

#[test]
fn consecutive_tabs_each_move_to_the_next_stop() {
    assert_eq!(printable("\t\tx"), "        x");
}

#[test]
fn trailing_tab_still_expands() {
    assert_eq!(printable("ab\t"), "ab  ");
}

#[test]
fn wide_characters_count_two_columns_before_a_tab() {
    assert_eq!(printable("日\tx"), "日  x");
    assert_eq!(printable("日本\tx"), "日本    x");
}

#[test]
fn escape_becomes_a_replacement_character() {
    assert_eq!(
        printable("\x1b[31mred\x1b[0m"),
        "\u{FFFD}[31mred\u{FFFD}[0m"
    );
}

#[test]
fn other_c0_control_characters_become_replacement_characters() {
    assert_eq!(
        printable("a\0b\x07c\x08d\re\x0bf\x0cg\x1fh"),
        "a\u{FFFD}b\u{FFFD}c\u{FFFD}d\u{FFFD}e\u{FFFD}f\u{FFFD}g\u{FFFD}h"
    );
}

#[test]
fn delete_becomes_a_replacement_character() {
    assert_eq!(printable("a\x7fb"), "a\u{FFFD}b");
}

#[test]
fn c1_control_characters_become_replacement_characters() {
    // U+009B is the one-character CSI that some terminals still obey.
    assert_eq!(printable("a\u{9b}31mb\u{85}c"), "a\u{FFFD}31mb\u{FFFD}c");
}

#[test]
fn replaced_character_counts_one_column_before_a_tab() {
    assert_eq!(printable("\x1b\tx"), "\u{FFFD}   x");
}

#[test]
fn tab_after_a_replaced_character_mid_stop_fills_to_the_stop() {
    assert_eq!(printable("ab\x1b\tx"), "ab\u{FFFD} x");
}

#[test]
fn tab_after_a_wide_character_mid_stop_fills_to_the_stop() {
    assert_eq!(printable("a日\tx"), "a日 x");
}

#[test]
fn wide_character_across_a_tab_stop_moves_the_tab_to_the_next_one() {
    // `日` takes columns 3 and 4, so the tab goes from column 5 to 8
    assert_eq!(printable("abc日\tx"), "abc日   x");
}

#[test]
fn second_tab_counts_from_where_the_first_one_ended() {
    assert_eq!(printable("a\tb\tc"), "a   b   c");
}

#[test]
fn tabs_after_text_of_different_lengths_line_up() {
    assert_eq!(printable("id\tname\tlevel"), "id  name    level");
}

#[test]
fn combining_marks_take_no_column_before_a_tab() {
    assert_eq!(printable("e\u{301}\tx"), "e\u{301}   x");
}

proptest! {
    #[test]
    fn printable_text_has_no_control_characters(text in any::<String>()) {
        let shown = printable(&text);
        prop_assert!(!shown.chars().any(char::is_control), "{:?} gave {:?}", text, shown);
    }

    #[test]
    fn printable_keeps_text_without_control_characters(text in "[^\\p{Cc}]*") {
        prop_assert_eq!(printable(&text), text.as_str());
    }

    #[test]
    fn text_after_a_tab_starts_on_a_tab_stop(
        before in "[a-z日本 ]{0,10}",
        after in "[a-z]{1,5}",
    ) {
        let text = format!("{before}\t{after}");
        let shown = printable(&text);
        let start = shown.len() - after.len();
        prop_assert_eq!(&shown[start..], after.as_str());
        let column = shown[..start].width();
        prop_assert_eq!(column % 4, 0, "{:?}", shown);
        prop_assert!(column > before.width() && column <= before.width() + 4, "{:?}", shown);
    }
}

// --- robustness ---

fn any_action() -> impl Strategy<Value = Action> {
    prop::sample::select(vec![
        Action::Up,
        Action::Down,
        Action::PageUp,
        Action::PageDown,
        Action::HalfPageUp,
        Action::HalfPageDown,
        Action::Top,
        Action::Bottom,
        Action::ScrollLeft,
        Action::ScrollRight,
        Action::ToggleThread,
        Action::ToggleWrap,
        Action::CyclePreset,
    ])
}

fn any_preset() -> impl Strategy<Value = Preset> {
    prop::sample::select(Preset::ALL.to_vec())
}

/// Lines that start an entry in some preset mixed with arbitrary bytes.
fn any_log() -> impl Strategy<Value = Vec<u8>> {
    let line = prop_oneof![
        prop::sample::select(EXAMPLES.map(|(_, line)| line.as_bytes().to_vec()).to_vec()),
        prop::collection::vec(any::<u8>(), 0..100),
    ];
    prop::collection::vec(line, 0..6).prop_map(|lines| lines.join(&b'\n'))
}

/// `app` drawn at `width`×`height` in `theme`, then again after each of `actions`.
fn drawn_after(
    app: &mut App,
    width: u16,
    height: u16,
    theme: Theme,
    actions: &[Action],
) -> Terminal<TestBackend> {
    let mut terminal = drawn(app, width, height, theme);
    for &action in actions {
        app.apply(action);
        draw(&mut terminal, app, theme);
    }
    terminal
}

proptest! {
    // A case whose file never finishes indexing fails only at the deadline, and so
    // would every step of shrinking it. Shrinking stops after two seconds instead.
    #![proptest_config(ProptestConfig {
        max_shrink_time: 2_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_file_in_any_preset_at_any_size_draws_only_printable_cells(
        preset in any_preset(),
        bytes in any_log(),
        width in 0u16..140,
        height in 0u16..40,
        actions in prop::collection::vec(any_action(), 0..12),
    ) {
        let mut app = app_in(preset, &bytes);
        let terminal = drawn_after(&mut app, width, height, Theme::Styled, &actions);
        for cell in &terminal.backend().buffer().content {
            prop_assert!(!cell.symbol().chars().any(char::is_control), "{:?}", cell.symbol());
        }
    }

    #[test]
    fn plain_theme_never_styles_a_cell(
        preset in any_preset(),
        bytes in any_log(),
        width in 0u16..140,
        height in 0u16..40,
        actions in prop::collection::vec(any_action(), 0..12),
    ) {
        let mut app = app_in(preset, &bytes);
        let terminal = drawn_after(&mut app, width, height, Theme::Plain, &actions);
        for cell in &terminal.backend().buffer().content {
            prop_assert_eq!(cell.style(), Cell::EMPTY.style());
        }
    }
}
