use std::fmt::Write;

use proptest::prelude::*;
use serde_json::{Map, Number, Value};

use super::*;

/// Each field of `line` as its key, its raw value and whether that is a string.
fn scan(line: &str) -> Vec<(&str, &str, bool)> {
    fields(line.as_bytes())
        .map(|field| {
            let key = std::str::from_utf8(field.key).expect("a key of the line");
            (key, &line[field.value], field.string)
        })
        .collect()
}

fn quoted(raw: &[u8]) -> Vec<u8> {
    [b"\"", raw, b"\""].concat()
}

/// What `field` holds, decoded by `serde_json`.
fn decoded(line: &[u8], field: &Field) -> (String, Value) {
    let key = serde_json::from_slice(&quoted(field.key)).expect("a JSON key");
    let raw = &line[field.value.clone()];
    if !field.string {
        assert_eq!(
            raw.trim_ascii(),
            raw,
            "a value without the whitespace around it"
        );
    }
    let value = if field.string {
        serde_json::from_slice(&quoted(raw))
    } else {
        serde_json::from_slice(raw)
    };
    (key, value.expect("a JSON value"))
}

/// Text full of what a scanner can trip over: quotes, backslashes, brackets, commas,
/// colons, control characters and multi-byte characters.
fn text() -> impl Strategy<Value = String> {
    let chars = vec![
        'a', 'b', '"', '\\', '/', '{', '}', '[', ']', ',', ':', ' ', '\n', '\u{1}', 'é', '😀',
    ];
    prop::collection::vec(prop::sample::select(chars), 0..8).prop_map(String::from_iter)
}

/// JSON whitespace, as much of it as may go between two tokens.
fn whitespace() -> impl Strategy<Value = String> {
    "[ \t\r\n]{0,2}"
}

fn value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<f64>().prop_filter_map("JSON has no NaN or infinity", |float| {
            Number::from_f64(float).map(Value::Number)
        }),
        text().prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::vec((text(), inner), 0..4)
                .prop_map(|entries| Value::Object(Map::from_iter(entries))),
        ]
    })
}

/// An object line with whitespace between every token, and the fields written into
/// it, in order. Keys may repeat.
fn object_line() -> impl Strategy<Value = (String, Vec<(String, Value)>)> {
    let field = (
        text(),
        value(),
        [whitespace(), whitespace(), whitespace(), whitespace()],
    );
    (
        whitespace(),
        prop::collection::vec(field, 0..6),
        whitespace(),
    )
        .prop_map(|(before, fields, inside)| {
            let mut line = format!("{before}{{{inside}");
            for (i, (key, value, [a, b, c, d])) in fields.iter().enumerate() {
                if i > 0 {
                    line.push(',');
                }
                let key = serde_json::to_string(key).expect("a key");
                let value = serde_json::to_string_pretty(value).expect("a value");
                write!(line, "{a}{key}{b}:{c}{value}{d}").expect("a String takes any text");
            }
            line.push('}');
            let fields = fields.into_iter().map(|(key, value, _)| (key, value));
            (line, fields.collect())
        })
}

/// An object line with one byte overwritten, or with one byte put in.
fn damaged_line() -> impl Strategy<Value = Vec<u8>> {
    (
        object_line(),
        any::<prop::sample::Index>(),
        any::<u8>(),
        any::<bool>(),
    )
        .prop_map(|((line, _), at, byte, insert)| {
            let mut line = line.into_bytes();
            let at = at.index(line.len() + 1);
            if insert || at == line.len() {
                line.insert(at, byte);
            } else {
                line[at] = byte;
            }
            line
        })
}

#[test]
fn an_escaped_backslash_before_the_closing_quote_ends_the_string() {
    assert_eq!(
        scan(r#"{"a":"x\\","b":"y\"}"}"#),
        [("a", r"x\\", true), ("b", r#"y\"}"#, true)]
    );
}

#[test]
fn a_nested_value_is_covered_whole_and_its_keys_are_not_fields() {
    assert_eq!(
        scan(r#"{"a":{"b":[1,"]}"]},"c":null}"#),
        [("a", r#"{"b":[1,"]}"]}"#, false), ("c", "null", false)]
    );
}

#[test]
fn a_value_nested_deeper_than_any_stack_is_covered_whole() {
    let depth = 1_000_000;
    let nested = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let line = format!(r#"{{"a":{nested},"b":1}}"#);
    assert_eq!(
        scan(&line),
        [("a", nested.as_str(), false), ("b", "1", false)]
    );
}

#[test]
fn a_line_that_is_not_an_object_has_no_fields() {
    for line in ["", " \t", "hello", "[1,2]", r#""a""#, "42", r#"x{"a":1}"#] {
        assert_eq!(scan(line), [], "{line:?}");
    }
}

#[test]
fn bytes_that_cannot_continue_the_object_end_it_after_the_fields_before_them() {
    let lines: [&[u8]; 4] = [
        b"{\"a\":1,\xff\"b\":2}",
        br#"{"a":1,"b"x:2}"#,
        br#"{"a":1,"b":}"#,
        br#"{"a":1,,"b":2}"#,
    ];
    for line in lines {
        let keys: Vec<_> = fields(line).map(|field| field.key).collect();
        assert_eq!(keys, [b"a"], "{}", line.escape_ascii());
    }
}

#[test]
fn invalid_utf8_inside_a_string_is_part_of_its_value() {
    let line = b"{\"a\":\"\xff\xfe\",\"b\":1}";
    assert_eq!(
        fields(line).collect::<Vec<_>>(),
        [
            Field {
                key: b"a",
                value: 6..8,
                string: true
            },
            Field {
                key: b"b",
                value: 14..15,
                string: false
            },
        ]
    );
}

#[test]
fn find_takes_the_first_of_a_repeated_key() {
    let line = br#"{"b":0,"a":1,"a":2}"#;
    let found = find(line, b"a").expect("a field `a`");
    assert_eq!(&line[found.value], b"1");
}

#[test]
fn find_misses_a_key_that_is_absent_or_only_nested() {
    assert_eq!(find(br#"{"x":{"a":1},"b":"a"}"#, b"a"), None);
}

proptest! {
    #[test]
    fn fields_agree_with_serde_json((line, written) in object_line()) {
        let line = line.as_bytes();
        let read: Vec<_> = fields(line).map(|field| (decoded(line, &field), field.string)).collect();
        let written: Vec<_> = written
            .into_iter()
            .map(|(key, value)| {
                let string = value.is_string();
                ((key, value), string)
            })
            .collect();
        prop_assert_eq!(read, written);
    }

    #[test]
    fn a_cut_line_yields_the_fields_before_the_cut((line, _) in object_line()) {
        let line = line.as_bytes();
        let whole: Vec<_> = fields(line).collect();
        for cut in 0..line.len() {
            let read: Vec<_> = fields(&line[..cut]).collect();
            prop_assert!(whole.starts_with(&read), "cut at {}: {:?}", cut, read);
            // A field is complete once the next one's value has begun.
            let complete = whole
                .windows(2)
                .take_while(|pair| pair[1].value.start <= cut)
                .count();
            prop_assert!(read.len() >= complete, "cut at {}: {:?}", cut, read);
        }
    }

    #[test]
    fn scanning_never_panics_and_stays_inside_the_line(
        line in prop_oneof![prop::collection::vec(any::<u8>(), 0..256), damaged_line()]
    ) {
        for field in fields(&line) {
            prop_assert!(field.value.start <= field.value.end);
            prop_assert!(field.value.end <= line.len());
        }
    }
}
