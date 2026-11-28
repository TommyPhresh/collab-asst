//! Value types for literals in the config datafiles.
//!
//! | Type | Example Input | Meaning |
//! | --- | --- | --- |
//! | [`Duration`] | `45m`, `1h30m`, `2w` | non-negative length of time |
//! | [`TimeOfDay`] | `07:30` | clock time without date; 24-hr clock |
//! | [`DurationRange`] | `20-30m`, `45m-1h` | inclusive min..=max duration |
//! | [`WeeklyAt`] | `sun 08:45` | some time on some weekday every week |
//! | [`MonthDay`] | `Feb 17` | calendar date without year |
//!
//! # Design
//!
//! - **Parsing is in [`FromStr`].** Each type implements `TryFrom<String>`
//! and is decorated with `#[serde(try_from = "String")]`, so serde is the
//! YAML-independent parser for everything.
//! - **Invalid values cannot be constructed at all.** Type fields are private,
//! and can therefore only be 'reached' thru parsers or the validated constructors.
//! This means that a `DurationRange` existing proves its `min <= max` is true.
//! - **[`Display`](fmt::Display) prints canonical form.** Canonical form always
//! parses to same value. Non-canonical yet valid input parses fine but gets
//! converted to canonical form (i.e. `90m` goes to `1h30m`).
//! - **There is no date math anywhere.** [`TimeOfDay`] and [`MonthDay`] don't
//! have `+` or `-`. Crossing midnight needs dates, not just times, so some other
//! module will handle it.
//!
//! # Example
//!
//! ```
//! use collab_asst::config::units::{Duration, DurationRange, TimeOfDay};
//!
//! let d: Duration = "1h30m".parse().unwrap();
//! assert_eq!(d.minutes(), 90);
//! assert_eq!("90m".parse::<Duration>().unwrap().to_string(), "1h30m");
//!
//! let t: TimeOfDay = "07:30".parse().unwrap();
//! assert_eq!((t.hour(), t.minute()), (7,30));
//!
//! assert!("30-20m".parse::<DurationRange>().is_err());
//! ```

use std::fmt;
use std::str::FromStr;
use serde::Deserialize;
use thiserror::Error;

/// Reveals why unit literal failed to parse/construct.
///
/// Variants with a `String` carry the entire bad input.
/// Callers with context on input's source (i.e. config path or expression span)
/// add context themselves.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnitError {
    #[error("empty value")]
    Empty,
    #[error("expected a number in form {0:?}")]
    BadNumber(String),
    #[error("number is too large in {0:?}")]
    Overflow(String),
    #[error("unknown unit {0:?} (expected w,d,h,m)")]
    UnknownUnit(String),
    #[error("missing unit after number in {0:?}")]
    MissingUnit(String),
    #[error("units must be descending (i.e. w,d,h,m) and unique in {0:?}")]
    UnitOrder(String),
    #[error("expected HH:MM, got {0:?}")]
    BadTime(String),
    #[error("hour {0} out of range 0-23")]
    HourOutOfRange(String),
    #[error("minute {0} out of range 0-59")]
    MinuteOutOfRange(String),
    #[error("expected MIN-MAX with units, got {0:?}")]
    BadRange(String),
    #[error("range is inverted: {min} > {max}")]
    RangeInverted {min: Duration, max: Duration},
    #[error("unknown weekday {0:?}")]
    UnknownWeekday(String),
    #[error("expected \"<Weekday> HH:MM\", got {0:?}")]
    BadWeekly(String),
    #[error("unknown month {0:?}")]
    UnknownMonth(String),
    #[error("expected \"<Month> <day>\", got {0:?}")]
    BadMonthDay(String),
    #[error("{month} has no day {day}")]
    DayOutOfRange { month: Month, day: u32 },
}

