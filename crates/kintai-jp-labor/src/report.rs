//! 月次の集計（賃金台帳向けの勤怠の項目）と保存期限の目安（`docs/spec/report.md`。AC-08.1〜08.6）。
//!
//! - 台帳の項目（[`ledger`] と [`ledger_for`]）。区分の結果（[`Classification`]）から、月の期間ごとの
//!   項目（[`LedgerRow`]）を作る。賃金の額や氏名は含まない。
//! - 丸め（[`Rounding`]）。時間外・休日・深夜の月の合計の丸めで、`none` と `monthly_30min` だけ。
//!   日ごとの丸めは値として存在しない（AC-08.4）。元の分は変えず、丸めた時間を別に出す。
//! - 出力（[`MonthlyReport`]）。従業員ごとの台帳の項目・休憩の不足・曖昧さの記録に、必ず
//!   `table_checked_on`・`review_status`・決まった注意（[`Notice`]）を付ける（AC-08.6）。
//! - 保存期限の目安（[`retention`]。`kintai retention`）。起算日は種類ごとの「遅い方」（AC-08.5）。
//!   自動では削除しない。
//!
//! ```
//! use kintai_core::Weekday;
//! use kintai_jp_labor::report::{Rounding, ledger};
//! use kintai_jp_labor::{ShiftInput, ShiftSet, WorkPolicy, WorkSystem, classify};
//!
//! let policy = WorkPolicy {
//!     work_system: WorkSystem::Standard,
//!     scheduled_minutes_per_day: 480,
//!     week_start: None,
//!     weekly_limit_minutes: None,
//!     special_44h_confirmed: None,
//!     legal_holiday_weekday: Some(Weekday::Sunday),
//!     month_start_day: 1,
//!     ambiguity: None,
//!     worker_kind: None,
//!     holiday_system: None,
//!     holiday_swaps: 0,
//!     night_window: None,
//! }
//! .validate()
//! .unwrap();
//! // 水曜 9:00〜18:30（休憩なし）: 法定内480分、時間外90分。
//! let shift = ShiftInput {
//!     start: "2026-10-07T09:00".parse().unwrap(),
//!     end: "2026-10-07T18:30".parse().unwrap(),
//!     breaks: Vec::new(),
//! };
//! let result = classify(&policy, &ShiftSet::new(&[shift]).unwrap(), None).unwrap();
//! let rows = ledger(&policy, &result, Rounding::Monthly30Min).unwrap();
//! assert_eq!((rows[0].work_days, rows[0].work_minutes, rows[0].overtime_minutes), (1, 570, 90));
//! assert_eq!(rows[0].rounded.unwrap().overtime, 2); // 1時間30分 → 2時間
//! ```

use std::collections::BTreeSet;
use std::fmt;

use kintai_core::punch::EmployeeId;
use kintai_core::{Date, MonthPeriod};

use crate::breaks::{BreakShortage, break_shortages};
use crate::classify::{Ambiguity, Classification, ClassifyError, DayResult, RecordDate, Refusal};
use crate::policy::ValidPolicy;
use crate::provenance::{
    CONFIRMED_ON, LABOR_STANDARDS_ACT, LABOR_STANDARDS_REGULATION, Law, Provenance,
};
use crate::shift::ShiftSet;

/// 表（法令の根拠の定数）の最終確認日。出力の `table_checked_on`。
pub const TABLE_CHECKED_ON: &str = CONFIRMED_ON;

/// 専門家の確認の状態（出力の `review_status`）。この版はすべて確認前。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReviewStatus {
    /// 専門家（社会保険労務士・弁護士など）の確認前（`unreviewed`）。
    Unreviewed,
}

impl ReviewStatus {
    /// 出力に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            ReviewStatus::Unreviewed => "unreviewed",
        }
    }
}

/// 計算に使わなかった事情の、決まった注意（出力の `notices`）。`report` `check` `leave` `explain` に必ず付ける。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Notice {
    /// 休日の振替を考慮していない。
    NoHolidaySwap,
    /// 他の事業場・他社の労働時間を通算していない（労働基準法第38条第1項）。
    NoCrossEmployerAggregation,
    /// 災害等の臨時の必要による労働を考慮していない（労働基準法第33条）。
    NoEmergencyWork,
    /// 週の上限を超えた分を、後ろの時間に割り当てるのは製品の規則。
    WeeklyAssignmentIsProductRule,
}

