use std::env;
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::panic;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Parser;
use clap::error::ErrorKind;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use ratatui::crossterm::execute;

use crate::app::App;
use crate::cli::Args;
use crate::clipboard::{Env, System};
use crate::config::Config;
use crate::filter::{Base, Classes, Filters, TimeFilter};
use crate::follow::Options;
use crate::source::{OpenError, Source};
use crate::ui::Theme;

pub mod app;
pub mod cli;
pub mod clipboard;
pub mod config;
pub mod export;
pub mod filter;
pub mod fold;
pub mod follow;
pub mod format;
pub mod histogram;
pub mod index;
pub mod keymap;
pub mod matcher;
pub mod mouse;
pub mod source;
pub mod ui;
pub mod viewport;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

/// The text after `vilog` in `vilog --version`, e.g. `0.3.0-dev.42 (a1b2c3d)`.
pub fn version_string(version: &str, commit: Option<&str>) -> String {
    match commit {
        Some(commit) => format!("{version} ({commit})"),
        None => version.to_string(),
    }
}

/// What `start` hands over when vilog goes on to show a file.
#[derive(Debug)]
pub struct Launch {
    pub args: Args,
    pub source: Source,
    pub config: Config,
}

/// Parses the command line (program name first), reads the configuration file and
/// opens the log file. The configuration file is the one `--config` names, which
/// has to exist, or else the one at [`config::default_path`], read only if it
/// exists; without either vilog runs on the defaults. `var` gives the environment
/// variables. A gzip file is decompressed here, with the progress on stderr.
/// Whatever ends vilog before the viewer starts is printed here and returned as `Err(exit code)`:
/// 0 after `--help` or `--version`, 1 for runtime errors such as an unusable file,
/// 2 for usage errors. A configuration file that can't be read or is invalid is a
/// runtime error that names the file. `version` is what `--version` prints after
/// `vilog`.
pub fn start<I, T>(
    argv: I,
    version: &str,
    var: impl Fn(&str) -> Option<OsString>,
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
        let _ = writeln!(stderr, "vilog: -: reading from stdin is not supported yet");
        return Err(USAGE);
    }
    let config = load_config(args.config.as_deref(), var).map_err(|(path, err)| {
        let _ = writeln!(stderr, "vilog: {}: {}", path.display(), err.trim_end());
        FAILURE
    })?;
    match open_showing_progress(&args.file, stderr) {
        Ok(source) => Ok(Launch {
            args,
            source,
            config,
        }),
        Err(err) => {
            let _ = writeln!(stderr, "vilog: {}: {err}", args.file.display());
            Err(FAILURE)
        }
    }
}

/// The configuration `given` names, which has to exist, or else the one at the
/// default path if there is a file; the path comes back with the error.
fn load_config(
    given: Option<&Path>,
    var: impl Fn(&str) -> Option<OsString>,
) -> Result<Config, (PathBuf, String)> {
    let (path, required) = match given {
        Some(path) => (path.to_owned(), true),
        None => match config::default_path(var) {
            Some(path) => (path, false),
            None => return Ok(Config::default()),
        },
    };
    match fs::read_to_string(&path) {
        Ok(text) => Config::parse(&text).map_err(|err| (path, err)),
        Err(err) if err.kind() == io::ErrorKind::NotFound && !required => Ok(Config::default()),
        Err(err) => Err((path, err.to_string())),
    }
}

/// Opens `path` like [`source::open`]. While a gzip file is decompressed, a line
/// `vilog: <path>: decompressing N%` is drawn over itself on `stderr`, once for each
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
                "\rvilog: {}: decompressing {percent}%",
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
/// read in the preset `--format` names, detected without it, `--level` and
/// `--class` set the level and class filters it starts with, and `--since` the time
/// filter, counted from the last entry. Timestamps without an
/// offset are local time in the system's time zone, and a time-only log is dated
/// from the file's modification time. Events are read from the terminal; a wait
/// with a timeout ends with `None` when nothing came.
pub fn run(launch: Launch, theme: Theme) -> io::Result<()> {
    let Launch {
        args,
        source,
        config,
    } = launch;
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
    app.configure(&config);
    if let Some(filters) = initial_filters(&args, &config) {
        app.set_filters(filters);
    }
    if let Some(filter) = initial_time_filter(&args) {
        app.set_time_filter(Some(filter));
    }
    app.set_clipboard(Box::new(System::new(Env::from_vars(|name| {
        env::var_os(name)
    }))));
    app.watch(&args.file, file, watch_options(&config))
        .map_err(|err| in_file(&args.file, err))?;
    // try_init can fail after raw mode is on; restore ignores what isn't set up.
    let mut terminal = ratatui::try_init().inspect_err(|_| ratatui::restore())?;
    let captures_mouse = captures_mouse(&args, &config);
    if captures_mouse && let Err(err) = capture_mouse() {
        ratatui::restore();
        return Err(err);
    }
    let result = event_loop(&mut terminal, &mut app, theme, Instant::now, |timeout| {
        let ready = timeout.map_or(Ok(true), event::poll)?;
        ready.then(event::read).transpose()
    });
    if captures_mouse {
        // Restoring the terminal matters more than a failure to say so.
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

/// Turns mouse capture on, and off again if the program panics, as ratatui's own hook
/// leaves it on.
fn capture_mouse() -> io::Result<()> {
    execute!(io::stdout(), EnableMouseCapture)?;
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        previous(info);
    }));
    Ok(())
}