/// Some non-negative length of time in minutes.
///
/// # Syntax
///
/// One or more `<number><unit>` components without spaces.
/// Units must be strictly descending and only appear once.
/// Units are `w`,`d`,`h`,`m` for weeks, days, hours, minutes respectively.
///
/// | Accepted | Rejected |
/// | --- | --- |
/// | `45m`, `8h`, `2w` | `45`, `45x` |
/// | `1h30m`, `1w2d3h4m` | `30m1h`, `1h1h` |
/// | `0m` | `-5m`, `1h 30m` `""` |
///
/// Total minutes must fit into a `u32`, otherwise [`UnitError::Overflow`].
///
/// Seconds are not supported because that's too anal.
///
/// # Example
/// 
/// ```
/// use collab_asst::config::units::Duration;
///
/// let d: Duration = "1w2d".parse().unwrap();
/// assert_eq!(d.minutes(), 9*24*60);
/// assert_eq!(d.to_string(), "1w2d");
/// assert!("30m1h".parse::<Duration>().is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct Duration(u32);

/// Duration units go largest to smallest.
///
/// Descending order is relied upon by parser and `Display`.
const UNITS: [(char,u32); 4] = [('w', 7*24*60), ('d', 24*60), ('h', 60), ('m', 1)];

impl Duration {
    pub const ZERO: Duration = Duration(0);
    /// Builds `Duration` from minute count only
    pub const fn from_minutes(minutes: u32) -> Self {
        Duration(minutes)
    }
    /// Total duration length in minutes
    pub const fn minutes(self) -> u32 {
        self.0
    }
}
/// Where in [`UNITS`] unit suffix is or `None`
fn unit_index(c: char) -> Option<usize> {
    UNITS.iter().position(|&(u,_)| u==c)
}

impl FromStr for Duration {
    type Err = UnitError;

    /// Parses `(DIGITS_UNIT)+` as long as units are strictly descending.
    ///
    /// # Errors
    ///
    /// [`Empty`](UnitError::Empty),
    /// [`BadNumber`](UnitError::BadNumber),
    /// [`MissingUnit`](UnitError::MissingUnit),
    /// [`UnknownUnit`](UnitError::UnknownUnit),
    /// [`UnitOrder`](UnitError::UnitOrder),
    /// [`Overflow`](UnitError::Overflow).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(UnitError::Empty);
        }
        let bytes = s.as_bytes();
        let mut i = 0;
        let mut total: u32 = 0;
        let mut last: Option<usize> = None;

        while i < bytes.len() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(UnitError::BadNumber(s.to_owned()));
            }
            let n: u32 = s[start..i]
                .parse()
                .map_err(|_| UnitError::Overflow(s.to_owned()))?;

            let Some(c) = s[i..].chars().next() else {
                return Err(UnitError::MissingUnit(s.to_owned()));
            };
            let idx = unit_index(c).ok_or(UnitError::UnknownUnit(c.to_string()))?;
            if last.is_some_and(|l| idx <= l) {
                return Err(UnitError::UnitOrder(s.to_owned()));
            }
            last = Some(idx);
            i += c.len_utf8();

            total = n
                .checked_mul(UNITS[idx].1)
                .and_then(|v| total.checked_add(v))
                .ok_or_else(|| UnitError::Overflow(s.to_owned()))?;
        }
        Ok(Duration(total))
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return f.write_str("0m");
        }
        let mut rest = self.0;
        for &(u, per) in &UNITS {
            let n = rest / per;
            if n > 0 {
                write!(f, "{n}{u}")?;
                rest %= per;
            }
        }
        Ok(())
    }
}

impl TryFrom<String> for Duration {
    type Error = UnitError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Clock time with no date, 24-hour format.
///
/// Stored as minutes since midnight in form `HH:MM`.
///
/// # No math
///
/// `TimeOfDay` has no `Add`/`Sub` capabilities implemented, these
/// are saved for a different module.
///
/// # Example
///
/// ```
/// use collab_asst::config::units::TimeOfDay;
///
/// let t: TimeOfDay = "03:45".parse().unwrap();
/// assert_eq!(t.minutes_since_midnight(), 225);
/// assert!("24:00".parse::<TimeOfDay>().is_err());
/// assert!("7:30".parse::<TimeOfDay>().is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct TimeOfDay(u16);

impl TimeOfDay {
    /// Creates time from an hour in (`0..=23`) and a minute in (`0..=59`).
    ///
    /// # Errors
    ///
    /// [`HourOutOfRange`](UnitError::HourOutOfRange),
    /// [`MinuteOutOfRange`](UnitError::MinuteOutOfRange).
    pub fn new(hour: u32, minute: u32) -> Result<Self, UnitError> {
        if hour > 23 {
            return Err(UnitError::HourOutOfRange(hour.to_string()));
        }
        if minute > 59 {
            return Err(UnitError::MinuteOutOfRange(minute.to_string()));
        }
        Ok(TimeOfDay((hour*60+minute) as u16))
    }

