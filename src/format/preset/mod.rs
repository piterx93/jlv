//! The log layouts jlv knows and how to recognise the first line of an entry in each.

use std::ops::Range;
use std::sync::LazyLock;

use clap::ValueEnum;
use regex::bytes::Regex;

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

impl Preset {
    /// What recognises this preset's entry-start lines. `Raw` has none: it doesn't
    /// group lines, so whoever reads a file under `Raw` treats every line as an entry.
    pub fn entry_parser(self) -> Option<EntryParser> {
        static LOGBACK: Pattern = Pattern::new(|| entry_regex(DATE_TIME, LOGBACK_COLUMNS));
        static LOGBACK_BASIC: Pattern = Pattern::new(|| entry_regex(TIME, LOGBACK_COLUMNS));
        static SPRING3: Pattern = Pattern::new(|| entry_regex(ISO_DATE_TIME, SPRING3_COLUMNS));
        static SPRING2: Pattern = Pattern::new(|| entry_regex(DATE_TIME, SPRING2_COLUMNS));
        static WILDFLY: Pattern = Pattern::new(|| entry_regex(DATE_TIME, WILDFLY_COLUMNS));
        static WILDFLY_CONSOLE: Pattern = Pattern::new(|| entry_regex(TIME, WILDFLY_COLUMNS));
        let regex: &'static Regex = match self {
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
            regex,
        })
    }
}

/// Recognises the first line of an entry in one preset and finds its columns.
#[derive(Debug, Clone, Copy)]
pub struct EntryParser {
    preset: Preset,
    regex: &'static Regex,
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
        let captures = self.regex.captures(line)?;
        let span = |name| captures.name(name).map(|column| column.range());
        Some(EntryHead {
            timestamp: span("timestamp")?,
            level: span("level")?,
            thread: span("thread")?,
            class: span("class")?,
            // Matching the message too would cost time proportional to its length on
            // every line, for a span that is known anyway.
            message: captures.get_match().end()..line.len(),
        })
    }

    /// Whether the timestamp has a date. Without one (`LogbackBasic`,
    /// `WildflyConsole`) it is only a time of day.
    pub fn has_date(self) -> bool {
        !matches!(self.preset, Preset::LogbackBasic | Preset::WildflyConsole)
    }
}

type Pattern = LazyLock<Regex>;

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
