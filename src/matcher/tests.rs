use proptest::collection::vec;
use proptest::prelude::*;

use super::*;

const PLAIN: Options = Options {
    regex: false,
    ignore_case: false,
};
const REGEX: Options = Options {
    regex: true,
    ignore_case: false,
};

fn find(pattern: &str, options: Options, line: &[u8]) -> Vec<Range<usize>> {
    Matcher::new(pattern, options).unwrap().find(line)
}

/// The occurrences of `pattern` in `line`, left to right, each search going on after
/// the previous occurrence.
fn naive_find(pattern: &[u8], line: &[u8], ignore_case: bool) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    while at + pattern.len() <= line.len() {
        let window = &line[at..at + pattern.len()];
        let equal = if ignore_case {
            window.eq_ignore_ascii_case(pattern)
        } else {
            window == pattern
        };
        if equal {
            found.push(at..at + pattern.len());
            at += pattern.len();
        } else {
            at += 1;
        }
    }
    found
}

fn options() -> impl Strategy<Value = Options> {
    (any::<bool>(), any::<bool>()).prop_map(|(regex, ignore_case)| Options { regex, ignore_case })
}

/// Patterns rich in regex syntax, valid or not, and now and then any string at all.
fn pattern() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[ab.*+?|()^$\\\\\\[\\]{}0-9]{0,6}",
        1 => any::<String>(),
    ]
}

/// Lines with many `a` and `b` among arbitrary bytes, so that patterns find something.
fn line() -> impl Strategy<Value = Vec<u8>> {
    vec(prop_oneof![Just(b'a'), Just(b'b'), any::<u8>()], 0..24)
}

#[test]
fn regex_mode_finds_each_match() {
    assert_eq!(find(r"\d+", REGEX, b"id=12 n=345"), [3..5, 8..11]);
}

#[test]
fn empty_pattern_finds_nothing() {
    for options in [PLAIN, REGEX] {
        let matcher = Matcher::new("", options).unwrap();
        assert!(matcher.find(b"anything").is_empty());
        assert!(!matcher.is_match(b"anything"));
    }
}

#[test]
fn invalid_regex_is_a_one_line_error_saying_what_is_wrong() {
    let message = Matcher::new("(unclosed", REGEX).unwrap_err();
    assert!(message.contains("unclosed"), "{message:?}");
    assert!(!message.contains('\n'), "{message:?}");
}

#[test]
fn regex_matches_around_invalid_utf8() {
    assert_eq!(find(r"\w+", REGEX, b"\xffab\xfe cd"), [1..3, 5..7]);
}

#[test]
fn plain_ignore_case_folds_non_ascii_letters() {
    let options = Options {
        regex: false,
        ignore_case: true,
    };
    assert_eq!(find("ärger", options, "Ärger".as_bytes()), vec![0..6]);
}

#[test]
fn regex_ignore_case_folds_non_ascii_letters() {
    let options = Options {
        regex: true,
        ignore_case: true,
    };
    assert_eq!(
        find("Ä.ger", options, "großer ärger".as_bytes()),
        vec![8..14]
    );
}

#[test]
fn anchors_match_at_the_start_and_end_of_the_line() {
    assert_eq!(find("^ab|ab$", REGEX, b"ab ab ab"), [0..2, 6..8]);
}

#[test]
fn empty_regex_matches_are_not_matches() {
    let matcher = Matcher::new("a*", REGEX).unwrap();
    assert_eq!(matcher.find(b"baab"), vec![1..3]);
    assert!(!matcher.is_match(b"bbb"));
}

proptest! {
    /// Plain text is never read as a regex, and ignoring case folds ASCII letters.
    #[test]
    fn plain_mode_agrees_with_a_naive_search(
        pattern in "[aAbB.*(]{1,3}",
        line in vec(prop::sample::select(b"aAbB.*(x\xff".to_vec()), 0..24),
        ignore_case in any::<bool>(),
    ) {
        let options = Options { regex: false, ignore_case };
        prop_assert_eq!(
            find(&pattern, options, &line),
            naive_find(pattern.as_bytes(), &line, ignore_case)
        );
    }

    #[test]
    fn matches_are_ordered_non_empty_and_within_the_line(
        pattern in pattern(),
        options in options(),
        line in line(),
    ) {
        let Ok(matcher) = Matcher::new(&pattern, options) else {
            return Ok(());
        };
        let mut end = 0;
        for range in matcher.find(&line) {
            prop_assert!(end <= range.start && range.start < range.end, "{range:?}");
            end = range.end;
        }
        prop_assert!(end <= line.len());
    }

    #[test]
    fn is_match_agrees_with_find(pattern in pattern(), options in options(), line in line()) {
        if let Ok(matcher) = Matcher::new(&pattern, options) {
            prop_assert_eq!(matcher.is_match(&line), !matcher.find(&line).is_empty());
        }
    }

    #[test]
    fn invalid_patterns_are_errors_without_newlines(pattern in pattern(), options in options()) {
        if let Err(message) = Matcher::new(&pattern, options) {
            prop_assert!(options.regex);
            prop_assert!(!message.is_empty() && !message.contains('\n'), "{message:?}");
        }
    }
}

#[test]
fn a_matcher_keeps_its_pattern_and_options_as_given() {
    let options = Options {
        regex: true,
        ignore_case: true,
    };
    let matcher = Matcher::new("time.*out", options).unwrap();
    assert_eq!(
        (matcher.pattern(), matcher.options()),
        ("time.*out", options)
    );
}
