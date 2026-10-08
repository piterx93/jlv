use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use clap::Parser;
use proptest::prelude::*;
use tempfile::TempDir;

use super::*;
use crate::cli::{Args, Format};

#[test]
fn version_without_commit_is_printed_as_is() {
    assert_eq!(version_string("0.3.0", None), "0.3.0");
}

#[test]
fn commit_follows_the_version_in_parentheses() {
    assert_eq!(
        version_string("0.3.0-dev.42", Some("a1b2c3d")),
        "0.3.0-dev.42 (a1b2c3d)"
    );
}

const LOG_LINE: &str = "12:00:00.123 [main] INFO  com.example.App -- Started\n";

#[derive(Debug)]
struct Outcome {
    result: Result<Launch, u8>,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn exit_code(&self) -> Option<u8> {
        self.result.as_ref().err().copied()
    }

    fn launch(self) -> Launch {
        match self.result {
            Ok(launch) => launch,
            Err(code) => panic!("exited with {code}: {}", self.stderr),
        }
    }
}

fn jlv_with_version<S: AsRef<OsStr>>(version: &str, args: &[S]) -> Outcome {
    let argv = std::iter::once(OsString::from("jlv"))
        .chain(args.iter().map(|arg| arg.as_ref().to_owned()));
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let result = start(argv, version, &mut stdout, &mut stderr);
    Outcome {
        result,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    }
}

fn jlv<S: AsRef<OsStr>>(args: &[S]) -> Outcome {
    jlv_with_version("0.3.0", args)
}

fn log_file(dir: &TempDir, name: impl AsRef<Path>) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, LOG_LINE).unwrap();
    path
}

fn defaults(file: impl Into<PathBuf>) -> Args {
    Args {
        file: file.into(),
        format: None,
        no_follow: false,
        no_mouse: false,
        config: None,
    }
}

fn runtime_error(path: &Path, reason: &str) -> String {
    format!("jlv: {}: {reason}\n", path.display())
}

// --- version and help: printed, then exit 0 ---

#[test]
fn long_version_prints_the_injected_version() {
    let out = jlv_with_version("0.3.0-dev.42 (a1b2c3d)", &["--version"]);
    assert_eq!(out.exit_code(), Some(0));
    assert_eq!(out.stdout, "jlv 0.3.0-dev.42 (a1b2c3d)\n");
    assert_eq!(out.stderr, "");
}

#[test]
fn short_v_prints_the_injected_version() {
    let out = jlv_with_version("1.2.3", &["-V"]);
    assert_eq!(out.exit_code(), Some(0));
    assert_eq!(out.stdout, "jlv 1.2.3\n");
}

#[test]
fn version_before_a_file_prints_the_version() {
    let out = jlv(&["--version", "/nonexistent/app.log"]);
    assert_eq!(out.exit_code(), Some(0));
    assert_eq!(out.stdout, "jlv 0.3.0\n");
    assert_eq!(out.stderr, "");
}

#[test]
fn version_with_a_valid_file_does_not_launch() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, "app.log");
    let out = jlv(&[file.as_os_str(), OsStr::new("-V")]);
    assert_eq!(out.exit_code(), Some(0));
    assert_eq!(out.stdout, "jlv 0.3.0\n");
    assert_eq!(out.stderr, "");
}

#[test]
fn help_goes_to_stdout_and_exits_0() {
    let out = jlv(&["--help"]);
    assert_eq!(out.exit_code(), Some(0));
    assert!(
        out.stdout.contains("Usage: jlv [OPTIONS] <FILE>"),
        "{}",
        out.stdout
    );
    assert_eq!(out.stderr, "");
}

// --- usage errors: exit 2 ---

#[test]
fn missing_file_argument_exits_with_2() {
    let out = jlv::<&str>(&[]);
    assert_eq!(out.exit_code(), Some(2));
    assert_eq!(out.stdout, "");
}

#[test]
fn missing_file_argument_names_file_and_shows_usage() {
    let out = jlv::<&str>(&[]);
    assert!(out.stderr.contains("<FILE>"), "{}", out.stderr);
    assert!(out.stderr.contains("Usage: jlv"), "{}", out.stderr);
}

#[test]
fn two_files_exit_with_2() {
    let dir = TempDir::new().unwrap();
    let a = log_file(&dir, "a.log");
    let b = log_file(&dir, "b.log");
    let out = jlv(&[a, b]);
    assert_eq!(out.exit_code(), Some(2));
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains("b.log"), "{}", out.stderr);
}

#[test]
fn unknown_flag_exits_with_2() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, "app.log");
    let out = jlv(&[file.as_os_str(), OsStr::new("--bogus")]);
    assert_eq!(out.exit_code(), Some(2));
    assert!(out.stderr.contains("--bogus"), "{}", out.stderr);
}

#[test]
fn unknown_format_exits_with_2() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, "app.log");
    let out = jlv(&[OsStr::new("-f"), OsStr::new("log4j"), file.as_os_str()]);
    assert_eq!(out.exit_code(), Some(2));
    assert_eq!(out.stdout, "");
}

