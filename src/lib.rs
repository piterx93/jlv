use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use clap::Parser;
use clap::error::ErrorKind;

use crate::cli::Args;

pub mod cli;
mod source;

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
    pub file: File,
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
        Ok(file) => Ok(Launch { args, file }),
        Err(err) => {
            let _ = writeln!(stderr, "jlv: {}: {err}", args.file.display());
            Err(FAILURE)
        }
    }
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
