use proptest::prelude::*;

use super::*;

const LEVELS: [(Level, &str); 6] = [
    (Level::Trace, "TRACE"),
    (Level::Debug, "DEBUG"),
    (Level::Info, "INFO"),
    (Level::Warn, "WARN"),
    (Level::Error, "ERROR"),
    (Level::Fatal, "FATAL"),
];

/// `strip_ansi` as text, for readable failures.
fn stripped(line: &[u8]) -> String {
    String::from_utf8_lossy(&strip_ansi(line)).into_owned()
}

// --- levels ---

#[test]
fn parses_every_level_name() {
    for (level, name) in LEVELS {
        assert_eq!(Level::parse(name.as_bytes()), Some(level), "{name}");
    }
}

#[test]
fn warning_is_an_alias_of_warn() {
    assert_eq!(Level::parse(b"WARNING"), Some(Level::Warn));
}

#[test]
fn letter_case_does_not_matter() {
    assert_eq!(Level::parse(b"info"), Some(Level::Info));
    assert_eq!(Level::parse(b"Debug"), Some(Level::Debug));
    assert_eq!(Level::parse(b"eRrOr"), Some(Level::Error));
    assert_eq!(Level::parse(b"Warning"), Some(Level::Warn));
}

#[test]
fn trailing_padding_is_ignored() {
    assert_eq!(Level::parse(b"INFO "), Some(Level::Info));
    assert_eq!(Level::parse(b"WARN  "), Some(Level::Warn));
}

#[test]
fn leading_padding_is_ignored() {
    assert_eq!(Level::parse(b" WARN"), Some(Level::Warn));
    assert_eq!(Level::parse(b"   INFO"), Some(Level::Info));
}

#[test]
fn padding_on_both_sides_is_ignored() {
    assert_eq!(Level::parse(b"  ERROR  "), Some(Level::Error));
    assert_eq!(Level::parse(b" WARNING "), Some(Level::Warn));
}

#[test]
fn tabs_count_as_padding() {
    assert_eq!(Level::parse(b"\tTRACE\t"), Some(Level::Trace));
}

#[test]
fn other_words_are_not_levels() {
    for word in [
        "SEVERE", "FINE", "CRITICAL", "NOTICE", "ERR", "WARN.", "INFOS", "[INFO]",
    ] {
        assert_eq!(Level::parse(word.as_bytes()), None, "{word}");
    }
}

#[test]
fn empty_token_is_not_a_level() {
    assert_eq!(Level::parse(b""), None);
}

#[test]
fn blank_token_is_not_a_level() {
    assert_eq!(Level::parse(b"   "), None);
}

#[test]
fn space_inside_the_name_is_not_padding() {
    assert_eq!(Level::parse(b"IN FO"), None);
    assert_eq!(Level::parse(b"WARN ING"), None);
}

#[test]
fn non_ascii_lookalikes_are_not_levels() {
    // Dotless i uppercases to a plain I in Unicode.
    assert_eq!(Level::parse("ınfo".as_bytes()), None);
    assert_eq!(Level::parse("ＩＮＦＯ".as_bytes()), None);
}

#[test]
fn invalid_utf8_next_to_a_name_is_not_a_level() {
    assert_eq!(Level::parse(b"INFO\xff"), None);
}

#[test]
fn escape_codes_are_not_padding() {
    assert_eq!(Level::parse(b"\x1b[32mINFO \x1b[0m"), None);
}

#[test]
fn level_wrapped_in_colour_codes_parses_after_stripping() {
    // As in WildFly console logs.
    let token = strip_ansi(b"\x1b[32mINFO \x1b[0m");
    assert_eq!(Level::parse(&token), Some(Level::Info));
}

/// The level filter keys 1 to 6 follow this order.
#[test]
fn levels_are_ordered_from_trace_to_fatal() {
    let levels = LEVELS.map(|(level, _)| level);
    assert!(levels.windows(2).all(|pair| pair[0] < pair[1]));
}

fn padding() -> impl Strategy<Value = String> {
    prop::collection::vec(prop_oneof![Just(' '), Just('\t')], 0..4)
        .prop_map(|chars| chars.into_iter().collect())
}

/// A level name or alias with its expected level.
fn level_name() -> impl Strategy<Value = (Level, &'static str)> {
    prop_oneof![
        prop::sample::select(LEVELS.to_vec()),
        Just((Level::Warn, "WARNING")),
    ]
}

