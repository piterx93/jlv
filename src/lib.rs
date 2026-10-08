use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;

use clap::Parser;
use clap::error::ErrorKind;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::Event;

use crate::app::{App, action};
use crate::cli::Args;
use crate::source::Source;
use crate::ui::Theme;

pub mod app;
pub mod cli;
pub mod format;
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

/// Parses the command line (program name first) and opens the log file. Whatever ends
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
    match source::open(&args.file) {
        Ok(source) => Ok(Launch { args, source }),
        Err(err) => {
            let _ = writeln!(stderr, "jlv: {}: {err}", args.file.display());
            Err(FAILURE)
        }
    }
}

/// Shows the file until the user quits. Switches the terminal to raw mode and the
/// alternate screen and restores it on return, error or panic.
pub fn run(launch: Launch, theme: Theme) -> io::Result<()> {
    let mut app = App::new(&launch.args.file, launch.source);
    // try_init can fail after raw mode is on; restore ignores what isn't set up.
    let mut terminal = ratatui::try_init().inspect_err(|_| ratatui::restore())?;
    let result = event_loop(
        &mut terminal,
        &mut app,
        theme,
        ratatui::crossterm::event::read,
    );
    ratatui::restore();
    result
}

/// Draws the screen, waits for the next event from `next_event`, applies the key's
/// action, and again, until the app should quit. Other events only redraw. Errors from
/// drawing or reading events end the loop.
pub fn event_loop<B>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    theme: Theme,
    mut next_event: impl FnMut() -> io::Result<Event>,
) -> io::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
{
    while !app.should_quit() {
        terminal
            .draw(|frame| ui::render(frame, app, theme))
            .map_err(io::Error::other)?;
        if let Event::Key(key) = next_event()?
            && let Some(action) = action(key)
        {
            app.apply(action);
        }
    }
    Ok(())
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
