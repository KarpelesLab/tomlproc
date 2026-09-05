//! TOML's four date and time types.
//!
//! TOML distinguishes an offset date-time from a local date-time, a local date
//! and a local time. All four are represented by a single [`Datetime`] struct
//! whose optional fields say which of the four it is; [`Datetime::kind`]
//! reports that directly.
//!
//! ```
//! use tomlproc::{Datetime, DatetimeKind};
//!
//! let dt: Datetime = "1979-05-27T07:32:00Z".parse().unwrap();
//! assert_eq!(dt.kind(), DatetimeKind::OffsetDatetime);
//! assert_eq!(dt.to_string(), "1979-05-27T07:32:00Z");
//! ```

use core::fmt;
use core::str::FromStr;

use crate::error::Error;

/// A calendar date: year, month and day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    /// The year, from 0 to 9999.
    pub year: u16,
    /// The month, from 1 to 12.
    pub month: u8,
    /// The day of the month, from 1 to the number of days in that month.
    pub day: u8,
}

impl Date {
    /// Builds a date, returning `None` if it does not exist in the proleptic
    /// Gregorian calendar (the calendar TOML dates are defined against).
    ///
    /// ```
    /// use tomlproc::Date;
    ///
    /// assert!(Date::new(2024, 2, 29).is_some());
    /// assert!(Date::new(2023, 2, 29).is_none());
    /// ```
    pub fn new(year: u16, month: u8, day: u8) -> Option<Date> {
        if year > 9999 {
            return None;
        }
        let max = Date::days_in_month(year, month)?;
        if day == 0 || day > max {
            return None;
        }
        Some(Date { year, month, day })
    }

    /// Whether `year` is a leap year in the proleptic Gregorian calendar.
    pub fn is_leap_year(year: u16) -> bool {
        (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
    }

    /// The number of days in `month` of `year`, or `None` if `month` is not in
    /// the range 1 to 12.
    pub fn days_in_month(year: u16, month: u8) -> Option<u8> {
        Some(match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if Date::is_leap_year(year) => 29,
            2 => 28,
            _ => return None,
        })
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// A time of day, with nanosecond precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time {
    /// The hour, from 0 to 23.
    pub hour: u8,
    /// The minute, from 0 to 59.
    pub minute: u8,
    /// The second, from 0 to 60 (60 being a leap second).
    pub second: u8,
    /// The sub-second part, in nanoseconds, from 0 to 999_999_999.
    pub nanosecond: u32,
}

impl Time {
    /// Builds a time, returning `None` if any component is out of range.
    ///
    /// A `second` of 60 is accepted: RFC 3339, which TOML's date-times follow,
    /// allows a leap second.
    pub fn new(hour: u8, minute: u8, second: u8, nanosecond: u32) -> Option<Time> {
        if hour > 23 || minute > 59 || second > 60 || nanosecond > 999_999_999 {
            return None;
        }
        Some(Time {
            hour,
            minute,
            second,
            nanosecond,
        })
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)?;
        if self.nanosecond != 0 {
            // Rendered digit by digit into a stack buffer: this runs with no
            // allocator, so there is no string to format into.
            let mut digits = [b'0'; 9];
            let mut rest = self.nanosecond;
            for slot in digits.iter_mut().rev() {
                *slot = b'0' + (rest % 10) as u8;
                rest /= 10;
            }
            let end = digits
                .iter()
                .rposition(|digit| *digit != b'0')
                .map_or(0, |last| last + 1);
            f.write_str(".")?;
            f.write_str(core::str::from_utf8(&digits[..end]).expect("ASCII digits"))?;
        }
        Ok(())
    }
}

/// The time zone offset of an offset date-time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Offset {
    /// UTC, written `Z`.
    Z,
    /// A fixed offset from UTC, in minutes, from -1439 to 1439.
    Custom {
        /// Minutes east of UTC; negative values are west of it.
        minutes: i16,
    },
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Offset::Z => f.write_str("Z"),
            Offset::Custom { minutes } => {
                let (sign, minutes) = if *minutes < 0 {
                    ('-', -*minutes)
                } else {
                    ('+', *minutes)
                };
                write!(f, "{}{:02}:{:02}", sign, minutes / 60, minutes % 60)
            }
        }
    }
}

/// Which of TOML's four date-time types a [`Datetime`] holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DatetimeKind {
    /// A date, a time and an offset: `1979-05-27T07:32:00Z`.
    OffsetDatetime,
    /// A date and a time, with no offset: `1979-05-27T07:32:00`.
    LocalDatetime,
    /// A date on its own: `1979-05-27`.
    LocalDate,
    /// A time on its own: `07:32:00`.
    LocalTime,
}