#[test]
fn usage_errors_are_printed_as_the_parser_words_them() {
    let argv = ["jlv", "--format", "log4j", "app.log"];
    let expected = Args::try_parse_from(argv).unwrap_err().to_string();
    let out = jlv(&argv[1..]);
    assert_eq!(out.stderr, expected);
}

#[test]
fn dash_says_stdin_is_not_supported_and_exits_with_2() {
    let out = jlv(&["-"]);
    assert_eq!(out.exit_code(), Some(2));
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        "jlv: -: reading from stdin is not supported yet\n"
    );
}

// --- valid files: handed over for viewing ---

#[test]
fn regular_file_is_handed_over_silently() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, "app.log");
    let out = jlv(&[&file]);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "");
    let launch = out.launch();
    assert_eq!(launch.args, defaults(&file));
    assert_eq!(launch.source.bytes(), LOG_LINE.as_bytes());
}

#[test]
fn empty_regular_file_is_handed_over() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("empty.log");
    fs::write(&file, "").unwrap();
    let launch = jlv(&[&file]).launch();
    assert_eq!(launch.args, defaults(&file));
    assert_eq!(launch.source.bytes(), b"");
}

#[test]
fn every_option_is_handed_over() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, "app.log");
    let launch = jlv(&[
        OsStr::new("-f"),
        OsStr::new("logback-basic"),
        OsStr::new("--no-follow"),
        OsStr::new("--no-mouse"),
        OsStr::new("--config"),
        OsStr::new("/nonexistent/config.toml"),
        file.as_os_str(),
    ])
    .launch();
    assert_eq!(
        launch.args,
        Args {
            file,
            format: Some(Format::LogbackBasic),
            no_follow: true,
            no_mouse: true,
            config: Some(PathBuf::from("/nonexistent/config.toml")),
        }
    );
}

#[test]
fn symlink_to_a_regular_file_opens_the_target() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, "app.log");
    let link = dir.path().join("current.log");
    symlink(&file, &link).unwrap();
    let launch = jlv(&[&link]).launch();
    assert_eq!(launch.args, defaults(&link));
    assert_eq!(launch.source.bytes(), LOG_LINE.as_bytes());
}

#[test]
fn path_with_spaces_is_handed_over() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("my app")).unwrap();
    let file = log_file(&dir, "my app/server log.txt");
    let launch = jlv(&[&file]).launch();
    assert_eq!(launch.args, defaults(&file));
    assert_eq!(launch.source.bytes(), LOG_LINE.as_bytes());
}

#[test]
fn non_utf8_path_is_handed_over() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, OsString::from_vec(b"caf\xe9.log".to_vec()));
    let launch = jlv(&[&file]).launch();
    assert_eq!(launch.args, defaults(&file));
    assert_eq!(launch.source.bytes(), LOG_LINE.as_bytes());
}

// --- runtime errors: exit 1, `jlv: <path>: <reason>` ---

#[test]
fn missing_file_exits_with_1() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("gone.log");
    let out = jlv(&[&missing]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        runtime_error(&missing, "no such file or directory")
    );
}

