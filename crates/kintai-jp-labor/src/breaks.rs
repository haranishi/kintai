//! 休憩の不足の警告（`docs/spec/punch.md` の「休憩の不足の警告」。AC-01.6・01.7）。
//!
//! 労働日ごとに、実労働（休憩を除く）の合計 L 分と、実際の休憩を比べる。必要な休憩は、
//! L ≤ 360 なら0分、360 < L ≤ 480 なら45分、480 < L なら60分（労働基準法第34条第1項）。
//! 8時間ちょうどは45分、8時間1分は60分で、60分に45分を足さない。
//! 実際の休憩は、勤務の内側の休憩と、同じ労働日の勤務と勤務の間の空きの合計。
//! 不足は警告だけで、休憩を自動で足し引きしない（勤務も区分の結果も変えない）。
//!
//! ```
//! use kintai_jp_labor::breaks::break_shortages;
//! use kintai_jp_labor::{ShiftInput, ShiftSet};
//!
//! // 9時から18時まで休憩なし（実労働540分）→ 60分の不足。
//! let shift = ShiftInput {
//!     start: "2026-10-07T09:00".parse().unwrap(),
//!     end: "2026-10-07T18:00".parse().unwrap(),
//!     breaks: Vec::new(),
//! };
//! let shortages = break_shortages(&ShiftSet::new(&[shift]).unwrap()).unwrap();
//! assert_eq!(shortages[0].shortage_minutes, 60);
//! ```

use std::fmt;

use kintai_core::Date;

use crate::classify::ClassifyError;
use crate::provenance::{CONFIRMED_ON, LABOR_STANDARDS_ACT, Provenance};
use crate::shift::ShiftSet;

/// 実労働がこの分を超えると、45分の休憩が要る（6時間）。
pub const BREAK_45_OVER_MINUTES: i64 = 360;
/// 実労働がこの分を超えると、60分の休憩が要る（8時間）。
pub const BREAK_60_OVER_MINUTES: i64 = 480;

/// 労働基準法第34条第1項（休憩）。
pub const LSA_34_1: Provenance = Provenance {
    law: LABOR_STANDARDS_ACT,
    article: "第34条第1項",
    summary: "労働時間が6時間を超える場合は少なくとも45分、8時間を超える場合は少なくとも1時間の休憩を、労働時間の途中に与える",
    confirmed_on: CONFIRMED_ON,
};

/// 実労働 `work_minutes` 分に要る休憩の分（0・45・60）。
#[must_use]
pub const fn required_break_minutes(work_minutes: i64) -> i64 {
    if work_minutes > BREAK_60_OVER_MINUTES {
        60
    } else if work_minutes > BREAK_45_OVER_MINUTES {
        45
    } else {
        0
    }
}

/// 休憩の不足の警告（1つの労働日）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BreakShortage {
    /// 労働日（始業の暦日）。
    pub date: Date,
    /// 実労働の合計（休憩を除く。分）。
    pub work_minutes: i64,
    /// 実際の休憩（勤務の内側の休憩と、同じ労働日の勤務と勤務の間の空き。分）。
    pub break_minutes: i64,
    /// 必要な休憩（分）。
    pub required_minutes: i64,
    /// 不足（`required_minutes − break_minutes`。1以上）。
    pub shortage_minutes: i64,
}

impl BreakShortage {
    /// 出力の `code` に出す文字列。
    pub const CODE: &'static str = "BreakShortage";
}

impl fmt::Display for BreakShortage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: 実労働{}分に対して休憩が{}分です（必要{}分、{}分不足。根拠: {}）",
            self.date,
            self.work_minutes,
            self.break_minutes,
            self.required_minutes,
            self.shortage_minutes,
            LSA_34_1.citation()
        )
    }
}

/// 労働日ごとに休憩の不足を調べ、不足のある日だけを日付順に返す（AC-01.6・01.7）。
///
/// 勤務と休憩は変えない（警告だけ）。
///
/// # Errors
///
/// 分の合計の桁あふれは [`ClassifyError::Overflow`]（検査済みの勤務では起きない）。
pub fn break_shortages(shifts: &ShiftSet) -> Result<Vec<BreakShortage>, ClassifyError> {
    let mut found = Vec::new();
    for group in shifts
        .shifts()
        .chunk_by(|a, b| a.labor_day() == b.labor_day())
    {
        let Some(first) = group.first() else {
            continue;
        };
        let mut work = 0_i64;
        let mut rest = 0_i64;
        for shift in group {
            work = add(work, shift.work_minutes())?;
            for inside in shift.breaks() {
                rest = add(rest, inside.len_minutes())?;
            }
        }
        // 同じ労働日の勤務と勤務の間の空き（勤務は始業の順で、重ならない）。
        for pair in group.windows(2) {
            if let [before, after] = pair {
                rest = add(
                    rest,
                    after.span().start().minutes_since(before.span().end()),
                )?;
            }
        }
        let required = required_break_minutes(work);
        if rest < required {
            found.push(BreakShortage {
                date: first.labor_day(),
                work_minutes: work,
                break_minutes: rest,
                required_minutes: required,
                shortage_minutes: required - rest,
            });
        }
    }
    Ok(found)
}

fn add(a: i64, b: i64) -> Result<i64, ClassifyError> {
    a.checked_add(b).ok_or(ClassifyError::Overflow)
}
