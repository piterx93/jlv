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
use crate::cli::Args;
use crate::filter::{Base, Classes, Filters, Levels, TimeFilter, TimeSpec};
use crate::format::{Level, Preset};
use crate::test_support;

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
        level: None,
        class: None,
        since: None,
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
fn unknown_level_exits_with_2() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, "app.log");
    let out = jlv(&[OsStr::new("-l"), OsStr::new("VERBOSE"), file.as_os_str()]);
    assert_eq!(out.exit_code(), Some(2));
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr
            .contains("TRACE, DEBUG, INFO, WARN, ERROR, FATAL"),
        "{}",
        out.stderr
    );
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
fn mouse_is_captured_by_default() {
    assert!(captures_mouse(&defaults("app.log")));
}

#[test]
fn no_mouse_leaves_mouse_capture_off() {
    let args = Args {
        no_mouse: true,
        ..defaults("app.log")
    };
    assert!(!captures_mouse(&args));
}

/// `initial_filters` for `-l` and `-c` as given, the rest of the command line at its
/// defaults.
fn initial(level: Option<&[Level]>, class: Option<&[&str]>) -> Option<Filters> {
    let args = Args {
        level: level.map(Levels::only),
        class: class.map(|rules| Classes::parse(rules.iter().copied()).unwrap()),
        ..defaults("app.log")
    };
    initial_filters(&args)
}

/// The levels and class rules of `filters`, failing the test if any other filter is on.
fn levels_and_classes(filters: Filters) -> (Levels, Classes) {
    assert!(filters.time.is_none(), "{filters:?}");
    assert!(filters.text.is_none(), "{filters:?}");
    (filters.levels, filters.classes)
}

#[test]
fn without_level_or_class_there_are_no_initial_filters() {
    assert!(initial(None, None).is_none());
}

#[test]
fn level_alone_sets_the_initial_levels_and_no_class_rules() {
    let filters = initial(Some(&[Level::Warn]), None).unwrap();
    assert_eq!(
        levels_and_classes(filters),
        (Levels::only(&[Level::Warn]), Classes::default())
    );
}

#[test]
fn class_alone_sets_the_initial_class_rules_and_every_level() {
    let filters = initial(None, Some(&["com.myco.*"])).unwrap();
    assert_eq!(
        levels_and_classes(filters),
        (Levels::default(), Classes::parse(["com.myco.*"]).unwrap())
    );
}

#[test]
fn level_and_class_together_set_both() {
    let filters = initial(Some(&[Level::Error]), Some(&["!org.hibernate"])).unwrap();
    assert_eq!(
        levels_and_classes(filters),
        (
            Levels::only(&[Level::Error]),
            Classes::parse(["!org.hibernate"]).unwrap()
        )
    );
}

#[test]
fn since_sets_the_initial_time_filter_counted_from_the_file() {
    let spec = TimeSpec::parse("10m").unwrap();
    let args = Args {
        since: Some(spec.clone()),
        ..defaults("app.log")
    };
    assert_eq!(
        initial_time_filter(&args),
        Some(TimeFilter {
            spec,
            base: Base::File
        })
    );
}

#[test]
fn without_since_there_is_no_initial_time_filter() {
    assert_eq!(initial_time_filter(&defaults("app.log")), None);
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
        OsStr::new("-l"),
        OsStr::new("warn,error"),
        OsStr::new("-c"),
        OsStr::new("com.myco.*,!org.hibernate"),
        OsStr::new("-s"),
        OsStr::new("10m"),
        OsStr::new("--config"),
        OsStr::new("/nonexistent/config.toml"),
        file.as_os_str(),
    ])
    .launch();
    assert_eq!(
        launch.args,
        Args {
            file,
            format: Some(Preset::LogbackBasic),
            no_follow: true,
            level: Some(Levels::only(&[Level::Warn, Level::Error])),
            class: Some(Classes::parse(["com.myco.*", "!org.hibernate"]).unwrap()),
            since: Some(TimeSpec::parse("10m").unwrap()),
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
#[cfg_attr(
    target_os = "macos",
    ignore = "APFS refuses file names that aren't UTF-8"
)]
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