    pub const fn hour(self) -> u32 {
        self.0 as u32 / 60
    }

    pub const fn minute(self) -> u32 {
        self.0 as u32 % 60
    }

    pub const fn minutes_since_midnight(self) -> u32 {
        self.0 as u32
    }
}

impl FromStr for TimeOfDay {
    type Err = UnitError;

    /// Parses from form `HH:MM`.
    ///
    /// # Errors
    ///
    /// [`Empty`](UnitError::Empty),
    /// [`BadTime`](UnitError::BadTime) for bad shape,
    /// [`HourOutOfRange`](UnitError::HourOutOfRange) or
    /// [`MinuteOutOfRange`](UnitError::MinuteOutOfRange) for impossible values.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(UnitError::Empty);
        }
        let b = s.as_bytes();
        let ok = b.len() == 5
            && b[2] == b':'
            && [0,1,3,4].iter().all(|&i| b[i].is_ascii_digit());
        if !ok {
            return Err(UnitError::BadTime(s.to_owned()));
        }
        let digit = |i: usize| (b[i] - b'0') as u32;
        TimeOfDay::new(digit(0)*10 + digit(1), digit(3)*10 + digit(4))
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour(), self.minute())
    }
}

impl TryFrom<String> for TimeOfDay {
    type Error = UnitError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Inclusive range of durations `min..=max` with `min <= max` guaranteed.
///
/// # Syntax
///
/// One of two forms differing on the first `-`
///
/// - **Shared unit**: naked number, then one duration unit, i.e. `20-30m`.
/// Duration unit applies to both `min` and `max`.
/// - **Multi-unit**: `min` and `max` both have units, i.e. `45m-1h`.
///
/// Illegal variants are `20m-30` (unit must be at least on `max`),
/// `20-1h30m` (multi-unit on `max` requires unit on `min`),
/// `30-20m` (`min` must be `<= max`),
/// `-30m` (no negatives),
/// `20-` (need both `min` and `max`).
///
/// Side note: `30-30m` is LEGAL.
///
/// # Example
///
/// ```
/// use collab_asst::config::units::DurationRange;
///
/// let r: DurationRange = "45m-1h".parse().unwrap();
/// assert_eq!((r.min().minutes(), r.max().minutes()), (45,60));
/// assert_eq!(r.to_string(), "45-60m");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct DurationRange {
    min: Duration,
    max: Duration,
}

impl DurationRange {
    /// Constructs `DurationRange`.
    ///
    /// # Errors
    ///
    /// [`RangeInverted`](UnitError::RangeInverted)
    pub fn new(min: Duration, max: Duration) -> Result<Self, UnitError> {
        if min > max {
            return Err(UnitError::RangeInverted {min, max });
        }
        Ok(DurationRange {min, max })
    }

    pub const fn min(self) -> Duration {
        self.min
    }

    pub const fn max(self) -> Duration {
        self.max
    }
}

impl FromStr for DurationRange {
    type Err = UnitError;

    /// Parses either form of [`DurationRange`].
    ///
    /// # Errors
    ///
    /// [`Empty`](UnitError::Empty),
    /// [`BadRange`](UnitError::BadRange) for bad shape,
    /// any [`Duration`] error that propagates,
    /// [`RangeInverted`](UnitError::RangeInverted).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(UnitError::Empty);
        }
        let bad = || UnitError::BadRange(s.to_owned());
        let (lhs, rhs) = s.split_once('-').ok_or_else(bad)?;
        if lhs.is_empty() || rhs.is_empty() {
            return Err(bad());
        }
        let max: Duration = rhs.parse()?;
        let min: Duration = if lhs.bytes().all(|b| b.is_ascii_digit()) {
            let unit = rhs.chars().last().ok_or_else(bad)?;
            let rhs_digits = &rhs[..rhs.len() - unit.len_utf8()];
            if !rhs_digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            format!("{lhs}{unit}").parse()?
        } else {
            lhs.parse()?
        };
        DurationRange::new(min, max)
    }
}

