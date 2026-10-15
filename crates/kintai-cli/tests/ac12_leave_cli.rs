//! `kintai leave`（AC-12.4）の試験。合成データは `tests/fixtures/synthetic/`。
//!
//! 1人分の出力が、同じ入力の年休のケース（`kintai.caseleave.v1` → `kintai.leave.v1`）と同じ項目・同じ値に
//! なることを確かめる（ケースは別実装との差分試験で照合されている）。

mod common;

use common::{failed_code, fixture_path, fixture_text, kintai, run_case, temp_dir, write_file};
use serde_json::{Value, json};

fn leave(extra: &[&str]) -> common::Run {
    let policy = fixture_path("policy.toml");
    let leaves = fixture_path("leaves.csv");
    let mut args = vec![
        "leave",
        "--policy",
        &policy,
        "--leaves",
        &leaves,
        "--as-of",
        "2026-10-31",
    ];
    args.extend_from_slice(extra);
    kintai(&args)
}

fn leave_with(policy: &str, leaves: &str, extra: &[&str]) -> common::Run {
    let dir = temp_dir("leave");
    let policy = write_file(&dir, "policy.toml", policy);
    let leaves = write_file(&dir, "leaves.csv", leaves);
    let mut args = vec![
        "leave",
        "--policy",
        &policy,
        "--leaves",
        &leaves,
        "--as-of",
        "2026-10-31",
    ];
    args.extend_from_slice(extra);
    kintai(&args)
}

/// 合成データの従業員の属性（`policy.toml` と同じ値）。
fn employee_case(id: &str) -> Value {
    let (hire, days, minutes, attendance) = match id {
        "E001" => ("2024-04-01", 5, 2100, json!(["meets", "meets", "meets"])),
        "E002" => ("2025-04-01", 4, 1920, json!(["meets"])),
        "E003" => (
            "2023-10-01",
            5,
            2100,
            json!(["meets", "meets", "below", "meets"]),
        ),
        "E004" => ("2026-04-01", 5, 2100, json!(["meets"])),
        _ => ("2025-04-01", 3, 1080, json!([])),
    };
    let taken: Vec<Value> = fixture_text("leaves.csv")
        .lines()
        .skip(1)
        .filter(|row| row.starts_with(&format!("{id},")))
        .map(|row| {
            let cells: Vec<&str> = row.split(',').collect();
            json!({"date": cells[1], "unit": cells[2]})
        })
        .collect();
    json!({
        "schema": "kintai.caseleave.v1",
        "as_of": "2026-10-31",
        "employee": {"hire_date": hire, "weekly_scheduled_days": days,
            "weekly_scheduled_minutes": minutes, "attendance": attendance},
        "taken": taken,
    })
}

/// AC-12.4・AC-12.9 JSON: 従業員ごとのロット・残数・5日取得義務・管理簿。ケースと同じ計算・同じ項目
/// （並びは `ac12_output_shape.rs`）。
#[test]
fn ac12_4_leave_json_matches_the_case() {
    let run = leave(&["--format", "json"]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    assert_eq!(out["schema"], "kintai.leavereport.v1");
    assert_eq!(out["as_of"], "2026-10-31");
    assert_eq!(out["table_checked_on"], "2026-10-15");
    assert_eq!(out["review_status"], "unreviewed");
    assert_eq!(out["notices"].as_array().unwrap().len(), 4);
    let employees = out["employees"].as_array().unwrap();
    assert_eq!(employees.len(), 5);
    for entry in employees {
        let id = entry["employee_id"].as_str().unwrap();
        let case = run_case(&employee_case(id));
        assert_eq!(case.exit, 0, "{id}: {case:?}");
        let mut expected = case.output();
        expected.as_object_mut().unwrap().remove("schema");
        let mut actual = entry.clone();
        actual.as_object_mut().unwrap().remove("employee_id");
        assert_eq!(actual, expected, "{id}");
        let keys: Vec<&String> = entry.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 6, "{id}");
    }
    // 年休の失効（E001 の 2024-10-01 の付与）。
    let e001 = &employees[0];
    assert_eq!(
        e001["lots"][0],
        json!({"grant_date": "2024-10-01", "granted_halves": 20, "expires": "2026-10-01",
            "used_halves": 13, "remaining_halves": 0, "expired_halves": 7})
    );
    assert_eq!(e001["balance_halves"], 44);
    assert_eq!(e001["five_day"][1]["status"], "Met");
    assert_eq!(employees[1]["notices"], json!(["AttendanceUnconfirmed"]));
}

