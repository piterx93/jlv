// CI stamps dev builds with JLV_VERSION / JLV_COMMIT; local builds fall back to Cargo.toml.
const VERSION: &str = match option_env!("JLV_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

fn version() -> String {
    match option_env!("JLV_COMMIT") {
        Some(commit) => format!("{VERSION} ({commit})"),
        None => VERSION.to_string(),
    }
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("-V" | "--version") => println!("jlv {}", version()),
        _ => {
            eprintln!("usage: jlv <FILE>");
            std::process::exit(2);
        }
    }
}
