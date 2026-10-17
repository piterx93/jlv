use std::cell::RefCell;
use std::ffi::OsString;
use std::io;

use super::*;

// --- entry text ---

#[test]
fn entry_text_strips_ansi_and_keeps_inner_line_endings() {
    let entry = b"\x1b[31mERROR\x1b[0m boom\r\n\x1b[2m\tat App.main\x1b[0m\n\tat App.run\r\n";
    assert_eq!(
        entry_text(entry),
        "ERROR boom\r\n\tat App.main\n\tat App.run"
    );
}

#[test]
fn entry_text_of_a_last_line_without_ending_is_the_line() {
    assert_eq!(entry_text(b"ERROR boom"), "ERROR boom");
}

#[test]
fn entry_text_replaces_invalid_utf8() {
    assert_eq!(entry_text(b"caf\xe9 \xff\n"), "caf\u{fffd} \u{fffd}");
}

// --- OSC 52 ---

#[test]
fn osc52_wraps_the_base64_text_in_the_sequence() {
    assert_eq!(osc52(b"hello"), b"\x1b]52;c;aGVsbG8=\x07");
}

#[test]
fn osc52_encodes_standard_base64_with_padding() {
    let cases: [(&[u8], &str); 8] = [
        (b"", ""),
        (b"f", "Zg=="),
        (b"fo", "Zm8="),
        (b"foo", "Zm9v"),
        (b"foob", "Zm9vYg=="),
        (b"fooba", "Zm9vYmE="),
        (b"foobar", "Zm9vYmFy"),
        (b"\xfb\xff", "+/8="),
    ];
    for (text, base64) in cases {
        let expected = format!("\x1b]52;c;{base64}\x07");
        assert_eq!(osc52(text), expected.as_bytes(), "{text:?}");
    }
}

#[test]
fn tmux_passthrough_doubles_every_escape() {
    assert_eq!(
        tmux_passthrough(b"\x1b]52;c;aGk=\x1b\\"),
        b"\x1bPtmux;\x1b\x1b]52;c;aGk=\x1b\x1b\\\x1b\\"
    );
}

// --- environment ---

/// The environment made of the variables `vars`.
fn env(vars: &[(&str, &str)]) -> Env {
    Env::from_vars(|name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(value))
    })
}

#[test]
fn no_variables_mean_local_outside_tmux() {
    assert_eq!(env(&[("HOME", "/home/me")]), Env::default());
}

#[test]
fn either_ssh_variable_means_ssh() {
    for name in ["SSH_CONNECTION", "SSH_TTY"] {
        let expected = Env {
            ssh: true,
            tmux: false,
        };
        assert_eq!(env(&[(name, "x")]), expected, "{name}");
    }
}

#[test]
fn tmux_variable_means_tmux() {
    let expected = Env {
        ssh: false,
        tmux: true,
    };
    assert_eq!(env(&[("TMUX", "/tmp/tmux-501/default,1,0")]), expected);
}

#[test]
fn empty_variables_count_as_not_set() {
    let vars = [("SSH_CONNECTION", ""), ("SSH_TTY", ""), ("TMUX", "")];
    assert_eq!(env(&vars), Env::default());
}

// --- copying ---

const LOCAL: Env = Env {
    ssh: false,
    tmux: false,
};

const SSH: Env = Env {
    ssh: true,
    tmux: false,
};

/// What came of copying `text` in `env` with a native clipboard that answers
/// `native`: the result, what the native clipboard was handed, and what was written
/// to the terminal.
fn copied(text: &str, env: Env, native: Result<(), ()>) -> (Via, Option<String>, Vec<u8>) {
    let handed = RefCell::new(None);
    let mut terminal = Vec::new();
    let via = copy(
        text,
        env,
        |text| {
            *handed.borrow_mut() = Some(text.to_owned());
            native
        },
        &mut terminal,
    )
    .unwrap();
    (via, handed.into_inner(), terminal)
}

#[test]
fn locally_copies_with_the_native_clipboard() {
    let (via, handed, written) = copied("boom", LOCAL, Ok(()));
    assert_eq!(via, Via::Native);
    assert_eq!(handed.as_deref(), Some("boom"));
    assert_eq!(written, b"");
}

#[test]
fn locally_the_native_clipboard_takes_text_over_the_osc52_limit() {
    let text = "x".repeat(OSC52_LIMIT + 1);
    let (via, handed, _) = copied(&text, LOCAL, Ok(()));
    assert_eq!(via, Via::Native);
    assert_eq!(handed, Some(text));
}

#[test]
fn over_ssh_writes_osc52_without_the_native_clipboard() {
    let (via, handed, written) = copied("boom", SSH, Ok(()));
    assert_eq!(via, Via::Osc52);
    assert_eq!(handed, None);
    assert_eq!(written, osc52(b"boom"));
}

#[test]
fn inside_tmux_wraps_osc52_in_passthrough() {
    let env = Env {
        ssh: true,
        tmux: true,
    };
    let (_, _, written) = copied("boom", env, Ok(()));
    assert_eq!(written, tmux_passthrough(&osc52(b"boom")));
}

#[test]
fn falls_back_to_osc52_when_the_native_clipboard_fails() {
    let (via, handed, written) = copied("boom", LOCAL, Err(()));
    assert_eq!(via, Via::Osc52);
    assert_eq!(handed.as_deref(), Some("boom"));
    assert_eq!(written, osc52(b"boom"));
}

#[test]
fn osc52_takes_text_of_exactly_100_kb() {
    let text = "x".repeat(102_400);
    let (via, _, written) = copied(&text, SSH, Ok(()));
    assert_eq!(via, Via::Osc52);
    assert_eq!(written, osc52(text.as_bytes()));
}

#[test]
fn osc52_refuses_text_over_100_kb_and_writes_nothing() {
    let text = "x".repeat(102_401);
    let mut terminal = Vec::new();
    let result = copy(&text, SSH, |_| Ok::<(), ()>(()), &mut terminal);
    assert!(matches!(result, Err(CopyError::TooLarge)), "{result:?}");
    assert_eq!(terminal, b"");
}

/// A terminal that is gone.
struct Closed;

impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
}

#[test]
fn returns_the_error_of_the_terminal() {
    let result = copy("boom", SSH, |_| Ok::<(), ()>(()), &mut Closed);
    let Err(CopyError::Terminal(err)) = result else {
        panic!("expected a terminal error, got {result:?}");
    };
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
}
