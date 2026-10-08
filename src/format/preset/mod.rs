//! The log layouts jlv knows and how to recognise the first line of an entry in each.

use std::fmt;
use std::ops::Range;
use std::sync::LazyLock;

use clap::ValueEnum;
use jiff::tz::TimeZone;
use regex::bytes::{CaptureLocations, Regex};

use crate::format::{LogTime, time};

#[cfg(test)]
mod tests;

/// A built-in log format. On the command line (`--format`) the names are the variant
/// names in kebab case: `logback`, `logback-basic`, …, `raw`.
///
/// - `Logback`, Logback / Log4j2 classic:
///   `2026-10-07 12:00:00.123 [main] INFO  com.example.App - Started`
/// - `LogbackBasic`, Logback's BasicConfigurator, time only:
///   `12:00:00.123 [main] INFO  com.example.App -- Started`
/// - `Spring3`, Spring Boot 3, the `[app]` group is optional:
///   `2026-10-07T12:00:00.123+02:00  INFO 12345 --- [app] [           main] o.s.b.SpringApplication                  : Started`
/// - `Spring2`, Spring Boot 2:
///   `2026-10-07 12:00:00.123  INFO 12345 --- [           main] o.s.b.SpringApplication                  : Started`
/// - `Wildfly`, WildFly `server.log`:
///   `2026-10-07 12:00:00,123 INFO  [org.jboss.as.server] (Controller Boot Thread) WFLYSRV0025: started`
/// - `WildflyConsole`, WildFly on the console, time only and often coloured:
///   `12:00:00,123 INFO  [org.jboss.as] (MSC service thread 1-2) WFLYSRV0049: starting`
/// - `Raw`, no structure: every line is an entry of its own.
//
// No doc comments on the variants: clap would turn them into help for each value and
// change the `--format` help and error texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub enum Preset {
    Logback,
    LogbackBasic,
    Spring3,
    Spring2,
    Wildfly,
    WildflyConsole,
    Raw,
}

/// The preset's name as the header shows it: `Logback`, `Logback basic`,
/// `Spring Boot 3`, `Spring Boot 2`, `WildFly`, `WildFly console` and `Raw`.
impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Preset::Logback => "Logback",
            Preset::LogbackBasic => "Logback basic",
            Preset::Spring3 => "Spring Boot 3",
            Preset::Spring2 => "Spring Boot 2",
            Preset::Wildfly => "WildFly",
            Preset::WildflyConsole => "WildFly console",
            Preset::Raw => "Raw",
        })
    }
}

impl Preset {
    /// Every preset in a fixed order: within a family the one with a date comes before
    /// the time-only one, and `Raw` comes last. Detection breaks ties in this order.
    pub const ALL: [Preset; 7] = [
        Preset::Logback,
        Preset::LogbackBasic,
        Preset::Spring3,
        Preset::Spring2,
        Preset::Wildfly,
        Preset::WildflyConsole,
        Preset::Raw,
    ];

    /// What recognises this preset's entry-start lines. `Raw` has none: it doesn't
    /// group lines, so whoever reads a file under `Raw` treats every line as an entry.
    pub fn entry_parser(self) -> Option<EntryParser> {
        static LOGBACK: Pattern = Pattern::new(|| EntryPattern::new(DATE_TIME, LOGBACK_COLUMNS));
        static LOGBACK_BASIC: Pattern = Pattern::new(|| EntryPattern::new(TIME, LOGBACK_COLUMNS));
        static SPRING3: Pattern =
            Pattern::new(|| EntryPattern::new(ISO_DATE_TIME, SPRING3_COLUMNS));
        static SPRING2: Pattern = Pattern::new(|| EntryPattern::new(DATE_TIME, SPRING2_COLUMNS));
        static WILDFLY: Pattern = Pattern::new(|| EntryPattern::new(DATE_TIME, WILDFLY_COLUMNS));
        static WILDFLY_CONSOLE: Pattern = Pattern::new(|| EntryPattern::new(TIME, WILDFLY_COLUMNS));
        let pattern: &'static EntryPattern = match self {
            Preset::Logback => &LOGBACK,
            Preset::LogbackBasic => &LOGBACK_BASIC,
            Preset::Spring3 => &SPRING3,
            Preset::Spring2 => &SPRING2,
            Preset::Wildfly => &WILDFLY,
            Preset::WildflyConsole => &WILDFLY_CONSOLE,
            Preset::Raw => return None,
        };
        Some(EntryParser {
            preset: self,
            pattern,
        })
    }
}