impl fmt::Display for DurationRange {
    /// Canonical form is the one with the unit only on `max`.
    /// Which unit is determined as the largest unit both sides
    /// divied evenly into.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (a,b) = (self.min.minutes(), self.max.minutes());
        let &(u,per) = UNITS
            .iter()
            .find(|&&(u,per)| {
                let fits = |m: u32| m.is_multiple_of(per) && (m != 0 || u == 'm');
                fits(a) && fits(b)
            })
            .expect("minutes always divide");
        write!(f, "{}-{}{u}", a / per, b / per)
    }
}

impl TryFrom<String> for DurationRange {
    type Error = UnitError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Day of the week, but Monday is first (as is correct).
///
/// Parses case-insensitively from either 3-letter or full names.
/// Displays as lowercase 3-letter name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    const ALL: [(Weekday, &'static str, &'static str); 7] = [
        (Weekday::Mon, "mon", "monday"),
        (Weekday::Tue, "tue", "tuesday"),
        (Weekday::Wed, "wed", "wednesday"),
        (Weekday::Thu, "Tthu", "thursday"),
        (Weekday::Fri,  "fri", "friday"),
        (Weekday::Sat,  "sat", "saturday"),
        (Weekday::Sun,  "sun", "sunday"),
    ];

    fn short(self) -> &'static str {
        match self {
            Weekday::Mon => "mon",
            Weekday::Tue => "tue",
            Weekday::Wed => "wed",
            Weekday::Thu => "thu",
            Weekday::Fri => "fri",
            Weekday::Sat => "sat",
            Weekday::Sun => "sun",
        }
    }
}

impl FromStr for Weekday {
    type Err = UnitError;

    /// Case-insensitive, accepts either 3-letter or full names.
    ///
    /// # Errors
    ///
    /// [`UnknownWeekday`](UnitError::UnknownWeekday) for any invalid input.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_ascii_lowercase();
        Weekday::ALL
            .iter()
            .find(|(_, short, long)| lower == *short || lower == *long)
            .map(|&(d, _, _)| d)
            .ok_or_else(|| UnitError::UnknownWeekday(s.to_owned()))
    }
}

impl fmt::Display for Weekday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.short())
    }
}

/// A recurring weekly time. Consists of a [`Weekday`] and a [`TimeOfDay`].
///
/// # Syntax
///
/// `<Weekday> HH:MM`. Exactly one ASCII space as separator.
///
/// # Example
///
/// ```
/// use collab_asst::config::units::{Weekday, TimeOfDay};
///
/// let w: WeeklyAt = "sunday 08:45".parse().unwrap();
/// assert_eq!(w.day, Weekday::Sun);
/// assert_eq!(w.to_string(), "sun 08:45");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct WeeklyAt {
    pub day: Weekday,
    pub at: TimeOfDay,
}

impl FromStr for WeeklyAt {
    type Err = UnitError;

    /// Parses `<Weekday> HH:MM`.
    ///
    /// # Errors
    ///
    /// [`Empty`](UnitError::Empty),
    /// [`BadWeekly`](UnitError::BadWeekly) if no space,
    /// any [`Weekday`] or [`TimeOfDay`] parse error that propagates.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(UnitError::Empty);
        }
        let (day, at) = s
            .split_once(' ')
            .ok_or_else(|| UnitError::BadWeekly(s.to_owned()))?;
        Ok(WeeklyAt {
            day: day.parse()?,
            at: at.parse()?,
        })
    }
}

impl fmt::Display for WeeklyAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.day, self.at)
    }
}

impl TryFrom<String> for WeeklyAt {
    type Error = UnitError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// A month of the year.
///
/// Parses case-insensitively from either 3-letter or full name.
/// Canonical form is the capitalized 3-letter name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Month {
    Jan,
    Feb,
    Mar,
    Apr,
    May,
    Jun,
    Jul,
    Aug,
    Sep,
    Oct,
    Nov,
    Dec,
}