impl Notice {
    /// 4つの注意（出力の順）。
    pub const ALL: [Notice; 4] = [
        Notice::NoHolidaySwap,
        Notice::NoCrossEmployerAggregation,
        Notice::NoEmergencyWork,
        Notice::WeeklyAssignmentIsProductRule,
    ];

    /// 出力の `code` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Notice::NoHolidaySwap => "NoHolidaySwap",
            Notice::NoCrossEmployerAggregation => "NoCrossEmployerAggregation",
            Notice::NoEmergencyWork => "NoEmergencyWork",
            Notice::WeeklyAssignmentIsProductRule => "WeeklyAssignmentIsProductRule",
        }
    }

    /// 人が読む文。
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Notice::NoHolidaySwap => {
                "休日の振替を考慮していません（振替の指定がないものとして計算しています）"
            }
            Notice::NoCrossEmployerAggregation => {
                "他の事業場・他社の労働時間を通算していません（労働基準法第38条第1項）"
            }
            Notice::NoEmergencyWork => {
                "災害等の臨時の必要による時間外・休日労働を考慮していません（労働基準法第33条）"
            }
            Notice::WeeklyAssignmentIsProductRule => {
                "週の上限を超えた分を、その週の後ろの時間に割り当てるのは、この製品の規則です（割り当てを直接定めた公的資料は確認できていません）"
            }
        }
    }

    /// 根拠の条文（製品の規則のものは `None`）。
    #[must_use]
    pub const fn provenance(self) -> Option<Provenance> {
        match self {
            Notice::NoCrossEmployerAggregation => Some(LSA_38_1),
            Notice::NoEmergencyWork => Some(LSA_33_1),
            Notice::NoHolidaySwap | Notice::WeeklyAssignmentIsProductRule => None,
        }
    }
}

/// 時間外・休日・深夜の月の合計の丸め（方針ファイルの `rounding`）。
///
/// 値は `none` と `monthly_30min` だけ。1日ごとの切り捨てや丸めは、通達が認めていないので、値として持たない（AC-08.4）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// 丸めない（`none`。既定）。
    #[default]
    None,
    /// 月の合計を、30分未満切り捨て・30分以上切り上げで時間に直した値を、別に出す（`monthly_30min`）。
    /// 元の分は変えない。60時間超の分は丸めない。
    Monthly30Min,
}

impl Rounding {
    /// すべての値。
    pub const ALL: [Rounding; 2] = [Rounding::None, Rounding::Monthly30Min];

    /// 方針ファイルの値。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Rounding::None => "none",
            Rounding::Monthly30Min => "monthly_30min",
        }
    }

    /// 方針ファイルの値から読む。ほかの値（日ごとの丸めなど）は `None`（不正な入力）。
    #[must_use]
    pub fn from_code(text: &str) -> Option<Rounding> {
        Rounding::ALL
            .into_iter()
            .find(|rounding| rounding.code() == text)
    }
}

/// 分を、30分未満切り捨て・30分以上切り上げで時間に直す（1か月の合計に使う端数処理。昭和63年基発第150号）。
#[must_use]
pub const fn round_to_hours(minutes: i64) -> i64 {
    let hours = minutes.div_euclid(60);
    if minutes.rem_euclid(60) >= 30 {
        hours + 1
    } else {
        hours
    }
}

/// `monthly_30min` で丸めた時間（時間）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RoundedHours {
    /// 延長時間数（`overtime_minutes` を丸めた時間）。
    pub overtime: i64,
    /// 休日労働時間数（`holiday_minutes` を丸めた時間）。
    pub holiday: i64,
    /// 深夜労働時間数（`night_minutes` を丸めた時間）。
    pub night: i64,
}

