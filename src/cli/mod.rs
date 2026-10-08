use std::path::PathBuf;

use clap::Parser;

use crate::format::Preset;

#[cfg(test)]
mod tests;

#[derive(Debug, PartialEq, Eq, Parser)]
#[command(
    name = "jlv",
    version,
    about = "Terminal log viewer for Java application logs"
)]
pub struct Args {
    /// Log file to open
    pub file: PathBuf,

    /// Force a format instead of detecting it
    #[arg(short, long, value_enum, value_name = "PRESET")]
    pub format: Option<Preset>,

    /// Open at the top, don't follow
    #[arg(long)]
    pub no_follow: bool,

    /// Disable mouse
    #[arg(long)]
    pub no_mouse: bool,

    /// Config file path
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}
