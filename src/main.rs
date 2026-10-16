use std::io;
use std::process::ExitCode;

use jlv::ui::Theme;

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
        |name| std::env::var_os(name),
        &mut io::stdout(),
        &mut io::stderr(),
    ) {
        Ok(launch) => {
            let theme = Theme::from_no_color(std::env::var_os("NO_COLOR").as_deref());
            match jlv::run(launch, theme) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("jlv: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(code) => ExitCode::from(code),
    }
}
