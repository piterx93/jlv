//! Grouping the fixture logs into entries and flagging the ones with an exception.

use std::ops::Range;

use insta::assert_snapshot;
use jlv::format::{Entry, EntryGrouper, Preset, strip_ansi};

mod common;

use common::{fixture, fixtures, lines};

/// Feeds `lines` to a grouper for `preset` and returns every entry, the open one last.
fn group(preset: Preset, lines: &[&[u8]]) -> Vec<Entry> {
    let mut grouper = EntryGrouper::new(preset);
    let mut entries: Vec<Entry> = lines.iter().filter_map(|line| grouper.push(line)).collect();
    entries.extend(grouper.open_entry().cloned());
    entries
}

fn text(line: &[u8], span: Range<usize>) -> String {
    String::from_utf8_lossy(&line[span]).into_owned()
}

/// The entries `preset` finds in the fixture `name`, one per line: the number of the
/// first line (from 1), its timestamp and level or `(headless)`, `+N` for N
/// continuation lines and `exception` if it has one.
fn outline(preset: Preset, name: &str) -> String {
    let bytes = fixture(name);
    let lines = lines(&bytes);
    let mut out = Vec::new();
    for entry in group(preset, &lines) {
        let first = entry.lines.start;
        let head = match entry.head {
            None => "(headless)".to_string(),
            Some(head) => {
                let line = strip_ansi(lines[first]);
                format!(
                    "{} {}",
                    text(&line, head.timestamp),
                    text(&line, head.level)
                )
            }
        };
        let mut row = format!("{:>2} {head}", first + 1);
        let continuation = entry.lines.len() - 1;
        if continuation > 0 {
            row += &format!(" +{continuation}");
        }
        if entry.has_exception {
            row += " exception";
        }
        out.push(row);
    }
    out.join("\n")
}

