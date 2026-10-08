use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStringExt;

use insta::assert_snapshot;
use proptest::prelude::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Cell;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use unicode_width::UnicodeWidthStr;

use super::*;
use crate::app::{Action, action};
use crate::format::Preset;
use crate::test_support::{
    app, app_in, char_key, key, named_app, numbered_log, row, unindexed_app,
};

fn draw(terminal: &mut Terminal<TestBackend>, app: &mut App, theme: Theme) {
    terminal.draw(|frame| render(frame, app, theme)).unwrap();
}

fn drawn(app: &mut App, width: u16, height: u16, theme: Theme) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    draw(&mut terminal, app, theme);
    terminal
}

/// The 80×24 screen after pressing `keys`, as text.
fn screen_after(app: &mut App, keys: &[KeyEvent]) -> String {
    let mut terminal = drawn(app, 80, 24, Theme::Styled);
    for &key in keys {
        app.apply(action(key).unwrap());
        draw(&mut terminal, app, Theme::Styled);
    }
    terminal.backend().to_string()
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
fn body_shows_a_row_per_entry() {
    let log = [
        "2026-10-07 12:00:00.123 [main] INFO  com.example.App - Started\n",
        "2026-10-07 12:00:01.500 [main] ERROR com.example.App - Failed\n",
        "java.lang.IllegalStateException: boom\n",
        "\tat com.example.App.main(App.java:10)\n",
        "2026-10-07 12:00:02.000 [main] WARN  com.example.App - Done\n",
    ]
    .concat();
    let mut app = app_in(Preset::Logback, log.as_bytes());
    let terminal = drawn(&mut app, 80, 24, Theme::Styled);
    let body: Vec<String> = (1..4).map(|y| row(&terminal, y)).collect();
    assert_snapshot!(body.join("\n"), @r"
    ▶2026-10-07 12:00:00.123 [main] INFO  com.example.App - Started
     2026-10-07 12:00:01.500 [main] ERROR com.example.App - Failed
     2026-10-07 12:00:02.000 [main] WARN  com.example.App - Done
    ");
    assert_eq!(row(&terminal, 4), "");
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
    ])
}

proptest! {
    // A case whose file never finishes indexing fails only at the deadline, and so
    // would every step of shrinking it. Shrinking stops after two seconds instead.
    #![proptest_config(ProptestConfig {
        max_shrink_time: 2_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_file_at_any_size_draws_only_printable_cells(
        bytes in prop::collection::vec(any::<u8>(), 0..512),
        width in 0u16..120,
        height in 0u16..40,
        actions in prop::collection::vec(any_action(), 0..12),
    ) {
        let mut app = app(&bytes);
        let mut terminal = drawn(&mut app, width, height, Theme::Styled);
        for action in actions {
            app.apply(action);
            draw(&mut terminal, &mut app, Theme::Styled);
        }
        for cell in &terminal.backend().buffer().content {
            prop_assert!(!cell.symbol().chars().any(char::is_control), "{:?}", cell.symbol());
        }
    }

    #[test]
    fn plain_theme_never_styles_a_cell(
        bytes in prop::collection::vec(any::<u8>(), 0..512),
        width in 0u16..120,
        height in 0u16..40,
        actions in prop::collection::vec(any_action(), 0..12),
    ) {
        let mut app = app(&bytes);
        let mut terminal = drawn(&mut app, width, height, Theme::Plain);
        for action in actions {
            app.apply(action);
            draw(&mut terminal, &mut app, Theme::Plain);
        }
        for cell in &terminal.backend().buffer().content {
            prop_assert_eq!(cell.style(), Cell::EMPTY.style());
        }
    }
}
