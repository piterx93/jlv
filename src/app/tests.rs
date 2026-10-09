use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use proptest::collection::vec;
use proptest::prelude::*;
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

use super::*;
use crate::index::Progress;
use crate::test_support;
use crate::test_support::{
    INDEX_DEADLINE, LARGE_INDEX_DEADLINE, app, app_in, char_key, ctrl, detected_app, following_app,
    key, named_app, numbered_log, unindexed_app, wait_for_index, wait_for_index_within,
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

// --- key mapping: the table ---

#[test]
fn left_arrow_scrolls_left() {
    assert_eq!(action(key(KeyCode::Left)), Some(Action::ScrollLeft));
}

#[test]
fn right_arrow_scrolls_right() {
    assert_eq!(action(key(KeyCode::Right)), Some(Action::ScrollRight));
}

#[test]
fn capital_f_toggles_follow() {
    assert_eq!(action(char_key('F')), Some(Action::ToggleFollow));
}

#[test]
fn capital_t_toggles_the_thread_column() {
    assert_eq!(action(char_key('T')), Some(Action::ToggleThread));
}

#[test]
fn hash_toggles_the_line_numbers() {
    assert_eq!(action(char_key('#')), Some(Action::ToggleLineNumbers));
}

#[test]
fn w_toggles_wrapping() {
    assert_eq!(action(char_key('w')), Some(Action::ToggleWrap));
}

#[test]
fn capital_p_cycles_the_preset() {
    assert_eq!(action(char_key('P')), Some(Action::CyclePreset));
}

#[test]
fn space_toggles_the_fold_of_the_selected_entry() {
    assert_eq!(action(char_key(' ')), Some(Action::ToggleFold));
}

#[test]
fn z_folds_every_entry() {
    assert_eq!(action(char_key('z')), Some(Action::FoldAll));
}

#[test]
fn capital_z_unfolds_every_entry() {
    assert_eq!(action(char_key('Z')), Some(Action::UnfoldAll));
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
        Some(Preset::WildflyConsole),
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
        Some(Preset::LogbackBasic),
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
fn app_without_a_format_reads_the_file_in_the_detected_preset() {
    let app = detected_app(
        [STARTED, FAILED, EXCEPTION, FRAME, DONE]
            .concat()
            .as_bytes(),
    );
    assert_eq!(app.settings().preset, Preset::Logback);
    assert_eq!(app.viewport().total(), 3);
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
fn line_numbers_are_shown_from_the_start() {
    assert!(hundred_lines().shows_line_numbers());
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

#[test]
fn toggling_the_line_numbers_hides_them_and_shows_them_again() {
    let mut app = applied(hundred_lines(), &[Action::ToggleLineNumbers]);
    assert!(!app.shows_line_numbers());
    app.apply(Action::ToggleLineNumbers);
    assert!(app.shows_line_numbers());
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

// --- cycling presets ---

fn cycled(mut app: App) -> App {
    app.apply(Action::CyclePreset);
    app
}

#[test]
fn cycling_reads_the_file_in_the_next_preset() {
    let app = cycled(logback(&[STARTED]));
    assert_eq!(app.settings().preset, Preset::LogbackBasic);
}

#[test]
fn cycling_past_raw_starts_again_at_the_first_preset() {
    let app = cycled(app(STARTED.as_bytes()));
    assert_eq!(app.settings().preset, Preset::Logback);
}

#[test]
fn cycling_indexes_the_file_again_in_the_new_preset() {
    let log = [STARTED, FAILED, EXCEPTION, FRAME, DONE].concat();
    let mut app = cycled(app(log.as_bytes()));
    wait_for_index(&mut app);
    assert_eq!(app.viewport().total(), 3);
    assert_eq!(app.line(2), DONE.trim_end());
}

#[test]
fn cycling_a_file_opened_at_the_top_selects_the_first_row_again() {
    let log = [STARTED, FAILED, EXCEPTION, FRAME, DONE].concat();
    let mut app = app(log.as_bytes());
    app.resize(10);
    let mut app = cycled(applied(app, &[Action::Down, Action::Down, Action::Down]));
    wait_for_index(&mut app);
    assert_eq!(pos(&app), (0, Some(0)));
}

#[test]
fn cycling_a_followed_file_shows_its_last_entries_in_the_new_preset_at_once() {
    let log = [STARTED, FAILED, EXCEPTION, FRAME, DONE].concat();
    let mut app = following_app(Preset::Raw, log.as_bytes());
    app.resize(10);
    wait_for_index(&mut app);
    assert_eq!(pos(&app), (0, Some(4)));
    let mut app = cycled(app);
    assert_eq!(app.viewport().total(), 3);
    assert_eq!(pos(&app), (0, Some(2)));
    assert_eq!(app.line(2), DONE.trim_end());
    wait_for_index(&mut app);
    assert_eq!(pos(&app), (0, Some(2)));
}

// --- follow mode ---

/// A viewer over `content` in `preset` on a 10-line screen, opened at the end to
/// follow it, with the whole file indexed.
fn followed(preset: Preset, content: &str) -> App {
    let mut app = following_app(preset, content.as_bytes());
    app.resize(10);
    wait_for_index(&mut app);
    app
}

/// `app` after the file changed as `change` says, with what changed indexed.
fn changed(mut app: App, change: FileChange) -> App {
    app.file_changed(change);
    wait_for_index(&mut app);
    app
}

/// `app` after its file grew to `content`.
fn grown(app: App, content: &str) -> App {
    changed(
        app,
        FileChange::Grew(test_support::source(content.as_bytes())),
    )
}

/// `app` after its file was cut short and holds `content` now.
fn truncated(app: App, content: &str) -> App {
    changed(
        app,
        FileChange::Truncated(test_support::source(content.as_bytes())),
    )
}

/// 20 numbered lines, followed.
fn twenty_lines_followed() -> App {
    followed(Preset::Raw, &numbered_log(20))
}

#[test]
fn opening_at_the_end_follows_the_file() {
    assert!(twenty_lines_followed().following());
}

#[test]
fn opening_at_the_top_does_not_follow_the_file() {
    assert!(!hundred_lines().following());
}

#[test]
fn appended_entries_are_shown_with_the_last_one_selected_while_following() {
    let app = grown(twenty_lines_followed(), &numbered_log(25));
    assert_eq!(pos(&app), (15, Some(24)));
    assert_eq!(app.line(24), numbered_log(25).lines().last().unwrap());
    assert!(app.following());
    assert_eq!(app.new_entries(), 0);
}

#[test]
fn every_move_away_from_the_bottom_stops_following() {
    for action in [Action::Up, Action::PageUp, Action::HalfPageUp, Action::Top] {
        let app = applied(twenty_lines_followed(), &[action]);
        assert!(!app.following(), "{action:?}");
    }
}

#[test]
fn actions_that_stay_at_the_bottom_keep_following() {
    let app = applied(
        twenty_lines_followed(),
        &[
            Action::Down,
            Action::PageDown,
            Action::HalfPageDown,
            Action::Bottom,
            Action::ScrollRight,
            Action::ToggleThread,
        ],
    );
    assert!(app.following());
}

#[test]
fn appends_while_paused_do_not_move_the_view() {
    let app = applied(twenty_lines_followed(), &[Action::PageUp]);
    let before = pos(&app);
    let app = grown(app, &numbered_log(25));
    assert_eq!(pos(&app), before);
    assert_eq!(app.viewport().total(), 25);
}

#[test]
fn appends_while_paused_count_as_new_entries() {
    let app = applied(twenty_lines_followed(), &[Action::Up]);
    let app = grown(app, &numbered_log(23));
    assert_eq!(app.new_entries(), 3);
    let app = grown(app, &numbered_log(25));
    assert_eq!(app.new_entries(), 5);
}

#[test]
fn lines_added_to_the_last_entry_are_not_new_entries() {
    let app = followed(Preset::Logback, &[STARTED, FAILED].concat());
    let app = applied(app, &[Action::Up]);
    let app = grown(app, &[STARTED, FAILED, EXCEPTION, FRAME, DONE].concat());
    assert_eq!(app.new_entries(), 1);
}

#[test]
fn toggling_follow_while_following_stops_following_in_place() {
    let app = applied(twenty_lines_followed(), &[Action::ToggleFollow]);
    assert!(!app.following());
    assert_eq!(pos(&app), (10, Some(19)));
}

#[test]
fn toggling_follow_while_paused_follows_again_from_the_bottom() {
    let app = applied(twenty_lines_followed(), &[Action::PageUp]);
    let app = applied(grown(app, &numbered_log(25)), &[Action::ToggleFollow]);
    assert!(app.following());
    assert_eq!(pos(&app), (15, Some(24)));
    assert_eq!(app.new_entries(), 0);
}

#[test]
fn going_to_the_bottom_follows_again() {
    let app = applied(twenty_lines_followed(), &[Action::Top]);
    let app = applied(grown(app, &numbered_log(25)), &[Action::Bottom]);
    assert!(app.following());
    assert_eq!(app.new_entries(), 0);
}

#[test]
fn moving_down_onto_the_last_row_does_not_follow_again() {
    let app = applied(twenty_lines_followed(), &[Action::Up, Action::Down]);
    assert_eq!(pos(&app), (10, Some(19)));
    assert!(!app.following());
}

#[test]
fn growth_with_the_index_complete_is_shown_at_once() {
    let mut app = twenty_lines_followed();
    app.file_changed(FileChange::Grew(test_support::source(
        numbered_log(21).as_bytes(),
    )));
    assert_eq!(app.viewport().total(), 21);
}

#[test]
fn growth_during_indexing_is_appended_to_the_complete_index() {
    // Appended entries are dated forward from the file as it was, which is dated back
    // from its modification time at noon: 11:00 stays on its day and 15:00 follows
    // it. Indexed at once, 15:00 would be too far past noon for that day.
    let before = "11:00:00.000 [main] INFO  com.example.App - before\n";
    let after = "15:00:00.000 [main] INFO  com.example.App - after\n";
    let mut app = unindexed_app(Preset::LogbackBasic, before.as_bytes());
    let grown = test_support::source([before, after].concat().as_bytes());
    app.file_changed(FileChange::Grew(grown));
    wait_for_index(&mut app);
    let timestamp = |i| app.row(i).unwrap().record.timestamp().unwrap().to_string();
    assert_eq!(app.viewport().total(), 2);
    assert_eq!(timestamp(0), "2026-10-08T11:00:00Z");
    assert_eq!(timestamp(1), "2026-10-08T15:00:00Z");
}

#[test]
fn growth_shorter_than_the_index_changes_nothing() {
    // The file was cut short between the watcher's look and the mapping.
    let app = grown(twenty_lines_followed(), &numbered_log(5));
    assert_eq!(app.viewport().total(), 20);
    assert_eq!(pos(&app), (10, Some(19)));
}

#[test]
fn truncation_while_paused_selects_the_truncation_row() {
    let app = applied(twenty_lines_followed(), &[Action::Up]);
    let app = truncated(app, "after 1\nafter 2\n");
    assert_eq!(pos(&app), (0, Some(0)));
    assert!(!app.following());
}

#[test]
fn truncation_while_paused_counts_the_new_content_as_new_entries() {
    let app = applied(twenty_lines_followed(), &[Action::Up]);
    let app = grown(app, &numbered_log(22));
    let app = truncated(app, "after 1\nafter 2\nafter 3\n");
    assert_eq!(app.new_entries(), 3);
}

#[test]
fn toggling_the_fold_on_the_truncation_row_does_nothing() {
    let log = [FAILED, EXCEPTION].concat();
    let app = truncated(followed(Preset::Logback, &log), &log);
    let app = applied(app, &[Action::Top, Action::ToggleFold]);
    assert_eq!(app.viewport().total(), 2);
    assert_eq!(pos(&app), (0, Some(0)));
}

#[test]
fn lines_are_read_from_the_new_content_after_truncation() {
    let app = applied(logback(&[FAILED, EXCEPTION, FRAME]), &[Action::UnfoldAll]);
    assert_eq!(app.line(2), FRAME.trim_end());
    let caused = "Caused by: java.io.IOException: x\n";
    let app = truncated(app, &[FAILED, caused, FRAME].concat());
    let app = applied(app, &[Action::UnfoldAll]);
    assert_eq!(app.line(2), caused.trim_end());
}

#[test]
fn folded_entries_stay_folded_as_the_file_grows() {
    let app = followed(Preset::Logback, &[STARTED, FAILED, EXCEPTION].concat());
    let app = grown(app, &[STARTED, FAILED, EXCEPTION, DONE].concat());
    assert_eq!(app.viewport().total(), 3);
}

#[test]
fn entry_gaining_lines_after_unfold_all_is_unfolded() {
    let app = applied(
        followed(Preset::Logback, &[STARTED, FAILED].concat()),
        &[Action::UnfoldAll],
    );
    let app = grown(app, &[STARTED, FAILED, EXCEPTION, FRAME].concat());
    assert_eq!(app.viewport().total(), 4);
}

#[test]
fn unfolded_last_entry_shows_the_lines_appended_to_it() {
    let app = followed(Preset::Logback, &[STARTED, FAILED, EXCEPTION].concat());
    let app = applied(app, &[Action::ToggleFold]);
    assert_eq!(app.viewport().total(), 3);
    let app = grown(app, &[STARTED, FAILED, EXCEPTION, FRAME].concat());
    assert_eq!(app.viewport().total(), 4);
    assert_eq!(app.line(3), FRAME.trim_end());
}

#[test]
fn deleted_file_is_waited_for_with_its_rows_kept() {
    let app = changed(twenty_lines_followed(), FileChange::Gone);
    assert!(app.waiting_for_file());
    assert_eq!(app.viewport().total(), 20);
    assert_eq!(pos(&app), (10, Some(19)));
}

#[test]
fn growth_of_the_deleted_file_keeps_waiting() {
    // The watcher reads the old file's last bytes after it reported it gone.
    let app = changed(twenty_lines_followed(), FileChange::Gone);
    let app = grown(app, &numbered_log(21));
    assert!(app.waiting_for_file());
    assert_eq!(app.viewport().total(), 21);
}

#[test]
fn file_back_at_the_path_is_shown_from_its_start() {
    let app = changed(twenty_lines_followed(), FileChange::Gone);
    let app = changed(
        app,
        FileChange::Replaced(test_support::source(b"new 1\nnew 2\n")),
    );
    assert!(!app.waiting_for_file());
    assert_eq!(screen(&app), ["new 1", "new 2"]);
    assert_eq!(app.viewport().selected(), Some(1));
    assert!(app.following());
}

/// Something that happens to a followed Logback file or its viewer.
#[derive(Debug, Clone)]
enum Step {
    Apply(Action),
    /// The file gets these bytes appended.
    Append(Vec<u8>),
    /// The file is cut short and then holds these bytes.
    Truncate(Vec<u8>),
    /// Another file holding these bytes takes the file's place.
    Replace(Vec<u8>),
    Gone,
}

/// Logback headers and continuation lines mixed with arbitrary bytes, not always
/// ending a line. The bytes leave out 0x1f, so that no file starts like a gzip file,
/// which is opened differently.
fn some_log() -> impl Strategy<Value = Vec<u8>> {
    let byte = any::<u8>().prop_map(|b| if b == 0x1f { b'?' } else { b });
    let part = prop_oneof![
        prop::sample::select(vec![STARTED, FAILED, EXCEPTION, FRAME, DONE])
            .prop_map(|line| line.as_bytes().to_vec()),
        vec(byte, 0..20),
    ];
    vec(part, 0..4).prop_map(|parts| parts.concat())
}

fn step() -> impl Strategy<Value = Step> {
    let action = prop::sample::select(vec![
        Action::Up,
        Action::Down,
        Action::PageUp,
        Action::Top,
        Action::Bottom,
        Action::ToggleFollow,
        Action::ToggleFold,
        Action::FoldAll,
        Action::UnfoldAll,
    ]);
    prop_oneof![
        4 => action.prop_map(Step::Apply),
        4 => some_log().prop_map(Step::Append),
        1 => some_log().prop_map(Step::Truncate),
        1 => some_log().prop_map(Step::Replace),
        1 => Just(Step::Gone),
    ]
}

/// Every row's line, top to bottom.
fn all_lines(app: &App) -> Vec<String> {
    (0..app.viewport().total())
        .map(|i| app.line(i).into_owned())
        .collect()
}

// --- gzip files ---

/// 100 numbered lines in a gzip file on a 10-line screen, opened at the end, with
/// the whole file indexed.
fn hundred_lines_gzipped() -> App {
    let mut app = test_support::gzip_app(&test_support::gzip(numbered_log(100).as_bytes()));
    app.resize(10);
    wait_for_index(&mut app);
    app
}

#[test]
fn gzip_file_opens_at_the_end_without_following() {
    let app = hundred_lines_gzipped();
    assert!(!app.following());
    assert_eq!(pos(&app), (90, Some(99)));
}

#[test]
fn going_to_the_bottom_of_a_gzip_file_does_not_follow() {
    let app = applied(hundred_lines_gzipped(), &[Action::Top, Action::Bottom]);
    assert!(!app.following());
    assert_eq!(pos(&app), (90, Some(99)));
}

#[test]
fn toggling_follow_on_a_gzip_file_does_nothing() {
    let app = applied(
        hundred_lines_gzipped(),
        &[Action::Top, Action::ToggleFollow],
    );
    assert!(!app.following());
    assert_eq!(pos(&app), (0, Some(0)));
}

/// Cycling the preset opens the file again the way it was opened, at the end, even
/// though a gzip file is never followed.
#[test]
fn cycling_the_preset_of_a_gzip_file_opened_at_the_end_stays_at_the_end() {
    let log: String = (1..=100)
        .map(|i| format!("2026-10-07 12:00:00.{i:03} [main] INFO  com.example.App - step {i}\n"))
        .collect();
    let mut app = test_support::gzip_app(&test_support::gzip(log.as_bytes()));
    app.resize(10);
    wait_for_index(&mut app);
    let mut app = cycled(app);
    wait_for_index(&mut app);
    assert_eq!(app.settings().preset, Preset::Logback);
    assert_eq!(pos(&app), (90, Some(99)));
    assert!(!app.following());
}

#[test]
fn gzip_file_is_not_watched() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("app.log.gz");
    fs::write(&path, test_support::gzip(numbered_log(100).as_bytes())).unwrap();
    let mut app = hundred_lines_gzipped();
    app.watch(&path, File::open(&path).unwrap(), Options::default())
        .unwrap();
    assert!(!app.expects_updates());
}

proptest! {
    #[test]
    fn followed_file_shows_the_rows_of_its_content_indexed_at_once(
        start in some_log(),
        steps in vec(step(), 0..8),
    ) {
        let mut app = following_app(Preset::Logback, &start);
        app.resize(5);
        wait_for_index(&mut app);
        let mut content = start;
        let mut truncation_row = false;
        for step in steps {
            match step {
                Step::Apply(action) => app.apply(action),
                Step::Append(bytes) => {
                    content.extend(bytes);
                    app.file_changed(FileChange::Grew(test_support::source(&content)));
                }
                Step::Truncate(bytes) => {
                    content = bytes;
                    truncation_row = true;
                    app.file_changed(FileChange::Truncated(test_support::source(&content)));
                }
                Step::Replace(bytes) => {
                    content = bytes;
                    truncation_row = false;
                    app.file_changed(FileChange::Replaced(test_support::source(&content)));
                }
                Step::Gone => app.file_changed(FileChange::Gone),
            }
            wait_for_index(&mut app);
            let total = app.viewport().total();
            prop_assert!(app.viewport().selected().is_none_or(|row| row < total));
        }
        app.apply(Action::UnfoldAll);
        let mut fresh = app_in(Preset::Logback, &content);
        fresh.apply(Action::UnfoldAll);
        let mut expected = all_lines(&fresh);
        if truncation_row {
            expected.insert(0, "── file truncated ──".to_string());
        }
        prop_assert_eq!(all_lines(&app), expected);
    }
}
