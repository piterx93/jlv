#[cfg(test)]
mod tests;

/// The text after `jlv` in `jlv --version`, e.g. `0.3.0-dev.42 (a1b2c3d)`.
pub fn version_string(version: &str, commit: Option<&str>) -> String {
    match commit {
        Some(commit) => format!("{version} ({commit})"),
        None => version.to_string(),
    }
}
