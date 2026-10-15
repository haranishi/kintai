//! 時点（日本時間の壁時計の分）。

use std::fmt;
use std::str::FromStr;

use crate::date::{Date, number};
use crate::error::CoreError;

/// 1日の分数。
pub const MINUTES_PER_DAY: i64 = 1440;

/// 時点。1970-01-01 00:00（日本時間の壁時計）からの整数の分。
///
/// 日付が [`Date`] の型の範囲（1900-01-01〜2200-12-31）にある分だけを表す。
/// 入力として読む（[`FromStr`]）のは 2000-01-01T00:00〜2099-12-31T23:59 だけ。
/// 日本にはサマータイムがないので、1日は常に1440分。
/// 表記は `YYYY-MM-DDTHH:MM`（読むときは区切りに空白も使える）。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    minutes: i64,
}

impl Instant {
    /// 表せる最初の時点（1900-01-01T00:00）。
    pub const MIN: Instant = Instant {
        minutes: Date::MIN.day_number() * MINUTES_PER_DAY,
    };
    /// 表せる最後の時点（2200-12-31T23:59）。
    pub const MAX: Instant = Instant {
        minutes: Date::MAX.day_number() * MINUTES_PER_DAY + MINUTES_PER_DAY - 1,
    };
    /// 入力として受け付ける最初の時点（2000-01-01T00:00）。
    pub const INPUT_MIN: Instant = Instant {
        minutes: Date::INPUT_MIN.day_number() * MINUTES_PER_DAY,
    };
    /// 入力として受け付ける最後の時点（2099-12-31T23:59）。2100-01-01T00:00 は範囲外。
    pub const INPUT_MAX: Instant = Instant {
        minutes: Date::INPUT_MAX.day_number() * MINUTES_PER_DAY + MINUTES_PER_DAY - 1,
    };

    /// 日付と、その日の何分目か（0〜1439）から作る。
    ///
    /// # Errors
    ///
    /// `minute_of_day` が1440以上なら [`CoreError::InvalidTime`]。
    pub fn new(date: Date, minute_of_day: u32) -> Result<Instant, CoreError> {
        let minute = i64::from(minute_of_day);
        if minute >= MINUTES_PER_DAY {
            return Err(CoreError::InvalidTime);
        }
        Ok(Instant {
            minutes: date.day_number() * MINUTES_PER_DAY + minute,
        })
    }

    /// 日付と時・分から作る。
    ///
    /// # Errors
    ///
    /// 24:00 は [`CoreError::Hour24`]、ほかの範囲外（25時、60分など）は [`CoreError::InvalidTime`]。
    pub fn from_hm(date: Date, hour: u32, minute: u32) -> Result<Instant, CoreError> {
        if hour == 24 && minute == 0 {
            return Err(CoreError::Hour24);
        }
        if hour > 23 || minute > 59 {
            return Err(CoreError::InvalidTime);
        }
        Instant::new(date, hour * 60 + minute)
    }

    /// 1970-01-01 00:00 からの分から作る。
    ///
    /// # Errors
    ///
    /// 型の範囲（1900-01-01T00:00〜2200-12-31T23:59）の外なら [`CoreError::OutOfRange`]。
    pub fn from_minutes(minutes: i64) -> Result<Instant, CoreError> {
        if (Instant::MIN.minutes..=Instant::MAX.minutes).contains(&minutes) {
            Ok(Instant { minutes })
        } else {
            Err(CoreError::OutOfRange)
        }
    }

    /// 範囲を確かめ済みの分から作る（クレートの中だけで使う）。
    pub(crate) const fn from_minutes_unchecked(minutes: i64) -> Instant {
        Instant { minutes }
    }

    /// 1970-01-01 00:00 からの分。
    #[must_use]
    pub const fn minutes(self) -> i64 {
        self.minutes
    }

    /// 入力の文字列 `YYYY-MM-DDTHH:MM`（区切りは `T` か空白）を読む（入力用の入口。[`FromStr`] と同じ）。
    ///
    /// # Errors
    ///
    /// 形の誤りは [`CoreError::InvalidFormat`]、暦にない日付は [`CoreError::InvalidDate`]、
    /// 24:00 は [`CoreError::Hour24`]、25時・60分などは [`CoreError::InvalidTime`]、
    /// 日付が 2000-01-01〜2099-12-31 の外は [`CoreError::OutOfRange`]。
    pub fn parse(text: &str) -> Result<Instant, CoreError> {
        text.parse()
    }

    /// 入力として受け付ける範囲（2000-01-01T00:00〜2099-12-31T23:59）にあるか。
    #[must_use]
    pub const fn is_input_range(self) -> bool {
        Instant::INPUT_MIN.minutes <= self.minutes && self.minutes <= Instant::INPUT_MAX.minutes
    }

    /// この時点の日付。
    #[must_use]
    pub const fn date(self) -> Date {
        Date::from_day_number_unchecked(self.minutes.div_euclid(MINUTES_PER_DAY))
    }

    /// その日の何分目か（0〜1439）。
    #[must_use]
    pub fn minute_of_day(self) -> u32 {
        u32::try_from(self.minutes.rem_euclid(MINUTES_PER_DAY)).unwrap_or(0)
    }

    /// 時（0〜23）。
    #[must_use]
    pub fn hour(self) -> u32 {
        self.minute_of_day() / 60
    }

    /// 分（0〜59）。
    #[must_use]
    pub fn minute(self) -> u32 {
        self.minute_of_day() % 60
    }

    /// `delta` 分後（負なら前）の時点。
    ///
    /// # Errors
    ///
    /// 結果が範囲の外なら [`CoreError::OutOfRange`]（桁あふれも同じ）。
    pub fn checked_add_minutes(self, delta: i64) -> Result<Instant, CoreError> {
        let target = self
            .minutes
            .checked_add(delta)
            .ok_or(CoreError::OutOfRange)?;
        Instant::from_minutes(target)
    }

    /// `earlier` から何分後か（`earlier` の方が後なら負）。
    #[must_use]
    pub const fn minutes_since(self, earlier: Instant) -> i64 {
        self.minutes - earlier.minutes
    }
}

impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}T{:02}:{:02}", self.date(), self.hour(), self.minute())
    }
}

impl fmt::Debug for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Instant({self})")
    }
}

impl FromStr for Instant {
    type Err = CoreError;

    /// `YYYY-MM-DDTHH:MM` または `YYYY-MM-DD HH:MM` を読む。秒や前後の空白は受け付けない。
    /// 入力として、日付が 2000-01-01〜2099-12-31 の外なら [`CoreError::OutOfRange`]。
    fn from_str(s: &str) -> Result<Instant, CoreError> {
        let (date_part, time_part) = s.split_at_checked(10).ok_or(CoreError::InvalidFormat)?;
        let time: [u8; 6] = time_part
            .as_bytes()
            .try_into()
            .map_err(|_| CoreError::InvalidFormat)?;
        let [separator, h1, h2, b':', m1, m2] = time else {
            return Err(CoreError::InvalidFormat);
        };
        if separator != b'T' && separator != b' ' {
            return Err(CoreError::InvalidFormat);
        }
        let hour = number(&[h1, h2]).ok_or(CoreError::InvalidFormat)?;
        let minute = number(&[m1, m2]).ok_or(CoreError::InvalidFormat)?;
        let date: Date = date_part.parse()?;
        Instant::from_hm(date, hour, minute)
    }
}