impl Month {
    const ALL: [(Month, &'static str, &'static str, u32); 12] = [
        (Month::Jan, "Jan", "january", 31),
        (Month::Feb, "Feb", "february", 29),
        (Month::Mar, "Mar", "march", 31),
        (Month::Apr, "Apr", "april", 30),
        (Month::May, "May", "may", 31),
        (Month::Jun, "Jun", "june", 30),
        (Month::Jul, "Jul", "july", 31),
        (Month::Aug, "Aug", "august", 31),
        (Month::Sep, "Sep", "september", 30),
        (Month::Oct, "Oct", "october", 31),
        (Month::Nov, "Nov", "november", 30),
        (Month::Dec, "Dec", "december", 31),
    ];

    fn entry(self) -> (Month, &'static str, &'static str, u32) {
        Month::ALL[self as usize]
    }

    pub fn max_days(self) -> u32 {
        self.entry().3
    }

    pub fn number(self) -> u32 {
        self as u32 + 1
    }
}

impl FromStr for Month {
    type Err = UnitError;

    /// Case-insensitive, accepting either 3-letter or full name.
    ///
    /// # Errors
    ///
    /// [`UnknownMonth`](UnitError::UnknownMonth) for an illegal name.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Month::ALL
            .iter()
            .find(|(_, short, long, _)| s.eq_ignore_ascii_case(short) || s.eq_ignore_ascii_case(long))
            .map(|&(m, ..)| m)
            .ok_or_else(|| UnitError::UnknownMonth(s.to_owned()))
    }
}

impl fmt::Display for Month {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.entry().1)
    }
}

/// A calendar date without year.
///
/// # Syntax
///
/// `<Month> <day>` with exactly one ASCII space and day as either 1 or 2 digits.
/// Day must exist in that month in *some* year (i.e. `Feb 29` parses).
/// Must be month-date format AND not include any date suffixes (like 'st').
///
/// # Ordering
///
/// Calendar order within one year is preserved. Ranges that cross the New Year
/// need [`MonthDay::within`] not `<=`.
///
/// # Leap Days
///
/// [`MonthDay::observed_in`] maps `Feb 29` to `Feb 28` in non-leap years.
///
/// # Example
///
/// ```
/// use collab_asst::config::units::MonthDay;
///
/// let bday: MonthDay = "Jan 2".parse().unwrap();
/// let from: MonthDay = "Dec 20".parse().unwrap();
/// let to: MonthDay = "Jan 5".parse().unwrap();
/// assert!(bday.within(from,to));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct MonthDay {
    month: Month,
    day: u8,
}

impl MonthDay {
    /// Constructs and validates `MonthDay`.
    ///
    /// # Errors
    ///
    /// [`DayOutOfRange`](UnitError::DayOutOfRange) for invalid days
    /// for the month in question.
    pub fn new(month: Month, day: u32) -> Result<Self, UnitError> {
        if day == 0 || day > month.max_days() {
            return Err(UnitError::DayOutOfRange {month, day});
        }
        Ok(MonthDay {month, day: day as u8 })
    }

    pub const fn month(self) -> Month {
        self.month
    }

    pub const fn day(self) -> u32 {
        self.day as u32
    }

    /// Converts `Feb 29` if not a leap year.
    pub fn observed_in(self, year: i32) -> MonthDay {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        if self.month == Month::Feb && self.day == 29 && !leap {
            MonthDay { month: Month::Feb, day: 28}
        } else {
            self
        }
    }

    /// Whether `self` is in `from...=to` inclusive.
    /// If `from >= to`, assumes year wrapping.
    pub fn within(self, from: MonthDay, to: MonthDay) -> bool {
        if from <= to {
            from <= self && self <= to
        } else {
          self >= from || self <= to
        }
    }
}

impl FromStr for MonthDay {
    type Err = UnitError;

    /// Parses `<Month> <day>`.
    ///
    /// # Errors
    ///
    /// [`Empty`](UnitError::Empty),
    /// [`BadMonthDay`](UnitError::BadMonthDay) if bad shape,
    /// [`UnknownMonth`](UnitError::UnknownMonth),
    /// [`DayOutOfRange`](UnitError::DayOutOfRange).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(UnitError::Empty);
        }
        let bad = || UnitError::BadMonthDay(s.to_owned());
        let (m,d) = s.split_once(' ').ok_or_else(bad)?;
        let month: Month = m.parse()?;
        if d.is_empty() || d.len() > 2 || !d.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        let day: u32 = d.parse().map_err(|_| bad())?;
        MonthDay::new(month, day)
    }
}