/// 台帳の項目（1人の、1つの月の期間）。分はすべて整数。
///
/// `work_minutes = within_minutes + overtime_minutes + holiday_minutes`。
/// `night_minutes` は他の項目と重なる独立の項目で、`work_minutes` に足さない。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LedgerRow {
    /// 月の期間（表示は初日）。
    pub period: MonthPeriod,
    /// 労働日数。実労働が1分以上ある労働日の数（年休の日は含めない）。
    pub work_days: i64,
    /// 労働時間数。時間外・休日労働を含む総実労働。
    pub work_minutes: i64,
    /// 延長時間数。法定外の時間外で、法定休日労働を含まない。
    pub overtime_minutes: i64,
    /// `overtime_minutes` の内数。月60時間超。
    pub over60_minutes: i64,
    /// 休日労働時間数。法定休日の労働。
    pub holiday_minutes: i64,
    /// 深夜労働時間数（22:00〜5:00）。他の項目と重なる。
    pub night_minutes: i64,
    /// 法定内。
    pub within_minutes: i64,
    /// `within_minutes` の内数。所定外法定内（日ごとに所定労働時間を引く簡易な値）。
    pub extra_within_minutes: i64,
    /// `monthly_30min` のときだけ、時間外・休日・深夜の月の合計を丸めた時間。
    pub rounded: Option<RoundedHours>,
}

impl LedgerRow {
    const fn empty(period: MonthPeriod) -> LedgerRow {
        LedgerRow {
            period,
            work_days: 0,
            work_minutes: 0,
            overtime_minutes: 0,
            over60_minutes: 0,
            holiday_minutes: 0,
            night_minutes: 0,
            within_minutes: 0,
            extra_within_minutes: 0,
            rounded: None,
        }
    }

    fn add_day(&mut self, day: &DayResult) -> Result<(), ReportError> {
        if day.total > 0 {
            self.work_days = add(self.work_days, 1)?;
        }
        self.work_minutes = add(self.work_minutes, day.total)?;
        self.overtime_minutes = add(self.overtime_minutes, day.overtime)?;
        self.over60_minutes = add(self.over60_minutes, day.over60)?;
        self.holiday_minutes = add(self.holiday_minutes, day.holiday)?;
        self.night_minutes = add(self.night_minutes, day.night)?;
        self.within_minutes = add(self.within_minutes, day.within)?;
        self.extra_within_minutes = add(self.extra_within_minutes, day.extra_within)?;
        Ok(())
    }

    fn round(&mut self, rounding: Rounding) {
        self.rounded = match rounding {
            Rounding::None => None,
            Rounding::Monthly30Min => Some(RoundedHours {
                overtime: round_to_hours(self.overtime_minutes),
                holiday: round_to_hours(self.holiday_minutes),
                night: round_to_hours(self.night_minutes),
            }),
        };
    }
}

/// 月次の集計と保存期限の誤り。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReportError {
    /// 区分の結果が、確定集計の拒否（`refused`）。台帳の項目は作らない。
    Refused,
    /// 月の期間の起算日が、方針の `month_start_day` と違う。
    PeriodMismatch,
    /// 同じ従業員が2回ある。
    DuplicateEmployee,
    /// 分や日数の合計が大きすぎる（検査済みの勤務では起きない）。
    Overflow,
    /// 期限の日付が、日付の型の範囲（1900〜2200年）の外。
    OutOfRange,
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ReportError::Refused => {
                "曖昧さのため確定集計を拒否した区分の結果からは、台帳の項目を作りません"
            }
            ReportError::PeriodMismatch => "月の期間の起算日が、方針の month_start_day と違います",
            ReportError::DuplicateEmployee => "同じ従業員が2回あります",
            ReportError::Overflow => "分の合計が大きすぎます",
            ReportError::OutOfRange => "期限の日付が範囲の外です",
        })
    }
}

impl std::error::Error for ReportError {}

impl From<ClassifyError> for ReportError {
    fn from(error: ClassifyError) -> ReportError {
        match error {
            ClassifyError::Overflow => ReportError::Overflow,
        }
    }
}

fn add(a: i64, b: i64) -> Result<i64, ReportError> {
    a.checked_add(b).ok_or(ReportError::Overflow)
}

