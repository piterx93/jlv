use std::env;
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use clap::Parser;
use clap::error::ErrorKind;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event};

use crate::app::{App, action};
use crate::cli::Args;
use crate::follow::Options;
use crate::source::{OpenError, Source};
use crate::ui::Theme;

pub mod app;
pub mod cli;
pub mod filter;
pub mod fold;
pub mod follow;
pub mod format;
pub mod index;
pub mod matcher;
pub mod source;
pub mod ui;
pub mod viewport;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

/// The text after `jlv` in `jlv --version`, e.g. `0.3.0-dev.42 (a1b2c3d)`.
pub fn version_string(version: &str, commit: Option<&str>) -> String {
    match commit {
        Some(commit) => format!("{version} ({commit})"),
        None => version.to_string(),
    }
}

/// What `start` hands over when jlv goes on to show a file.
#[derive(Debug)]
pub struct Launch {
    pub args: Args,
    pub source: Source,
}

/// Parses the command line (program name first) and opens the log file. A gzip file
/// is decompressed here, with the progress on stderr. Whatever ends
/// jlv before the viewer starts is printed here and returned as `Err(exit code)`:
/// 0 after `--help` or `--version`, 1 for runtime errors such as an unusable file,
/// 2 for usage errors. `version` is what `--version` prints after `jlv`.
pub fn start<I, T>(
    argv: I,
    version: &str,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<Launch, u8>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    // Write errors are ignored here and below: if stdout or stderr is gone there is
    // nobody left to tell.
    let args = Args::try_parse_from(argv)
        .map_err(|err| report_parse_exit(&err, version, stdout, stderr))?;
    if args.file == Path::new("-") {
        let _ = writeln!(stderr, "jlv: -: reading from stdin is not supported yet");
        return Err(USAGE);
    }
    match open_showing_progress(&args.file, stderr) {
        Ok(source) => Ok(Launch { args, source }),
        Err(err) => {
            let _ = writeln!(stderr, "jlv: {}: {err}", args.file.display());
            Err(FAILURE)
        }
    }
}

/// Opens `path` like [`source::open`]. While a gzip file is decompressed, a line
/// `jlv: <path>: decompressing N%` is drawn over itself on `stderr`, once for each
/// percentage, and cleared at the end so the terminal is as before.
fn open_showing_progress(path: &Path, stderr: &mut dyn Write) -> Result<Source, OpenError> {
    // Only a gzip file reports progress, but asking for the size costs next to nothing.
    let size = fs::metadata(path).map_or(0, |metadata| metadata.len());
    let mut shown = None;
    let opened = source::open_in(path, &env::temp_dir(), |read| {
        // The file can change between the two looks at its size.
        let percent = (read * 100).checked_div(size).map_or(100, |p| p.min(100));
        if shown < Some(percent) {
            shown = Some(percent);
            let _ = write!(
                stderr,
                "\rjlv: {}: decompressing {percent}%",
                path.display()
            );
        }
    });
    if shown.is_some() {
        let _ = write!(stderr, "\r\x1b[2K");
    }
    opened
}

/// Shows the file until the user quits. Switches the terminal to raw mode and the
/// alternate screen and restores it on return, error or panic.
///
/// It opens at the end of the file, or at the top with `--no-follow`, and watches a
/// plain file either way, so that `F` can follow it ([`App::watch`]); a gzip file is
/// read-only and never watched. The file is
/// read in the preset `--format` names, detected without it. Timestamps without an
/// offset are local time in the system's time zone, and a time-only log is dated
/// from the file's modification time. Events are read from the terminal; a wait
/// with a timeout ends with `None` when nothing came.
pub fn run(launch: Launch, theme: Theme) -> io::Result<()> {
    let Launch { args, source } = launch;
    // Without an mtime a time-only log is dated from now, the latest its entries can be.
    let mtime = source.modified().unwrap_or_else(Timestamp::now);
    let follow = !args.no_follow;
    // Opened again for the watcher, as the source doesn't keep its file.
    let file = File::open(&args.file).map_err(|err| in_file(&args.file, err))?;
    let mut app = App::new(
        &args.file,
        source,
        args.format,
        TimeZone::system(),
        mtime,
        follow,
    );
    app.watch(&args.file, file, Options::default())
        .map_err(|err| in_file(&args.file, err))?;
    // try_init can fail after raw mode is on; restore ignores what isn't set up.
    let mut terminal = ratatui::try_init().inspect_err(|_| ratatui::restore())?;
    let result = event_loop(&mut terminal, &mut app, theme, |timeout| {
        let ready = timeout.map_or(Ok(true), event::poll)?;
        ready.then(event::read).transpose()
    });
    ratatui::restore();
    result
}

/// Draws the screen, waits for the next event from `next_event`, applies the key's
/// action, and again, until the app should quit. Other events, and a wait that timed
/// out, only redraw. Errors from drawing or reading events end the loop.
///
/// Before every drawing the app takes in what its index found since the last one
/// ([`App::poll_index`]), and what happened to the file if it watches one. While
/// indexing goes on, and while the app [watches](App::watch) the file, `next_event`
/// is asked to wait at most 100 ms, so that the screen keeps up with them;
/// `next_event(Some(timeout))` returns `Ok(None)` when nothing came in time.
/// Otherwise it is asked to wait for as long as it takes (`None`).
pub fn event_loop<B>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    theme: Theme,
    mut next_event: impl FnMut(Option<Duration>) -> io::Result<Option<Event>>,
) -> io::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
{
    while !app.should_quit() {
        app.poll_index();
        terminal
            .draw(|frame| ui::render(frame, app, theme))
            .map_err(io::Error::other)?;
        let timeout = app.expects_updates().then_some(REDRAW);
        if let Some(Event::Key(key)) = next_event(timeout)?
            && let Some(action) = action(key)
        {
            app.apply(action);
        }
    }
    Ok(())
}

/// How long the event loop waits for an event while indexing goes on or the file is
/// watched, before it takes in what changed and redraws.
const REDRAW: Duration = Duration::from_millis(100);

/// `err` saying which file it is about, for the one line jlv prints.
fn in_file(path: &Path, err: io::Error) -> io::Error {
    io::Error::new(err.kind(), format!("{}: {err}", path.display()))
}

const SUCCESS: u8 = 0;
const FAILURE: u8 = 1;
const USAGE: u8 = 2;

/// clap stops parsing for help and version as well as for usage errors. Prints what
/// it has to say and returns the exit code.
fn report_parse_exit(
    err: &clap::Error,
    version: &str,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    // clap's own version text would show the Cargo version, not the injected one.
    if err.kind() == ErrorKind::DisplayVersion {
        let _ = writeln!(stdout, "jlv {version}");
        SUCCESS
    } else if err.use_stderr() {
        let _ = write!(stderr, "{err}");
        USAGE
    } else {
        let _ = write!(stdout, "{err}");
        SUCCESS
    }
}