/// AC-12.4・AC-12.10 text: ロット・残数・5日取得義務・管理簿と、管理簿の保存の目安。日数は日に直す。
/// 末尾に確認日と参考値の1行。
#[test]
fn ac12_4_leave_text_has_every_section() {
    let run = leave(&[]);
    assert_eq!(run.exit, 0, "{run:?}");
    let text = &run.stdout;
    for expected in [
        "E001（雇入れ 2024-04-01。週5日・週2100分）",
        "2024-10-01  10日  2026-10-01  6.5日   0日  3.5日",
        "残数: 22日",
        "NotMet（期間が過ぎて5日に満たない）",
        "2025-10-01（付与11日）: 2025-12-26 1日",
        "E005（雇入れ 2025-04-01。週3日・週1080分）",
        "NotApplicable（10日未満の付与）",
        "AttendanceUnconfirmed",
        "管理簿の保存: 付与の期間中と満了後5年が本則",
        "table_checked_on）: 2026-10-15",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
    assert!(
        text.ends_with("この出力は参考値です（専門家の確認前。review_status: unreviewed）。\n")
    );
}

/// 時間単位の取得は対応外（エラーに従業員ID）。`--employee` でほかの人は計算できる。
#[test]
fn hourly_leave_is_unsupported_with_the_id() {
    let leaves = format!("{}E002,2026-09-01,hours\n", fixture_text("leaves.csv"));
    let run = leave_with(&fixture_text("policy.toml"), &leaves, &[]);
    assert_eq!(failed_code(&run, 3), "LeaveHourly");
    assert!(
        run.stderr.contains("E002") && run.stderr.contains("労働基準法第39条第4項"),
        "{}",
        run.stderr
    );
    let other = leave_with(
        &fixture_text("policy.toml"),
        &leaves,
        &["--employee", "E001"],
    );
    assert_eq!(other.exit, 0, "{other:?}");
    // 基準日より後の時間単位の記録は、まだ数えないので対応外にしない。
    let later = format!("{}E002,2026-11-02,hours\n", fixture_text("leaves.csv"));
    assert_eq!(
        leave_with(&fixture_text("policy.toml"), &later, &[]).exit,
        0
    );
}

/// 付与日数の表に当たらない所定労働日数（週0日）は対応外。
#[test]
fn out_of_table_is_unsupported() {
    let policy = fixture_text("policy.toml").replace(
        "weekly_scheduled_days = 3\nweekly_scheduled_minutes = 1080",
        "weekly_scheduled_days = 0\nweekly_scheduled_minutes = 0",
    );
    let run = leave_with(&policy, &fixture_text("leaves.csv"), &[]);
    assert_eq!(failed_code(&run, 3), "LeaveOutOfTable");
    assert!(run.stderr.contains("E005"), "{}", run.stderr);
}

/// 残数が足りない取得は、休暇CSVの行番号つきの不正な入力。
#[test]
fn insufficient_balance_names_the_line() {
    let leaves = format!("{}E004,2026-09-01,full\n", fixture_text("leaves.csv"));
    let run = leave_with(&fixture_text("policy.toml"), &leaves, &[]);
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    assert!(run.stderr.contains("休暇CSVの21行目"), "{}", run.stderr);
}

/// 休暇CSVの従業員IDは方針にあること。日付の範囲の外の `--as-of` は不正な入力。
#[test]
fn leave_inputs_are_checked() {
    let leaves = format!("{}ZZ9,2026-09-01,full\n", fixture_text("leaves.csv"));
    let run = leave_with(&fixture_text("policy.toml"), &leaves, &[]);
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    assert!(!run.stderr.contains("ZZ9"));
    let policy = fixture_path("policy.toml");
    let leaves = fixture_path("leaves.csv");
    for bad in ["2100-01-01", "2026-02-30", "20261031"] {
        let run = kintai(&[
            "leave", "--policy", &policy, "--leaves", &leaves, "--as-of", bad,
        ]);
        assert_eq!(failed_code(&run, 2), "InvalidInput", "{bad}");
    }
    let csv = kintai(&[
        "leave",
        "--policy",
        &policy,
        "--leaves",
        &leaves,
        "--as-of",
        "2026-10-31",
        "--format",
        "csv",
    ]);
    assert_eq!(failed_code(&csv, 2), "InvalidInput");
}

/// 方針の明示の付与（`grants`）を使うと、出勤率は使わない（ケースと同じ）。
#[test]
fn explicit_grants_are_used() {
    let policy = fixture_text("policy.toml").replace(
        "id = \"E004\"\nhire_date = \"2026-04-01\"",
        "id = \"E004\"\nhire_date = \"2026-04-01\"\ngrants = [{date = \"2026-04-01\", halves = 20}]",
    );
    let run = leave_with(
        &policy,
        &fixture_text("leaves.csv"),
        &["--format", "json", "--employee", "E004"],
    );
    assert_eq!(run.exit, 0, "{run:?}");
    let e004 = &run.output()["employees"][0];
    assert_eq!(e004["lots"][0]["grant_date"], "2026-04-01");
    assert_eq!(e004["notices"], json!([]));
}