impl fmt::Display for MonthDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.month, self.day)
    }
}

impl TryFrom<String> for MonthDay {
    type Error = UnitError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn duration(s: &str) -> u32 {
        s.parse::<Duration>().unwrap().minutes()
    }

    #[test]
    fn duration_accepts() {
        assert_eq!(duration("45m"), 45);
        assert_eq!(duration("8h"), 480);
        assert_eq!(duration("1h30m"), 90);
        assert_eq!(duration("2w"), 20160);
        assert_eq!(duration("1w2d3h4m"), 10080 + 2880 + 180 + 4);
        assert_eq!(duration("0m"), 0);
    }

    #[test]
    fn duration_rejects() {
        use UnitError::*;
        let cases: &[(&str, fn(&UnitError) -> bool)] = &[
            ("", |e| matches!(e, Empty)),
            ("m", |e| matches!(e, BadNumber(_))),
            ("45", |e| matches!(e, MissingUnit(_))),
            ("45x", |e| matches!(e, UnknownUnit(u) if u == "x")),
            ("-5m", |e| matches!(e, BadNumber(_))),
            ("30m1h", |e| matches!(e, UnitOrder(_))),
            ("1h1h", |e| matches!(e, UnitOrder(_))),
            ("99999999999m", |e| matches!(e, Overflow(_))),
            ("429429495924h", |e| matches!(e, Overflow(_))),
            ("1h 30m", |e| matches!(e, BadNumber(_))),
        ];
        for (input, check) in cases {
            let err = input.parse::<Duration>().unwrap_err();
            assert!(check(&err), "{input:?} gave {err:?}");
        }
    }

    #[test]
    fn time_accepts() {
        let t = |s: &str| s.parse::<TimeOfDay>().unwrap().minutes_since_midnight();
        assert_eq!(t("00:00"), 0);
        assert_eq!(t("23:59"), 1439);
        assert_eq!(t("07:30"), 450);
        assert_eq!(t("03:45"), 225);
    }

    #[test]
    fn range_accepts() {
        let r = |s: &str| {
            let r = s.parse::<DurationRange>().unwrap();
            (r.min().minutes(), r.max().minutes())
        };
        assert_eq!(r("20-30m"), (20, 30));
        assert_eq!(r("2-3h"), (120, 180));
        assert_eq!(r("45m-1h"), (45, 60));
        assert_eq!(r("30-30m"), (30, 30));
        assert_eq!(r("1h-1h30m"), (60, 90));
    }

    #[test]
    fn range_rejects() {
        for s in ["", "30-20m", "20m-30", "-30m", "20-", "20-1h30m", "20m", "1h-30m"] {
            assert!(s.parse::<DurationRange>().is_err(), "{s:?} should fail");
        }
        assert!(matches!(
            "30-20m".parse::<DurationRange>(),
            Err(UnitError::RangeInverted { .. })
        ));
    }

    #[test]
    fn weekly_accepts_and_rejects() {
        let w: WeeklyAt = "Sun 08:45".parse().unwrap();
        assert_eq!(w.day, Weekday::Sun);
        assert_eq!(w.at.minutes_since_midnight(), 8*60+45);
        assert_eq!("sunday 08:45".parse::<WeeklyAt>().unwrap(), w);
        assert_eq!("SUN 08:45".parse::<WeeklyAt>().unwrap(), w);

        for s in ["", "Sum 08:45", "Sun", "Sun08:45", "Sun  08:45", "Sun 8:45"] {
            assert!(s.parse::<WeeklyAt>().is_err(), "{s:?} should fail");
        }
    }

    #[test]
    fn round_trips() {
        for s in ["45m", "8h", "1h30m", "2w", "1w2d3h4m", "0m"] {
            assert_eq!(s.parse::<Duration>().unwrap().to_string(), s);
        }
        assert_eq!("90m".parse::<Duration>().unwrap().to_string(), "1h30m");

        for s in ["00:00", "07:30", "23:59"] {
            assert_eq!(s.parse::<TimeOfDay>().unwrap().to_string(), s);
        }
        for s in ["20-30m", "2-3h", "0-0m", "1-25h"] {
            assert_eq!(s.parse::<DurationRange>().unwrap().to_string(), s);
        }
        assert_eq!("1h-1h30m".parse::<DurationRange>().unwrap().to_string(), "60-90m");
        let r: DurationRange = "45m-1h".parse().unwrap();
        assert_eq!(r.to_string().parse::<DurationRange>().unwrap(), r);
        assert_eq!("sunday 08:45".parse::<WeeklyAt>().unwrap().to_string(), "sun 08:45");
    }