/// 1つの月の期間の台帳の項目（AC-08.1〜08.3）。実労働のない期間は、すべて0の行になる。
///
/// 労働日の属する月の期間で数える（区分の結果の `days` の日付）。
///
/// # Errors
///
/// 区分の結果が拒否なら [`ReportError::Refused`]、`period` の起算日が方針と違えば
/// [`ReportError::PeriodMismatch`]、合計の桁あふれは [`ReportError::Overflow`]。
pub fn ledger_for(
    policy: &ValidPolicy,
    classification: &Classification,
    period: MonthPeriod,
    rounding: Rounding,
) -> Result<LedgerRow, ReportError> {
    if classification.refused.is_some() {
        return Err(ReportError::Refused);
    }
    if period.start_day() != policy.month_start_day() {
        return Err(ReportError::PeriodMismatch);
    }
    let mut row = LedgerRow::empty(period);
    for day in classification
        .days
        .iter()
        .filter(|day| period.contains(day.date))
    {
        row.add_day(day)?;
    }
    row.round(rounding);
    Ok(row)
}

/// 実労働のある月の期間ごとの台帳の項目（期間の順。AC-08.1〜08.3）。
///
/// # Errors
///
/// 区分の結果が拒否なら [`ReportError::Refused`]、合計の桁あふれは [`ReportError::Overflow`]。
pub fn ledger(
    policy: &ValidPolicy,
    classification: &Classification,
    rounding: Rounding,
) -> Result<Vec<LedgerRow>, ReportError> {
    if classification.refused.is_some() {
        return Err(ReportError::Refused);
    }
    let mut rows: Vec<LedgerRow> = Vec::new();
    for day in &classification.days {
        let period = day.date.month_period(policy.month_start_day());
        if rows.last().is_none_or(|last| last.period != period) {
            rows.push(LedgerRow::empty(period));
        }
        if let Some(row) = rows.last_mut() {
            row.add_day(day)?;
        }
    }
    for row in &mut rows {
        row.round(rounding);
    }
    Ok(rows)
}

/// 月次の集計に渡す、1人分の入力。
#[derive(Clone, Copy, Debug)]
pub struct ReportInput<'a> {
    /// 従業員ID。
    pub employee: &'a EmployeeId,
    /// 勤務（休憩の不足を見るのに使う）。
    pub shifts: &'a ShiftSet,
    /// その勤務の区分の結果。
    pub classification: &'a Classification,
}

/// 1人分の月次の集計。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmployeeReport {
    /// 従業員ID。
    pub employee: EmployeeId,
    /// 台帳の項目（拒否のときは空）。
    pub ledger: Vec<LedgerRow>,
    /// 曖昧さによる確定集計の拒否（出力の `refused`）。
    pub refused: Option<Refusal>,
    /// 曖昧さの記録（`use_default` のときだけ）。
    pub ambiguities: Vec<Ambiguity>,
    /// 休憩の不足の警告（日付順）。
    pub break_shortages: Vec<BreakShortage>,
}

/// 月次の集計の出力（`kintai report` の中身）。表の確認日・確認の状態・注意が、必ず付く（AC-08.6）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthlyReport {
    /// 従業員ごと（従業員ID順）。
    pub employees: Vec<EmployeeReport>,
    /// 丸めの方針。
    pub rounding: Rounding,
    /// 計算に使わなかった事情の注意（4つ、決まった順）。
    pub notices: [Notice; 4],
    /// 表の最終確認日（`table_checked_on`）。
    pub table_checked_on: &'static str,
    /// 専門家の確認の状態（`review_status`）。
    pub review_status: ReviewStatus,
}

