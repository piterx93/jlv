use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

use super::*;
use crate::filter::{Classes, Levels, TimeSpec};
use crate::format::{Level, Preset};

fn parse(args: &[&str]) -> Result<Args, clap::Error> {
    Args::try_parse_from(std::iter::once("jlv").chain(args.iter().copied()))
}

fn defaults(file: &str) -> Args {
    Args {
        file: PathBuf::from(file),
        format: None,
        no_follow: false,
        level: None,
        class: None,
        since: None,
        no_mouse: false,
        config: None,
    }
}

#[test]
fn command_definition_is_consistent() {
    Args::command().debug_assert();
}

#[test]
fn file_alone_leaves_every_option_at_its_default() {
    assert_eq!(parse(&["app.log"]).unwrap(), defaults("app.log"));
}

#[test]
fn short_f_forces_a_format() {
    let args = parse(&["-f", "spring3", "app.log"]).unwrap();
    assert_eq!(args.format, Some(Preset::Spring3));
}

#[test]
fn long_format_forces_a_format() {
    let args = parse(&["--format", "wildfly", "app.log"]).unwrap();
    assert_eq!(args.format, Some(Preset::Wildfly));
}

#[test]
fn format_accepts_the_equals_form() {
    let args = parse(&["--format=raw", "app.log"]).unwrap();
    assert_eq!(args.format, Some(Preset::Raw));
}

#[test]
fn every_preset_name_maps_to_its_preset() {
    let presets = [
        ("logback", Preset::Logback),
        ("logback-basic", Preset::LogbackBasic),
        ("spring3", Preset::Spring3),
        ("spring2", Preset::Spring2),
        ("wildfly", Preset::Wildfly),
        ("wildfly-console", Preset::WildflyConsole),
        ("raw", Preset::Raw),
    ];
    for (name, preset) in presets {
        let args = parse(&["-f", name, "app.log"]).unwrap();
        assert_eq!(args.format, Some(preset), "-f {name}");
    }
}

#[test]
fn no_follow_flag_is_parsed() {
    let args = parse(&["--no-follow", "app.log"]).unwrap();
    assert_eq!(
        args,
        Args {
            no_follow: true,
            ..defaults("app.log")
        }
    );
}

#[test]
fn no_mouse_flag_is_parsed() {
    let args = parse(&["--no-mouse", "app.log"]).unwrap();
    assert_eq!(
        args,
        Args {
            no_mouse: true,
            ..defaults("app.log")
        }
    );
}

#[test]
fn config_takes_a_path() {
    let args = parse(&["--config", "/etc/jlv.toml", "app.log"]).unwrap();
    assert_eq!(args.config, Some(PathBuf::from("/etc/jlv.toml")));
}

#[test]
fn options_may_follow_the_file() {
    let args = parse(&["app.log", "--no-mouse", "-f", "logback", "--no-follow"]).unwrap();
    assert_eq!(
        args,
        Args {
            format: Some(Preset::Logback),
            no_follow: true,
            no_mouse: true,
            ..defaults("app.log")
        }
    );
}

#[test]
fn short_v_asks_for_the_version() {
    assert_eq!(
        parse(&["-V"]).unwrap_err().kind(),
        ErrorKind::DisplayVersion
    );
}

#[test]
fn long_version_asks_for_the_version() {
    assert_eq!(
        parse(&["--version"]).unwrap_err().kind(),
        ErrorKind::DisplayVersion
    );
}

#[test]
fn version_wins_over_a_file() {
    assert_eq!(
        parse(&["app.log", "--version"]).unwrap_err().kind(),
        ErrorKind::DisplayVersion
    );
}

#[test]
fn missing_file_is_a_usage_error() {
    assert_eq!(
        parse(&[]).unwrap_err().kind(),
        ErrorKind::MissingRequiredArgument
    );
}

#[test]
fn second_file_is_rejected() {
    assert_eq!(
        parse(&["a.log", "b.log"]).unwrap_err().kind(),
        ErrorKind::UnknownArgument
    );
}

#[test]
fn unknown_format_is_an_invalid_value() {
    assert_eq!(
        parse(&["-f", "log4j", "app.log"]).unwrap_err().kind(),
        ErrorKind::InvalidValue
    );
}

#[test]
fn unknown_format_error_lists_every_preset_name() {
    let message = parse(&["-f", "log4j", "app.log"]).unwrap_err().to_string();
    assert!(
        message.contains(
            "[possible values: logback, logback-basic, spring3, spring2, wildfly, wildfly-console, raw]"
        ),
        "{message}"
    );
}