/// A TOML date-time: an offset date-time, a local date-time, a local date or a
/// local time, depending on which fields are present.
///
/// The four valid shapes are exactly the ones the constructors below produce;
/// building one field-by-field with an unusable combination (an offset with no
/// time, say) is possible but will not round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Datetime {
    /// The date part, absent for a local time.
    pub date: Option<Date>,
    /// The time part, absent for a local date.
    pub time: Option<Time>,
    /// The time zone offset, present only for an offset date-time.
    pub offset: Option<Offset>,
}

impl Datetime {
    /// An offset date-time, such as `1979-05-27T07:32:00Z`.
    pub fn offset_datetime(date: Date, time: Time, offset: Offset) -> Datetime {
        Datetime {
            date: Some(date),
            time: Some(time),
            offset: Some(offset),
        }
    }

    /// A local date-time, such as `1979-05-27T07:32:00`.
    pub fn local_datetime(date: Date, time: Time) -> Datetime {
        Datetime {
            date: Some(date),
            time: Some(time),
            offset: None,
        }
    }

    /// A local date, such as `1979-05-27`.
    pub fn local_date(date: Date) -> Datetime {
        Datetime {
            date: Some(date),
            time: None,
            offset: None,
        }
    }

    /// A local time, such as `07:32:00`.
    pub fn local_time(time: Time) -> Datetime {
        Datetime {
            date: None,
            time: Some(time),
            offset: None,
        }
    }

    /// Which of the four TOML date-time types this value is.
    ///
    /// A value with neither a date nor a time — which the parser never produces
    /// — reports [`DatetimeKind::LocalDate`].
    pub fn kind(&self) -> DatetimeKind {
        match (
            self.date.is_some(),
            self.time.is_some(),
            self.offset.is_some(),
        ) {
            (true, true, true) => DatetimeKind::OffsetDatetime,
            (true, true, false) => DatetimeKind::LocalDatetime,
            (false, true, _) => DatetimeKind::LocalTime,
            _ => DatetimeKind::LocalDate,
        }
    }
}

impl fmt::Display for Datetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(date) = &self.date {
            write!(f, "{date}")?;
            if self.time.is_some() {
                f.write_str("T")?;
            }
        }
        if let Some(time) = &self.time {
            write!(f, "{time}")?;
        }
        if let Some(offset) = &self.offset {
            write!(f, "{offset}")?;
        }
        Ok(())
    }
}

impl From<Date> for Datetime {
    fn from(date: Date) -> Datetime {
        Datetime::local_date(date)
    }
}

impl From<Time> for Datetime {
    fn from(time: Time) -> Datetime {
        Datetime::local_time(time)
    }
}

impl FromStr for Datetime {
    type Err = Error;

    fn from_str(s: &str) -> Result<Datetime, Error> {
        match scan(s.as_bytes()) {
            Ok(Some((dt, used))) if used == s.len() => Ok(dt),
            Ok(Some(_)) => Err(Error::fixed("trailing data after date-time")),
            Ok(None) => Err(Error::fixed("expected a date-time")),
            Err(message) => Err(Error::fixed(message)),
        }
    }
}

fn all_digits(b: &[u8]) -> bool {
    b.iter().all(u8::is_ascii_digit)
}

fn num(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |acc, c| acc * 10 + u32::from(c - b'0'))
}

/// Whether `b` starts with something shaped like `HH:`, used to decide whether
/// the space in `1979-05-27 07:32:00` separates a date from a time or ends the
/// value.
fn looks_like_time(b: &[u8]) -> bool {
    b.len() >= 3 && b[2] == b':' && all_digits(&b[..2])
}

/// Parses `HH:MM[:SS[.fraction]]`, returning the time and the bytes consumed.
///
/// Seconds have been optional since TOML 1.1; a time without them is on the
/// minute.
fn scan_time(b: &[u8]) -> Result<(Time, usize), &'static str> {
    if b.len() < 5 || b[2] != b':' || !all_digits(&b[..2]) || !all_digits(&b[3..5]) {
        return Err("expected a time of the form HH:MM");
    }
    let (hour, minute) = (num(&b[..2]) as u8, num(&b[3..5]) as u8);
    let mut i = 5;
    let mut second = 0;
    let mut nanosecond = 0u32;
    let has_seconds = b.get(i) == Some(&b':');
    if has_seconds {
        if b.len() < i + 3 || !all_digits(&b[i + 1..i + 3]) {
            return Err("expected two digits of seconds");
        }
        second = num(&b[i + 1..i + 3]) as u8;
        i += 3;
    }
    // A fractional part belongs to the seconds, so it needs them.
    if has_seconds && b.get(i) == Some(&b'.') {
        let start = i + 1;
        let mut end = start;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        if end == start {
            return Err("expected at least one digit after the decimal point");
        }
        // Digits past nanosecond precision are truncated, as the TOML spec
        // allows: "truncation is implementation-specific".
        for (idx, c) in b[start..end].iter().take(9).enumerate() {
            nanosecond += u32::from(c - b'0') * 10u32.pow(8 - idx as u32);
        }
        i = end;
    }
    let time = Time::new(hour, minute, second, nanosecond).ok_or("time is out of range")?;
    Ok((time, i))
}