impl MonthlyReport {
    /// 従業員ごとの区分の結果から、月次の集計を作る。
    ///
    /// `period` があれば、その月の期間だけの行（実労働がなくても1行）と、その期間の休憩の不足・曖昧さの記録を出す。
    /// なければ、実労働のある期間をすべて出す。曖昧さで拒否された従業員は、台帳の項目を空にして `refused` を付ける。
    ///
    /// # Errors
    ///
    /// 同じ従業員が2回あれば [`ReportError::DuplicateEmployee`]、`period` の起算日が方針と違えば
    /// [`ReportError::PeriodMismatch`]、合計の桁あふれは [`ReportError::Overflow`]。
    pub fn build(
        policy: &ValidPolicy,
        rounding: Rounding,
        period: Option<MonthPeriod>,
        inputs: &[ReportInput<'_>],
    ) -> Result<MonthlyReport, ReportError> {
        let mut seen = BTreeSet::new();
        let mut employees = Vec::with_capacity(inputs.len());
        for input in inputs {
            if !seen.insert(input.employee) {
                return Err(ReportError::DuplicateEmployee);
            }
            employees.push(employee_report(policy, rounding, period, input)?);
        }
        employees.sort_by(|a, b| a.employee.cmp(&b.employee));
        Ok(MonthlyReport {
            employees,
            rounding,
            notices: Notice::ALL,
            table_checked_on: TABLE_CHECKED_ON,
            review_status: ReviewStatus::Unreviewed,
        })
    }
}

fn employee_report(
    policy: &ValidPolicy,
    rounding: Rounding,
    period: Option<MonthPeriod>,
    input: &ReportInput<'_>,
) -> Result<EmployeeReport, ReportError> {
    let classification = input.classification;
    let in_period = |date: Date| period.is_none_or(|period| period.contains(date));
    let ledger = match (&classification.refused, period) {
        (Some(_), _) => Vec::new(),
        (None, Some(period)) => vec![ledger_for(policy, classification, period, rounding)?],
        (None, None) => ledger(policy, classification, rounding)?,
    };
    let ambiguities = classification
        .ambiguities
        .iter()
        .filter(|record| match record.date {
            RecordDate::LaborDay(date) => in_period(date),
            // 週の記録は、週の7日のどれかが期間に入れば出す。
            RecordDate::WeekStart(week) => (0..7).any(|offset| {
                Date::from_day_number(week.first_day_number().saturating_add(offset))
                    .is_ok_and(in_period)
            }),
        })
        .copied()
        .collect();
    let break_shortages = break_shortages(input.shifts)?
        .into_iter()
        .filter(|shortage| in_period(shortage.date))
        .collect();
    Ok(EmployeeReport {
        employee: input.employee.clone(),
        ledger,
        refused: classification.refused.clone(),
        ambiguities,
        break_shortages,
    })
}

// ---- 保存期限の目安 ----

/// 労働安全衛生規則（2026-10-01 施行の版）。
pub const INDUSTRIAL_SAFETY_REGULATION: Law = Law {
    name: "労働安全衛生規則",
    law_id: "347M50002000032",
    revision_id: "347M50002000032_20261001_508M60000100116",
    enforced_on: "2026-10-01",
    url: "https://laws.e-gov.go.jp/law/347M50002000032",
};

const fn act(article: &'static str, summary: &'static str) -> Provenance {
    Provenance {
        law: LABOR_STANDARDS_ACT,
        article,
        summary,
        confirmed_on: CONFIRMED_ON,
    }
}

const fn regulation(article: &'static str, summary: &'static str) -> Provenance {
    Provenance {
        law: LABOR_STANDARDS_REGULATION,
        article,
        summary,
        confirmed_on: CONFIRMED_ON,
    }
}

/// 労働基準法第33条第1項: 災害等による臨時の必要がある場合の時間外・休日労働。
pub const LSA_33_1: Provenance = act(
    "第33条第1項",
    "災害その他避けることのできない事由による臨時の必要がある場合の時間外・休日労働",
);
/// 労働基準法第38条第1項: 事業場を異にする場合の労働時間の通算。
pub const LSA_38_1: Provenance = act(
    "第38条第1項",
    "労働時間は、事業場を異にする場合においても通算する",
);
/// 労働基準法第108条: 賃金台帳。
pub const LSA_108: Provenance = act("第108条", "賃金台帳の調製と記入");
/// 労働基準法第109条: 記録の保存（5年）。
pub const LSA_109: Provenance = act(
    "第109条",
    "労働者名簿、賃金台帳、労働関係に関する重要な書類を5年間保存する",
);
/// 労働基準法附則第143条第1項: 第109条の5年を、当分の間3年とする。
pub const LSA_SUPPL_143_1: Provenance = act("附則第143条第1項", "第109条の保存期間は、当分の間3年");
/// 労働基準法施行規則第54条第1項: 賃金台帳の記入事項（労働日数・労働時間数・延長・休日・深夜の時間数）。
pub const REG_54_1: Provenance = regulation(
    "第54条第1項",
    "賃金台帳に、労働日数、労働時間数、延長時間数、休日労働時間数、深夜労働時間数などを記入する",
);
/// 労働基準法施行規則第54条第2項: 就業規則の所定の時間・休日による時間数で代えられる。
pub const REG_54_2: Provenance = regulation(
    "第54条第2項",
    "就業規則で法と異なる所定労働時間・休日を定めたときは、それによる時間数で代えられる",
);
/// 労働基準法施行規則第56条第1項第2号: 賃金台帳の起算日は最後の記入をした日。
pub const REG_56_1_ITEM2: Provenance = regulation(
    "第56条第1項第2号",
    "賃金台帳の保存期間の起算日は、最後の記入をした日",
);
/// 労働基準法施行規則第56条第1項第5号: 重要な書類の起算日は完結の日。
pub const REG_56_1_ITEM5: Provenance = regulation(
    "第56条第1項第5号",
    "賃金その他労働関係に関する重要な書類の保存期間の起算日は、その完結の日",
);
/// 労働基準法施行規則第56条第2項: 賃金の支払期日の方が遅ければ、支払期日を起算日とする。
pub const REG_56_2: Provenance = regulation(
    "第56条第2項",
    "記録に係る賃金の支払期日の方が遅いときは、支払期日を起算日とする",
);
/// 労働基準法施行規則第56条第3項: 年次有給休暇管理簿などに第2項を準用する。
pub const REG_56_3: Provenance = regulation(
    "第56条第3項",
    "年次有給休暇管理簿などに、第56条第2項（支払期日の方が遅いとき）を準用する",
);
/// 労働基準法施行規則第24条の7: 年次有給休暇管理簿（期間中と満了後5年）。
pub const REG_24NO7: Provenance = regulation(
    "第24条の7",
    "年次有給休暇管理簿を作成し、付与の期間中と満了後5年間保存する",
);
/// 労働基準法施行規則附則第71条: 第24条の7などの5年を、当分の間3年とする。
pub const REG_SUPPL_71: Provenance =
    regulation("附則第71条", "第24条の7などの保存期間は、当分の間3年");
/// 労働安全衛生規則第52条の7の3第2項: 労働時間の状況の記録の3年保存。
pub const ISR_52NO7NO3_2: Provenance = Provenance {
    law: INDUSTRIAL_SAFETY_REGULATION,
    article: "第52条の7の3第2項",
    summary: "把握した労働時間の状況の記録を作成し、3年間保存する",
    confirmed_on: CONFIRMED_ON,
};

/// 保存の目安を出す記録の種類（`kintai retention --kind`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RetentionKind {
    /// 賃金台帳（`wage-ledger`）。起算日は最後の記入をした日と支払期日の遅い方。
    WageLedger,
    /// 出勤簿・タイムカードなどの重要な書類（`attendance`）。起算日は完結の日と支払期日の遅い方。
    Attendance,
    /// 労働時間の状況の記録（`status-record`。労働安全衛生規則第52条の7の3）。起算日の定めがないので、
    /// 製品の決めごととして完了の日から数える（法定の起算日とは言わない）。
    StatusRecord,
    /// 年次有給休暇管理簿（`leave-register`）。起算日は期間の満了の日と支払期日の遅い方。
    LeaveRegister,
}

