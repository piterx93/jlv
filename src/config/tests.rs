use std::collections::HashMap;

use proptest::prelude::*;
use ratatui::style::Color;

use super::*;
use crate::app::Action;
use crate::keymap::Key;

fn fg(color: Color) -> Style {
    Style::new().fg(color)
}

fn parsed(text: &str) -> Config {
    Config::parse(text).unwrap_or_else(|err| panic!("{text:?} doesn't parse: {err}"))
}

/// Fails the test unless `text` is an error that mentions every one of `needles`.
fn assert_error(text: &str, needles: &[&str]) {
    let err = Config::parse(text).expect_err(text);
    for needle in needles {
        assert!(err.contains(needle), "{needle:?} missing from {err:?}");
    }
}

// --- defaults ---

#[test]
fn defaults_are_those_of_the_prd() {
    let expected = Config {
        ui: Ui {
            timestamp_format: "%Y-%m-%d %H:%M:%S%.3f".to_string(),
            show_thread: false,
            show_histogram: true,
            show_line_numbers: true,
            mouse: true,
            wrap: false,
        },
        colors: Colors {
            trace: fg(Color::DarkGray),
            debug: fg(Color::Cyan),
            info: fg(Color::Green),
            warn: fg(Color::Yellow),
            error: fg(Color::Red),
            fatal: Style::new().fg(Color::White).bg(Color::Red).bold(),
            line_number: fg(Color::DarkGray),
        },
        follow: Follow {
            poll_interval: Duration::from_millis(250),
        },
        filters: Filters {
            default_class_excludes: Classes::default(),
        },
        keys: Keymap::default(),
    };
    assert_eq!(Config::default(), expected);
}

#[test]
fn empty_file_gives_the_defaults() {
    assert_eq!(parsed(""), Config::default());
}

#[test]
fn a_key_left_out_keeps_its_default() {
    let mut expected = Config::default();
    expected.ui.show_thread = true;
    assert_eq!(parsed("[ui]\nshow_thread = true\n"), expected);
}

#[test]
fn a_level_jlv_does_not_know_has_the_terminal_default_style() {
    assert_eq!(Config::default().colors.level(None), Style::new());
}

// --- sections ---

#[test]
fn ui_section_sets_every_ui_key() {
    let config = parsed(concat!(
        "[ui]\n",
        "timestamp_format = \"%d.%m. %H:%M:%S\"\n",
        "show_thread = true\n",
        "show_histogram = false\n",
        "show_line_numbers = false\n",
        "mouse = false\n",
        "wrap = true\n",
    ));
    assert_eq!(
        config.ui,
        Ui {
            timestamp_format: "%d.%m. %H:%M:%S".to_string(),
            show_thread: true,
            show_histogram: false,
            show_line_numbers: false,
            mouse: false,
            wrap: true,
        }
    );
}

#[test]
fn colors_section_takes_colour_names_and_hex_colours() {
    let config = parsed(concat!(
        "[colors]\n",
        "trace = \"gray\"\n",
        "debug = \"blue\"\n",
        "info = \"lightgreen\"\n",
        "warn = \"magenta\"\n",
        "error = \"#ff8800\"\n",
        "fatal = \"lightred\"\n",
        "line_number = \"white\"\n",
    ));
    assert_eq!(
        config.colors,
        Colors {
            trace: fg(Color::Gray),
            debug: fg(Color::Blue),
            info: fg(Color::LightGreen),
            warn: fg(Color::Magenta),
            error: fg(Color::Rgb(0xff, 0x88, 0x00)),
            fatal: fg(Color::LightRed),
            line_number: fg(Color::White),
        }
    );
}

#[test]
fn inline_table_colour_sets_any_of_foreground_background_and_bold() {
    let config = parsed(concat!(
        "[colors]\n",
        "fatal = { fg = \"black\", bg = \"yellow\", bold = true }\n",
        "error = { bg = \"red\" }\n",
    ));
    assert_eq!(
        config.colors.fatal,
        Style::new().fg(Color::Black).bg(Color::Yellow).bold()
    );
    assert_eq!(config.colors.error, Style::new().bg(Color::Red));
}

