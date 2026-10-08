//! What an entry's timestamp says: a point in time, or only a time of day.
//!
//! The presets write their timestamps in a few fixed-width shapes, so they are read
//! with a small byte reader rather than jiff's general parser, which also accepts
//! shapes no preset writes (a date only, `HH:mm`, `+0200`, `:60` read as `:59`, and
//! an offset that `parse_datetime` silently drops) and takes about twice as long.

use jiff::Timestamp;
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{Offset, TimeZone};

#[cfg(test)]
mod tests;

/// A parsed entry timestamp, to full nanosecond precision.
///
/// - `Instant`: the timestamp has a date. An explicit offset in it (`+02:00`, `Z`)
///   decides which instant it is; without one, the time zone the caller gives.
/// - `TimeOfDay`: the preset only writes the time (`LogbackBasic`,
///   `WildflyConsole`). Which day it belongs to is worked out from the file later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogTime {
    Instant(jiff::Timestamp),
    TimeOfDay(jiff::civil::Time),
}

/// `HH:mm:ss.f`.
pub(super) fn time_of_day(span: &[u8]) -> Option<Time> {
    read_whole(span, Reader::time)
}

/// `yyyy-MM-dd HH:mm:ss.f`, local time in `zone`.
pub(super) fn local_instant(span: &[u8], zone: &TimeZone) -> Option<Timestamp> {
    let date_time = read_whole(span, |reader| reader.date_time(b' '))?;
    local_to_instant(date_time, zone)
}

/// `date_time` read as local time in `zone`. In a DST gap it takes the offset from
/// before the gap, in a fold the earlier instant. `None` only at the ends of jiff's
/// range.
pub(super) fn local_to_instant(date_time: DateTime, zone: &TimeZone) -> Option<Timestamp> {
    zone.to_ambiguous_timestamp(date_time).compatible().ok()
}

/// `yyyy-MM-dd`.
pub(super) fn date(span: &[u8]) -> Option<Date> {
    read_whole(span, Reader::date)
}

/// `yyyy-MM-ddTHH:mm:ss.f` followed by `Z` or `±hh:mm`.
pub(super) fn instant_with_offset(span: &[u8]) -> Option<Timestamp> {
    let (date_time, offset) = read_whole(span, |reader| {
        Some((reader.date_time(b'T')?, reader.offset()?))
    })?;
    offset.to_timestamp(date_time).ok()
}

/// What `read` makes of `span`, as long as it reads all of it.
fn read_whole<'a, T>(span: &'a [u8], read: impl FnOnce(&mut Reader<'a>) -> Option<T>) -> Option<T> {
    let mut reader = Reader(span);
    let value = read(&mut reader)?;
    reader.0.is_empty().then_some(value)
}

/// The bytes of a timestamp that haven't been read yet. Every method reads one part
/// from the front and gives `None` if it isn't there or is out of range.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn date_time(&mut self, separator: u8) -> Option<DateTime> {
        let date = self.date()?;
        self.byte(&[separator])?;
        Some(date.to_datetime(self.time()?))
    }

    fn date(&mut self) -> Option<Date> {
        let year = self.digits::<4>()?;
        self.byte(b"-")?;
        let month = self.two_digits()?;
        self.byte(b"-")?;
        let day = self.two_digits()?;
        Date::new(year, month, day).ok()
    }

    fn time(&mut self) -> Option<Time> {
        let hour = self.two_digits()?;
        self.byte(b":")?;
        let minute = self.two_digits()?;
        self.byte(b":")?;
        let second = self.two_digits()?;
        self.byte(b".,")?;
        let nanosecond = self.fraction()?;
        Time::new(hour, minute, second, nanosecond).ok()
    }

    fn offset(&mut self) -> Option<Offset> {
        let sign = match self.byte(b"Z+-")? {
            b'Z' => return Some(Offset::UTC),
            b'-' => -1,
            _ => 1,
        };
        let hours = i32::from(self.two_digits()?);
        self.byte(b":")?;
        let minutes = i32::from(self.two_digits()?);
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
