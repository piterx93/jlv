//! Writes a synthetic Java application log, to try jlv on a big file:
//!
//! ```sh
//! cargo run --release --example generate_log -- /tmp/app.log 2G
//! cargo run --release --example generate_log -- /tmp/spring.log 500M --format spring3
//! ```
//!
//! The benchmarks write the same log; `docs/benchmarks.md` says how to run them.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use jlv::format::Preset;

#[path = "../benches/support/generator.rs"]
#[expect(dead_code, reason = "the probe line is for the benchmarks")]
mod generator;

use generator::{Spec, generate, parse_size};

#[derive(Parser)]
struct Args {
    /// Where to write the log; a file there is replaced.
    path: PathBuf,
    /// About how big it gets, e.g. 2G, 64M or 4096.
    #[arg(default_value = "1G", value_parser = parse_size)]
    size: u64,
    #[arg(long, value_enum, default_value = "logback")]
    format: Preset,
    /// Another seed gives another log of the same kind.
    #[arg(long, default_value_t = 1)]
    seed: u64,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let spec = Spec {
        preset: args.format,
        size: args.size,
        seed: args.seed,
    };
    let started = Instant::now();
    match generate(&args.path, spec) {
        Ok(entries) => {
            let seconds = started.elapsed().as_secs_f64();
            eprintln!(
                "{}: {entries} entries in {seconds:.1} s",
                args.path.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{}: {err}", args.path.display());
            ExitCode::FAILURE
        }
    }
}
