//! 査読 L2 の変異（`tools/mutations/cli-review.json`）を赤くする試験（kintai-core）。

use kintai_core::Instant;
use kintai_core::punch::{
    EmployeeId, Punch, PunchError, PunchKind, PunchTime, SecondsPolicy, pair,
};

fn shift(employee: &EmployeeId, start: Instant, minutes: i64) -> [Punch; 2] {
    let end = start.checked_add_minutes(minutes).unwrap();
    [
        Punch {
            employee: employee.clone(),
            time: PunchTime::new(start, 0).unwrap(),
            kind: PunchKind::In,
        },
        Punch {
            employee: employee.clone(),
            time: PunchTime::new(end, 0).unwrap(),
            kind: PunchKind::Out,
        },
    ]
}

/// AC-10.6・L2-M11 1人1か月1,000件の勤務は、暦の月（年と月の組）ごとに数える。2025年10月の1,000件と
/// 2026年10月の1件は別の月なので受け付け、2025年10月にもう1件足すと1,001件で拒否する。
#[test]
fn l2_m11_shift_count_is_per_year_and_month() {
    let employee = EmployeeId::new("E001").unwrap();
    let first: Instant = "2025-10-01T00:00".parse().unwrap();
    // 10分の勤務を40分ごとに1,000件（2025-10-28 まで）。
    let mut punches: Vec<Punch> = (0..1000)
        .flat_map(|i| shift(&employee, first.checked_add_minutes(i * 40).unwrap(), 10))
        .collect();
    punches.extend(shift(&employee, "2026-10-05T09:00".parse().unwrap(), 60));
    assert!(pair(&punches, SecondsPolicy::WorkerFavorable).is_ok());
    punches.extend(shift(&employee, "2025-10-31T09:00".parse().unwrap(), 60));
    assert!(matches!(
        pair(&punches, SecondsPolicy::WorkerFavorable),
        Err(PunchError::TooManyShifts { .. })
    ));
}
