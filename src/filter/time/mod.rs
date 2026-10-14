//! The time filter as the user writes it (PRD §9.3): a window of the last minutes,
//! hours or days, or a range of times, and what it covers once the last entry and
//! the wall clock are known.

use std::fmt;

use std::str::FromStr;

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use super::TimeRange;

#[cfg(test)]
mod tests;

/// A time filter as written, before it is dated.
///
/// - `Nm`, `Nh`, `Nd`: the last N minutes, hours or days (a day being 24 hours),
///   N at least 1.
/// - `12:00-12:30`, `12:00:15-12:00:45`, `12:00-`: a range of times without a date,
///   on the date of the last entry. An end before the start crosses midnight.
/// - `2026-10-07 12:00..2026-10-07 13:00`, `2026-10-07 12:00..`: a range with dates.
///   An end before the start is an error.
/// - `12:30`, `2026-10-07 12:30`: a single time, the range of that one minute (or
///   second).
///
/// Hours, minutes and seconds have two digits. Whitespace around the spec
/// and around its parts is ignored. Every range starts at its start, inclusive, and
/// ends with the last instant its end states: `12:30` takes in all of 12:30:59.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSpec(Spec);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Spec {
    /// The last `count` of `unit` before the base.
    Last { count: u32, unit: Unit },
    /// The one minute or second `at` states.
    At(Point),
    /// From `start` to `end`, open-ended without an end. Either both have a date or
    /// neither has.
    Range { start: Point, end: Option<Point> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Minutes,
    Hours,
    Days,
}

/// A time as written, with its date if it has one, and whether it states seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    date: Option<Date>,
    time: Time,
    seconds: bool,
}

/// What "the last N" counts back from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Base {
    /// The last entry in the file, so that it works for old logs.
    #[default]
    File,
    /// The wall clock.
    Now,
}

/// The time filter: a spec and the base its "last N" counts back from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeFilter {
    pub spec: TimeSpec,
    pub base: Base,
}

impl Base {
    /// The other base.
    pub fn toggled(self) -> Base {
        match self {
            Base::File => Base::Now,
            Base::Now => Base::File,
        }
    }
}

const MALFORMED: &str = "is not like 10m, 12:00-12:30 or 2026-10-07 12:00..";
const BACKWARDS: &str = "ends before it starts";

impl TimeSpec {
    /// The spec `spec` writes. One that is none of the forms above is an error that
    /// quotes it.
    pub fn parse(spec: &str) -> Result<TimeSpec, String> {
        Spec::parse(spec.trim())
            .map(TimeSpec)
            .map_err(|why| format!("time spec '{spec}' {why}"))
    }
}

impl Spec {
    fn parse(written: &str) -> Result<Spec, &'static str> {
        if let Some(last) = Spec::last(written) {
            return last;
        }
        if let Some((start, end)) = written.split_once("..") {
            let dated = |written: &str| Point::parse(written).filter(|point| point.date.is_some());
            let start = dated(start).ok_or(MALFORMED)?;
            let end = optional(end, dated)?;
            // Both are dated, so the date they are put on doesn't matter.
            if end.is_some_and(|end| end.end_on(Date::MIN) <= start.on(Date::MIN)) {
                return Err(BACKWARDS);
            }
            return Ok(Spec::Range { start, end });
        }
        if let Some(at) = Point::parse(written) {
            return Ok(Spec::At(at));
        }
        let undated = |written: &str| Point::parse(written).filter(|point| point.date.is_none());
        let (start, end) = written.split_once('-').ok_or(MALFORMED)?;
        Ok(Spec::Range {
            start: undated(start).ok_or(MALFORMED)?,
            end: optional(end, undated)?,
        })
    }

    /// `Nm`, `Nh` or `Nd` as the last N, `None` for anything else.
    fn last(written: &str) -> Option<Result<Spec, &'static str>> {
        let (digits, unit) = written.split_at_checked(written.len().checked_sub(1)?)?;
        let unit = match unit {
            "m" => Unit::Minutes,
            "h" => Unit::Hours,
            "d" => Unit::Days,
            _ => return None,
        };
        let count = number(digits, digits.len())?;
        Some(if count == 0 {
            Err(MALFORMED)
        } else {
            Ok(Spec::Last { count, unit })
        })
    }
}

/// The point `written` states with `parse`, `None` if it is blank.
fn optional(
    written: &str,
    parse: impl Fn(&str) -> Option<Point>,
) -> Result<Option<Point>, &'static str> {
    if written.trim().is_empty() {
        Ok(None)
    } else {
        parse(written).map(Some).ok_or(MALFORMED)
    }
}

impl Unit {
    fn seconds(self) -> i64 {
        match self {
            Unit::Minutes => 60,
            Unit::Hours => 3600,
            Unit::Days => 86_400,
        }
    }

    fn suffix(self) -> char {
        match self {
            Unit::Minutes => 'm',
            Unit::Hours => 'h',
            Unit::Days => 'd',
        }
    }
}

impl Point {
    /// `12:30`, `12:30:15` or either after a date like `2026-10-07`.
    fn parse(written: &str) -> Option<Point> {
        let mut parts = written.split_whitespace();
        let (date, time) = match (parts.next()?, parts.next(), parts.next()) {
            (time, None, _) => (None, time),
            (date, Some(time), None) => (Some(parse_date(date)?), time),
            _ => return None,
        };
        let mut fields = time.split(':');
        let hour = number(fields.next()?, 2)?;
        let minute = number(fields.next()?, 2)?;
        let second = fields.next().map(|second| number(second, 2));
        if fields.next().is_some() {
            return None;
        }
        Some(Point {
            date,
            time: Time::new(hour, minute, second.unwrap_or(Some(0))?, 0).ok()?,
            seconds: second.is_some(),
        })
    }

