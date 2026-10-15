//! 変異試験（`tools/mutations/report.json`）で生き残った誤りを赤くする試験。
//!
//! AC-03.15 と `cli.md` の `report`・`explain`: 月の最初の週が `data_from` より前に始まると、その週は
//! `IncompleteWeek`（日付は週の起算日）。月の期間を指定した集計は、週の7日のどれかが期間に入る
//! 週の記録を出し、説明はその週の各日に注意を付ける。それまでの試験は、労働日の記録（勤務の
//! 始業の日付）だけを見ていた。

mod common;

use common::{date, policy, shift};
use kintai_core::punch::EmployeeId;
use kintai_core::{MonthStartDay, Weekday};
use kintai_jp_labor::explain::{ExplainNote, explain_day};
use kintai_jp_labor::report::{MonthlyReport, ReportInput, Rounding};
use kintai_jp_labor::{Ambiguity, AmbiguityCode, AmbiguityMode, RecordDate, ShiftSet, classify};

/// AC-03.15 10月1日（木）からの打刻で、週（日曜起算）の起算日は9月27日。その週の記録は、
/// 9月27日が期間の外でも、週の3日（10月1〜3日）が入る10月の集計に出る。9月の集計にも出て、
/// 11月の集計には出ない。
#[test]
fn ac03_15_incomplete_week_record_is_in_the_report_of_each_month_it_touches() {
    let mut base = policy(Weekday::Sunday);
    base.ambiguity = Some(AmbiguityMode::UseDefault);
    let policy = base.validate().unwrap();
    let shifts = ShiftSet::new(&[
        shift("2026-10-01T09:00", "2026-10-01T17:00"),
        shift("2026-10-05T09:00", "2026-10-05T17:00"),
    ])
    .unwrap();
    let result = classify(&policy, &shifts, Some(date("2026-10-01"))).unwrap();
    let record = Ambiguity {
        date: RecordDate::WeekStart(date("2026-09-27").week(Weekday::Sunday)),
        code: AmbiguityCode::IncompleteWeek,
    };
    assert_eq!(result.ambiguities, [record]);

    let employee = EmployeeId::new("E001").unwrap();
    let inputs = [ReportInput {
        employee: &employee,
        shifts: &shifts,
        classification: &result,
    }];
    let first = MonthStartDay::new(1).unwrap();
    let report_for = |day: &str| {
        let period = date(day).month_period(first);
        MonthlyReport::build(&policy, Rounding::None, Some(period), &inputs).unwrap()
    };
    let october = report_for("2026-10-01");
    assert_eq!(october.employees[0].ambiguities, [record]);
    assert_eq!(october.employees[0].ledger[0].work_days, 2);
    assert_eq!(report_for("2026-09-01").employees[0].ambiguities, [record]);
    assert_eq!(report_for("2026-11-01").employees[0].ambiguities, []);
}

/// AC-03.15 `explain`: 打刻のそろっていない週（`IncompleteWeek`、`use_default`）の記録は、その週の
/// 日の説明に注意として付く。次の週（10月4日に始まる、そろった週）の日には付かない。
#[test]
fn ac03_15_incomplete_week_note_is_attached_to_the_days_of_that_week() {
    let mut base = policy(Weekday::Sunday);
    base.ambiguity = Some(AmbiguityMode::UseDefault);
    let policy = base.validate().unwrap();
    let shifts = ShiftSet::new(&[
        shift("2026-10-01T09:00", "2026-10-01T17:00"),
        shift("2026-10-05T09:00", "2026-10-05T17:00"),
    ])
    .unwrap();
    let result = classify(&policy, &shifts, Some(date("2026-10-01"))).unwrap();
    let thursday = explain_day(&policy, &shifts, &result, date("2026-10-01")).unwrap();
    assert_eq!(
        thursday.notes,
        [ExplainNote::Ambiguity(AmbiguityCode::IncompleteWeek)]
    );
    assert_eq!(thursday.notes[0].code(), "IncompleteWeek");
    let monday = explain_day(&policy, &shifts, &result, date("2026-10-05")).unwrap();
    assert_eq!(monday.notes, []);
}
