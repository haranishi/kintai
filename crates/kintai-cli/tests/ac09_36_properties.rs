//! 36協定のケースを無作為の入力でバイナリに通す性質の試験。
//!
//! - AC-09.5: 正しいケースは終了コード0。壊した入力（型違い、知らない項目、`null`、範囲外、壊れたバイト列）は
//!   パニックせず、`InvalidInput`（終了コード2）。36協定のケースに対応外（3）はない。
//! - AC-09.7: 出力は1行で、指摘はコードの表の順。合計が `null` の n にだけ `UnknownHistory` が付く。

mod common;

use common::{Run, run_case, run_case_bytes};
use proptest::prelude::*;
use serde_json::{Value, json};

const CODE_ORDER: [&str; 11] = [
    "NoAgreement",
    "MonthlyOvertimeExceeded",
    "GeneralMonthlyLimitExceeded",
    "AnnualOvertimeExceeded",
    "GeneralAnnualLimitExceeded",
    "SpecialCountExceeded",
    "SpecialMonthlyTotalExceeded",
    "SpecialAnnualExceeded",
    "MonthlyTotalReached100h",
    "MultiMonthSumExceeded",
    "UnknownHistory",
];

fn month_text(index: i64, day: i64) -> String {
    format!(
        "{:04}-{:02}-{day:02}",
        index.div_euclid(12),
        index.rem_euclid(12) + 1
    )
}

/// 正しいケース（協定・協定月・歯抜けの履歴）。
fn valid_case() -> impl Strategy<Value = Value> {
    (
        prop::sample::select(vec!["none", "general", "special"]),
        1_i64..=12,
        (
            0_i64..=2700,
            0_i64..=21_600,
            0_i64..=5999,
            0_i64..=43_200,
            0_i64..=6,
        ),
        2021_i64 * 12..2024 * 12,
        1_i64..=28,
        prop::collection::vec((0_i64..=7000, 0_i64..=3000), 0..15),
        prop::collection::vec(prop::option::of(0_i64..=7000), 6),
    )
        .prop_map(|(kind, year_start, limits, first, day, months, history)| {
            let mut agreement = json!({"kind": kind, "year_start_month": year_start});
            if kind != "none" {
                agreement["monthly_limit_minutes"] = json!(limits.0);
                agreement["annual_limit_minutes"] = json!(limits.1);
            }
            if kind == "special" {
                agreement["special_monthly_total_limit_minutes"] = json!(limits.2);
                agreement["special_annual_limit_minutes"] = json!(limits.3);
                agreement["special_months_max"] = json!(limits.4);
            }
            let months: Vec<Value> = months
                .iter()
                .enumerate()
                .map(|(i, (o, h))| {
                    json!({"start": month_text(first + i64::try_from(i).unwrap(), day),
                           "overtime": o, "holiday": h})
                })
                .collect();
            let history: Vec<Value> = history
                .iter()
                .enumerate()
                .filter_map(|(back, total)| {
                    total.map(|t| {
                        json!({"start": month_text(first - 1 - i64::try_from(back).unwrap(), day),
                               "total": t})
                    })
                })
                .collect();
            json!({"schema": "kintai.case36.v1", "agreement": agreement,
                   "months": months, "history": history})
        })
}

/// 終了コードと出力の形が決まりのとおりか。
fn check_shape(run: &Run) {
    match run.exit {
        0 => {
            assert_eq!(run.stderr, "");
            assert!(run.stdout.ends_with('\n') && !run.stdout.trim_end().contains('\n'));
            let out = run.output();
            assert_eq!(out["schema"], "kintai.limits36.v1");
            assert_eq!(out["notices"], json!(["OwnEmployerOnly"]));
            for month in out["months"].as_array().unwrap() {
                let findings = month["findings"].as_array().unwrap();
                let order: Vec<(usize, u64)> = findings
                    .iter()
                    .map(|f| {
                        let code = f["code"].as_str().unwrap();
                        let rank = CODE_ORDER.iter().position(|c| *c == code).unwrap();
                        (rank, f["n"].as_u64().unwrap_or(0))
                    })
                    .collect();
                let mut sorted = order.clone();
                sorted.sort_unstable();
                assert_eq!(order, sorted, "{month}");
                for n in 2..=6_u64 {
                    let unknown = findings
                        .iter()
                        .any(|f| f["code"] == "UnknownHistory" && f["n"] == n);
                    assert_eq!(unknown, month["sums"][n.to_string()].is_null(), "{month}");
                }
            }
        }
        2 => {
            assert_eq!(run.stdout, "");
            assert_eq!(run.error_code(), "InvalidInput");
        }
        other => panic!("36協定のケースで決まりにない終了コード {other}: {run:?}"),
    }
}

/// 壊すときに入れる値（型違い、範囲外、`null` など）。
fn junk() -> impl Strategy<Value = Value> {
    prop::sample::select(vec![
        Value::Null,
        json!(true),
        json!(-1),
        json!(0),
        json!(44_641),
        json!(1.5),
        json!("x"),
        json!("2021-02-30"),
        json!("2100-01-01"),
        json!([]),
        json!({}),
        json!(18_446_744_073_709_551_615_u64),
        json!(-9_223_372_036_854_775_808_i64),
    ])
}

/// ケースの中の、壊す場所。
const PLACES: [&str; 12] = [
    "/schema",
    "/agreement",
    "/agreement/kind",
    "/agreement/year_start_month",
    "/agreement/monthly_limit_minutes",
    "/agreement/special_months_max",
    "/months",
    "/months/0/start",
    "/months/0/overtime",
    "/months/0/holiday",
    "/history",
    "/history/0/total",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// AC-09.5 AC-09.7 正しいケースは終了コード0で、出力の形が決まりのとおり。
    #[test]
    fn ac09_5_valid_cases_succeed(case in valid_case()) {
        let run = run_case(&case);
        prop_assert_eq!(run.exit, 0, "{} {:?}", case, run);
        check_shape(&run);
        prop_assert_eq!(
            run.output()["months"].as_array().unwrap().len(),
            case["months"].as_array().unwrap().len()
        );
    }

    /// AC-09.5 項目を型違い・範囲外・`null` に替えても、パニックせず、終了コードは0か2。
    #[test]
    fn ac09_5_replaced_items_never_crash(
        case in valid_case(),
        place in prop::sample::select(PLACES.to_vec()),
        value in junk(),
        add_unknown in any::<bool>(),
    ) {
        let mut broken = case;
        if let Some(slot) = broken.pointer_mut(place) {
            *slot = value;
        }
        // 協定がオブジェクトでなくなったときは、知らない項目を足さない（足す場所がない）。
        let unknown_added = match broken["agreement"].as_object_mut() {
            Some(agreement) if add_unknown => {
                agreement.insert("unknown_item".to_owned(), json!(1));
                true
            }
            _ => false,
        };
        let run = run_case(&broken);
        check_shape(&run);
        if unknown_added {
            prop_assert_eq!(run.exit, 2);
        }
    }

    /// AC-09.5 正しいケースの JSON のバイト列を壊しても、パニックしない。
    #[test]
    fn ac09_5_mutated_bytes_never_crash(
        case in valid_case(),
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