impl RetentionKind {
    /// すべての種類。
    pub const ALL: [RetentionKind; 4] = [
        RetentionKind::WageLedger,
        RetentionKind::Attendance,
        RetentionKind::StatusRecord,
        RetentionKind::LeaveRegister,
    ];

    /// `--kind` の値。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            RetentionKind::WageLedger => "wage-ledger",
            RetentionKind::Attendance => "attendance",
            RetentionKind::StatusRecord => "status-record",
            RetentionKind::LeaveRegister => "leave-register",
        }
    }

    /// `--kind` の値から読む。
    #[must_use]
    pub fn from_code(text: &str) -> Option<RetentionKind> {
        RetentionKind::ALL
            .into_iter()
            .find(|kind| kind.code() == text)
    }

    /// 現行の法定の最低（経過措置による当分の間の年数）。
    #[must_use]
    pub const fn statutory_current_years(self) -> i32 {
        3
    }

    /// 本則の年数（労働時間の状況の記録は、本則も3年）。
    #[must_use]
    pub const fn statutory_base_years(self) -> i32 {
        match self {
            RetentionKind::StatusRecord => 3,
            RetentionKind::WageLedger
            | RetentionKind::Attendance
            | RetentionKind::LeaveRegister => 5,
        }
    }

    /// 製品の既定の年数（どれも5年）。
    #[must_use]
    pub const fn product_default_years(self) -> i32 {
        5
    }

    /// 起算日が法令の定めによるか（労働時間の状況の記録だけは、製品の決めごと）。
    #[must_use]
    pub const fn start_is_statutory(self) -> bool {
        !matches!(self, RetentionKind::StatusRecord)
    }

    /// 根拠の条文。
    #[must_use]
    pub const fn provenances(self) -> &'static [Provenance] {
        match self {
            RetentionKind::WageLedger => &[LSA_109, LSA_SUPPL_143_1, REG_56_1_ITEM2, REG_56_2],
            RetentionKind::Attendance => &[LSA_109, LSA_SUPPL_143_1, REG_56_1_ITEM5, REG_56_2],
            RetentionKind::StatusRecord => &[ISR_52NO7NO3_2],
            RetentionKind::LeaveRegister => &[REG_24NO7, REG_SUPPL_71, REG_56_3],
        }
    }
}

