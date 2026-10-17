use std::io;
use std::process::ExitCode;

use vilog::ui::Theme;

// CI stamps dev builds with VILOG_VERSION / VILOG_COMMIT; local builds fall back to Cargo.toml.
const VERSION: &str = match option_env!("VILOG_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

fn main() -> ExitCode {
    let version = vilog::version_string(VERSION, option_env!("VILOG_COMMIT"));
    match vilog::start(
        std::env::args_os(),
        &version,
        |name| std::env::var_os(name),
        &mut io::stdout(),
        &mut io::stderr(),
    ) {
        Ok(launch) => {
            let theme = Theme::from_no_color(std::env::var_os("NO_COLOR").as_deref());
            match vilog::run(launch, theme) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("vilog: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(code) => ExitCode::from(code),
    }
}