// --- gzip files: decompressed with the progress on stderr ---

/// A gzip file `name` in `dir` holding `content`.
fn gzip_file(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, test_support::gzip(content)).unwrap();
    path
}

/// The progress line is drawn over itself and cleared once the file is decompressed,
/// so the terminal is as before when the viewer starts.
#[test]
fn gzip_file_shows_its_decompression_progress_on_stderr() {
    let dir = TempDir::new().unwrap();
    let file = gzip_file(&dir, "app.log.gz", LOG_LINE.as_bytes());
    let out = jlv(&[&file]);
    assert_eq!(
        out.stderr,
        format!("\rjlv: {}: decompressing 100%\r\x1b[2K", file.display())
    );
}

/// Each percentage is shown once, as the compressed bytes are read.
#[test]
fn decompression_progress_rises_to_100_percent() {
    let dir = TempDir::new().unwrap();
    let content = LOG_LINE.repeat(20_000);
    let file = gzip_file(&dir, "big.log.gz", content.as_bytes());
    let out = jlv(&[&file]);
    let shown = out
        .stderr
        .strip_suffix("\r\x1b[2K")
        .unwrap_or_else(|| panic!("not cleared: {:?}", out.stderr));
    let prefix = format!("jlv: {}: decompressing ", file.display());
    let mut lines = shown.split('\r');
    assert_eq!(lines.next(), Some(""), "{shown:?}");
    let percents: Vec<u64> = lines
        .map(|line| {
            let percent = line.strip_prefix(&prefix).and_then(|p| p.strip_suffix('%'));
            percent
                .unwrap_or_else(|| panic!("{line:?}"))
                .parse()
                .unwrap()
        })
        .collect();
    assert!(percents.len() > 1, "{percents:?}");
    assert!(percents.is_sorted_by(|a, b| a < b), "{percents:?}");
    assert_eq!(percents.last(), Some(&100));
}

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
    use std::thread;
    use std::time::{Duration, Instant};

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };

    use crate::app::{App, Panel};
    use crate::event_loop;
    use crate::format::Preset;
    use crate::test_support::{
        LARGE_INDEX_DEADLINE, app, char_key, following_app, hide_line_numbers, numbered_log, row,
        unindexed_app,
    };
    use crate::ui::Theme;

    /// Hands out `events` one by one, whatever the timeout; asking for more is a test
    /// failure, since the loop should have stopped at `q`.
    fn script(events: Vec<Event>) -> impl FnMut(Option<Duration>) -> io::Result<Option<Event>> {
        let mut events = VecDeque::from(events);
        move |_| Ok(Some(events.pop_front().expect("read past the last event")))
    }

    fn keys(chars: &str) -> Vec<Event> {
        chars.chars().map(|c| Event::Key(char_key(c))).collect()
    }

    fn terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(80, 24)).unwrap()
    }

    /// 100 lines without line numbers.
    fn hundred_lines() -> App {
        let mut app = app(numbered_log(100).as_bytes());
        hide_line_numbers(&mut app);
        app
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
    fn first_screen_of_a_followed_file_shows_its_end() {
        let mut terminal = terminal();
        // Several chunks of the index, so it is far from complete at the first screen.
        let mut app = following_app(Preset::Raw, numbered_log(10_000).as_bytes());
        hide_line_numbers(&mut app);
        event_loop(&mut terminal, &mut app, Theme::Styled, script(keys("q"))).unwrap();
        assert_eq!(
            row(&terminal, 1),
            " 12:00:00.9979 [main] INFO  com.example.App - step 9979"
        );
        assert_eq!(
            row(&terminal, 22),
            "▶12:00:00.10000 [main] INFO  com.example.App - step 10000"
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
    fn typed_characters_go_into_an_open_class_panel() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        let events = vec![
            Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
            Event::Key(char_key('q')),
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        ];
        event_loop(&mut terminal, &mut app, Theme::Styled, script(events)).unwrap();
        match app.panel() {
            Some(Panel::Classes(panel)) => assert_eq!(panel.lines, ["q"]),
            other => panic!("the class panel is not open: {other:?}"),
        }
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
    fn mouse_clicks_go_to_the_app_by_the_screen_drawn() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 4,
            modifiers: KeyModifiers::NONE,
        };
        let events = vec![Event::Mouse(click), Event::Key(char_key('q'))];
        event_loop(&mut terminal, &mut app, Theme::Styled, script(events)).unwrap();
        assert_eq!(app.viewport().selected(), Some(3));
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
        let failing = |_| {
            reads += 1;
            Err(io::Error::other("terminal went away"))
        };
        let err = event_loop(&mut terminal, &mut app, Theme::Styled, failing).unwrap_err();
        assert_eq!(err.to_string(), "terminal went away");
        assert_eq!(reads, 1);
    }

    #[test]
    fn waits_without_a_timeout_once_the_index_is_complete() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        let mut waits = Vec::new();
        let mut events = script(keys("jq"));
        event_loop(&mut terminal, &mut app, Theme::Styled, |timeout| {
            waits.push(timeout);
            events(timeout)
        })
        .unwrap();
        assert_eq!(waits, [None, None]);
    }

    #[test]
    fn timed_out_wait_redraws_and_waits_again() {
        let mut terminal = terminal();
        let mut app = hundred_lines();
        let mut events = VecDeque::from([None, Some(Event::Key(char_key('q')))]);
        let mut reads = 0;
        event_loop(&mut terminal, &mut app, Theme::Styled, |_| {
            reads += 1;
            Ok(events.pop_front().expect("read past the last event"))
        })
        .unwrap();
        assert_eq!(reads, 2);
        assert!(app.should_quit());
    }

    /// A file indexed in several chunks: 2.2 MB of time-only lines.
    fn slow_to_index() -> App {
        unindexed_app(Preset::LogbackBasic, numbered_log(40_000).as_bytes())
    }

    /// Waits as a terminal would, up to the timeout, and says nothing came. Asked to
    /// wait without a timeout, it answers `q`; so does it once indexing should long
    /// be over, to end a loop that never gets there.
    fn quit_once_waiting_without_timeout(
        waits: &mut Vec<Option<Duration>>,
    ) -> impl FnMut(Option<Duration>) -> io::Result<Option<Event>> + '_ {
        let started = Instant::now();
        move |timeout| {
            waits.push(timeout);
            match timeout {
                Some(timeout) if started.elapsed() < LARGE_INDEX_DEADLINE => {
                    thread::sleep(timeout.min(Duration::from_millis(10)));
                    Ok(None)
                }
                _ => Ok(Some(Event::Key(char_key('q')))),
            }
        }
    }

    #[test]
    fn while_indexing_waits_at_most_100_ms_at_a_time() {
        let mut terminal = terminal();
        let mut app = slow_to_index();
        let mut waits = Vec::new();
        event_loop(
            &mut terminal,
            &mut app,
            Theme::Styled,
            quit_once_waiting_without_timeout(&mut waits),
        )
        .unwrap();
        let (last, before) = waits.split_last().unwrap();
        assert_eq!(*last, None);
        assert!(!before.is_empty());
        assert!(
            before.iter().all(|wait| wait
                .is_some_and(|wait| wait > Duration::ZERO && wait <= Duration::from_millis(100))),
            "{waits:?}"
        );
    }

    #[test]
    fn entries_are_taken_in_while_waiting_and_drawn() {
        let mut terminal = terminal();
        let mut app = slow_to_index();
        hide_line_numbers(&mut app);
        let mut waits = Vec::new();
        event_loop(
            &mut terminal,
            &mut app,
            Theme::Styled,
            quit_once_waiting_without_timeout(&mut waits),
        )
        .unwrap();
        assert!(app.progress().is_complete());
        assert_eq!(app.viewport().total(), 40_000);
        assert_eq!(
            row(&terminal, 0),
            "jlv ─ app.log ─ 2.2 MB ─ Logback basic                                   I:40000"
        );
        assert_eq!(
            row(&terminal, 1),
            "▶12:00:00.001 INFO  com.example.App          step 1"
        );
    }
}
