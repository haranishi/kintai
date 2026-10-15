//! 年休のケースを無作為の入力でバイナリに通す性質の試験。
//!
//! - AC-09.6: どんな入力でも、パニックせず、終了コードは0・2・3のどれか。壊した入力（型違い、知らない項目、
//!   `null`、範囲外、壊れたバイト列）は `InvalidInput` か、年休の対応外のコード。
//! - AC-09.7: 成功した出力は1行で、ロット・5日・管理簿が付与ごとに1行ずつ、付与日の昇順。残数の合計が一致する。

mod common;

use common::{Run, run_case, run_case_bytes};
use proptest::prelude::*;
use serde_json::{Value, json};

fn day_text(days_from_2018: i64) -> String {
    // 2018-01-01 からの日数を表記にする（試験の中だけの換算。うるう年を数える）。
    let mut year = 2018;
    let mut rest = days_from_2018;
    loop {
        let length = if year % 4 == 0 { 366 } else { 365 };
        if rest < length {
            break;
        }
        rest -= length;
        year += 1;
    }
    let lengths = [
        31,
        if year % 4 == 0 { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for length in lengths {
        if rest < length {
            break;
        }
        rest -= length;
        month += 1;
    }
    format!("{year:04}-{month:02}-{:02}", rest + 1)
}

/// 年休のケース（対応外や残数の不足も起きうる、形の正しい入力）。
fn case() -> impl Strategy<Value = Value> {
    (
        0_i64..1500,
        0_i64..2500,
        (
            0_i64..=7,
            prop::sample::select(vec![0_i64, 1200, 1800, 2400]),
            prop::option::of(0_i64..=366),
        ),
        prop::collection::vec(
            prop::sample::select(vec!["meets", "below", "unknown"]),
            0..8,
        ),
        prop::option::weighted(
            0.3,
            prop::collection::vec((-100_i64..2500, 1_i64..=40), 0..5),
        ),
        prop::collection::vec(
            (
                100_i64..2600,
                prop::sample::select(vec!["full", "full", "half", "hours"]),
            ),
            0..12,
        ),
    )
        .prop_map(
            |(hire, span, (days, minutes, annual), attendance, grants, taken)| {
                json!({
                    "schema": "kintai.caseleave.v1",
                    "as_of": day_text(hire + span),
                    "employee": {
                        "hire_date": day_text(hire),
                        "weekly_scheduled_days": days,
                        "weekly_scheduled_minutes": minutes,
                        "annual_scheduled_days": annual,
                        "attendance": attendance,
                        "grants": grants.map(|list| list.iter().map(|(offset, halves)| {
                            json!({"date": day_text(hire + offset), "halves": halves})
                        }).collect::<Vec<_>>()),
                    },
                    "taken": taken.iter().map(|(offset, unit)| {
                        json!({"date": day_text(hire + offset), "unit": unit})
                    }).collect::<Vec<_>>(),
                })
            },
        )
}

fn check_shape(run: &Run) {
    match run.exit {
        0 => {
            assert_eq!(run.stderr, "");
            assert!(run.stdout.ends_with('\n') && !run.stdout.trim_end().contains('\n'));
            let out = run.output();
            assert_eq!(out["schema"], "kintai.leave.v1");
            let lots = out["lots"].as_array().unwrap();
            assert_eq!(lots.len(), out["five_day"].as_array().unwrap().len());
            assert_eq!(lots.len(), out["register"].as_array().unwrap().len());
            let mut balance = 0;
            let mut previous = String::new();
            for lot in lots {
                let date = lot["grant_date"].as_str().unwrap().to_owned();
                assert!(previous <= date, "付与日の昇順");
                previous = date;
                balance += lot["remaining_halves"].as_i64().unwrap();
                assert_eq!(
                    lot["granted_halves"].as_i64().unwrap(),
                    lot["used_halves"].as_i64().unwrap()
                        + lot["remaining_halves"].as_i64().unwrap()
                        + lot["expired_halves"].as_i64().unwrap()
                );
            }
            assert_eq!(out["balance_halves"].as_i64().unwrap(), balance);
            let notices = out["notices"].as_array().unwrap();
            assert!(notices.is_empty() || notices == &[json!("AttendanceUnconfirmed")]);
        }
        2 => {
            assert_eq!(run.stdout, "");
            assert_eq!(run.error_code(), "InvalidInput");
        }
        3 => {
            assert_eq!(run.stdout, "");
            let code = run.error_code();
            assert!(
                ["LeaveHourly", "LeaveSplitGrant", "LeaveOutOfTable"].contains(&code.as_str()),
                "{code}"
            );
        }
        other => panic!("決まりにない終了コード {other}: {run:?}"),
    }
}

fn junk() -> impl Strategy<Value = Value> {
    prop::sample::select(vec![
        Value::Null,
        json!(false),
        json!(-1),
        json!(0),
        json!(201),
        json!(367),
        json!(10_081),
        json!(2.0),
        json!("hours"),
        json!("2024-02-30"),
        json!("2100-01-01"),
        json!([]),
        json!({}),
        json!(18_446_744_073_709_551_615_u64),
    ])
}

const PLACES: [&str; 13] = [
    "/schema",
    "/as_of",
    "/employee",
    "/employee/hire_date",
    "/employee/weekly_scheduled_days",
    "/employee/weekly_scheduled_minutes",
    "/employee/annual_scheduled_days",
    "/employee/attendance",
    "/employee/grants",
    "/taken",
    "/taken/0/date",
    "/taken/0/unit",
    "/employee/attendance/0",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// AC-09.6 AC-09.7 形の正しいケースは、0（成功）・2（残数の不足など）・3（対応外）のどれかで、出力の形が決まりのとおり。
    #[test]
    fn ac09_6_cases_end_with_a_defined_code(case in case()) {
        check_shape(&run_case(&case));
    }

    /// AC-09.6 項目を型違い・範囲外・`null` に替えても、パニックしない。知らない項目は不正な入力。
    #[test]
    fn ac09_6_replaced_items_never_crash(
        case in case(),
        place in prop::sample::select(PLACES.to_vec()),
        value in junk(),
        add_unknown in any::<bool>(),
    ) {
        let mut broken = case;
        if let Some(slot) = broken.pointer_mut(place) {
            *slot = value;
        }
        if let Some(employee) = broken["employee"]
            .as_object_mut()
            .filter(|_| add_unknown)
        {
            employee.insert("unknown_item".to_owned(), json!(1));
        }
        let run = run_case(&broken);
        check_shape(&run);
        if add_unknown && broken["employee"].is_object() {
            prop_assert_eq!(run.exit, 2);
        }
    }

    /// AC-09.6 正しいケースの JSON のバイト列を壊しても、パニックしない。
    #[test]
    fn ac09_6_mutated_bytes_never_crash(
        case in case(),
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0_u8..3), 1..6),
    ) {
        let mut bytes = serde_json::to_vec(&case).unwrap();
        for (index, byte, kind) in edits {
            let at = index.index(bytes.len().max(1)).min(bytes.len());
            match kind {
                0 => { if at < bytes.len() { bytes[at] = byte; } }
                1 => bytes.insert(at, byte),
                _ => { if at < bytes.len() { bytes.remove(at); } }
            }
        }
        check_shape(&run_case_bytes(&bytes));
    }
}
