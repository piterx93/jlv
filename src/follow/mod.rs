//! Following a file as it is written to, rotated, truncated or deleted (PRD §8).

mod liveness;
pub mod rotation;
mod watcher;

pub use liveness::Liveness;
pub use watcher::{Event, Options, Watcher};