proptest! {
    #[test]
    fn level_parsing_never_panics_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..32)
    ) {
        Level::parse(&bytes);
    }

    #[test]
    fn any_casing_and_padding_of_a_name_parses(
        (level, name) in level_name(),
        lower in prop::collection::vec(any::<bool>(), 7),
        before in padding(),
        after in padding(),
    ) {
        let cased: String = name
            .chars()
            .zip(&lower)
            .map(|(c, &lower)| if lower { c.to_ascii_lowercase() } else { c })
            .collect();
        let token = format!("{before}{cased}{after}");
        prop_assert_eq!(Level::parse(token.as_bytes()), Some(level));
    }

    #[test]
    fn only_level_names_parse(token in "[ \tA-Za-z]{0,10}") {
        let word = token.trim_matches([' ', '\t']).to_ascii_uppercase();
        let is_name = word == "WARNING" || LEVELS.iter().any(|(_, name)| *name == word);
        prop_assert_eq!(Level::parse(token.as_bytes()).is_some(), is_name);
    }
}

// --- strip_ansi ---

#[test]
fn line_without_escapes_is_borrowed_unchanged() {
    let line = b"12:00:00,123 INFO  [org.jboss.as] (MSC service thread 1-2) started";
    let out = strip_ansi(line);
    assert!(matches!(out, Cow::Borrowed(_)));
    assert_eq!(&*out, &line[..]);
}

#[test]
fn empty_line_is_borrowed_empty() {
    let out = strip_ansi(b"");
    assert!(matches!(out, Cow::Borrowed(_)));
    assert_eq!(&*out, b"");
}

#[test]
fn removes_colour_codes() {
    assert_eq!(stripped(b"\x1b[31mERROR\x1b[0m"), "ERROR");
}

#[test]
fn removes_colour_codes_with_several_parameters() {
    assert_eq!(stripped(b"\x1b[1;38;5;208mWARN\x1b[m"), "WARN");
    assert_eq!(stripped(b"\x1b[38;2;255;0;0mERROR\x1b[39m"), "ERROR");
}

#[test]
fn removes_adjacent_sequences() {
    assert_eq!(
        stripped(b"\x1b[1m\x1b[31m\x1b[4mFATAL\x1b[0m\x1b[0m"),
        "FATAL"
    );
}

#[test]
fn keeps_the_text_around_colour_codes() {
    let line = b"12:00:00,123 \x1b[32mINFO \x1b[0m [org.jboss.as] (MSC service thread 1-2) \
                 WFLYSRV0049: started";
    assert_eq!(
        stripped(line),
        "12:00:00,123 INFO  [org.jboss.as] (MSC service thread 1-2) WFLYSRV0049: started"
    );
}

#[test]
fn keeps_utf8_text_between_sequences() {
    assert_eq!(
        stripped("\x1b[33mZażółć gęślą 日本\x1b[0m".as_bytes()),
        "Zażółć gęślą 日本"
    );
}

#[test]
fn csi_ends_at_its_first_final_byte() {
    assert_eq!(stripped(b"\x1b[0mmore"), "more");
    assert_eq!(stripped(b"\x1b[2J@home"), "@home");
}

#[test]
fn removes_cursor_and_erase_sequences() {
    assert_eq!(stripped(b"\x1b[2K\x1b[1Gdone\x1b[10;20H!"), "done!");
}

#[test]
fn removes_csi_with_private_parameters() {
    assert_eq!(stripped(b"\x1b[?25lhidden\x1b[?25h"), "hidden");
}

#[test]
fn removes_csi_with_intermediate_bytes() {
    assert_eq!(stripped(b"\x1b[2 qblock"), "block");
}

#[test]
fn removes_osc_ended_by_bel() {
    assert_eq!(stripped(b"\x1b]0;WildFly\x07started"), "started");
}

#[test]
fn removes_osc_ended_by_string_terminator() {
    assert_eq!(
        stripped(b"see \x1b]8;;https://example.com/a?b=[1]\x1b\\docs\x1b]8;;\x1b\\ now"),
        "see docs now"
    );
}

#[test]
fn removes_two_byte_escape_sequences() {
    assert_eq!(stripped(b"\x1b7saved\x1b8"), "saved");
    assert_eq!(stripped(b"\x1bMup\x1bcreset"), "upreset");
    assert_eq!(stripped(b"\x1b=keypad\x1b>"), "keypad");
}

#[test]
fn removes_charset_selection_with_its_final_byte() {
    // `tput sgr0` writes this on xterm.
    assert_eq!(stripped(b"\x1b(B\x1b[mINFO"), "INFO");
}

#[test]
fn unterminated_csi_at_end_of_line_is_dropped() {
    assert_eq!(stripped(b"INFO \x1b[3"), "INFO ");
    assert_eq!(stripped(b"INFO \x1b[38;5;"), "INFO ");
}

#[test]
fn bare_csi_introducer_at_end_of_line_is_dropped() {
    assert_eq!(stripped(b"INFO \x1b["), "INFO ");
}

