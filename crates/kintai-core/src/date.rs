//! 日付（型は 1900-01-01〜2200-12-31、入力として読むのは 2000-01-01〜2099-12-31）。

use std::fmt;
use std::str::FromStr;

use crate::calendar::{civil_from_days, days_from_civil, days_in_month};
use crate::error::CoreError;
use crate::period::{MonthPeriod, MonthStartDay, Week};
use crate::weekday::Weekday;

/// 1970-01-01 から 1900-01-01 までの日数（負）。
const MIN_DAY_NUMBER: i64 = -25_567;
/// 1970-01-01 から 2200-12-31 までの日数。
const MAX_DAY_NUMBER: i64 = 84_370;
/// 1970-01-01 から 2000-01-01 までの日数。
const INPUT_MIN_DAY_NUMBER: i64 = 10_957;
/// 1970-01-01 から 2099-12-31 までの日数。
const INPUT_MAX_DAY_NUMBER: i64 = 47_481;

/// 西暦の暦日。
///
/// 型は 1900-01-01〜2200-12-31 を持てる（年休の失効日のように、計算の結果が2099年を超えることがあるため）。
/// 入力として読む（[`FromStr`]）のは 2000-01-01〜2099-12-31 だけで、範囲外は拒否する。
/// 中身は 1970-01-01 を 0 とする日数で、並べ替えは暦の順になる。
/// 表記は `YYYY-MM-DD`（[`FromStr`] と [`fmt::Display`] が往復で一致する）。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    days: i64,
}

impl Date {
    /// 型が持てる最初の日（1900-01-01）。
    pub const MIN: Date = Date {
        days: MIN_DAY_NUMBER,
    };
    /// 型が持てる最後の日（2200-12-31）。
    pub const MAX: Date = Date {
        days: MAX_DAY_NUMBER,
    };
    /// 入力として受け付ける最初の日（2000-01-01）。
    pub const INPUT_MIN: Date = Date {
        days: INPUT_MIN_DAY_NUMBER,
    };
    /// 入力として受け付ける最後の日（2099-12-31）。
    pub const INPUT_MAX: Date = Date {
        days: INPUT_MAX_DAY_NUMBER,
    };

    /// 年・月・日から作る。
    ///
    /// # Errors
    ///
    /// 暦にない日付（2月30日、13月など）は [`CoreError::InvalidDate`]、
    /// 型の範囲（1900-01-01〜2200-12-31）の外は [`CoreError::OutOfRange`]。
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Date, CoreError> {
        let Some(last) = days_in_month(year, month) else {
            return Err(CoreError::InvalidDate);
        };
        if day == 0 || day > last {
            return Err(CoreError::InvalidDate);
        }
        Date::from_day_number(days_from_civil(
            i64::from(year),
            i64::from(month),
            i64::from(day),
        ))
    }

    /// 1970-01-01 を 0 とする日数から作る。
    ///
    /// # Errors
    ///
    /// 型の範囲（1900-01-01〜2200-12-31）の外は [`CoreError::OutOfRange`]。
    pub fn from_day_number(days: i64) -> Result<Date, CoreError> {
        if (MIN_DAY_NUMBER..=MAX_DAY_NUMBER).contains(&days) {
            Ok(Date { days })
        } else {
            Err(CoreError::OutOfRange)
        }
    }

    /// 範囲を確かめ済みの日数から作る（クレートの中だけで使う）。
    pub(crate) const fn from_day_number_unchecked(days: i64) -> Date {
        Date { days }
    }

    /// 1970-01-01 を 0 とする日数。
    #[must_use]
    pub const fn day_number(self) -> i64 {
        self.days
    }

    /// 入力の文字列 `YYYY-MM-DD` を読む（入力用の入口。[`FromStr`] と同じ）。
    ///
    /// # Errors
    ///
    /// 形の誤りは [`CoreError::InvalidFormat`]、暦にない日付は [`CoreError::InvalidDate`]、
    /// 2000-01-01〜2099-12-31 の外は [`CoreError::OutOfRange`]。
    pub fn parse(text: &str) -> Result<Date, CoreError> {
        text.parse()
    }