#[test]
fn relative_path_is_reported_as_given() {
    let out = jlv(&["no/such/dir/app.log"]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(
        out.stderr,
        "jlv: no/such/dir/app.log: no such file or directory\n"
    );
}

#[test]
fn directory_exits_with_1() {
    let dir = TempDir::new().unwrap();
    let out = jlv(&[dir.path()]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(out.stderr, runtime_error(dir.path(), "is a directory"));
}

#[test]
fn symlink_to_a_directory_is_rejected() {
    let dir = TempDir::new().unwrap();
    let link = dir.path().join("logs");
    symlink(dir.path(), &link).unwrap();
    let out = jlv(&[&link]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(out.stderr, runtime_error(&link, "is a directory"));
}

#[test]
fn dangling_symlink_is_reported_as_missing() {
    let dir = TempDir::new().unwrap();
    let link = dir.path().join("current.log");
    symlink(dir.path().join("rotated-away.log"), &link).unwrap();
    let out = jlv(&[&link]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(
        out.stderr,
        runtime_error(&link, "no such file or directory")
    );
}

#[test]
fn character_device_is_rejected() {
    let out = jlv(&["/dev/null"]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(out.stderr, "jlv: /dev/null: not a regular file\n");
}

#[test]
fn symlink_to_a_device_is_rejected() {
    let dir = TempDir::new().unwrap();
    let link = dir.path().join("null.log");
    symlink("/dev/null", &link).unwrap();
    let out = jlv(&[&link]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(out.stderr, runtime_error(&link, "not a regular file"));
}

#[test]
fn unix_socket_is_rejected() {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("app.sock");
    let _listener = UnixListener::bind(&socket).unwrap();
    let out = jlv(&[&socket]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(out.stderr, runtime_error(&socket, "not a regular file"));
}

#[test]
fn missing_path_with_spaces_is_reported_verbatim() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("my app/server log.txt");
    let out = jlv(&[&missing]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(
        out.stderr,
        runtime_error(&missing, "no such file or directory")
    );
}

#[test]
fn missing_non_utf8_path_is_reported_lossily() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join(OsString::from_vec(b"caf\xe9.log".to_vec()));
    let out = jlv(&[&missing]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(
        out.stderr,
        format!(
            "jlv: {}/caf\u{FFFD}.log: no such file or directory\n",
            dir.path().display()
        )
    );
}

#[test]
fn file_error_wins_over_valid_options() {
    let out = jlv(&["--no-follow", "-f", "raw", "/nonexistent/app.log"]);
    assert_eq!(out.exit_code(), Some(1));
    assert_eq!(
        out.stderr,
        "jlv: /nonexistent/app.log: no such file or directory\n"
    );
}

// --- robustness ---

fn arg_token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("-f".to_string()),
        Just("--format".to_string()),
        Just("--no-follow".to_string()),
        Just("--no-mouse".to_string()),
        Just("--config".to_string()),
        Just("-V".to_string()),
        Just("--help".to_string()),
        Just("-".to_string()),
        Just("--".to_string()),
        Just("/dev/null".to_string()),
        Just("/nonexistent/app.log".to_string()),
        Just("Cargo.toml".to_string()),
        Just("src".to_string()),
        prop::sample::select(vec!["logback", "wildfly-console", "raw"]).prop_map(String::from),
        "[a-z -]{0,12}",
    ]
}

proptest! {
    #[test]
    fn every_command_line_launches_or_exits_with_an_explanation(
        args in prop::collection::vec(arg_token(), 0..6)
    ) {
        let out = jlv(args.as_slice());
        match out.result {
            Ok(_) => prop_assert_eq!(&out.stderr, "", "{:?}", args),
            Err(0) => {
                prop_assert_ne!(&out.stdout, "", "{:?}", args);
                prop_assert_eq!(&out.stderr, "", "{:?}", args);
            }
            Err(1 | 2) => prop_assert_ne!(&out.stderr, "", "{:?}", args),
            Err(code) => prop_assert!(false, "exit code {} for {:?}", code, args),
        }
    }
}

// --- event loop ---

mod event_loop {
    use std::collections::VecDeque;
    use std::io;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
    };

    use crate::app::App;
    use crate::event_loop;
    use crate::test_support::{app, char_key, numbered_log, row};
    use crate::ui::Theme;

    /// Hands out `events` one by one; asking for more is a test failure, since the
    /// loop should have stopped at `q`.
    fn script(events: Vec<Event>) -> impl FnMut() -> io::Result<Event> {
        let mut events = VecDeque::from(events);
        move || Ok(events.pop_front().expect("read past the last event"))
    }

    fn keys(chars: &str) -> Vec<Event> {
        chars.chars().map(|c| Event::Key(char_key(c))).collect()
    }

    fn terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(80, 24)).unwrap()
    }

    fn hundred_lines() -> App {
        app(numbered_log(100).as_bytes())
    }

    #[test]
    fn q_ends_the_loop() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        let result = event_loop(&mut terminal, &mut app, Theme::Styled, script(keys("q")));
        assert!(result.is_ok(), "{result:?}");
        assert!(app.should_quit());
    }

    #[test]
    fn first_screen_is_drawn_before_waiting_for_a_key() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        event_loop(&mut terminal, &mut app, Theme::Styled, script(keys("q"))).unwrap();
        assert_eq!(
            row(&terminal, 1),
            "▶12:00:00.001 [main] INFO  com.example.App - step 1"
        );
    }

    #[test]
    fn keys_are_applied_and_the_screen_redrawn() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        event_loop(&mut terminal, &mut app, Theme::Styled, script(keys("Gq"))).unwrap();
        assert_eq!(app.viewport().selected(), Some(99));
        assert_eq!(
            row(&terminal, 22),
            "▶12:00:00.100 [main] INFO  com.example.App - step 100"
        );
    }

    #[test]
    fn unbound_keys_and_other_events_are_ignored() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        let mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 3,
            row: 3,
            modifiers: KeyModifiers::NONE,
        };
        let events = vec![
            Event::Key(char_key('x')),
            Event::FocusGained,
            Event::Mouse(mouse),
            Event::Resize(100, 40),
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Event::Key(char_key('q')),
        ];
        event_loop(&mut terminal, &mut app, Theme::Styled, script(events)).unwrap();
        assert_eq!(app.viewport().selected(), Some(1));
    }

    #[test]
    fn theme_is_used_for_drawing() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        event_loop(&mut terminal, &mut app, Theme::Plain, script(keys("q"))).unwrap();
        let empty = ratatui::buffer::Cell::EMPTY.style();
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .all(|cell| cell.style() == empty)
        );
    }

    #[test]
    fn error_reading_events_ends_the_loop_with_that_error() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        let mut reads = 0;
        let failing = || {
            reads += 1;
            Err(io::Error::other("terminal went away"))
        };
        let err = event_loop(&mut terminal, &mut app, Theme::Styled, failing).unwrap_err();
        assert_eq!(err.to_string(), "terminal went away");
        assert_eq!(reads, 1);
    }
}