/// The levels `-l <arg>` turns on.
fn levels(arg: &str) -> Levels {
    parse(&["-l", arg, "app.log"]).unwrap().level.unwrap()
}

/// The message of the usage error `-l <arg>` makes.
fn level_error(arg: &str) -> String {
    let err = parse(&["-l", arg, "app.log"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ValueValidation, "{err}");
    err.to_string()
}

const VALID_LEVELS: &str = "TRACE, DEBUG, INFO, WARN, ERROR, FATAL";

#[test]
fn short_l_turns_on_only_the_levels_it_lists() {
    assert_eq!(
        levels("WARN,ERROR"),
        Levels::only(&[Level::Warn, Level::Error])
    );
}

#[test]
fn long_level_turns_on_only_the_levels_it_lists() {
    let args = parse(&["--level", "FATAL", "app.log"]).unwrap();
    assert_eq!(args.level, Some(Levels::only(&[Level::Fatal])));
}

#[test]
fn level_names_ignore_case() {
    assert_eq!(
        levels("warn,Error"),
        Levels::only(&[Level::Warn, Level::Error])
    );
}

#[test]
fn warning_means_warn() {
    assert_eq!(levels("WARNING"), Levels::only(&[Level::Warn]));
}

#[test]
fn empty_items_in_the_level_list_are_skipped() {
    assert_eq!(
        levels("WARN,,ERROR,"),
        Levels::only(&[Level::Warn, Level::Error])
    );
}

#[test]
fn a_level_named_twice_counts_once() {
    assert_eq!(levels("WARN,warn,WARNING"), Levels::only(&[Level::Warn]));
}

#[test]
fn unknown_level_error_names_it_and_lists_the_valid_ones() {
    let message = level_error("WARN,VERBOSE");
    assert!(message.contains("VERBOSE"), "{message}");
    assert!(message.contains(VALID_LEVELS), "{message}");
}

#[test]
fn empty_level_list_is_an_error_listing_the_valid_levels() {
    let message = level_error("");
    assert!(message.contains(VALID_LEVELS), "{message}");
}

#[test]
fn short_and_long_class_set_the_initial_class_rules() {
    for option in ["-c", "--class"] {
        let args = parse(&[option, "com.myco.*,!org.hibernate", "app.log"]).unwrap();
        assert_eq!(
            args.class,
            Some(Classes::parse(["com.myco.*", "!org.hibernate"]).unwrap()),
            "{option}"
        );
    }
}

#[test]
fn class_rules_are_split_at_commas_skipping_empty_items() {
    let args = parse(&["-c", ",com.myco.*,, !org.hibernate ,", "app.log"]).unwrap();
    assert_eq!(
        args.class,
        Some(Classes::parse(["com.myco.*", "!org.hibernate"]).unwrap())
    );
}

#[test]
fn a_class_rule_that_does_not_parse_is_a_usage_error_naming_it() {
    let err = parse(&["-c", "com.myco.*,!", "app.log"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ValueValidation, "{err}");
    assert!(err.to_string().contains("'!'"), "{err}");
}

#[test]
fn short_and_long_since_set_the_initial_time_spec() {
    for option in ["-s", "--since"] {
        let args = parse(&[option, "12:00-12:30", "app.log"]).unwrap();
        assert_eq!(args.since, TimeSpec::parse("12:00-12:30").ok(), "{option}");
    }
}

#[test]
fn a_time_spec_that_does_not_parse_is_a_usage_error_saying_why() {
    let err = parse(&["-s", "soon", "app.log"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ValueValidation, "{err}");
    let why = TimeSpec::parse("soon").unwrap_err();
    assert!(err.to_string().contains(&why), "{err}");
}

#[test]
fn dash_is_taken_as_the_file_name() {
    assert_eq!(parse(&["-"]).unwrap().file, PathBuf::from("-"));
}

#[test]
fn path_with_spaces_is_kept_whole() {
    let args = parse(&["/var/log/my app/server log.txt"]).unwrap();
    assert_eq!(args.file, PathBuf::from("/var/log/my app/server log.txt"));
}

#[test]
fn non_utf8_path_is_kept_byte_for_byte() {
    let raw = OsString::from_vec(b"/var/log/caf\xe9.log".to_vec());
    let args = Args::try_parse_from([OsString::from("jlv"), raw.clone()]).unwrap();
    assert_eq!(args.file, PathBuf::from(raw));
}