/// 保存期限の目安（`kintai retention` の中身）。日付は、その日まで保存する最後の日。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Retention {
    /// 記録の種類。
    pub kind: RetentionKind,
    /// 起算日。
    pub start: Date,
    /// 起算日が法令の定めによるか（偽なら製品の決めごと）。
    pub start_is_statutory: bool,
    /// 現行の法定の最低（当分の間3年）の期限（`statutory_current`）。
    pub statutory_current: Date,
    /// 本則の期限（`statutory_base`）。
    pub statutory_base: Date,
    /// 製品の既定（5年）の期限（`product_default`）。
    pub product_default: Date,
    /// 表の最終確認日（`table_checked_on`）。
    pub table_checked_on: &'static str,
    /// 専門家の確認の状態（`review_status`）。
    pub review_status: ReviewStatus,
}

/// 保存期限の目安を求める（AC-08.5）。
///
/// 起算日は、`completed`（賃金台帳は最後の記入をした日、重要な書類は完結の日、年次有給休暇管理簿は期間の満了の日）と
/// `wage_due`（その記録に係る賃金の支払期日）の遅い方。労働時間の状況の記録は `completed`（製品の決めごと）。
/// 期限は民法の期間の数え方で求める: 初日を入れず（起算日の翌日から数え）、n 年後の応当日の前日（応当日がなければ
/// その月の末日）まで。ふつうは起算日の n 年後の同じ日付になる。
///
/// # Errors
///
/// 期限が日付の型の範囲（1900〜2200年）の外なら [`ReportError::OutOfRange`]（入力の範囲の日付では起きない）。
pub fn retention(
    kind: RetentionKind,
    completed: Date,
    wage_due: Date,
) -> Result<Retention, ReportError> {
    let start = if kind.start_is_statutory() {
        completed.max(wage_due)
    } else {
        completed
    };
    Ok(Retention {
        kind,
        start,
        start_is_statutory: kind.start_is_statutory(),
        statutory_current: keep_until(start, kind.statutory_current_years())?,
        statutory_base: keep_until(start, kind.statutory_base_years())?,
        product_default: keep_until(start, kind.product_default_years())?,
        table_checked_on: TABLE_CHECKED_ON,
        review_status: ReviewStatus::Unreviewed,
    })
}

/// 起算日 `start` から `years` 年の期間の最後の日（民法140条・143条2項）。
fn keep_until(start: Date, years: i32) -> Result<Date, ReportError> {
    let first = start
        .checked_add_days(1)
        .map_err(|_| ReportError::OutOfRange)?;
    let year = first
        .year()
        .checked_add(years)
        .ok_or(ReportError::OutOfRange)?;
    if let Ok(anniversary) = Date::from_ymd(year, first.month(), first.day()) {
        return anniversary
            .checked_add_days(-1)
            .map_err(|_| ReportError::OutOfRange);
    }
    // 応当日がない（2月29日の n 年後が平年）ときは、その月の末日。
    let last = kintai_core::days_in_month(year, first.month()).ok_or(ReportError::OutOfRange)?;
    Date::from_ymd(year, first.month(), last).map_err(|_| ReportError::OutOfRange)
}
