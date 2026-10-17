//! Copying text to the clipboard, as `y` copies the selected entry: over SSH with an
//! OSC 52 escape sequence written to the terminal, locally with the native clipboard,
//! falling back to OSC 52 when that fails.

use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;
use std::io::{self, Write};

use crate::export::write_entries;

#[cfg(test)]
mod tests;

/// The most text, in bytes before encoding, that is sent in an OSC 52 sequence:
/// 100 KB. Terminals cap the sequences they take, and more would only be dropped.
pub const OSC52_LIMIT: usize = 100 * 1024;

/// What `y` copies of an entry: its bytes with the ANSI escape codes stripped from
/// every line as [`export::write_entries`](crate::export::write_entries) strips them,
/// the line endings between lines kept and the last line's dropped. Bytes that are
/// not UTF-8 become U+FFFD.
pub fn entry_text(entry: &[u8]) -> String {
    let mut text = Vec::new();
    // Only fails when writing to a Vec does, which it doesn't.
    let _ = write_entries(&mut text, entry, std::iter::once(0..entry.len()));
    if text.ends_with(b"\n") {
        text.pop();
    }
    if text.ends_with(b"\r") {
        text.pop();
    }
    String::from_utf8_lossy(&text).into_owned()
}

/// The OSC 52 sequence that sets the clipboard to `text`: `ESC ] 52 ; c ; <base64>
/// BEL`, the base64 being the standard alphabet with padding (RFC 4648).
pub fn osc52(text: &[u8]) -> Vec<u8> {
    let mut sequence = b"\x1b]52;c;".to_vec();
    base64_into(text, &mut sequence);
    sequence.push(0x07);
    sequence
}

/// `sequence` wrapped for tmux to pass it on to the terminal: `ESC P tmux;`, the
/// sequence with every ESC doubled, `ESC \`.
pub fn tmux_passthrough(sequence: &[u8]) -> Vec<u8> {
    let mut wrapped = b"\x1bPtmux;".to_vec();
    for &byte in sequence {
        wrapped.push(byte);
        if byte == 0x1b {
            wrapped.push(byte);
        }
    }
    wrapped.extend_from_slice(b"\x1b\\");
    wrapped
}

/// What the environment says about where jlv runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Env {
    /// Logged in over SSH: `SSH_CONNECTION` or `SSH_TTY` is set.
    pub ssh: bool,
    /// Inside tmux: `TMUX` is set.
    pub tmux: bool,
}

impl Env {
    /// The environment as `var` gives the variables, `None` for one that isn't set. A
    /// variable set to the empty string counts as not set.
    pub fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Self {
        let set = |name| var(name).is_some_and(|value| !value.is_empty());
        Self {
            ssh: set("SSH_CONNECTION") || set("SSH_TTY"),
            tmux: set("TMUX"),
        }
    }
}

/// How text got to the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Native,
    Osc52,
}

/// Why text didn't get to the clipboard.
#[derive(Debug)]
pub enum CopyError {
    /// OSC 52 was the way left and the text is longer than [`OSC52_LIMIT`].
    TooLarge,
    /// Writing the OSC 52 sequence to the terminal failed.
    Terminal(io::Error),
}

impl fmt::Display for CopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "entry too large for OSC 52 (over 100 KB)"),
            Self::Terminal(err) => write!(f, "{err}"),
        }
    }
}

/// Something that puts text on the clipboard.
pub trait Clipboard: Debug {
    fn copy(&mut self, text: &str) -> Result<Via, CopyError>;
}

/// Puts `text` on the clipboard. Over SSH it writes the [OSC 52 sequence](osc52) to
/// `terminal`, [wrapped](tmux_passthrough) inside tmux; otherwise it hands `text` to
/// `native`, and writes the sequence only if that fails. Text longer than
/// [`OSC52_LIMIT`] is never written; `native` takes text of any length.
pub fn copy<E>(
    text: &str,
    env: Env,
    native: impl FnOnce(&str) -> Result<(), E>,
    terminal: &mut impl Write,
) -> Result<Via, CopyError> {
    if !env.ssh && native(text).is_ok() {
        return Ok(Via::Native);
    }
    if text.len() > OSC52_LIMIT {
        return Err(CopyError::TooLarge);
    }
    let sequence = osc52(text.as_bytes());
    let sequence = if env.tmux {
        tmux_passthrough(&sequence)
    } else {
        sequence
    };
    terminal
        .write_all(&sequence)
        .and_then(|()| terminal.flush())
        .map_err(CopyError::Terminal)?;
    Ok(Via::Osc52)
}

/// The clipboard of the system jlv runs on: [`copy`] with `arboard` as the native
/// clipboard and stdout as the terminal.
pub struct System {
    env: Env,
    // Kept for the life of the process: on X11 and Wayland the process that set the
    // text serves it to whoever pastes.
    board: Option<arboard::Clipboard>,
}

impl System {
    pub fn new(env: Env) -> Self {
        Self { env, board: None }
    }
}

impl Debug for System {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("System")
            .field("env", &self.env)
            .finish_non_exhaustive()
    }
}

impl Clipboard for System {
    fn copy(&mut self, text: &str) -> Result<Via, CopyError> {
        let board = &mut self.board;
        let native = |text: &str| {
            match board {
                Some(board) => board,
                // Left empty when this fails, so that the next copy tries again.
                None => board.insert(arboard::Clipboard::new()?),
            }
            .set_text(text)
        };
        copy(text, self.env, native, &mut io::stdout().lock())
    }
}

fn base64_into(bytes: &[u8], out: &mut Vec<u8>) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in bytes.chunks(3) {
        let group = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, &b)| acc | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(group >> (18 - 6 * i) & 0x3f) as usize]);
            } else {
                out.push(b'=');
            }
        }
    }
}
