//! The real binary: exit codes, stdout/stderr and the cases that need a separate process.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const NOBODY: u32 = 65534;

fn jlv() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jlv"))
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn expected_version() -> String {
    let version = option_env!("JLV_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    match option_env!("JLV_COMMIT") {
        Some(commit) => format!("jlv {version} ({commit})\n"),
        None => format!("jlv {version}\n"),
    }
}

fn log_file(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    fs::write(
        &path,
        "12:00:00.123 [main] INFO  com.example.App -- Started\n",
    )
    .unwrap();
    path
}

fn mkfifo(path: &Path) {
    let status = Command::new("mkfifo").arg(path).status().unwrap();
    assert!(status.success(), "mkfifo {}", path.display());
}

/// Opening a FIFO for reading blocks until a writer shows up, so a jlv that opens
/// before checking the file type would hang; fail instead.
fn output_within(cmd: &mut Command, limit: Duration) -> Output {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > limit {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("jlv still running after {limit:?}, did it open the FIFO?");
        }
        thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn long_version_prints_name_and_version() {
    let out = jlv().arg("--version").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout(&out), expected_version());
    assert_eq!(stderr(&out), "");
}

#[test]
fn short_v_prints_name_and_version() {
    let out = jlv().arg("-V").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout(&out), expected_version());
}

#[test]
fn version_with_a_file_prints_the_version_and_exits_0() {
    let dir = TempDir::new().unwrap();
    let file = log_file(&dir, "app.log");
    let out = jlv().arg(&file).arg("--version").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout(&out), expected_version());
    assert_eq!(stderr(&out), "");
}

#[test]
fn missing_file_argument_exits_2_with_usage_on_stderr() {
    let out = jlv().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("Usage: jlv"), "{}", stderr(&out));
}

#[test]
fn unknown_format_exits_2() {
    let out = jlv().args(["-f", "log4j", "app.log"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).contains("invalid value 'log4j'"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn dash_exits_2_saying_stdin_is_not_supported() {
    let out = jlv().arg("-").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        "jlv: -: reading from stdin is not supported yet\n"
    );
}

#[test]
fn missing_file_exits_1_with_path_and_reason() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("gone.log");
    let out = jlv().arg(&missing).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert_eq!(
        stderr(&out),
        format!("jlv: {}: no such file or directory\n", missing.display())
    );
}

#[test]
fn directory_exits_1() {
    let dir = TempDir::new().unwrap();
    let out = jlv().arg(dir.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr(&out),
        format!("jlv: {}: is a directory\n", dir.path().display())
    );
}

#[test]
fn fifo_is_rejected_without_blocking() {
    let dir = TempDir::new().unwrap();
    let fifo = dir.path().join("pipe.log");
    mkfifo(&fifo);
    let out = output_within(jlv().arg(&fifo), Duration::from_secs(5));
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr(&out),
        format!("jlv: {}: not a regular file\n", fifo.display())
    );
}

#[test]
fn symlink_to_a_fifo_is_rejected_without_blocking() {
    let dir = TempDir::new().unwrap();
    let fifo = dir.path().join("pipe");
    mkfifo(&fifo);
    let link = dir.path().join("app.log");
    std::os::unix::fs::symlink(&fifo, &link).unwrap();
    let out = output_within(jlv().arg(&link), Duration::from_secs(5));
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr(&out),
        format!("jlv: {}: not a regular file\n", link.display())
    );
}

/// Root reads files whatever their mode, so when the tests run as root (the dev
/// container) the binary is started as `nobody`; elsewhere it keeps the test's own
/// user. Either way the file is unreadable for the jlv process.
#[test]
fn unreadable_file_exits_1_with_permission_denied() {
    let dir = TempDir::new().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let file = log_file(&dir, "secret.log");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();

    let mut cmd = jlv();
    cmd.arg(&file);
    if fs::metadata(&file).unwrap().uid() == 0 {
        // Under `cargo cov` nobody can't write the profile into target/, and the
        // profiler would complain on stderr.
        let profiles = dir.path().join("profiles");
        fs::create_dir(&profiles).unwrap();
        fs::set_permissions(&profiles, fs::Permissions::from_mode(0o777)).unwrap();
        cmd.uid(NOBODY)
            .gid(NOBODY)
            .env("LLVM_PROFILE_FILE", profiles.join("jlv-%p.profraw"));
    }
    let out = cmd.output().unwrap();

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr(&out),
        format!("jlv: {}: permission denied\n", file.display())
    );
}