/// Recognises the first line of an entry in one preset and finds its columns.
#[derive(Debug, Clone, Copy)]
pub struct EntryParser {
    preset: Preset,
    pattern: &'static EntryPattern,
}

impl EntryParser {
    /// The columns of `line` if it starts an entry, else `None`. `line` has no line
    /// ending and is already ANSI-stripped; the spans are byte ranges into it.
    /// Only the layout counts: a line with an impossible date such as `2026-13-45`
    /// still starts an entry, and so does one whose level is a word [`Level`] doesn't
    /// know, such as `SEVERE`.
    ///
    /// [`Level`]: crate::format::Level
    pub fn parse(self, line: &[u8]) -> Option<EntryHead> {
        self.parse_with(line, &mut self.locations())
    }

    /// Room for [`parse_with`](Self::parse_with) to find the columns in.
    pub(in crate::format) fn locations(self) -> CaptureLocations {
        self.pattern.regex.capture_locations()
    }

    /// [`parse`](Self::parse) in `locations`, which [`locations`](Self::locations)
    /// gave and which are reused from line to line. Fresh ones cost an allocation and
    /// an update of a reference count that every thread parsing with this preset
    /// shares, which keeps parsing on many threads at once from getting any faster.
    pub(in crate::format) fn parse_with(
        self,
        line: &[u8],
        locations: &mut CaptureLocations,
    ) -> Option<EntryHead> {
        let matched = self.pattern.regex.captures_read(locations, line)?;
        let span = |group| locations.get(group).map(|(start, end)| start..end);
        let groups = &self.pattern.groups;
        Some(EntryHead {
            timestamp: span(groups.timestamp)?,
            level: span(groups.level)?,
            thread: span(groups.thread)?,
            class: span(groups.class)?,
            // Matching the message too would cost time proportional to its length on
            // every line, for a span that is known anyway.
            message: matched.end()..line.len(),
        })
    }

    /// Whether the timestamp has a date. Without one (`LogbackBasic`,
    /// `WildflyConsole`) it is only a time of day.
    pub fn has_date(self) -> bool {
        !matches!(self.preset, Preset::LogbackBasic | Preset::WildflyConsole)
    }

    /// What the timestamp span `span` (the bytes of [`EntryHead::timestamp`]) says:
    /// an instant if this preset writes a date, else a time of day. The fraction of a
    /// second follows a `.` or a `,` and has 1 to 9 digits. A timestamp without an
    /// offset is local time in `zone`; one that falls into a gap or a fold of `zone`
    /// is resolved the way jiff's `compatible` disambiguation does. `None` if `span`
    /// isn't a timestamp or names a date or time that doesn't exist (`2026-02-30`,
    /// `25:00`, `12:60`) or that jiff can't represent.
    pub fn timestamp(self, span: &[u8], zone: &TimeZone) -> Option<LogTime> {
        if !self.has_date() {
            time::time_of_day(span).map(LogTime::TimeOfDay)
        } else if self.has_offset() {
            time::instant_with_offset(span).map(LogTime::Instant)
        } else {
            time::local_instant(span, zone).map(LogTime::Instant)
        }
    }

    /// Whether the timestamp ends in an offset. Only Spring Boot 3 writes one, in
    /// ISO 8601 with a `T` before the time.
    fn has_offset(self) -> bool {
        self.preset == Preset::Spring3
    }
}

type Pattern = LazyLock<EntryPattern>;

/// A preset's entry-start regex and which of its groups holds each column.
#[derive(Debug)]
struct EntryPattern {
    regex: Regex,
    groups: ColumnGroups,
}