    #[test]
    fn through_yaml() {
        #[derive(Debug, Deserialize)]
        struct Doc {
            d: Duration,
            t: TimeOfDay,
            r: DurationRange,
            w: WeeklyAt,
        }
        let doc: Doc = serde_yaml_ng::from_str(
            "d: 1h30m\nt: 07:30\nr: 20-30m\nw: Sun 08:45\n",
        )
        .unwrap();
        assert_eq!(doc.d.minutes(), 90);
        assert_eq!(doc.t.to_string(), "07:30");
        assert_eq!(doc.r.max().minutes(), 30);
        assert_eq!(doc.w.day, Weekday::Sun);

        let err = serde_yaml_ng::from_str::<Doc>("d: 30m1h\nt: 07:30\nr: 20-30m\nw: Sun 08:45\n")
            .unwrap_err();
        assert!(err.to_string().contains("descending"), "{err}");
    }
}

mod month_day_tests {
    use super::*;
    
    fn md(s: &str) -> MonthDay {
        s.parse().unwrap()    
    }

    #[test]
    fn accepts() {
        assert_eq!(md("Feb 17"), MonthDay::new(Month::Feb, 17).unwrap());
        assert_eq!(md("february 17"), md("Feb 17"));
        assert_eq!(md("FEB 07"), md("Feb 7"));
        assert_eq!(md("Feb 29").day(), 29);
        assert_eq!(md("Dec 31").month().number(), 12);
    }

    #[test]
    fn rejects() {
        for s in ["", "Feb", "17 Feb", "Feb 30", "Apr 31", "Feb 0", "Feb 100", "Fbr 17", "Feb17", "Feb -1", "Feb 1st"] {
            assert!(s.parse::<MonthDay>().is_err(), "{s:?} should fail");
        }
        assert!(matches!(
            "Feb 30".parse::<MonthDay>(),
            Err(UnitError::DayOutOfRange { month: Month::Feb, day: 30 })
        ));
    }

    #[test]
    fn leap_day() {
        assert_eq!(md("Feb 29").observed_in(2027), md("Feb 28"));
        assert_eq!(md("Feb 29").observed_in(2028), md("Feb 29"));
        assert_eq!(md("Feb 29").observed_in(2100), md("Feb 28"));
        assert_eq!(md("Feb 29").observed_in(2000), md("Feb 29"));
        assert_eq!(md("Mar 1").observed_in(2027), md("Mar 1"));
    }

    #[test]
    fn ordering_and_wrapping_ranges() {
        assert!(md("Jan 31") < md("Feb 1"));
        assert!(md("Mar 17").within(md("Mar 1"), md("Mar 31")));
        assert!(!md("Apr 1").within(md("Mar 1"), md("Mar 31")));
        let (from,to) = (md("Dec 20"), md("Jan 5"));
        for s in ["Dec 20", "Dec 25", "Jan 2", "Jan 5"] {
            assert!(md(s).within(from,to), "{s} should be in range");
        }
        for s in ["Dec 18", "Jan 6", "Jul 4"] {
            assert!(!md(s).within(from,to), "{s} should not be in range");
        }
    }

    #[test]
    fn round_trip_and_yaml() {
        for s in ["Feb 17", "Dec 25", "Feb 29"] {
            assert_eq!(md(s).to_string(), s);
        }
        assert_eq!(md("february 07").to_string(), "Feb 7");

        #[derive(Deserialize)]
        struct P {
            birthday: Option<MonthDay>,
        }
        let p: P = serde_yaml_ng::from_str("birthday: \"Mar 14\"").unwrap();
        assert_eq!(p.birthday, Some(md("Mar 14")));
        let p: P = serde_yaml_ng::from_str("{}").unwrap();
        assert_eq!(p.birthday, None);
    }
}

