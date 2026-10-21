use std::path::PathBuf;

use clap::Parser;

use crate::filter::{Classes, Levels, TimeSpec};
use crate::format::{Format, Level};

#[cfg(test)]
mod tests;

#[derive(Debug, PartialEq, Eq, Parser)]
#[command(
    name = "vilog",
    version,
    about = "Terminal log viewer for application logs"
)]
pub struct Args {
    /// Log file to open
    pub file: PathBuf,

    /// Force a format instead of detecting it
    #[arg(short, long, value_enum, value_name = "NAME")]
    pub format: Option<&'static Format>,

    /// Open at the top, don't follow
    #[arg(long)]
    pub no_follow: bool,

    /// Initial level filter, e.g. WARN,ERROR
    #[arg(short, long, value_name = "LEVELS", value_parser = parse_levels)]
    pub level: Option<Levels>,

    /// Initial class filter, e.g. 'com.myco.*,!org.hibernate'
    #[arg(short, long, value_name = "RULES", value_parser = parse_classes)]
    pub class: Option<Classes>,

    /// Initial time filter, e.g. 10m or 12:00-12:30
    #[arg(short, long, value_name = "SPEC", value_parser = TimeSpec::parse)]
    pub since: Option<TimeSpec>,

    /// Disable mouse
    #[arg(long)]
    pub no_mouse: bool,

    /// Config file path
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

/// The levels `-l/--level` names, comma-separated, the only ones shown at first.
/// Names are read as `Level::parse` reads them, aliases included; empty items are skipped and
/// a level named twice counts once. An unknown name, or no name at all, is an error
/// that lists the valid names.
pub fn parse_levels(arg: &str) -> Result<Levels, String> {
    let valid = || {
        let names: Vec<_> = Level::ALL.iter().map(|level| level.name()).collect();
        names.join(", ")
    };
    let levels = arg
        .split(',')
        .filter(|name| !name.is_empty())
        .map(|name| {
            Level::parse(name.as_bytes())
                .ok_or_else(|| format!("unknown level '{name}', expected some of {}", valid()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if levels.is_empty() {
        return Err(format!("no level given, expected some of {}", valid()));
    }
    Ok(Levels::only(&levels))
}

/// The class rules `-c/--class` names, comma-separated, as [`Classes::parse`] reads
/// them. A rule it can't read is an error that names it.
pub fn parse_classes(arg: &str) -> Result<Classes, String> {
    Classes::parse(arg.split(','))
}
