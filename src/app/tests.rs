use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

use super::*;
use crate::index::Progress;
use crate::test_support;
use crate::test_support::{
    INDEX_DEADLINE, LARGE_INDEX_DEADLINE, app, app_in, char_key, ctrl, following_app, key,
    named_app, numbered_log, unindexed_app, wait_for_index, wait_for_index_within,
};

/// `(top, selected)`: the first line on screen and the selected line.
fn pos(app: &App) -> (usize, Option<usize>) {
    (app.viewport().top(), app.viewport().selected())
}

/// 100 lines on a 10-line screen.
fn hundred_lines() -> App {
    let mut app = app(numbered_log(100).as_bytes());
    app.resize(10);
    app
}

fn applied(mut app: App, actions: &[Action]) -> App {
    for &action in actions {
        app.apply(action);
    }
    app
}

// --- key mapping: moves ---

#[test]
fn j_moves_down() {
    assert_eq!(action(char_key('j')), Some(Action::Down));
}

#[test]
fn down_arrow_moves_down() {
    assert_eq!(action(key(KeyCode::Down)), Some(Action::Down));
}

#[test]
fn k_moves_up() {
    assert_eq!(action(char_key('k')), Some(Action::Up));
}

#[test]
fn up_arrow_moves_up() {
    assert_eq!(action(key(KeyCode::Up)), Some(Action::Up));
}

#[test]
fn page_down_key_pages_down() {
    assert_eq!(action(key(KeyCode::PageDown)), Some(Action::PageDown));
}

#[test]
fn page_up_key_pages_up() {
    assert_eq!(action(key(KeyCode::PageUp)), Some(Action::PageUp));
}

#[test]
fn ctrl_d_moves_half_a_page_down() {
    assert_eq!(action(ctrl('d')), Some(Action::HalfPageDown));
}

#[test]
fn ctrl_u_moves_half_a_page_up() {
    assert_eq!(action(ctrl('u')), Some(Action::HalfPageUp));
}

#[test]
fn g_goes_to_the_top() {
    assert_eq!(action(char_key('g')), Some(Action::Top));
}

#[test]
fn home_goes_to_the_top() {
    assert_eq!(action(key(KeyCode::Home)), Some(Action::Top));
}

#[test]
fn capital_g_goes_to_the_bottom() {
    assert_eq!(action(char_key('G')), Some(Action::Bottom));
}

#[test]
fn capital_g_reported_with_shift_goes_to_the_bottom() {
    let shift_g = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
    assert_eq!(action(shift_g), Some(Action::Bottom));
}

#[test]
fn end_goes_to_the_bottom() {
    assert_eq!(action(key(KeyCode::End)), Some(Action::Bottom));
}

// --- key mapping: quitting ---

#[test]
fn q_quits() {
    assert_eq!(action(char_key('q')), Some(Action::Quit));
}

#[test]
fn f10_quits() {
    assert_eq!(action(key(KeyCode::F(10))), Some(Action::Quit));
}

#[test]
fn ctrl_c_quits() {
    assert_eq!(action(ctrl('c')), Some(Action::Quit));
}

// --- key mapping: what does nothing ---

#[test]
fn d_and_u_without_ctrl_do_nothing() {
    assert_eq!(action(char_key('d')), None);
    assert_eq!(action(char_key('u')), None);
}

#[test]
fn c_without_ctrl_does_nothing() {
    assert_eq!(action(char_key('c')), None);
}

#[test]
fn letters_with_ctrl_are_not_plain_letters() {
    assert_eq!(action(ctrl('j')), None);
    assert_eq!(action(ctrl('g')), None);
    assert_eq!(action(ctrl('q')), None);
}

fn shifted(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

// Shift is ignored only for characters, which carry their case themselves.

#[test]
fn shift_end_does_nothing() {
    assert_eq!(action(shifted(KeyCode::End)), None);
}

#[test]
fn shift_home_does_nothing() {
    assert_eq!(action(shifted(KeyCode::Home)), None);
}

#[test]
fn shift_down_does_nothing() {
    assert_eq!(action(shifted(KeyCode::Down)), None);
}

#[test]
fn shift_page_down_does_nothing() {
    assert_eq!(action(shifted(KeyCode::PageDown)), None);
}

#[test]
fn shift_f10_does_nothing() {
    assert_eq!(action(shifted(KeyCode::F(10))), None);
}

#[test]
fn letters_with_alt_are_not_plain_letters() {
    let alt_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT);
    assert_eq!(action(alt_j), None);
}

#[test]
fn unbound_keys_do_nothing() {
    assert_eq!(action(char_key('x')), None);
    assert_eq!(action(key(KeyCode::Enter)), None);
    assert_eq!(action(key(KeyCode::Esc)), None);
    assert_eq!(action(key(KeyCode::F(1))), None);
}

