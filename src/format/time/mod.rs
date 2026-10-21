//! What an entry's timestamp says: a point in time, or only a time of day.
//!
//! Timestamps are read with a small byte reader rather than jiff's general parser,
//! which accepts shapes no log writes (a date only, `HH:mm`), silently drops the
//! offset in `parse_datetime` and takes about twice as long.

use jiff::Timestamp;
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{Offset, TimeZone};

#[cfg(test)]
mod tests;

/// A parsed entry timestamp, to full nanosecond precision.
///
/// - `Instant`: the timestamp has a date. An explicit offset in it (`+02:00`, `Z`)
///   decides which instant it is; without one, the time zone the caller gives.
/// - `TimeOfDay`: the format only writes the time (`logback-basic`,
///   `wildfly-console`). Which day it belongs to is worked out from the file later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogTime {
    Instant(jiff::Timestamp),
    TimeOfDay(jiff::civil::Time),
}

/// What the timestamp `span` says, read as a whole. The shapes:
///
/// - a date as `yyyy-MM-dd` or `yyyy/MM/dd`, then `T` or a space, then the time;
/// - the time as `HH:mm:ss`, then optionally a fraction of 1 to 9 digits after `.`
///   or `,`; second 60, a leap second, is read as second 59;
/// - after a date and time, optionally an offset `Z`, `±hh:mm`, `±hhmm` or `±hh`,
///   with or without one space before it;
/// - the time alone, without a date or an offset, which gives a time of day.
///
/// A date and time without an offset is local time in `zone`; one that falls into a
/// gap or a fold of `zone` is resolved the way jiff's `compatible` disambiguation
/// does. `None` for any other bytes and for a date, time or offset that doesn't exist
/// (`2026-02-30`, `25:00`, `+02:60`) or an instant jiff can't represent.
pub(crate) fn read(span: &[u8], zone: &TimeZone) -> Option<LogTime> {
    let mut reader = Reader(span);
    // A date starts with four digits, a time of day with two and a colon.
    if span.get(2) == Some(&b':') {
        let time = reader.time()?;
        return reader.is_done().then_some(LogTime::TimeOfDay(time));
    }
    let date_time = reader.date_time()?;
    let instant = if reader.is_done() {
        local_to_instant(date_time, zone)?
    } else {
        reader.byte(b" ");
        reader.offset()?.to_timestamp(date_time).ok()?
    };
    reader.is_done().then_some(LogTime::Instant(instant))
}

/// An unquoted number in a JSON or logfmt value read as epoch time. The number of
/// integer digits gives the unit: up to 11 digits are seconds, which may have a
/// fraction of 1 to 9 digits after a `.`; 13 digits are milliseconds, 16
/// microseconds and 19 nanoseconds, all without a fraction. `None` for any other
/// count of digits and for anything that isn't such a number (a sign, an exponent).
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the JSON and logfmt readers of the next milestone call it"
    )
)]
pub(crate) fn epoch(number: &[u8]) -> Option<Timestamp> {
    let integer_len = number.iter().take_while(|b| b.is_ascii_digit()).count();
    let (integer, rest) = number.split_at(integer_len);
    let unit: i128 = match integer_len {
        1..=11 => 1_000_000_000,
        13 => 1_000_000,
        16 => 1_000,
        19 => 1,
        _ => return None,
    };
    let mut nanos = integer
        .iter()
        .fold(0i128, |value, digit| value * 10 + i128::from(digit - b'0'))
        * unit;
    if !rest.is_empty() {
        if unit != 1_000_000_000 {
            return None;
        }
        let mut reader = Reader(rest);
        reader.byte(b".")?;
        nanos += i128::from(reader.fraction()?);
        reader.is_done().then_some(())?;
    }
    Timestamp::from_nanosecond(nanos).ok()
}

