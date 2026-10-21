//! Reading one JSON line's columns with a [`Mapping`], in a single pass over its fields.

use std::ops::Range;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::{Mapping, object};
use crate::format::json::{self, Field};
use crate::format::{Level, LogTime, time};

/// The exception candidates of PRD §6.2. They are not part of the [`Mapping`]: a line's
/// exception is the first of them it has a value for.
const EXCEPTION: &[&str] = &[
    "stack_trace",
    "error.stack_trace",
    "stacktrace",
    "exception",
    "exc_info",
    "@x",
    "Exception",
    "stack",
    "err.stack",
];

/// What the index keeps of a JSON line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct JsonColumns {
    pub timestamp: Option<Timestamp>,
    pub level: Option<Level>,
    /// The logger's text between its quotes, as a byte range into the line.
    pub logger: Option<Range<usize>>,
    pub has_exception: bool,
}

impl Mapping {
    /// The columns of `line`, a JSON object line (see [`object`]; anything else reads
    /// as no columns). Timestamp strings are read as [`time::read`] reads them, in
    /// `zone` if they have no offset, and unquoted numbers as epoch time; a quoted
    /// number is a string. Levels are strings or numbers on the pino scale. A line of
    /// a CLEF file (one that maps `@t`) without a level is Information.
    pub fn read(&self, line: &[u8], zone: &TimeZone) -> JsonColumns {
        let Some(at) = object(line) else {
            return JsonColumns::default();
        };
        let object = &line[at..];
        let (mut timestamp, mut level, mut logger) = (None, None, None);
        let mut has_exception = false;
        for field in json::fields(object) {
            let pick = |key: Option<&str>| resolve(object, &field, key?);
            timestamp = timestamp.or_else(|| pick(self.timestamp));
            level = level.or_else(|| pick(self.level));
            logger = logger.or_else(|| pick(self.logger));
            has_exception = has_exception || is_exception(object, &field);
        }
        let clef = self.timestamp == Some("@t");
        JsonColumns {
            timestamp: timestamp.and_then(|field| timestamp_of(object, &field, zone)),
            level: match level {
                Some(field) => level_of(object, &field),
                None => clef.then_some(Level::Info),
            },
            logger: logger
                .filter(|field| field.string)
                .map(|field| field.value.start + at..field.value.end + at),
            has_exception,
        }
    }
}

/// `field` as the value of `key`, if it is: the field written with that very key or,
/// for a dotted key, the path one object deep. Ranges are into `object`.
fn resolve<'a>(object: &'a [u8], field: &Field<'a>, key: &str) -> Option<Field<'a>> {
    let key = key.as_bytes();
    if field.key == key {
        return Some(field.clone());
    }
    let inner = key.strip_prefix(field.key)?.strip_prefix(b".")?;
    if field.string {
        return None;
    }
    let start = field.value.start;
    let mut nested = json::find(&object[field.value.clone()], inner)?;
    nested.value = nested.value.start + start..nested.value.end + start;
    Some(nested)
}

fn is_exception(object: &[u8], field: &Field<'_>) -> bool {
    EXCEPTION.iter().any(|key| {
        resolve(object, field, key).is_some_and(|found| {
            let value = &object[found.value];
            // `null` and an empty container hold no exception either.
            if found.string {
                !value.is_empty()
            } else {
                !matches!(value, b"null" | b"{}" | b"[]")
            }
        })
    })
}

fn timestamp_of(object: &[u8], field: &Field<'_>, zone: &TimeZone) -> Option<Timestamp> {
    let value = &object[field.value.clone()];
    if field.string {
        match time::read(value, zone)? {
            LogTime::Instant(instant) => Some(instant),
            LogTime::TimeOfDay(_) => None,
        }
    } else {
        time::epoch(value)
    }
}

fn level_of(object: &[u8], field: &Field<'_>) -> Option<Level> {
    let value = &object[field.value.clone()];
    if field.string {
        Level::parse(value)
    } else {
        Level::from_pino(str::from_utf8(value).ok()?.parse().ok()?)
    }
}
