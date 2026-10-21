use std::iter;

use proptest::prelude::*;

use super::*;

fn choose(lines: &[&str]) -> Mapping {
    Mapping::choose(lines.iter().map(|line| line.as_bytes()))
}

/// Lines that a service prints before it logs JSON.
const BANNER: [&str; 4] = [
    "Starting orders v1.4.2",
    "=======================",
    "Listening on 0.0.0.0:8080",
    "",
];

#[test]
fn each_column_takes_its_first_candidate_present_on_every_line() {
    let line = r#"{"msg":"a","message":"b","ts":1,"time":2,"caller":"x","logger":"y","thread":"t","severity":"I","level":"info"}"#;
    assert_eq!(
        choose(&[line, line]),
        Mapping {
            timestamp: Some("time"),
            level: Some("level"),
            logger: Some("logger"),
            thread: Some("thread"),
            message: Some("message"),
        }
    );
}

#[test]
fn candidate_on_exactly_half_of_the_lines_wins_over_a_later_one_on_all() {
    let lines = [
        r#"{"time":1,"ts":1}"#,
        r#"{"ts":2}"#,
        r#"{"time":3,"ts":3}"#,
        r#"{"ts":4}"#,
    ];
    assert_eq!(choose(&lines).timestamp, Some("time"));
}

#[test]
fn candidate_on_fewer_than_half_of_the_lines_is_passed_over() {
    let lines = [r#"{"time":1,"ts":1}"#, r#"{"ts":2}"#, r#"{"ts":3}"#];
    assert_eq!(choose(&lines).timestamp, Some("ts"));
}

#[test]
fn keys_that_differ_between_lines_leave_the_column_unmapped_when_none_is_on_half() {
    let lines = [r#"{"message":"a"}"#, r#"{"msg":"b"}"#, r#"{"event":"c"}"#];
    assert_eq!(choose(&lines).message, None);
}

#[test]
fn clef_file_maps_its_level_key_even_when_most_lines_leave_it_out() {
    // Serilog's compact JSON writes `@l` only for levels other than Information.
    let lines = [
        r#"{"@t":"2026-03-14T08:12:01.104Z","@mt":"Started"}"#,
        r#"{"@t":"2026-03-14T08:12:02.310Z","@mt":"Order {Id} created","Id":42}"#,
        r#"{"@t":"2026-03-14T08:12:03.020Z","@l":"Warning","@mt":"Retry {N}","N":1}"#,
    ];
    assert_eq!(choose(&lines).level, Some("@l"));
}

#[test]
fn file_without_any_candidate_key_maps_nothing() {
    let lines = [r#"{"status":200,"path":"/orders"}"#, r#"{"status":503}"#];
    assert_eq!(choose(&lines), Mapping::default());
}

#[test]
fn banner_lines_do_not_count_toward_the_half() {
    // 3 of the 4 JSON lines have `level`, but only 3 of all 8 lines.
    let json = [
        r#"{"level":"info","msg":"a"}"#,
        r#"{"level":"info","msg":"b"}"#,
        r#"{"msg":"c"}"#,
        r#"{"level":"warn","msg":"d"}"#,
    ];
    let lines = [&BANNER[..], &json[..]].concat();
    assert_eq!(choose(&lines).level, Some("level"));
}

#[test]
fn keys_of_a_first_line_after_a_byte_order_mark_count() {
    assert_eq!(
        choose(&["\u{feff}{\"msg\":\"started\"}"]).message,
        Some("msg")
    );
}

#[test]
fn dotted_candidate_matches_a_literal_dotted_key() {
    let line = r#"{"log.level":"INFO","message":"a"}"#;
    assert_eq!(choose(&[line]).level, Some("log.level"));
}

#[test]
fn dotted_candidate_matches_the_same_path_one_object_deep() {
    let line = r#"{"log":{"level":"INFO","logger":"com.acme.App"},"message":"a"}"#;
    let mapping = choose(&[line]);
    assert_eq!(
        (mapping.level, mapping.logger),
        (Some("log.level"), Some("log.logger"))
    );
}

#[test]
fn dotted_key_one_object_deep_has_its_value_range_in_the_whole_line() {
    let line = br#"{"message":"a","log":{"logger":"com.acme.App","level":"WARN"}}"#;
    let field = lookup(line, "log.level").expect("log.level is one object deep");
    assert_eq!(&line[field.value], b"WARN");
}

#[test]
fn dotted_candidate_is_not_searched_two_objects_deep() {
    let line = r#"{"process":{"thread":{"name":"main"}},"message":"a"}"#;
    assert_eq!(choose(&[line]).thread, None);
}

#[test]
fn key_that_only_starts_with_a_candidate_does_not_match_it() {
    // ECS writes `event.dataset`; `event` is a message candidate.
    let line = r#"{"event.dataset":"orders","eventually":"x"}"#;
    assert_eq!(choose(&[line]).message, None);
}

#[test]
fn reads_no_line_past_the_detection_sample() {
    let lines = vec![r#"{"msg":"a"}"#.as_bytes(); 200];
    let failing = iter::from_fn(|| -> Option<&'static [u8]> { panic!("read one line too many") });
    let mapping = Mapping::choose(lines.into_iter().chain(failing));
    assert_eq!(mapping.message, Some("msg"));
}

proptest! {
    #[test]
    fn never_panics_on_arbitrary_lines(
        lines in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..64), 0..8),
    ) {
        Mapping::choose(lines.iter().map(Vec::as_slice));
    }

    #[test]
    fn never_panics_on_json_like_lines(lines in prop::collection::vec(json_soup(), 0..8)) {
        Mapping::choose(lines.iter().map(Vec::as_slice));
    }
}

/// Bytes that JSON log lines are made of, so random input gets far into an object.
fn json_soup() -> impl Strategy<Value = Vec<u8>> {
    let token = prop_oneof![
        prop::sample::select(vec![
            &b"{"[..],
            b"}",
            b"\"",
            b":",
            b",",
            b"\\",
            b"\\\"",
            b"\"log\"",
            b"\"level\"",
            b"\"msg\"",
            b"\"process\"",
            b"\"thread.name\"",
            b"\"info\"",
            b"null",
            b"[",
            b"]",
            b"\xef\xbb\xbf",
            b" ",
        ])
        .prop_map(<[u8]>::to_vec),
        prop::collection::vec(any::<u8>(), 1..3),
    ];
    prop::collection::vec(token, 0..32).prop_map(|tokens| tokens.concat())
}
