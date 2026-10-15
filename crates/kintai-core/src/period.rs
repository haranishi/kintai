//! 週と月の期間。
//!
//! どちらも、初日が入力の範囲の外（1999年12月や2100年1月）になることがある。
//! 例えば起算日16の月の期間で 2000-01-05 が属する期間の初日は 1999-12-16。
//! 初日は日数で持ち、[`Date`] へは型の範囲（1900〜2200年）を確かめて変える。表示（`YYYY-MM-DD`）はいつでもできる。

use std::fmt;

use crate::calendar::{civil_from_days, days_from_civil};
use crate::date::Date;
use crate::error::CoreError;
use crate::weekday::Weekday;

/// 週。起算日（任意の曜日）から7暦日。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Week {
    first_day_number: i64,
    start: Weekday,
}

impl Week {
    /// 日付 `date` が属する、起算日 `start` の週。
    #[must_use]
    pub fn containing(date: Date, start: Weekday) -> Week {
        let offset = i64::from(date.weekday().days_since(start));
        Week {
            first_day_number: date.day_number() - offset,
            start,
        }
    }

    /// 初日の日数（1970-01-01 を 0）。
    #[must_use]
    pub const fn first_day_number(self) -> i64 {
        self.first_day_number
    }

    /// 初日。
    ///
    /// # Errors
    ///
    /// 初日が型の範囲（1900-01-01〜2200-12-31）の外なら [`CoreError::OutOfRange`]。
    pub fn first_day(self) -> Result<Date, CoreError> {
        Date::from_day_number(self.first_day_number)
    }

    /// 起算の曜日。
    #[must_use]
    pub const fn start_weekday(self) -> Weekday {
        self.start
    }

    /// 日付 `date` がこの週に属するか。
    #[must_use]
    pub fn contains(self, date: Date) -> bool {
        let day = date.day_number();
        self.first_day_number <= day && day < self.first_day_number + 7
    }
}

impl fmt::Display for Week {
    /// 初日を `YYYY-MM-DD` で書く。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = civil_from_days(self.first_day_number);
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

/// 月の起算日（1〜28）。29日以降は、月によって存在しないので受け付けない。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonthStartDay(u32);

impl MonthStartDay {
    /// 毎月1日。
    pub const FIRST: MonthStartDay = MonthStartDay(1);

    /// 起算日を作る。
    ///
    /// # Errors
    ///
    /// 1〜28 の外なら [`CoreError::InvalidMonthStartDay`]。
    pub fn new(day: u32) -> Result<MonthStartDay, CoreError> {
        if (1..=28).contains(&day) {
            Ok(MonthStartDay(day))
        } else {
            Err(CoreError::InvalidMonthStartDay)
        }
    }

    /// 日（1〜28）。
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// 月の期間。起算日から1か月（次の月の起算日の前日まで）。
///
/// 並べ替えは、初日の年・月の順になる（同じ起算日どうしなら暦の順）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonthPeriod {
    year: i32,
    month: u32,
    start_day: MonthStartDay,
}

impl MonthPeriod {
    /// 日付 `date` が属する、起算日 `start_day` の月の期間。
    #[must_use]
    pub fn containing(date: Date, start_day: MonthStartDay) -> MonthPeriod {
        let (year, month) = if date.day() >= start_day.get() {
            (date.year(), date.month())
        } else if date.month() == 1 {
            (date.year() - 1, 12)
        } else {
            (date.year(), date.month() - 1)
        };
        MonthPeriod {
            year,
            month,
            start_day,
        }
    }

    /// 初日の年。
    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    /// 初日の月（1〜12）。
    #[must_use]
    pub const fn month(self) -> u32 {
        self.month
    }

    /// 起算日。
    #[must_use]
    pub const fn start_day(self) -> MonthStartDay {
        self.start_day
    }

    /// 初日の日数（1970-01-01 を 0）。
    #[must_use]
    pub fn first_day_number(self) -> i64 {
        days_from_civil(
            i64::from(self.year),
            i64::from(self.month),
            i64::from(self.start_day.get()),
        )
    }

    /// 次の期間の初日の日数（1970-01-01 を 0）。
    #[must_use]
    pub fn next_first_day_number(self) -> i64 {
        let (year, month) = if self.month == 12 {
            (i64::from(self.year) + 1, 1)
        } else {
            (i64::from(self.year), i64::from(self.month) + 1)
        };
        days_from_civil(year, month, i64::from(self.start_day.get()))
    }

    /// 初日。
    ///
    /// # Errors
    ///
    /// 初日が型の範囲（1900-01-01〜2200-12-31）の外なら [`CoreError::OutOfRange`]。
    pub fn first_day(self) -> Result<Date, CoreError> {
        Date::from_day_number(self.first_day_number())
    }

    /// 次の期間の初日。
    ///
    /// # Errors
    ///
    /// 次の期間の初日が型の範囲（1900-01-01〜2200-12-31）の外なら [`CoreError::OutOfRange`]。
    pub fn next_first_day(self) -> Result<Date, CoreError> {
        Date::from_day_number(self.next_first_day_number())
    }

    /// 日付 `date` がこの期間に属するか。
    #[must_use]
    pub fn contains(self, date: Date) -> bool {
        let day = date.day_number();
        self.first_day_number() <= day && day < self.next_first_day_number()
    }
}

impl fmt::Display for MonthPeriod {
    /// 初日を `YYYY-MM-DD` で書く（範囲の外でも書ける）。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}",
            self.year,
            self.month,
            self.start_day.get()
        )
    }
}