#[test]
fn unterminated_osc_at_end_of_line_is_dropped() {
    assert_eq!(stripped(b"done \x1b]8;;https://example.com"), "done ");
}

#[test]
fn osc_cut_inside_its_string_terminator_is_dropped() {
    assert_eq!(stripped(b"done \x1b]0;title\x1b"), "done ");
}

#[test]
fn esc_inside_csi_starts_a_new_sequence() {
    assert_eq!(stripped(b"\x1b[3\x1b[31mERROR"), "ERROR");
}

#[test]
fn esc_inside_csi_can_start_a_two_byte_sequence() {
    assert_eq!(stripped(b"\x1b[3\x1b7saved"), "saved");
}

#[test]
fn esc_inside_osc_starts_a_new_sequence() {
    assert_eq!(stripped(b"\x1b]0;title\x1b[32mINFO msg"), "INFO msg");
}

#[test]
fn esc_inside_osc_can_start_a_two_byte_sequence() {
    assert_eq!(stripped(b"\x1b]0;title\x1b7saved"), "saved");
}

#[test]
fn esc_inside_charset_selection_starts_a_new_sequence() {
    assert_eq!(stripped(b"\x1b(\x1b[31mERROR"), "ERROR");
}

#[test]
fn lone_esc_at_end_of_line_is_dropped() {
    assert_eq!(stripped(b"done\x1b"), "done");
}

#[test]
fn line_of_only_esc_becomes_empty() {
    assert_eq!(stripped(b"\x1b"), "");
}

#[test]
fn esc_before_a_non_ascii_byte_is_dropped_and_the_byte_kept() {
    assert_eq!(&*strip_ansi("a\x1bé".as_bytes()), "aé".as_bytes());
}

#[test]
fn eight_bit_csi_is_left_alone() {
    let line = b"\x9b31mERROR";
    assert_eq!(&*strip_ansi(line), &line[..]);
}

#[test]
fn eight_bit_osc_is_left_alone() {
    let line = b"\x9d0;title\x07rest";
    assert_eq!(&*strip_ansi(line), &line[..]);
}

/// Bytes that make up escape sequences, mixed with arbitrary ones, so that random
/// input hits sequences, cut-off sequences and their terminators often.
fn ansi_soup() -> impl Strategy<Value = Vec<u8>> {
    let byte = prop_oneof![
        4 => Just(0x1b),
        2 => Just(b'['),
        2 => Just(b']'),
        1 => Just(0x07),
        1 => Just(b'\\'),
        1 => Just(b'm'),
        1 => Just(b';'),
        1 => Just(b'('),
        2 => b'0'..=b'9',
        2 => any::<u8>(),
    ];
    prop::collection::vec(byte, 0..64)
}

fn text_without_esc() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>().prop_filter("no ESC", |&b| b != 0x1b), 0..32)
}

fn sgr() -> impl Strategy<Value = Vec<u8>> {
    "[0-9;]{0,12}".prop_map(|params| format!("\x1b[{params}m").into_bytes())
}

proptest! {
    #[test]
    fn stripping_never_panics_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..256)
    ) {
        strip_ansi(&bytes);
    }

    #[test]
    fn output_contains_no_esc(bytes in ansi_soup()) {
        prop_assert!(!strip_ansi(&bytes).contains(&0x1b));
    }

    #[test]
    fn stripping_twice_equals_stripping_once(bytes in ansi_soup()) {
        let once = strip_ansi(&bytes).into_owned();
        prop_assert_eq!(strip_ansi(&once).into_owned(), once);
    }

    #[test]
    fn output_is_never_longer_than_input(bytes in ansi_soup()) {
        prop_assert!(strip_ansi(&bytes).len() <= bytes.len());
    }

    #[test]
    fn stripping_only_removes_bytes(bytes in ansi_soup()) {
        let out = strip_ansi(&bytes);
        let mut rest = bytes.iter();
        prop_assert!(out.iter().all(|b| rest.any(|r| r == b)), "{:?} -> {:?}", bytes, out);
    }

    #[test]
    fn text_without_esc_is_borrowed_unchanged(text in text_without_esc()) {
        let out = strip_ansi(&text);
        prop_assert!(matches!(out, Cow::Borrowed(_)));
        prop_assert_eq!(&*out, &text[..]);
    }

    #[test]
    fn colour_codes_between_texts_leave_the_texts_joined(
        before in text_without_esc(),
        colour in sgr(),
        after in text_without_esc(),
    ) {
        let line = [&before[..], &colour, &after].concat();
        prop_assert_eq!(strip_ansi(&line).into_owned(), [before, after].concat());
    }
}