#[test]
fn follow_section_sets_the_poll_interval() {
    let config = parsed("[follow]\npoll_interval_ms = 1000\n");
    assert_eq!(config.follow.poll_interval, Duration::from_secs(1));
}

#[test]
fn filters_section_makes_each_class_exclude_an_exclude_rule() {
    let config =
        parsed("[filters]\ndefault_class_excludes = [\"org.hibernate.SQL\", \"com.zaxxer.*\"]\n");
    assert_eq!(
        config.filters.default_class_excludes,
        Classes::parse(["!org.hibernate.SQL", "!com.zaxxer.*"]).unwrap()
    );
}

fn key(text: &str) -> Key {
    text.parse().unwrap()
}

/// Quit, given a list, loses its default `Ctrl-c`.
#[test]
fn keys_section_binds_each_named_action_to_one_key_or_a_list() {
    let config = parsed("[keys]\ntoggle_follow = \"f\"\nquit = [\"q\", \"F10\"]\n");
    let expected = Keymap::remapped(&[
        (Action::ToggleFollow, vec![key("f")]),
        (Action::Quit, vec![key("q"), key("F10")]),
    ])
    .unwrap();
    assert_eq!(config.keys, expected);
}

#[test]
fn key_of_another_action_is_free_once_that_action_is_remapped_too() {
    let keys = parsed("[keys]\ntoggle_follow = \"q\"\nquit = \"Q\"\n").keys;
    let press = |text| {
        let Key { code, modifiers } = key(text);
        keys.action(ratatui::crossterm::event::KeyEvent::new(code, modifiers))
    };
    assert_eq!(
        (press("q"), press("Q"), press("F")),
        (Some(Action::ToggleFollow), Some(Action::Quit), None)
    );
}

// --- errors ---

#[test]
fn unknown_keys_are_errors_naming_the_key_and_its_line() {
    let cases = [
        (
            "[ui]\nshow_thread = true\nshow_threads = true\n",
            "show_threads",
            "line 3",
        ),
        (
            "[follow]\npoll_interval_ms = 100\n\n[colours]\ninfo = \"blue\"\n",
            "colours",
            "line 4",
        ),
        (
            "[colors]\nfatal = { fg = \"white\", blod = true }\n",
            "blod",
            "line 2",
        ),
    ];
    for (text, key, line) in cases {
        assert_error(text, &[key, line]);
    }
}

#[test]
fn invalid_toml_is_an_error_naming_its_line() {
    assert_error("[ui]\nshow_thread = \n", &["line 2"]);
}

#[test]
fn value_of_the_wrong_type_is_an_error_naming_the_key_and_its_line() {
    assert_error(
        "[ui]\n\nshow_thread = \"yes\"\n",
        &["show_thread", "line 3"],
    );
}

#[test]
fn unknown_colour_name_is_an_error_naming_the_colour_and_its_line() {
    assert_error("[colors]\ninfo = \"pink\"\n", &["pink", "line 2"]);
}

#[test]
fn colour_of_the_wrong_type_is_an_error_saying_what_was_expected() {
    assert_error(
        "[colors]\nwarn = 5\n",
        &["warn", "line 2", "expected a colour"],
    );
}

#[test]
fn poll_interval_of_zero_is_an_error_naming_the_key_and_its_line() {
    assert_error(
        "[follow]\npoll_interval_ms = 0\n",
        &["poll_interval_ms", "line 2"],
    );
}

/// `%K` is no strftime directive, so no timestamp could be shown with it.
#[test]
fn timestamp_format_strftime_cannot_use_is_an_error_naming_the_key_and_its_line() {
    assert_error(
        "[ui]\ntimestamp_format = \"%H:%M %K\"\n",
        &["timestamp_format", "line 2"],
    );
}

/// A control character in a timestamp would be written into the table rows.
#[test]
fn timestamp_format_that_makes_control_characters_is_an_error_naming_the_key_and_its_line() {
    for format in ["%H:%M%n", "%t%H:%M", "%H:%M\\t"] {
        assert_error(
            &format!("[ui]\ntimestamp_format = \"{format}\"\n"),
            &["timestamp_format", "line 2"],
        );
    }
}