/// Group indices, so that a match isn't searched for the columns by name.
#[derive(Debug)]
struct ColumnGroups {
    timestamp: usize,
    level: usize,
    thread: usize,
    class: usize,
}

impl EntryPattern {
    fn new(timestamp: &str, columns: &str) -> EntryPattern {
        let regex = entry_regex(timestamp, columns);
        let group = |name| {
            regex
                .capture_names()
                .position(|group| group == Some(name))
                .expect("the built-in patterns have a group for every column")
        };
        let groups = ColumnGroups {
            timestamp: group("timestamp"),
            level: group("level"),
            thread: group("thread"),
            class: group("class"),
        };
        EntryPattern { regex, groups }
    }
}

// The patterns are byte patterns without Unicode (`-u`), so a line that isn't UTF-8
// still parses, and `.` takes any byte (`s`). Fractions of a second have 1 to 9
// digits after a `.` or a `,`. Dates and times are only checked for their shape:
// an impossible one still starts an entry.
const TIME: &str = r"\d{2}:\d{2}:\d{2}[.,]\d{1,9}";
const DATE_TIME: &str = r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}[.,]\d{1,9}";
const ISO_DATE_TIME: &str = r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[.,]\d{1,9}(?:Z|[+-]\d{2}:\d{2})";

// What lies between the timestamp and the separator before the message.

/// A thread in brackets runs up to the first `] `: it may hold a `]` of its own as
/// long as no space follows it. Spring 3 relies on this to tell the thread from the
/// groups around it.
macro_rules! bracketed_thread {
    () => {
        r"(?P<thread>(?:[^\]]|\]+[^\] ])+\]*)\] "
    };
}

// %d [%thread] %-5level %logger - %msg, with `--` in BasicConfigurator.
const LOGBACK_COLUMNS: &str = concat!(
    r" \[",
    bracketed_thread!(),
    r"(?P<level>[A-Za-z]+) +(?P<class>\S+) +--?"
);
// %d %5p PID --- [app] [%15.15t] [correlation] %-40.40logger : %m
// Spring Boot 3.2 added the `[app]` group and, with tracing, the correlation group:
// `[trace-span] `, padded with spaces where an id is missing, all blank outside a
// span. The app group is lazy: without an app the thread's bracket would otherwise
// pass for one, and the thread can't swallow a following `] ` to make up for it.
const SPRING3_COLUMNS: &str = concat!(
    r" +(?P<level>[A-Za-z]+) \d+ --- (?:\[[^\]]*\] )??\[ *",
    bracketed_thread!(),
    r"(?:\[[0-9a-f ]{32}[- ][0-9a-f ]{16}\] )?(?P<class>\S+) +:"
);
// %d %5p PID --- [%15.15t] %-40.40logger : %m
const SPRING2_COLUMNS: &str = concat!(
    r" +(?P<level>[A-Za-z]+) \d+ --- \[ *",
    bracketed_thread!(),
    r"(?P<class>\S+) +:"
);
// %d %-5p [%c] (%t) %s%e. The thread is lazy: it ends at the first `) `.
const WILDFLY_COLUMNS: &str = r" (?P<level>[A-Za-z]+) +\[(?P<class>.+?)\] \((?P<thread>.+?)\)";

/// An entry starts at column 0 with a timestamp. Its message starts after the space
/// that ends the match; an empty one may have lost that space to trimming.
fn entry_regex(timestamp: &str, columns: &str) -> Regex {
    let pattern = format!(r"(?s-u)^(?P<timestamp>{timestamp}){columns}(?: |$)");
    Regex::new(&pattern).expect("the built-in patterns are valid regexes")
}

/// The columns of an entry's first line, as byte ranges into that line. Padding
/// around a column (`INFO `, `[           main]`, a class padded to 40) is not part of
/// it. The message runs to the end of the line and may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryHead {
    pub timestamp: Range<usize>,
    pub level: Range<usize>,
    pub thread: Range<usize>,
    pub class: Range<usize>,
    pub message: Range<usize>,
}
