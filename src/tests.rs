use super::*;

#[test]
fn version_without_commit_is_printed_as_is() {
    assert_eq!(version_string("0.3.0", None), "0.3.0");
}

#[test]
fn commit_follows_the_version_in_parentheses() {
    assert_eq!(
        version_string("0.3.0-dev.42", Some("a1b2c3d")),
        "0.3.0-dev.42 (a1b2c3d)"
    );
}