    /// 入力として受け付ける範囲（2000-01-01〜2099-12-31）にあるか。
    #[must_use]
    pub const fn is_input_range(self) -> bool {
        INPUT_MIN_DAY_NUMBER <= self.days && self.days <= INPUT_MAX_DAY_NUMBER
    }

    /// 年・月・日。型の範囲（1900〜2200年）の中では、どの値も型に収まる。
    fn ymd(self) -> (i32, u32, u32) {
        let (y, m, d) = civil_from_days(self.days);
        (
            i32::try_from(y).unwrap_or(i32::MIN),
            u32::try_from(m).unwrap_or(0),
            u32::try_from(d).unwrap_or(0),
        )
    }

    /// 西暦の年（1900〜2200）。
    #[must_use]
    pub fn year(self) -> i32 {
        self.ymd().0
    }

    /// 月（1〜12）。
    #[must_use]
    pub fn month(self) -> u32 {
        self.ymd().1
    }

    /// 日（1〜31）。
    #[must_use]
    pub fn day(self) -> u32 {
        self.ymd().2
    }

    /// 曜日。
    #[must_use]
    pub const fn weekday(self) -> Weekday {
        Weekday::from_day_number(self.days)
    }

    /// その月の日数（月末の日）。
    #[must_use]
    pub fn days_in_month(self) -> u32 {
        let (year, month, _) = self.ymd();
        days_in_month(year, month).unwrap_or(0)
    }

    /// `days` 日後（負なら前）の日付。
    ///
    /// # Errors
    ///
    /// 結果が型の範囲（1900-01-01〜2200-12-31）の外なら [`CoreError::OutOfRange`]。
    pub fn checked_add_days(self, days: i64) -> Result<Date, CoreError> {
        let target = self.days.checked_add(days).ok_or(CoreError::OutOfRange)?;
        Date::from_day_number(target)
    }

    /// `earlier` から何日後か（`earlier` の方が後なら負）。
    #[must_use]
    pub const fn days_since(self, earlier: Date) -> i64 {
        self.days - earlier.days
    }

    /// この日付が属する週（起算日 `start` から7暦日）。
    #[must_use]
    pub fn week(self, start: Weekday) -> Week {
        Week::containing(self, start)
    }

    /// この日付が属する月の期間（起算日 `start_day` から1か月）。
    #[must_use]
    pub fn month_period(self, start_day: MonthStartDay) -> MonthPeriod {
        MonthPeriod::containing(self, start_day)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Date({self})")
    }
}

impl FromStr for Date {
    type Err = CoreError;

    /// `YYYY-MM-DD` を読む。前後の空白や、桁の違う表記は受け付けない。
    /// 入力として、2000-01-01〜2099-12-31 の外は [`CoreError::OutOfRange`]。
    fn from_str(s: &str) -> Result<Date, CoreError> {
        let bytes: [u8; 10] = s
            .as_bytes()
            .try_into()
            .map_err(|_| CoreError::InvalidFormat)?;
        let [y1, y2, y3, y4, b'-', m1, m2, b'-', d1, d2] = bytes else {
            return Err(CoreError::InvalidFormat);
        };
        let year = number(&[y1, y2, y3, y4]).ok_or(CoreError::InvalidFormat)?;
        let month = number(&[m1, m2]).ok_or(CoreError::InvalidFormat)?;
        let day = number(&[d1, d2]).ok_or(CoreError::InvalidFormat)?;
        let year = i32::try_from(year).map_err(|_| CoreError::InvalidFormat)?;
        let date = Date::from_ymd(year, month, day)?;
        if date.is_input_range() {
            Ok(date)
        } else {
            Err(CoreError::OutOfRange)
        }
    }
}

/// ASCII の数字の並びを読む（4桁まで）。数字でない文字があれば `None`。
pub(crate) fn number(digits: &[u8]) -> Option<u32> {
    digits.iter().try_fold(0_u32, |acc, &b| {
        if b.is_ascii_digit() {
            Some(acc * 10 + u32::from(b - b'0'))
        } else {
            None
        }
    })
}
