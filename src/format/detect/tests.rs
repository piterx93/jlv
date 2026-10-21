use std::iter;

use proptest::prelude::*;

use super::*;
use crate::format::BUILT_IN;
use crate::test_support::{EXAMPLES, built_in};

const LOGBACK: &str = EXAMPLES[0].1;
const WILDFLY: &str = EXAMPLES[4].1;

/// Lines a JVM prints before the application logs anything. None of them is blank, a
/// continuation or an entry in any preset.
const JVM_NOISE: [&str; 5] = [
    "Picked up JAVA_TOOL_OPTIONS: -Xmx512m -Dfile.encoding=UTF-8",
    "OpenJDK 64-Bit Server VM warning: Options -Xverify:none and -noverify were deprecated in JDK 13",
    "WARNING: An illegal reflective access operation has occurred",
    "Listening for transport dt_socket at address: 5005",
    "[0.004s][info][gc] Using G1",
];

/// A stack trace: every line a continuation.
const STACK_TRACE: [&str; 5] = [
    "java.lang.IllegalStateException: Order 42 has no items",
    "\tat com.example.OrderService.place(OrderService.java:88)",
    "\tat com.example.OrderController.post(OrderController.java:31)",
    "Caused by: java.io.IOException: Connection reset",
    "\t... 12 more",
];

fn detect_lines<S: AsRef<str>>(lines: &[S]) -> Option<&'static Format> {
    detect(lines.iter().map(|line| line.as_ref().as_bytes()))
}

/// `n` lines cycling through `lines`.
fn repeat(lines: &[&str], n: usize) -> Vec<String> {
    lines
        .iter()
        .cycle()
        .take(n)
        .map(|line| line.to_string())
        .collect()
}

/// `n` lines of JVM noise.
fn noise(n: usize) -> Vec<String> {
    repeat(&JVM_NOISE, n)
}

/// `n` Logback entry lines.
fn logback(n: usize) -> Vec<String> {
    repeat(&[LOGBACK], n)
}

/// `n` Logback lines followed by `m` lines of noise.
fn logback_then_noise(n: usize, m: usize) -> Vec<String> {
    [logback(n), noise(m)].concat()
}

/// `lines`, then a line whose reading fails the test.
fn then_fail<'a>(lines: &'a [String]) -> impl Iterator<Item = &'a [u8]> {
    lines
        .iter()
        .map(|line| line.as_bytes())
        .chain(iter::from_fn(|| -> Option<&'a [u8]> {
            panic!("read one line too many")
        }))
}

// --- what is recognised ---

#[test]
fn recognizes_each_preset_from_its_example_line() {
    for (id, line) in EXAMPLES {
        assert_eq!(detect_lines(&[line]), Some(built_in(id)), "{line}");
    }
}

#[test]
fn preset_matching_most_lines_wins() {
    let lines = [repeat(&[LOGBACK], 35), repeat(&[WILDFLY], 65)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("wildfly")));
}

#[test]
fn mixed_presets_without_a_majority_are_not_recognized() {
    let lines = [repeat(&[LOGBACK], 50), repeat(&[WILDFLY], 50)].concat();
    assert_eq!(detect_lines(&lines), None);
}

// --- the threshold ---

#[test]
fn exactly_sixty_percent_is_recognized() {
    assert_eq!(
        detect_lines(&logback_then_noise(3, 2)),
        Some(built_in("logback"))
    );
    assert_eq!(
        detect_lines(&logback_then_noise(60, 40)),
        Some(built_in("logback"))
    );
}

#[test]
fn under_sixty_percent_is_not_recognized() {
    // 119 of 199 is 59.8%, so 59% is not enough either.
    assert_eq!(detect_lines(&logback_then_noise(119, 80)), None);
}

// --- nothing to score ---

#[test]
fn empty_file_is_not_recognized() {
    assert_eq!(detect_lines::<&str>(&[]), None);
}

#[test]
fn file_of_only_blank_lines_is_not_recognized() {
    assert_eq!(detect_lines(&["", "", "   ", "\t", ""]), None);
}

#[test]
fn file_of_only_stack_trace_lines_is_not_recognized() {
    assert_eq!(detect_lines(&repeat(&STACK_TRACE, 40)), None);
}

// --- continuation lines ---

