//! トップレベルの `data_from` と、方針の `holiday_swaps`（`docs/spec/case-format.md`、`docs/spec/validation.md`）。
//! 実際のバイナリを動かす。
//!
//! - AC-13.1: `data_from` の型の誤りは手順1、書式・範囲の誤りは手順4（対応外の手順3が先に返る）。
//!   `holiday_swaps` の型の誤りは手順1。
//! - AC-13.5: 勤務と日付の検査のうち、`data_from` の書式の誤り（2月30日、区切りの誤りなど）と範囲外が不正な入力になる。
//! - AC-03.15: `data_from` が週の起算日より後なら `IncompleteWeek`。出力の日付は週の起算日。
//! - AC-09.4: `IncompleteWeek` による拒否も終了コード4。

mod common;

use common::{assert_invalid, assert_unsupported, base_policy, case_with, run_case, valid_case};
use serde_json::{Value, json};

fn with_data_from(case: &Value, data_from: Value) -> Value {
    let mut case = case.clone();
    case["data_from"] = data_from;
    case
}

/// AC-13.5 `data_from` の書式の誤りと範囲外は、不正な入力。
#[test]
fn ac13_5_bad_data_from_is_invalid() {
    for text in [
        "2026-02-30",
        "2026/10/04",
        "2026-10-4",
        "2026-10-04T00:00",
        "",
        "1999-12-31",
        "2100-01-01",
    ] {
        assert_invalid(&with_data_from(&valid_case(), json!(text)));
    }
    for good in [json!("2026-10-04"), Value::Null] {
        let run = run_case(&with_data_from(&valid_case(), good.clone()));
        assert_eq!(run.exit, 0, "{good}: {run:?}");
    }
}

/// AC-13.1 `data_from` の書式の誤り（手順4）より、対応外（手順3）が先。型の誤り（手順1）はさらに先。
#[test]
fn ac13_1_data_from_errors_and_the_order() {
    let mut flex = base_policy();
    flex["work_system"] = json!("flex");
    let unsupported_case = case_with(flex, json!([]));
    assert_unsupported(
        &with_data_from(&unsupported_case, json!("2026-02-30")),
        "Flex",
    );
    assert_unsupported(
        &with_data_from(&unsupported_case, json!("1999-12-31")),
        "Flex",
    );
    for wrong_type in [json!(20_261_004), json!(["2026-10-04"]), json!(true)] {
        assert_invalid(&with_data_from(&unsupported_case, wrong_type));
    }
}

/// AC-13.1 `holiday_swaps` は配列か `null`。型の誤りは手順1で、対応外より先。
#[test]
fn ac13_1_holiday_swaps_type() {
    for wrong in [json!("2026-10-04"), json!(1), json!({})] {
        let mut policy = base_policy();
        policy["work_system"] = json!("flex");
        policy["holiday_swaps"] = wrong;
        assert_invalid(&case_with(policy, json!([])));
    }
    for empty in [json!([]), Value::Null] {
        let mut policy = base_policy();
        policy["holiday_swaps"] = empty.clone();
        let run = run_case(&case_with(policy, json!([])));
        assert_eq!(run.exit, 0, "{empty}: {run:?}");
    }
}

/// 表から外れた `worker_kind` の値（一般の建設・砂糖製造）は、定義にない値として不正な入力。
#[test]
fn removed_worker_kinds_are_not_defined() {
    for value in ["construction", "sugar_manufacturing"] {
        let mut policy = base_policy();
        policy["worker_kind"] = json!(value);
        assert_invalid(&case_with(policy, json!([])));
    }
}

fn october_only() -> Value {
    json!([
        {"start": "2026-10-01T09:00", "end": "2026-10-01T17:00"},
        {"start": "2026-10-02T09:00", "end": "2026-10-02T17:00"},
        {"start": "2026-10-03T09:00", "end": "2026-10-03T17:00"}
    ])
}

/// AC-03.15 AC-09.4 週の途中から打刻がそろっている入力は、既定で拒否（終了コード4）。日付は週の起算日。
#[test]
fn ac03_15_incomplete_week_through_the_binary() {
    let case = with_data_from(
        &case_with(base_policy(), october_only()),
        json!("2026-10-01"),
    );
    let run = run_case(&case);
    assert_eq!(run.exit, 4, "{run:?}");
    assert_eq!(
        run.output()["refused"],
        json!({"code": "AmbiguousBoundary", "dates": ["2026-09-27"]})
    );

    let mut policy = base_policy();
    policy["ambiguity"] = json!("use_default");
    let case = with_data_from(&case_with(policy, october_only()), json!("2026-10-01"));
    let run = run_case(&case);
    assert_eq!(run.exit, 0, "{run:?}");
    assert_eq!(
        run.output()["ambiguities"],
        json!([{"date": "2026-09-27", "code": "IncompleteWeek"}])
    );

    // 週の起算日以前の data_from なら付かない。
    let case = with_data_from(
        &case_with(base_policy(), october_only()),
        json!("2026-09-27"),
    );
    assert_eq!(run_case(&case).exit, 0);
}
