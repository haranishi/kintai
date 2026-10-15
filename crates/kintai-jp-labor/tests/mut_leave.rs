//! 変異試験（`tools/mutations/leave.json`）で生き残った誤りを赤くする試験。
//!
//! どれも「その日を含む」側の境界で、それまでの試験は内側の日だけを見ていた。
//! - AC-06.6・06.10: 5日の期間の最後の日（`期間の終わり(付与日, 12)`）の取得は、期間に入る。
//! - AC-06.10: `as_of` の当日の取得は数え、残数も検査する（数えないのは `as_of` より後だけ）。
//! - AC-06.10: `as_of` の当日の明示の付与は出す（出さないのは `as_of` より後だけ）。

use kintai_core::Date;
use kintai_jp_labor::leave::{
    Attendance, FiveDayStatus, GrantInput, LeaveError, LeaveInput, LeaveResult, LeaveUnit,
    TakenInput, WorkingTerms, compute,
};

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

fn full_time() -> WorkingTerms {
    WorkingTerms {
        weekly_scheduled_days: 5,
        weekly_scheduled_minutes: 2400,
        annual_scheduled_days: None,
    }
}

fn taken(list: &[(&str, LeaveUnit)]) -> Vec<TakenInput> {
    list.iter()
        .map(|&(day, unit)| TakenInput {
            date: date(day),
            unit,
        })
        .collect()
}

/// 2024-04-01 に雇入れ（最初の付与日は 2024-10-01、5日の期間は 2025-09-30 まで）。
fn statutory(as_of: &str, records: &[(&str, LeaveUnit)]) -> LeaveInput {
    LeaveInput {
        as_of: date(as_of),
        hire_date: date("2024-04-01"),
        terms: full_time(),
        attendance: vec![Attendance::Meets, Attendance::Meets],
        grants: None,
        taken: taken(records),
    }
}

fn register_dates(result: &LeaveResult, row: usize) -> Vec<String> {
    result.register[row]
        .taken
        .iter()
        .map(|record| record.date.to_string())
        .collect()
}

/// AC-06.6 5日の期間は付与日から期間の終わりの日まで（両端を含む）。最後の日（2025-09-30）の1日を
/// 足して5日になれば `Met`。管理簿の行にも、その日の取得が出る。
#[test]
fn ac06_6_taking_on_the_last_day_of_the_window_counts() {
    let records = [
        ("2025-09-22", LeaveUnit::Full),
        ("2025-09-24", LeaveUnit::Full),
        ("2025-09-25", LeaveUnit::Full),
        ("2025-09-26", LeaveUnit::Full),
        ("2025-09-30", LeaveUnit::Full),
    ];
    let result = compute(&statutory("2025-10-15", &records)).unwrap();
    let first = &result.five_day[0];
    assert_eq!(first.base_date, date("2024-10-01"));
    assert_eq!(first.window_end, date("2025-09-30"));
    assert_eq!(
        (first.taken_halves, first.status, first.needed_halves),
        (10, FiveDayStatus::Met, 0)
    );
    assert_eq!(
        register_dates(&result, 0),
        [
            "2025-09-22",
            "2025-09-24",
            "2025-09-25",
            "2025-09-26",
            "2025-09-30"
        ]
    );
    assert_eq!(result.register[0].taken_halves, 10);
    // 次の付与（2025-10-01）の期間には入らない。
    assert_eq!(result.five_day[1].base_date, date("2025-10-01"));
    assert_eq!(result.five_day[1].taken_halves, 0);
    assert_eq!(result.five_day[1].status, FiveDayStatus::InProgress);
}

/// AC-06.10 `as_of` の当日の取得は数える（残数・5日の期間・管理簿）。
#[test]
fn ac06_10_taking_on_as_of_is_counted() {
    let result = compute(&statutory("2024-10-01", &[("2024-10-01", LeaveUnit::Full)])).unwrap();
    let lot = &result.lots[0];
    assert_eq!(
        (
            lot.grant_date,
            lot.granted_halves,
            lot.used_halves,
            lot.remaining_halves
        ),
        (date("2024-10-01"), 20, 2, 18)
    );
    assert_eq!(result.balance_halves, 18);
    assert_eq!(result.five_day[0].taken_halves, 2);
    assert_eq!(register_dates(&result, 0), ["2024-10-01"]);
    // 翌日の記録は数えない（as_of より後）。
    let later = compute(&statutory(
        "2024-10-01",
        &[
            ("2024-10-01", LeaveUnit::Half),
            ("2024-10-02", LeaveUnit::Full),
        ],
    ))
    .unwrap();
    assert_eq!(later.lots[0].used_halves, 1);
}

/// AC-06.10 `as_of` の当日の記録は、残数の検査もする（付与の前日の取得は不正な入力）。
#[test]
fn ac06_10_balance_is_checked_for_a_record_on_as_of() {
    assert_eq!(
        compute(&statutory("2024-09-30", &[("2024-09-30", LeaveUnit::Half)])),
        Err(LeaveError::InsufficientBalance { index: 0 })
    );
    // as_of より後なら数えず、検査もしない。
    let ignored = compute(&statutory("2024-09-29", &[("2024-09-30", LeaveUnit::Half)])).unwrap();
    assert_eq!(ignored.lots, []);
}

/// AC-06.10 明示の付与も、`as_of` の当日の付与は出す（`as_of` より後の付与は出さない）。
#[test]
fn ac06_10_explicit_grant_on_as_of_is_shown() {
    let input = LeaveInput {
        as_of: date("2026-04-01"),
        hire_date: date("2026-04-01"),
        terms: full_time(),
        attendance: Vec::new(),
        grants: Some(vec![
            GrantInput {
                date: date("2026-04-01"),
                halves: 20,
            },
            GrantInput {
                date: date("2026-04-02"),
                halves: 2,
            },
        ]),
        taken: taken(&[("2026-04-01", LeaveUnit::Half)]),
    };
    let result = compute(&input).unwrap();
    assert_eq!(result.lots.len(), 1);
    let lot = &result.lots[0];
    assert_eq!(
        (lot.grant_date, lot.granted_halves, lot.used_halves),
        (date("2026-04-01"), 20, 1)
    );
    assert_eq!(result.balance_halves, 19);
    assert_eq!(result.register.len(), 1);
    assert_eq!(result.five_day[0].window_end, date("2027-03-31"));
}