#[test]
fn continuation_lines_are_left_out_of_the_score() {
    // 2 of 102 lines are entries, but 100 are a stack trace.
    let lines = [logback(2), repeat(&STACK_TRACE, 100)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

#[test]
fn lines_starting_with_a_space_or_a_tab_are_continuations() {
    let wrapped = ["   wrapped message text", "\twrapped message text"];
    let lines = [logback(2), repeat(&wrapped, 20)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

#[test]
fn exception_lines_are_continuations_without_leading_whitespace() {
    let exceptions = [
        "java.lang.IllegalStateException: Order 42 has no items",
        "Caused by: java.io.IOException: Connection reset",
        "com.example.PaymentError",
    ];
    let lines = [logback(2), repeat(&exceptions, 20)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

#[test]
fn continuation_check_looks_at_the_line_without_colour_codes() {
    let frame = "\x1b[31m\tat com.example.OrderService.place(OrderService.java:88)\x1b[0m";
    let lines = [logback(2), repeat(&[frame], 20)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

// --- the sample ---

#[test]
fn sample_is_the_first_200_lines() {
    // 120 of the first 200 lines are entries: 60%. With one line more or less in the
    // sample it would be under.
    let lines = [noise(80), logback(120), noise(1000)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

#[test]
fn lines_after_200_lines_of_jvm_startup_noise_do_not_count() {
    let lines = [noise(200), logback(500)].concat();
    assert_eq!(detect_lines(&lines), None);
}

#[test]
fn continuation_lines_count_toward_the_sample() {
    let lines = [repeat(&STACK_TRACE, 200), logback(50)].concat();
    assert_eq!(detect_lines(&lines), None);
}

#[test]
fn empty_lines_do_not_count_toward_the_sample() {
    let lines = [repeat(&[""], 300), logback(5)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

#[test]
fn lines_of_only_spaces_and_tabs_do_not_count_toward_the_sample() {
    let lines = [repeat(&[" ", "\t", "  \t  "], 300), logback(5)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

#[test]
fn lines_of_only_colour_codes_do_not_count_toward_the_sample() {
    let lines = [repeat(&["\x1b[0m", "\x1b[32m  \x1b[0m"], 300), logback(5)].concat();
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

#[test]
fn reads_no_line_past_the_sample() {
    let lines = logback(200);
    assert_eq!(detect(then_fail(&lines)), Some(built_in("logback")));
}

#[test]
fn reads_past_blank_lines_up_to_the_200th_other_line() {
    let lines = [repeat(&[""], 50), noise(80), logback(120)].concat();
    assert_eq!(detect(then_fail(&lines)), Some(built_in("logback")));
}

#[test]
fn reads_at_most_2000_lines() {
    let lines = repeat(&[""], 2000);
    assert_eq!(detect(then_fail(&lines)), None);
}

#[test]
fn line_2000_is_still_read() {
    let lines = [repeat(&[""], 1999), logback(1)].concat();
    assert_eq!(detect(then_fail(&lines)), Some(built_in("logback")));
}

// --- ANSI colours ---

#[test]
fn colour_codes_are_stripped_before_matching() {
    let coloured = "\x1b[2m2026-10-07 12:00:00.123\x1b[0m [main] \x1b[32mINFO \x1b[0m \
                    com.example.App - Started";
    let lines = repeat(&[coloured], 10);
    assert_eq!(detect_lines(&lines), Some(built_in("logback")));
}

// --- properties ---

/// What a line is, by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// An entry line of the format with this id.
    Entry(&'static str),
    Noise,
    Continuation,
    Blank,
}

fn entry_line() -> impl Strategy<Value = (Kind, String)> {
    (prop::sample::select(EXAMPLES.to_vec()), any::<bool>()).prop_map(
        |((preset, line), coloured)| {
            let line = if coloured {
                format!("\x1b[32m{line}\x1b[0m")
            } else {
                line.to_string()
            };
            (Kind::Entry(preset), line)
        },
    )
}

fn entry_line_of(preset: &'static str) -> (Kind, String) {
    let (_, line) = EXAMPLES
        .into_iter()
        .find(|&(own, _)| own == preset)
        .expect("every preset but Raw has an example");
    (Kind::Entry(preset), line.to_string())
}

fn noise_line() -> impl Strategy<Value = (Kind, String)> {
    prop::sample::select(JVM_NOISE.to_vec()).prop_map(|line| (Kind::Noise, line.to_string()))
}

fn continuation_line() -> impl Strategy<Value = (Kind, String)> {
    let lines = [&STACK_TRACE[..], &["  wrapped text", "\tindented"]].concat();
    prop::sample::select(lines).prop_map(|line| (Kind::Continuation, line.to_string()))
}

fn blank_line() -> impl Strategy<Value = (Kind, String)> {
    prop::sample::select(vec!["", " ", "\t", " \t ", "\x1b[0m"])
        .prop_map(|line| (Kind::Blank, line.to_string()))
}

/// Any kind of line, mostly ones of `dominant` so that its score lands near 60%.
fn line_around(dominant: &'static str) -> impl Strategy<Value = (Kind, String)> {
    prop_oneof![
        6 => Just(entry_line_of(dominant)),
        1 => entry_line(),
        2 => noise_line(),
        2 => continuation_line(),
        1 => blank_line(),
    ]
}

fn structured_preset() -> impl Strategy<Value = &'static str> {
    prop::sample::select(EXAMPLES.map(|(preset, _)| preset).to_vec())
}

/// Up to `max` lines of every kind, with one preset more common than the others.
fn log(max: usize) -> impl Strategy<Value = Vec<(Kind, String)>> {
    structured_preset()
        .prop_flat_map(move |dominant| prop::collection::vec(line_around(dominant), 0..max))
}

fn non_blank_line() -> impl Strategy<Value = (Kind, String)> {
    prop_oneof![entry_line(), noise_line(), continuation_line()]
}

fn texts(lines: &[(Kind, String)]) -> Vec<&str> {
    lines.iter().map(|(_, line)| line.as_str()).collect()
}

/// The id of the format detection should find, worked out by hand from what each line
/// is, for logs whose sample is the whole log. Formats are tried in table order.
fn expected(lines: &[(Kind, String)]) -> Option<&'static str> {
    let scored = lines
        .iter()
        .filter(|(kind, _)| matches!(kind, Kind::Entry(_) | Kind::Noise))
        .count();
    let best = BUILT_IN
        .iter()
        .map(|format| {
            let entries = lines
                .iter()
                .filter(|(kind, _)| *kind == Kind::Entry(format.id))
                .count();
            (format.id, entries)
        })
        .find(|&(_, entries)| scored > 0 && entries * 5 >= scored * 3);
    best.map(|(id, _)| id)
}

/// `lines` with `inserted` put in at the given positions, each taken modulo the length
/// at that point.
fn with_inserted(
    lines: &[(Kind, String)],
    inserted: &[(usize, (Kind, String))],
) -> Vec<(Kind, String)> {
    let mut out = lines.to_vec();
    for (at, line) in inserted {
        out.insert(at % (out.len() + 1), line.clone());
    }
    out
}

proptest! {
    #[test]
    fn detection_never_panics_on_arbitrary_lines(
        lines in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..100), 0..300)
    ) {
        detect(lines.iter().map(Vec::as_slice));
    }

    #[test]
    fn agrees_with_counting_by_hand(lines in log(200)) {
        prop_assert_eq!(detect_lines(&texts(&lines)).map(|format| format.id), expected(&lines));
    }

    #[test]
    fn blank_lines_change_nothing(
        lines in log(250),
        blanks in prop::collection::vec((any::<usize>(), blank_line()), 0..50),
    ) {
        let with_blanks = with_inserted(&lines, &blanks);
        prop_assert_eq!(
            detect_lines(&texts(&with_blanks)),
            detect_lines(&texts(&lines))
        );
    }

    #[test]
    fn continuation_lines_within_the_sample_change_nothing(
        lines in log(100),
        continuations in prop::collection::vec((any::<usize>(), continuation_line()), 0..100),
    ) {
        let with_continuations = with_inserted(&lines, &continuations);
        prop_assert_eq!(
            detect_lines(&texts(&with_continuations)),
            detect_lines(&texts(&lines))
        );
    }

    #[test]
    fn lines_after_200_others_change_nothing(
        sample in prop::collection::vec(non_blank_line(), 200),
        rest in log(50),
    ) {
        let whole = [sample.clone(), rest].concat();
        prop_assert_eq!(detect_lines(&texts(&whole)), detect_lines(&texts(&sample)));
    }
}