#[test]
fn releasing_a_key_does_nothing() {
    let release = KeyEvent::new_with_kind(
        KeyCode::Char('q'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    assert_eq!(action(release), None);
}

#[test]
fn holding_a_key_repeats_its_action() {
    let repeat =
        KeyEvent::new_with_kind(KeyCode::Char('j'), KeyModifiers::NONE, KeyEventKind::Repeat);
    assert_eq!(action(repeat), Some(Action::Down));
}

// --- the file ---

#[test]
fn file_name_is_the_last_component_of_the_path() {
    let app = named_app("/var/log/wildfly/server.log", b"x\n");
    assert_eq!(app.file_name(), "server.log");
}

#[test]
fn relative_file_name_is_the_last_component_too() {
    let app = named_app("logs/server.log", b"x\n");
    assert_eq!(app.file_name(), "server.log");
}

#[test]
fn non_utf8_file_name_is_decoded_lossily() {
    let path = PathBuf::from(OsString::from_vec(b"/var/log/caf\xe9.log".to_vec()));
    let app = named_app(path, b"x\n");
    assert_eq!(app.file_name(), "caf\u{FFFD}.log");
}

#[test]
fn size_is_the_file_length_in_bytes() {
    assert_eq!(app(b"one\ntwo\r\n").size(), 9);
}

#[test]
fn empty_file_has_size_zero() {
    assert_eq!(app(b"").size(), 0);
}

// --- rows from the index ---

const STARTED: &str = "2026-10-07 12:00:00.123 [main] INFO  com.example.App - Started\n";
const FAILED: &str = "2026-10-07 12:00:01.500 [main] ERROR com.example.App - Failed\n";
const EXCEPTION: &str = "java.lang.IllegalStateException: boom\n";
const FRAME: &str = "\tat com.example.App.main(App.java:10)\n";
const DONE: &str = "2026-10-07 12:00:02.000 [main] WARN  com.example.App - Done\n";

fn logback(lines: &[&str]) -> App {
    app_in(Preset::Logback, lines.concat().as_bytes())
}

#[test]
fn every_entry_is_a_row() {
    let app = logback(&[STARTED, FAILED, EXCEPTION, FRAME, DONE]);
    assert_eq!(app.viewport().total(), 3);
}

#[test]
fn row_shows_the_first_line_of_its_entry() {
    let app = logback(&[STARTED, FAILED, EXCEPTION, FRAME, DONE]);
    assert_eq!(app.line(1), FAILED.trim_end());
    assert_eq!(app.line(2), DONE.trim_end());
}

#[test]
fn row_of_the_headless_entry_shows_the_first_line_of_the_file() {
    let app = logback(&[EXCEPTION, FRAME, STARTED]);
    assert_eq!(app.viewport().total(), 2);
    assert_eq!(app.line(0), EXCEPTION.trim_end());
}

#[test]
fn row_of_an_entry_with_crlf_endings_has_no_line_ending() {
    let text = [FAILED, FRAME, DONE].concat().replace('\n', "\r\n");
    let app = app_in(Preset::Logback, text.as_bytes());
    assert_eq!(app.line(0), FAILED.trim_end());
    assert_eq!(app.line(1), DONE.trim_end());
}

#[test]
fn last_row_without_a_newline_shows_the_whole_line() {
    let app = app(b"first\nlast");
    assert_eq!(app.line(1), "last");
}

#[test]
fn new_app_has_no_rows_before_the_index_is_polled() {
    let app = unindexed_app(Preset::Raw, numbered_log(100).as_bytes());
    assert_eq!(app.viewport().total(), 0);
    assert_eq!(app.viewport().selected(), None);
}

#[test]
fn app_indexes_in_the_preset_zone_and_anchor_it_was_given() {
    let mtime: Timestamp = "2026-10-08T12:00:00Z".parse().unwrap();
    let zone = TimeZone::get("Europe/Warsaw").unwrap();
    let app = App::new(
        Path::new("/var/log/console.log"),
        test_support::source(b"12:00:00,123 INFO  [org.jboss.as] (main) started\n"),
        Preset::WildflyConsole,
        zone.clone(),
        mtime,
        false,
    );
    let settings = app.settings();
    assert_eq!(settings.preset, Preset::WildflyConsole);
    assert_eq!(settings.zone, zone);
    assert_eq!(
        settings.anchor,
        Anchor::Modified("2026-10-08T14:00:00".parse().unwrap())
    );
}

#[test]
fn anchor_comes_from_a_date_in_the_file_name() {
    let app = App::new(
        Path::new("/var/log/server.log.2026-10-06"),
        test_support::source(b"x\n"),
        Preset::LogbackBasic,
        test_support::zone(),
        test_support::mtime(),
        false,
    );
    assert_eq!(
        app.settings().anchor,
        Anchor::Named("2026-10-06".parse().unwrap())
    );
}

#[test]
fn new_app_has_indexed_none_of_the_file() {
    let content = numbered_log(100);
    let app = unindexed_app(Preset::Raw, content.as_bytes());
    let total = content.len() as u64;
    assert_eq!(app.progress(), Progress { indexed: 0, total });
}

#[test]
fn polling_the_index_to_the_end_brings_in_every_row() {
    let content = numbered_log(100);
    let mut app = unindexed_app(Preset::Raw, content.as_bytes());
    wait_for_index(&mut app);
    let total = content.len() as u64;
    assert_eq!(
        app.progress(),
        Progress {
            indexed: total,
            total
        }
    );
    assert_eq!(app.viewport().total(), 100);
    assert_eq!(pos(&app), (0, Some(0)));
}

#[test]
fn empty_file_is_indexed_without_rows() {
    let app = app(b"");
    assert!(app.progress().is_complete());
    assert_eq!(app.viewport().total(), 0);
}

#[test]
fn rows_taken_in_later_keep_the_selection() {
    let mut app = unindexed_app(Preset::LogbackBasic, numbered_log(40_000).as_bytes());
    app.resize(10);
    let deadline = Instant::now() + LARGE_INDEX_DEADLINE;
    while app.viewport().total() < 2 {
        assert!(Instant::now() < deadline, "no rows came in");
        thread::sleep(Duration::from_millis(1));
        app.poll_index();
    }
    app.apply(Action::Down);
    wait_for_index_within(&mut app, LARGE_INDEX_DEADLINE);
    assert_eq!(app.viewport().total(), 40_000);
    assert_eq!(pos(&app), (0, Some(1)));
}

#[test]
fn every_line_of_the_file_is_a_row() {
    let app = app(numbered_log(100).as_bytes());
    assert_eq!(app.viewport().total(), 100);
}

#[test]
fn line_is_the_text_without_its_line_ending() {
    let app = app(b"first\r\nsecond\n");
    assert_eq!(app.line(0), "first");
    assert_eq!(app.line(1), "second");
}

#[test]
fn line_with_invalid_utf8_is_decoded_lossily() {
    let app = app(b"caf\xe9 au lait\n");
    assert_eq!(app.line(0), "caf\u{FFFD} au lait");
}

#[test]
fn line_keeps_control_characters_for_the_ui_to_replace() {
    let app = app(b"\tat \x1b[31mred\n");
    assert_eq!(app.line(0), "\tat \x1b[31mred");
}

// --- starting state ---

#[test]
fn new_app_is_at_the_top_with_the_first_line_selected() {
    assert_eq!(pos(&hundred_lines()), (0, Some(0)));
}

#[test]
fn new_app_keeps_running() {
    assert!(!hundred_lines().should_quit());
}

#[test]
fn empty_file_has_no_selection() {
    let mut app = app(b"");
    app.resize(10);
    assert_eq!(pos(&app), (0, None));
}

#[test]
fn resize_sets_the_screen_height() {
    let app = hundred_lines();
    assert_eq!(app.viewport().height(), 10);
}

// --- actions ---

#[test]
fn down_selects_the_next_line() {
    let app = applied(hundred_lines(), &[Action::Down]);
    assert_eq!(pos(&app), (0, Some(1)));
}

#[test]
fn up_selects_the_previous_line() {
    let app = applied(hundred_lines(), &[Action::Down, Action::Down, Action::Up]);
    assert_eq!(pos(&app), (0, Some(1)));
}

#[test]
fn page_down_moves_a_screen_down() {
    let app = applied(hundred_lines(), &[Action::Down, Action::PageDown]);
    assert_eq!(pos(&app), (10, Some(11)));
}

#[test]
fn page_up_moves_a_screen_up() {
    let app = applied(
        hundred_lines(),
        &[Action::PageDown, Action::PageDown, Action::PageUp],
    );
    assert_eq!(pos(&app), (10, Some(10)));
}

#[test]
fn half_page_down_moves_half_a_screen_down() {
    let app = applied(hundred_lines(), &[Action::HalfPageDown]);
    assert_eq!(pos(&app), (5, Some(5)));
}

#[test]
fn half_page_up_moves_half_a_screen_up() {
    let app = applied(hundred_lines(), &[Action::PageDown, Action::HalfPageUp]);
    assert_eq!(pos(&app), (5, Some(5)));
}

#[test]
fn bottom_selects_the_last_line_on_the_last_screen() {
    let app = applied(hundred_lines(), &[Action::Bottom]);
    assert_eq!(pos(&app), (90, Some(99)));
}

#[test]
fn top_selects_the_first_line_on_the_first_screen() {
    let app = applied(hundred_lines(), &[Action::Bottom, Action::Top]);
    assert_eq!(pos(&app), (0, Some(0)));
}

#[test]
fn quit_asks_the_viewer_to_stop() {
    let app = applied(hundred_lines(), &[Action::Quit]);
    assert!(app.should_quit());
}

#[test]
fn quit_does_not_move() {
    let app = applied(hundred_lines(), &[Action::Down, Action::Quit]);
    assert_eq!(pos(&app), (0, Some(1)));
}

#[test]
fn moves_do_not_quit() {
    let app = applied(
        hundred_lines(),
        &[
            Action::Down,
            Action::Up,
            Action::PageDown,
            Action::PageUp,
            Action::HalfPageDown,
            Action::HalfPageUp,
            Action::Bottom,
            Action::Top,
        ],
    );
    assert!(!app.should_quit());
}

#[test]
fn moves_in_an_empty_file_keep_it_without_a_selection() {
    let mut app = app(b"");
    app.resize(10);
    let app = applied(
        app,
        &[
            Action::Down,
            Action::PageDown,
            Action::HalfPageDown,
            Action::Bottom,
            Action::Up,
            Action::PageUp,
            Action::HalfPageUp,
            Action::Top,
        ],
    );
    assert_eq!(pos(&app), (0, None));
}

// --- resizing ---

#[test]
fn growing_at_the_bottom_fills_the_screen_with_earlier_lines() {
    let mut app = applied(hundred_lines(), &[Action::Bottom]);
    app.resize(20);
    assert_eq!(pos(&app), (80, Some(99)));
}

#[test]
fn shrinking_at_the_bottom_keeps_the_last_line_on_screen() {
    let mut app = applied(hundred_lines(), &[Action::Bottom]);
    app.resize(4);
    assert_eq!(pos(&app), (96, Some(99)));
}

#[test]
fn page_size_follows_the_new_height() {
    let mut app = hundred_lines();
    app.resize(20);
    app.apply(Action::PageDown);
    assert_eq!(pos(&app), (20, Some(20)));
}

// --- following ---

/// The lines of the rows on screen, top to bottom.
fn screen(app: &App) -> Vec<String> {
    let viewport = app.viewport();
    let end = viewport.total().min(viewport.top() + viewport.height());
    (viewport.top()..end)
        .map(|i| app.line(i).into_owned())
        .collect()
}

#[test]
fn following_shows_the_last_screen_until_the_whole_index_takes_over() {
    // Several chunks of the index, so that some come in before it is complete.
    let log = numbered_log(10_000);
    let last_screen: Vec<&str> = log.lines().skip(9_990).collect();
    let mut app = following_app(Preset::Raw, log.as_bytes());
    app.resize(10);
    let deadline = Instant::now() + LARGE_INDEX_DEADLINE;
    loop {
        app.poll_index();
        assert_eq!(screen(&app), last_screen, "{:?}", app.progress());
        assert_eq!(
            app.viewport().selected(),
            Some(app.viewport().top() + 9),
            "{:?}",
            app.progress()
        );
        if app.progress().is_complete() {
            break;
        }
        assert!(Instant::now() < deadline, "{:?}", app.progress());
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(pos(&app), (9_990, Some(9_999)));
}

#[test]
fn following_reads_the_last_entries_once_the_screen_has_lines() {
    let log = numbered_log(100);
    let mut app = following_app(Preset::Raw, log.as_bytes());
    app.resize(0);
    app.resize(10);
    assert_eq!(screen(&app), log.lines().skip(90).collect::<Vec<_>>());
    assert_eq!(app.viewport().selected(), Some(app.viewport().top() + 9));
}

#[test]
fn following_a_file_whose_tail_is_too_far_back_waits_for_the_whole_index() {
    // The last line starts further from the end than the tail may read.
    let mut log = b"first\nsecond\n".to_vec();
    log.resize(log.len() + TAIL_LIMIT + 1, b'x');
    let mut app = following_app(Preset::Raw, &log);
    app.resize(10);
    let deadline = Instant::now() + INDEX_DEADLINE;
    loop {
        app.poll_index();
        if app.progress().is_complete() {
            break;
        }
        assert_eq!(app.viewport().total(), 0, "{:?}", app.progress());
        assert!(Instant::now() < deadline, "{:?}", app.progress());
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(pos(&app), (0, Some(2)));
}

#[test]
fn following_a_file_shorter_than_the_screen_selects_its_last_entry() {
    let mut app = following_app(
        Preset::Logback,
        [STARTED, FAILED, EXCEPTION, FRAME, DONE]
            .concat()
            .as_bytes(),
    );
    app.resize(10);
    app.poll_index();
    assert_eq!(pos(&app), (0, Some(2)));
    assert_eq!(app.line(2), DONE.trim_end());
    wait_for_index(&mut app);
    assert_eq!(pos(&app), (0, Some(2)));
}
