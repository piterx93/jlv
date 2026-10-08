use std::io;
use std::process::ExitCode;

// CI stamps dev builds with JLV_VERSION / JLV_COMMIT; local builds fall back to Cargo.toml.
const VERSION: &str = match option_env!("JLV_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

fn main() -> ExitCode {
    let version = jlv::version_string(VERSION, option_env!("JLV_COMMIT"));
    match jlv::start(
        std::env::args_os(),
        &version,
        &mut io::stdout(),
        &mut io::stderr(),
    ) {
        // the viewer arrives in #5
        Ok(_launch) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}