    /// When it starts, on `date` unless it has a date of its own.
    fn on(&self, date: Date) -> DateTime {
        self.date.unwrap_or(date).to_datetime(self.time)
    }

    /// When the minute or second it states ends, on `date` unless it has a date of
    /// its own; the latest time there is for one that ends past it.
    fn end_on(&self, date: Date) -> DateTime {
        let step = SignedDuration::from_secs(if self.seconds { 1 } else { 60 });
        self.on(date).checked_add(step).unwrap_or(DateTime::MAX)
    }
}

fn parse_date(written: &str) -> Option<Date> {
    let mut fields = written.split('-');
    let year = number(fields.next()?, 4)?;
    let month = number(fields.next()?, 2)?;
    let day = number(fields.next()?, 2)?;
    if fields.next().is_some() {
        return None;
    }
    Date::new(year, month, day).ok()
}

/// `written` as a number if it is `digits` decimal digits and fits a `T`.
fn number<T: FromStr>(written: &str, digits: usize) -> Option<T> {
    let decimal =
        digits > 0 && written.len() == digits && written.bytes().all(|b| b.is_ascii_digit());
    decimal.then(|| written.parse().ok()).flatten()
}

/// As [`TimeSpec::parse`] reads it, without extra whitespace: `10m`, `12:00-12:30`,
/// `2026-10-07 12:00..2026-10-07 13:00`.
impl fmt::Display for TimeSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Spec::Last { count, unit } => write!(f, "{count}{}", unit.suffix()),
            Spec::At(at) => write!(f, "{at}"),
            Spec::Range { start, end } => {
                let between = if start.date.is_some() { ".." } else { "-" };
                write!(f, "{start}{between}")?;
                end.map_or(Ok(()), |end| write!(f, "{end}"))
            }
        }
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(date) = self.date {
            write!(f, "{date} ")?;
        }
        write!(f, "{:02}:{:02}", self.time.hour(), self.time.minute())?;
        if self.seconds {
            write!(f, ":{:02}", self.time.second())?;
        }
        Ok(())
    }
}

impl TimeSpec {
    /// Where the spec starts, `last` being the timestamp of the last entry, in
    /// `zone`: the start of the [range](TimeFilter::range) it covers counted from the
    /// file.
    pub fn start(&self, last: Timestamp, zone: &TimeZone) -> Timestamp {
        let filter = TimeFilter {
            spec: self.clone(),
            base: Base::File,
        };
        filter.range(last, last, zone).start
    }
}

impl TimeFilter {
    /// What the filter covers, `last` being the timestamp of the last entry and
    /// `now` the wall clock, in `zone`. "The last N" starts N before `last`, or
    /// before `now` with [`Base::Now`], and has no end. A range without a date is on
    /// the date `last` has in `zone`, whatever the base; one that crosses midnight is
    /// the one that starts on that date, or the day before if `last` is earlier in
    /// the day than its start. A start before the earliest time there is starts
    /// there.
    pub fn range(&self, last: Timestamp, now: Timestamp, zone: &TimeZone) -> TimeRange {
        let last_seen = zone.to_datetime(last);
        let date = last_seen.date();
        let (start, end) = match self.spec.0 {
            Spec::Last { count, unit } => {
                let base = match self.base {
                    Base::File => last,
                    Base::Now => now,
                };
                let back = SignedDuration::from_secs(i64::from(count) * unit.seconds());
                let start = base.checked_sub(back).unwrap_or(Timestamp::MIN);
                return TimeRange { start, end: None };
            }
            Spec::At(at) => (at.on(date), Some(at.end_on(date))),
            Spec::Range { start, end: None } => (start.on(date), None),
            Spec::Range {
                start,
                end: Some(end),
            } => {
                let (start, end) = (start.on(date), end.end_on(date));
                // Only a range without dates can end before it starts.
                if end <= start {
                    across_midnight(start, end, last_seen)
                } else {
                    (start, Some(end))
                }
            }
        };
        TimeRange {
            start: timestamp(start, zone),
            end: end.map(|end| timestamp(end, zone)),
        }
    }
}

/// The range from `start` to `end` that crosses midnight once, both being on the
/// same date: from `start` on that date to `end` the day after, or a day earlier if
/// `last_seen` is earlier in the day than `start`.
fn across_midnight(
    start: DateTime,
    end: DateTime,
    last_seen: DateTime,
) -> (DateTime, Option<DateTime>) {
    if last_seen.time() < start.time() {
        (start.yesterday().unwrap_or(DateTime::MIN), Some(end))
    } else {
        (start, Some(end.tomorrow().unwrap_or(DateTime::MAX)))
    }
}

/// `at` in `zone`, or the earliest or latest instant there is for a time beyond them.
fn timestamp(at: DateTime, zone: &TimeZone) -> Timestamp {
    let beyond = if at.year() < 0 {
        Timestamp::MIN
    } else {
        Timestamp::MAX
    };
    zone.to_ambiguous_timestamp(at)
        .compatible()
        .unwrap_or(beyond)
}

/// `last 10m (file)` or `last 10m (now)` for the last N, the spec as written for a
/// range, whose base doesn't matter.
impl fmt::Display for TimeFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.spec.0, self.base) {
            (Spec::Last { .. }, Base::File) => write!(f, "last {} (file)", self.spec),
            (Spec::Last { .. }, Base::Now) => write!(f, "last {} (now)", self.spec),
            _ => write!(f, "{}", self.spec),
        }
    }
}
