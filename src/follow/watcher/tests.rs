use std::fs;
use std::sync::mpsc;

use super::*;

/// Runs `f` on a thread of its own and returns what it gives, failing if that takes
/// longer than a few seconds, so that a loop that never ends fails the test instead of
/// hanging it.
fn within_seconds<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, result) = mpsc::channel();
    thread::spawn(move || {
        let _ = done.send(f());
    });
    result
        .recv_timeout(Duration::from_secs(5))
        .expect("took too long")
}

#[test]
fn opening_a_path_through_a_regular_file_fails_rather_than_finding_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("app.log");
    fs::write(&file, "a\n").unwrap();
    assert!(open(&file.join("app.log")).is_err());
}

#[test]
fn snapshot_holds_inode_size_and_first_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    let content = "x".repeat(PREFIX_LEN + 10);
    fs::write(&path, &content).unwrap();
    let file = File::open(&path).unwrap();
    let inode = file.metadata().unwrap().ino();
    let seen = within_seconds(move || snapshot(&file).unwrap());
    let expected = Snapshot {
        inode,
        size: content.len() as u64,
        prefix: content.as_bytes()[..PREFIX_LEN].to_vec(),
    };
    assert_eq!(seen, expected);
}

#[test]
fn snapshot_of_a_file_that_cannot_be_read_fails() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("app.log"), "a\n").unwrap();
    // A directory opens like a file and has a size, but reading it fails.
    let file = File::open(dir.path()).unwrap();
    assert!(within_seconds(move || snapshot(&file)).is_err());
}
