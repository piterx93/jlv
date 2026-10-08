use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::Path;

#[derive(Debug)]
pub enum OpenError {
    Io(io::Error),
    Directory,
    NotRegularFile,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => f.write_str(&describe(err)),
            Self::Directory => f.write_str("is a directory"),
            Self::NotRegularFile => f.write_str("not a regular file"),
        }
    }
}

impl From<io::Error> for OpenError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Opens a log file, following symlinks. The type is checked before opening because
/// opening a FIFO for reading blocks until something writes to it.
pub fn open(path: &Path) -> Result<File, OpenError> {
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() {
        return Err(OpenError::Directory);
    }
    if !metadata.is_file() {
        return Err(OpenError::NotRegularFile);
    }
    Ok(File::open(path)?)
}

/// Turns `No such file or directory (os error 2)` into `no such file or directory`,
/// the way `cat` and friends phrase it.
fn describe(err: &io::Error) -> String {
    let text = err.to_string();
    let text = text
        .split_once(" (os error ")
        .map_or(text.as_str(), |(reason, _)| reason);
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}
