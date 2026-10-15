//! 日付・分・区間など、勤怠の計算に使う入出力を持たない部品。
//!
//! 日本にはサマータイムがないので、扱うのは暦の日付と、日本時間の壁時計の分だけです。
//! 外部の依存を持たず、ファイルも通信も扱いません。仕様は `docs/spec/core.md`。
//!
//! - [`Date`]: 暦日。入力として読むのは 2000-01-01〜2099-12-31、型は 1900〜2200年を持てる。表記は `YYYY-MM-DD`。
//! - [`Instant`]: 1970-01-01 00:00 からの整数の分。表記は `YYYY-MM-DDTHH:MM`。
//! - [`Interval`]: 半開区間 `[開始, 終了)`。交差、重なり、暦日の 0:00 での分割。
//! - [`Weekday`]・[`Week`]・[`MonthStartDay`]・[`MonthPeriod`]: 週と月の期間。
//! - [`punch`] は、打刻の列を勤務と休憩に対にする（第2周）。
//!
//! ```
//! use kintai_core::{Date, Instant, Interval, MonthStartDay, Weekday};
//!
//! let start: Instant = "2026-10-07T22:00".parse().unwrap();
//! let end: Instant = "2026-10-08T05:00".parse().unwrap();
//! let night = Interval::new(start, end).unwrap();
//! assert_eq!(night.len_minutes(), 420);
//! assert_eq!(night.split_at_midnight().len(), 2);
//!
//! let day: Date = "2026-10-07".parse().unwrap();
//! assert_eq!(day.weekday(), Weekday::Wednesday);
//! let period = day.month_period(MonthStartDay::new(16).unwrap());
//! assert_eq!(period.to_string(), "2026-09-16");
//! ```

mod calendar;
mod date;
mod error;
mod instant;
mod interval;
mod period;
pub mod punch;
mod weekday;

pub use calendar::{days_in_month, is_leap_year};
pub use date::Date;
pub use error::CoreError;
pub use instant::{Instant, MINUTES_PER_DAY};
pub use interval::Interval;
pub use period::{MonthPeriod, MonthStartDay, Week};
pub use weekday::Weekday;