/// `date_time` read as local time in `zone`. In a DST gap it takes the offset from
/// before the gap, in a fold the earlier instant. `None` only at the ends of jiff's
/// range.
pub(super) fn local_to_instant(date_time: DateTime, zone: &TimeZone) -> Option<Timestamp> {
    // Just before jiff's first instant, an offset (up to ±25:59:59) trips a debug
    // assertion in jiff instead of an error, and in release gives an invalid instant.
    // `DateTime::MIN` plus twice the largest offset is past that for every offset.
    const FIRST: DateTime = jiff::civil::datetime(-9999, 1, 3, 3, 59, 58, 0);
    if date_time < FIRST {
        return None;
    }
    zone.to_ambiguous_timestamp(date_time).compatible().ok()
}

/// `yyyy-MM-dd`.
pub(super) fn date(span: &[u8]) -> Option<Date> {
    let mut reader = Reader(span);
    let date = reader.date()?;
    reader.is_done().then_some(date)
}

/// The bytes of a timestamp that haven't been read yet. Every method reads one part
/// from the front and gives `None` if it isn't there or is out of range.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn is_done(&self) -> bool {
        self.0.is_empty()
    }

    fn date_time(&mut self) -> Option<DateTime> {
        let date = self.date()?;
        self.byte(b"T ")?;
        Some(date.to_datetime(self.time()?))
    }

    fn date(&mut self) -> Option<Date> {
        let year = self.digits::<4>()?;
        let separator = self.byte(b"-/")?;
        let month = self.two_digits()?;
        self.byte(&[separator])?;
        let day = self.two_digits()?;
        Date::new(year, month, day).ok()
    }

    fn time(&mut self) -> Option<Time> {
        let hour = self.two_digits()?;
        self.byte(b":")?;
        let minute = self.two_digits()?;
        self.byte(b":")?;
        // Second 60 is a leap second, which jiff's `Time` doesn't have.
        let second = match self.two_digits()? {
            60 => 59,
            second => second,
        };
        let nanosecond = match self.byte(b".,") {
            Some(_) => self.fraction()?,
            None => 0,
        };
        Time::new(hour, minute, second, nanosecond).ok()
    }

    fn offset(&mut self) -> Option<Offset> {
        let sign = match self.byte(b"Z+-")? {
            b'Z' => return Some(Offset::UTC),
            b'-' => -1,
            _ => 1,
        };
        let hours = i32::from(self.two_digits()?);
        let colon = self.byte(b":").is_some();
        let minutes = if colon || !self.is_done() {
            i32::from(self.two_digits()?)
        } else {
            0
        };
        if minutes >= 60 {
            return None;
        }
        Offset::from_seconds(sign * (hours * 3600 + minutes * 60)).ok()
    }

    /// One to nine digits, as nanoseconds.
    fn fraction(&mut self) -> Option<i32> {
        let len = self.0.iter().take_while(|b| b.is_ascii_digit()).count();
        if !(1..=9).contains(&len) {
            return None;
        }
        let (digits, rest) = self.0.split_at(len);
        self.0 = rest;
        let value = digits
            .iter()
            .fold(0, |value, digit| value * 10 + i32::from(digit - b'0'));
        // `len` is at most 9, so neither the cast nor the power overflows.
        Some(value * 10i32.pow(9 - len as u32))
    }

    fn two_digits(&mut self) -> Option<i8> {
        // Two decimal digits are at most 99, which fits an `i8`.
        self.digits::<2>().map(|value| value as i8)
    }

    /// `N` decimal digits, `N` at most 4 so that the value fits an `i16`.
    fn digits<const N: usize>(&mut self) -> Option<i16> {
        let (digits, rest) = self.0.split_first_chunk::<N>()?;
        let mut value = 0;
        for &digit in digits {
            if !digit.is_ascii_digit() {
                return None;
            }
            value = value * 10 + i16::from(digit - b'0');
        }
        self.0 = rest;
        Some(value)
    }

    /// The next byte if it is one of `allowed`.
    fn byte(&mut self, allowed: &[u8]) -> Option<u8> {
        let (&first, rest) = self.0.split_first()?;
        if !allowed.contains(&first) {
            return None;
        }
        self.0 = rest;
        Some(first)
    }
}