#[test]
fn class_exclude_that_names_no_class_is_an_error_naming_the_key_and_its_line() {
    assert_error(
        "[filters]\ndefault_class_excludes = [\"*\"]\n",
        &["default_class_excludes", "line 2"],
    );
}

#[test]
fn key_bound_to_two_actions_is_an_error_naming_both() {
    let cases = [
        ("[keys]\ntoggle_follow = \"q\"\n", ["toggle_follow", "quit"]),
        (
            "[keys]\ncopy = \"v\"\ntoggle_wrap = [\"w\", \"v\"]\n",
            ["copy", "toggle_wrap"],
        ),
    ];
    for (text, actions) in cases {
        assert_error(text, &actions);
    }
}

#[test]
fn unknown_action_is_an_error_naming_it_and_its_line() {
    assert_error(
        "[keys]\nquit = \"x\"\ntoggle_folow = \"f\"\n",
        &["unknown action", "toggle_folow", "line 3"],
    );
}

#[test]
fn empty_key_list_is_an_error_naming_the_action_and_its_line() {
    assert_error(
        "[keys]\nquit = \"x\"\ntoggle_follow = []\n",
        &["toggle_follow", "line 3"],
    );
}

#[test]
fn key_outside_the_syntax_is_an_error_naming_it_and_its_line() {
    assert_error(
        "[keys]\nhelp = \"h\"\nquit = [\"x\", \"Ctl-q\"]\n",
        &["unknown key", "Ctl-q", "line 3"],
    );
}

#[test]
fn keys_of_the_wrong_type_are_errors_saying_what_was_expected() {
    let cases = [
        (
            "keys = \"q\"\n",
            "expected a table of action names and keys",
        ),
        ("[keys]\nquit = 5\n", "expected a key or a list of keys"),
    ];
    for (text, expected) in cases {
        assert_error(text, &[expected]);
    }
}

// --- where the file is ---

fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let vars: HashMap<String, OsString> = vars
        .iter()
        .map(|&(name, value)| (name.to_string(), OsString::from(value)))
        .collect();
    move |name| vars.get(name).cloned()
}

#[test]
fn default_path_is_under_xdg_config_home() {
    let var = env(&[("XDG_CONFIG_HOME", "/cfg"), ("HOME", "/home/me")]);
    assert_eq!(
        default_path(var),
        Some(PathBuf::from("/cfg/jlv/config.toml"))
    );
}

#[test]
fn default_path_is_under_home_without_xdg_config_home() {
    for vars in [
        &[("HOME", "/home/me")][..],
        &[("XDG_CONFIG_HOME", ""), ("HOME", "/home/me")],
    ] {
        assert_eq!(
            default_path(env(vars)),
            Some(PathBuf::from("/home/me/.config/jlv/config.toml")),
            "{vars:?}"
        );
    }
}

#[test]
fn there_is_no_default_path_without_xdg_config_home_or_home() {
    assert_eq!(default_path(env(&[])), None);
}

// --- robustness ---

/// Lines that are valid or nearly valid configuration, so that arbitrary
/// combinations of them reach past the TOML parser.
fn config_line() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::sample::select(vec![
            "[ui]",
            "[colors]",
            "[follow]",
            "[filters]",
            "[keys]",
            "show_thread = true",
            "timestamp_format = \"%H:%M:%S%.3f\"",
            "fatal = { fg = \"white\", bg = \"red\", bold = true }",
            "info = \"#12345\"",
            "poll_interval_ms = 18446744073709551615",
            "poll_interval_ms = -1",
            "default_class_excludes = [\"org.hibernate.SQL\", \"!\"]",
            "toggle_follow = \"q\"",
            "quit = [\"Q\", \"Ctrl-\"]",
            "help = []",
            "toggle_level_info = [\"i\", \"i\"]",
        ])
        .prop_map(String::from),
        "[a-z_]{1,20} = (true|\"[a-z%#. ]{0,12}\"|[0-9]{1,6}|\\[\\])",
        any::<String>(),
    ]
}

proptest! {
    #[test]
    fn parse_never_panics_on_arbitrary_text(lines in prop::collection::vec(config_line(), 0..8)) {
        let _ = Config::parse(&lines.join("\n"));
    }
}