#[test]
fn groups_the_logback_fixture() {
    assert_snapshot!(outline(Preset::Logback, "logback.log"), @r"
     1 2026-10-07 09:14:02.118 INFO
     2 2026-10-07 09:14:02.121 DEBUG
     3 2026-10-07 09:14:03.502 INFO
     4 2026-10-07 09:14:04.877 INFO
     5 2026-10-07 09:14:06.010 INFO
     6 2026-10-07 09:15:11.342 INFO
     7 2026-10-07 09:15:11.360 DEBUG
     8 2026-10-07 09:15:12.004 WARN
     9 2026-10-07 09:15:14.006 WARN
    10 2026-10-07 09:15:18.011 ERROR +11 exception
    22 2026-10-07 09:15:18.020 INFO
    23 2026-10-07 09:16:00.000 TRACE
    24 2026-10-07 09:16:00.003 DEBUG
    25 2026-10-07 09:16:30.517 INFO
    26 2026-10-07 09:17:45.250 ERROR +6 exception
    33 2026-10-07 09:18:02.771 INFO
    34 2026-10-07 09:18:09.903 WARN
    35 2026-10-07 09:19:40.118 INFO
    36 2026-10-07 09:20:00.000 INFO
    37 2026-10-07 09:21:13.640 WARN
    38 2026-10-07 09:22:05.402 ERROR +10 exception
    49 2026-10-07 09:23:59.999 INFO
    ");
}

#[test]
fn groups_the_logback_basic_fixture() {
    assert_snapshot!(outline(Preset::LogbackBasic, "logback-basic.log"), @r"
     1 23:58:41.007 INFO
     2 23:58:41.093 DEBUG
     3 23:58:41.102 DEBUG
     4 23:58:42.330 INFO
     5 23:58:42.331 INFO
     6 23:59:10.874 WARN
     7 23:59:31.502 INFO
     8 23:59:31.510 INFO
     9 23:59:58.996 DEBUG
    10 00:00:00.004 DEBUG
    11 00:00:02.417 ERROR +10 exception
    22 00:00:02.420 INFO
    23 00:00:02.421 INFO
    24 00:00:03.008 INFO
    25 00:00:03.010 INFO
    26 00:00:03.011 WARN
    27 00:00:03.009 DEBUG
    28 00:00:03.150 INFO
    29 00:00:04.902 ERROR +7 exception
    37 00:00:04.903 TRACE
    38 00:00:05.000 INFO
    39 00:00:05.001 DEBUG
    ");
}

/// The banner before the first entry is a headless entry; blank lines inside and after
/// the stack traces stay in their entries.
#[test]
fn groups_the_spring3_fixture() {
    assert_snapshot!(outline(Preset::Spring3, "spring3.log"), @r"
     1 (headless) +9
    11 2026-10-07T09:14:02.118+02:00 INFO
    12 2026-10-07T09:14:02.121+02:00 DEBUG
    13 2026-10-07T09:14:02.122+02:00 INFO
    14 2026-10-07T09:14:03.502+02:00 INFO
    15 2026-10-07T09:14:03.517+02:00 INFO
    16 2026-10-07T09:14:03.518+02:00 INFO
    17 2026-10-07T09:14:03.581+02:00 INFO
    18 2026-10-07T09:14:04.877+02:00 INFO
    19 2026-10-07T09:14:05.920+02:00 WARN
    20 2026-10-07T09:14:06.010+02:00 INFO
    21 2026-10-07T09:15:11.330+02:00 INFO
    22 2026-10-07T09:15:11.342+02:00 INFO
    23 2026-10-07T09:15:12.004+02:00 WARN
    24 2026-10-07T09:15:18.011+02:00 ERROR +11 exception
    36 2026-10-07T09:16:00.000+02:00 TRACE
    37 2026-10-07T09:16:30.517+02:00 INFO
    38 2026-10-07T09:17:45.250+02:00 ERROR +7 exception
    46 2026-10-07T09:18:02.771+02:00 INFO
    47 2026-10-07T09:20:00.000+02:00 INFO
    48 2026-10-07T09:21:13.640+02:00 WARN
    49 2026-10-07T09:23:59.990+02:00 INFO
    50 2026-10-07T09:23:59.999+02:00 INFO
    51 2026-10-07T09:30:12.004+02:00 INFO
    52 2026-10-07T09:30:16.220+02:00 INFO
    53 2026-10-07T09:31:02.401+02:00 INFO
    54 2026-10-07T09:31:02.415+02:00 INFO
    55 2026-10-07T09:31:02.590+02:00 ERROR +5 exception
    61 2026-10-07T09:32:00.000+02:00 TRACE
    ");
}

#[test]
fn groups_the_spring2_fixture() {
    assert_snapshot!(outline(Preset::Spring2, "spring2.log"), @r"
     1 2026-10-07 09:14:02.118 INFO
     2 2026-10-07 09:14:02.121 DEBUG
     3 2026-10-07 09:14:02.122 INFO
     4 2026-10-07 09:14:03.502 INFO
     5 2026-10-07 09:14:03.517 INFO
     6 2026-10-07 09:14:03.518 INFO
     7 2026-10-07 09:14:03.581 INFO
     8 2026-10-07 09:14:03.582 INFO
     9 2026-10-07 09:14:04.877 INFO
    10 2026-10-07 09:14:06.010 INFO
    11 2026-10-07 09:15:11.342 INFO
    12 2026-10-07 09:15:11.360 DEBUG
    13 2026-10-07 09:15:12.004 WARN
    14 2026-10-07 09:15:18.011 ERROR +11 exception
    26 2026-10-07 09:16:00.000 TRACE
    27 2026-10-07 09:17:45.250 ERROR +7 exception
    35 2026-10-07 09:18:02.771 INFO
    36 2026-10-07 09:19:40.118 INFO
    37 2026-10-07 09:20:00.000 INFO
    38 2026-10-07 09:21:13.640 WARN
    39 2026-10-07 09:23:59.999 INFO
    ");
}

/// Lines 41 and 42 are a stack trace printed to stderr: WildFly logs each of its lines
/// as an entry of its own, and an entry's first line is never checked.
#[test]
fn groups_the_wildfly_fixture() {
    assert_snapshot!(outline(Preset::Wildfly, "wildfly.log"), @r"
     1 2026-10-07 09:13:51,202 INFO
     2 2026-10-07 09:13:51,804 INFO
     3 2026-10-07 09:13:51,813 INFO
     4 2026-10-07 09:13:51,980 INFO
     5 2026-10-07 09:13:53,114 INFO
     6 2026-10-07 09:13:53,640 WARN
     7 2026-10-07 09:13:54,021 INFO
     8 2026-10-07 09:13:55,388 INFO
     9 2026-10-07 09:13:57,912 INFO
    10 2026-10-07 09:13:59,103 INFO
    11 2026-10-07 09:13:59,170 INFO
    12 2026-10-07 09:13:59,240 INFO
    13 2026-10-07 09:15:11,342 INFO
    14 2026-10-07 09:15:11,360 DEBUG
    15 2026-10-07 09:15:12,004 WARN
    16 2026-10-07 09:15:18,011 ERROR +9 exception
    26 2026-10-07 09:16:00,000 TRACE
    27 2026-10-07 09:17:45,250 ERROR +11 exception
    39 2026-10-07 09:18:02,771 INFO
    40 2026-10-07 09:18:02,772 INFO
    41 2026-10-07 09:18:09,903 ERROR
    42 2026-10-07 09:18:09,904 ERROR
    43 2026-10-07 09:19:40,118 INFO
    44 2026-10-07 09:21:13,640 WARN
    45 2026-10-07 09:21:13,651 WARN
    46 2026-10-07 09:22:30,004 INFO
    47 2026-10-07 09:23:59,990 INFO
    ");
}

/// The coloured lines are entries once stripped. The entry at line 37 continues with
/// `\tCaused by: …` lines, which don't match `^Caused by: `, so it has no exception;
/// neither has the service report at line 42.
#[test]
fn groups_the_wildfly_console_fixture() {
    assert_snapshot!(outline(Preset::WildflyConsole, "wildfly-console.log"), @r"
     1 (headless) +11
    13 23:59:56,102 INFO
    14 23:59:56,690 INFO
    15 23:59:56,698 INFO
    16 23:59:56,871 INFO
    17 23:59:57,630 INFO
    18 23:59:58,014 WARN
    19 23:59:58,412 INFO
    20 23:59:59,021 INFO
    21 23:59:59,388 INFO
    22 00:00:00,912 INFO
    23 00:00:01,233 ERROR +13 exception
    37 00:00:01,301 ERROR +2
    40 00:00:01,340 INFO
    41 00:00:01,338 INFO
    42 00:00:01,342 INFO +1
    44 00:00:01,410 INFO
    45 00:00:01,413 INFO
    46 00:00:01,415 ERROR
    47 00:00:14,880 INFO
    48 00:00:15,002 DEBUG
    49 00:00:20,117 INFO
    50 00:01:13,640 WARN
    51 00:02:00,000 INFO
    52 00:02:00,145 INFO
    ");
}

#[test]
fn raw_makes_every_fixture_line_an_entry_without_exceptions() {
    for (_, name) in fixtures() {
        let bytes = fixture(name);
        let lines = lines(&bytes);
        let entries = group(Preset::Raw, &lines);
        assert_eq!(entries.len(), lines.len(), "{name}");
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.lines, i..i + 1, "{name}");
            assert_eq!(entry.head, None, "{name}:{}", i + 1);
            assert!(!entry.has_exception, "{name}:{}", i + 1);
        }
    }
}
