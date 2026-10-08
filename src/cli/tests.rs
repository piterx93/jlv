use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

use super::*;

fn parse(args: &[&str]) -> Result<Args, clap::Error> {
    Args::try_parse_from(std::iter::once("jlv").chain(args.iter().copied()))
}

fn defaults(file: &str) -> Args {
    Args {
        file: PathBuf::from(file),
        format: None,
        no_follow: false,
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
    assert_eq!(args.format, Some(Format::Spring3));
}

#[test]
fn long_format_forces_a_format() {
    let args = parse(&["--format", "wildfly", "app.log"]).unwrap();
    assert_eq!(args.format, Some(Format::Wildfly));
}

#[test]
fn format_accepts_the_equals_form() {
    let args = parse(&["--format=raw", "app.log"]).unwrap();
    assert_eq!(args.format, Some(Format::Raw));
}

#[test]
fn every_preset_name_maps_to_its_format() {
    let presets = [
        ("logback", Format::Logback),
        ("logback-basic", Format::LogbackBasic),
        ("spring3", Format::Spring3),
        ("spring2", Format::Spring2),
        ("wildfly", Format::Wildfly),
        ("wildfly-console", Format::WildflyConsole),
        ("raw", Format::Raw),
    ];
    for (name, format) in presets {
        let args = parse(&["-f", name, "app.log"]).unwrap();
        assert_eq!(args.format, Some(format), "-f {name}");
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
            format: Some(Format::Logback),
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