/// Whether the viewer turns on mouse capture: when the configuration's `mouse` says
/// so and `--no-mouse` wasn't given.
pub fn captures_mouse(args: &Args, config: &Config) -> bool {
    config.ui.mouse && !args.no_mouse
}

/// The filters the viewer starts with: the levels of `--level`, and the class rules
/// of `--class` or else the configuration's `default_class_excludes`, every other
/// filter off. `None` without `--level`, `--class` and class excludes, so that no
/// filter runs.
pub fn initial_filters(args: &Args, config: &Config) -> Option<Filters> {
    let classes = args
        .class
        .as_ref()
        .unwrap_or(&config.filters.default_class_excludes);
    if args.level.is_none() && *classes == Classes::default() {
        return None;
    }
    Some(Filters {
        levels: args.level.unwrap_or_default(),
        classes: classes.clone(),
        ..Filters::default()
    })
}

/// How the watcher follows the file: `notify` on, and a look every
/// `poll_interval_ms` of the configuration.
pub fn watch_options(config: &Config) -> Options {
    Options {
        notify: true,
        poll: config.follow.poll_interval,
    }
}

/// The time filter the viewer starts with: the spec of `--since`, counted from the
/// last entry in the file. `None` without it.
pub fn initial_time_filter(args: &Args) -> Option<TimeFilter> {
    let spec = args.since.clone()?;
    Some(TimeFilter {
        spec,
        base: Base::File,
    })
}

/// Draws the screen, waits for the next event from `next_event`, hands a key to the
/// app ([`App::key`]), and again, until the app should quit. Other events, and a wait
/// that timed out, only redraw. Errors from drawing or reading events end the loop.
///
/// Before every drawing the app is told the time `now` says ([`App::tick`]) and takes
/// in what its index found since the last one ([`App::poll_index`]), and what
/// happened to the file if it watches one. While indexing goes on, and while the app
/// [watches](App::watch) the file, `next_event` is asked to wait at most 100 ms, so
/// that the screen keeps up with them; `next_event(Some(timeout))` returns `Ok(None)`
/// when nothing came in time. It is asked to wait no longer than until the dot in the
/// header changes, so that the dot changes on screen without a key. Otherwise it is
/// asked to wait for as long as it takes (`None`).
pub fn event_loop<B>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    theme: Theme,
    mut now: impl FnMut() -> Instant,
    mut next_event: impl FnMut(Option<Duration>) -> io::Result<Option<Event>>,
) -> io::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
{
    while !app.should_quit() {
        app.tick(now());
        app.poll_index();
        terminal
            .draw(|frame| ui::render(frame, app, theme))
            .map_err(io::Error::other)?;
        let redraw = app.expects_updates().then_some(REDRAW);
        let timeout = [redraw, app.dot_changes_in()].into_iter().flatten().min();
        match next_event(timeout)? {
            Some(Event::Key(key)) => app.key(key),
            Some(Event::Mouse(mouse)) => app.mouse(mouse, now()),
            _ => {}
        }
    }
    Ok(())
}

/// How long the event loop waits for an event while indexing goes on or the file is
/// watched, before it takes in what changed and redraws.
const REDRAW: Duration = Duration::from_millis(100);

/// `err` saying which file it is about, for the one line vilog prints.
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
        let _ = writeln!(stdout, "vilog {version}");
        SUCCESS
    } else if err.use_stderr() {
        let _ = write!(stderr, "{err}");
        USAGE
    } else {
        let _ = write!(stdout, "{err}");
        SUCCESS
    }
}