/// Scans a date-time at the start of `b`.
///
/// Returns `Ok(None)` when `b` does not begin with something shaped like a
/// date-time (so the caller can try to parse a number instead), and `Err` when
/// it does but is malformed or out of range.
pub(crate) fn scan(b: &[u8]) -> Result<Option<(Datetime, usize)>, &'static str> {
    if b.len() >= 5 && b[4] == b'-' && all_digits(&b[..4]) {
        if b.len() < 10 || b[7] != b'-' || !all_digits(&b[5..7]) || !all_digits(&b[8..10]) {
            return Err("expected a date of the form YYYY-MM-DD");
        }
        let date = Date::new(
            num(&b[..4]) as u16,
            num(&b[5..7]) as u8,
            num(&b[8..10]) as u8,
        )
        .ok_or("date is out of range")?;
        let mut i = 10;
        match b.get(i) {
            Some(b'T' | b't') => i += 1,
            Some(b' ') if looks_like_time(&b[i + 1..]) => i += 1,
            // A bare date; anything else here terminates the value.
            _ => return Ok(Some((Datetime::local_date(date), i))),
        }
        let (time, used) = scan_time(&b[i..])?;
        i += used;
        let offset = match b.get(i) {
            Some(b'Z' | b'z') => {
                i += 1;
                Some(Offset::Z)
            }
            Some(c @ (b'+' | b'-')) => {
                let negative = *c == b'-';
                if b.len() < i + 6
                    || b[i + 3] != b':'
                    || !all_digits(&b[i + 1..i + 3])
                    || !all_digits(&b[i + 4..i + 6])
                {
                    return Err("expected a time offset of the form +HH:MM");
                }
                let (hours, minutes) = (num(&b[i + 1..i + 3]), num(&b[i + 4..i + 6]));
                if hours > 23 || minutes > 59 {
                    return Err("time offset is out of range");
                }
                i += 6;
                let total = (hours * 60 + minutes) as i16;
                Some(Offset::Custom {
                    minutes: if negative { -total } else { total },
                })
            }
            _ => None,
        };
        Ok(Some((
            Datetime {
                date: Some(date),
                time: Some(time),
                offset,
            },
            i,
        )))
    } else if b.len() >= 3 && b[2] == b':' && all_digits(&b[..2]) {
        let (time, used) = scan_time(b)?;
        Ok(Some((Datetime::local_time(time), used)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_date_stops_before_a_non_time() {
        // The space must not be eaten when what follows is not a time.
        let (dt, used) = scan(b"1979-05-27 , 1").unwrap().unwrap();
        assert_eq!(used, 10);
        assert_eq!(dt.kind(), DatetimeKind::LocalDate);
    }

    #[test]
    fn not_a_datetime() {
        assert!(scan(b"1234").unwrap().is_none());
        assert!(scan(b"1.5e3").unwrap().is_none());
        assert!(scan(b"-17").unwrap().is_none());
    }

    #[test]
    fn ranges_are_checked() {
        assert!(Date::new(2024, 2, 29).is_some());
        assert!(Date::new(2023, 2, 29).is_none());
        assert!(Date::new(2023, 13, 1).is_none());
        assert!(Date::new(2023, 1, 0).is_none());

        // A leap second is allowed; the next one is not.
        assert!(Time::new(23, 59, 60, 0).is_some());
        assert!(Time::new(23, 59, 61, 0).is_none());
        assert!(Time::new(24, 0, 0, 0).is_none());
        assert!(Time::new(0, 0, 0, 1_000_000_000).is_none());
    }

    /// Everything here runs without an allocator, so the parse side is checked
    /// without asking for one.
    #[test]
    fn parses_without_an_allocator() {
        for s in [
            "1979-05-27T07:32:00Z",
            "1979-05-27",
            "07:32",
            "2010-02-03 14:15",
        ] {
            assert!(s.parse::<Datetime>().is_ok(), "{s}");
        }
        for s in ["1979-05-32", "07:32.5", "not a date"] {
            assert!(s.parse::<Datetime>().is_err(), "{s}");
        }
    }
}
