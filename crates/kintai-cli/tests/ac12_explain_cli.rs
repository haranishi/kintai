//! `kintai explain`（AC-12.5）の試験。合成データは `tests/fixtures/synthetic/`。
//!
//! E001 の 2026-10-08（木）は 9:00〜23:00、休憩 12:00〜13:00 と 18:00〜18:30。所定は420分なので、
//! 9:00〜12:00 と 13:00〜17:00 が法定内、17:00〜18:00 が所定外法定内、18:30〜22:00 が時間外、
//! 22:00〜23:00 が時間外・深夜（厚生労働省『時間外・休日労働と割増賃金』の例1と同じ読み）。

mod common;

use common::{failed_code, fixture_path, kintai};
use serde_json::{Value, json};

fn explain(employee: &str, date: &str, extra: &[&str]) -> common::Run {
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let mut args = vec![
        "explain",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--employee",
        employee,
        "--date",
        date,
    ];
    args.extend_from_slice(extra);
    kintai(&args)
}

fn segment_summary(out: &Value) -> Vec<(String, String, i64, String, bool, Option<i64>)> {
    out["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|segment| {
            (
                segment["start"].as_str().unwrap().to_owned(),
                segment["end"].as_str().unwrap().to_owned(),
                segment["minutes"].as_i64().unwrap(),
                segment["category"].as_str().unwrap().to_owned(),
                segment["night"].as_bool().unwrap(),
                segment["premium"]["percent"].as_i64(),
            )
        })
        .collect()
}

fn articles(segment: &Value) -> Vec<String> {
    segment["premium"]["basis"]
        .as_array()
        .unwrap()
        .iter()
        .map(|basis| {
            assert_eq!(basis["confirmed_on"], "2026-10-15", "根拠ごとの確認日");
            assert!(basis["revision_id"].as_str().unwrap().contains('_'));
            assert!(
                basis["url"]
                    .as_str()
                    .unwrap()
                    .starts_with("https://laws.e-gov.go.jp/law/")
            );
            format!(
                "{}{}",
                basis["law"].as_str().unwrap(),
                basis["article"].as_str().unwrap()
            )
        })
        .collect()
}

/// AC-12.5・AC-12.9 区間ごとに区分・割増の下限・根拠の条文・確認日が出る（`docs/spec/cli-output.md` の
/// 項目名。並びは `ac12_output_shape.rs`）。
#[test]
fn ac12_5_explain_json_has_segments_with_premium_and_basis() {
    let run = explain("E001", "2026-10-08", &["--format", "json"]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    assert_eq!(out["schema"], "kintai.explain.v1");
    assert_eq!(
        (out["employee_id"].as_str(), out["date"].as_str()),
        (Some("E001"), Some("2026-10-08"))
    );
    assert_eq!(out["weekday"], "thu");
    assert_eq!(out["legal_holiday"], false);
    assert_eq!(out["national_holiday"], Value::Null);
    assert_eq!(out["holiday_table"]["status"], "Official");
    assert!(
        out["holiday_table"]["source"]
            .as_str()
            .unwrap()
            .contains("内閣府")
    );
    let summary = segment_summary(&out);
    let expected = [
        (
            "2026-10-08T09:00",
            "2026-10-08T12:00",
            180,
            "within",
            false,
            Some(0),
        ),
        (
            "2026-10-08T13:00",
            "2026-10-08T17:00",
            240,
            "within",
            false,
            Some(0),
        ),
        (
            "2026-10-08T17:00",
            "2026-10-08T18:00",
            60,
            "extra_within",
            false,
            Some(0),
        ),
        (
            "2026-10-08T18:30",
            "2026-10-08T22:00",
            210,
            "overtime",
            false,
            Some(25),
        ),
        (
            "2026-10-08T22:00",
            "2026-10-08T23:00",
            60,
            "overtime",
            true,
            Some(50),
        ),
    ];
    assert_eq!(summary.len(), expected.len(), "{summary:?}");
    for (actual, expected) in summary.iter().zip(expected) {
        assert_eq!(
            (
                actual.0.as_str(),
                actual.1.as_str(),
                actual.2,
                actual.3.as_str(),
                actual.4,
                actual.5
            ),
            expected
        );
    }
    assert_eq!(
        articles(&out["segments"][0]),
        ["労働基準法第32条第1項", "労働基準法第32条第2項"]
    );
    let night = articles(&out["segments"][4]);
    assert!(
        night.contains(&"労働基準法第37条第4項".to_owned()),
        "{night:?}"
    );
    assert!(
        night.contains(&"労働基準法施行規則第20条第1項".to_owned()),
        "{night:?}"
    );
    assert_eq!(
        out["day"],
        json!({"date": "2026-10-08", "total": 750, "within": 480, "extra_within": 60, "overtime": 270,
            "over60": 0, "holiday": 0, "night": 60, "night_within": 0, "night_overtime": 60,
            "night_over60": 0, "night_holiday": 0})
    );
    assert_eq!(out["notes"][0]["code"], "ExtraWithinIsContractual");
    assert_eq!(out["refused"], Value::Null);
    assert_eq!(out["table_checked_on"], "2026-10-15");
    assert_eq!(out["review_status"], "unreviewed");
    assert_eq!(out["notices"].as_array().unwrap().len(), 4);
}

/// 国民の祝日は説明に出すが、法定休日ではない（法定休日は方針の曜日。AC-07.6 をコマンドから）。
#[test]
fn national_holiday_is_not_a_legal_holiday() {
    let run = explain("E002", "2026-10-12", &["--format", "json"]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    assert_eq!(
        out["national_holiday"],
        json!({"name": "スポーツの日", "kind": "Statutory"})
    );
    assert_eq!(out["legal_holiday"], false);
    let categories: Vec<String> = segment_summary(&out).into_iter().map(|s| s.3).collect();
    assert!(
        !categories.contains(&"holiday".to_owned()),
        "{categories:?}"
    );
    assert_eq!(
        out["segments"][0]["premium"]["percent"], 25,
        "法定内の深夜は25%"
    );
    assert_eq!(
        out["segments"][0]["end"], "2026-10-13T02:00",
        "翌日にかかる区間"
    );
}

/// 法定休日（日曜）の労働は、法定休日の区分で35%。
#[test]
fn legal_holiday_work_is_35_percent() {
    let run = explain("E003", "2026-10-18", &["--format", "json"]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    assert_eq!(out["legal_holiday"], true);
    assert_eq!(
        segment_summary(&out),
        [(
            "2026-10-18T09:00".to_owned(),
            "2026-10-18T15:00".to_owned(),
            360,
            "holiday".to_owned(),
            false,
            Some(35)
        )]
    );
}

/// 月60時間を超えた時間外の区間は50%（E004 は10月の21日目から60時間超）。
#[test]
fn over60_segments_are_50_percent() {
    let run = explain("E004", "2026-10-29", &["--format", "json"]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    let last = segment_summary(&out).pop().unwrap();
    assert_eq!(
        last,
        (
            "2026-10-29T17:30".to_owned(),
            "2026-10-29T20:30".to_owned(),
            180,
            "over60".to_owned(),
            false,
            Some(50)
        )
    );
    let before = explain("E004", "2026-10-27", &["--format", "json"]).output();
    assert_eq!(segment_summary(&before).pop().unwrap().3, "overtime");
}

/// 拒否された従業員は、区間を出さず `refused`（終了コード4）。
#[test]
fn refused_employee_exits_4() {
    let run = explain("E005", "2026-10-07", &["--format", "json"]);
    assert_eq!(run.exit, 4, "{run:?}");
    let out = run.output();
    assert_eq!(out["refused"]["code"], "AmbiguousBoundary");
    assert_eq!(out["segments"], json!([]));
    assert_eq!(out["day"], Value::Null);
    let text = explain("E005", "2026-10-07", &[]);
    assert_eq!(text.exit, 4);
    assert!(text.stdout.contains("確定集計を拒否"), "{}", text.stdout);
}

/// 実労働のない日は、区間が空（終了コード0）。
#[test]
fn day_without_work_has_no_segments() {
    let run = explain("E001", "2026-10-10", &["--format", "json"]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    assert_eq!(out["segments"], json!([]));
    assert_eq!(out["day"], Value::Null);
    let text = explain("E001", "2026-10-10", &[]);
    assert!(
        text.stdout.contains("この日の実労働はありません"),
        "{}",
        text.stdout
    );
}

/// AC-12.5・AC-12.10 text: 区間の表に、区分・割増の下限・根拠の条文、末尾に確認日と参考値の1行。
#[test]
fn ac12_5_explain_text() {
    let run = explain("E001", "2026-10-08", &[]);
    assert_eq!(run.exit, 0, "{run:?}");
    let text = &run.stdout;
    assert!(
        text.contains("22:00〜23:00   60分  時間外・深夜         50%"),
        "{text}"
    );
    assert!(text.contains("労働基準法施行規則第20条第1項"), "{text}");
    assert!(text.contains("祝日の表の出典: 内閣府"), "{text}");
    assert!(text.contains("table_checked_on）: 2026-10-15"));
    assert!(
        text.ends_with("この出力は参考値です（専門家の確認前。review_status: unreviewed）。\n")
    );
}

/// 方針にない従業員、範囲外の日付は不正な入力。
#[test]
fn explain_arguments_are_checked() {
    assert_eq!(
        failed_code(&explain("E999", "2026-10-08", &[]), 2),
        "InvalidInput"
    );
    assert_eq!(
        failed_code(&explain("=E001", "2026-10-08", &[]), 2),
        "InvalidInput"
    );
    assert_eq!(
        failed_code(&explain("E001", "1999-12-31", &[]), 2),
        "InvalidInput"
    );
    assert_eq!(
        failed_code(&explain("E001", "2026-10-08", &["--format", "csv"]), 2),
        "InvalidInput"
    );
}

/// `--data-from`（`report` と同じ意味）: E004 の打刻だけ（月曜 9/28 から）では、既定で 9/27 の週が
/// `IncompleteWeek` になり拒否される。`--data-from 2026-09-27` で避けられる。
#[test]
fn data_from_avoids_the_incomplete_week_in_explain() {
    let dir = common::temp_dir("explain-data-from");
    let policy = common::write_file(
        &dir,
        "policy.toml",
        common::policy_with_employees(&["E004"]),
    );
    let punches = common::write_file(
        &dir,
        "punches.csv",
        common::csv_with_employees(&common::fixture_text("punches.csv"), &["E004"]),
    );
    let base = [
        "explain",
        "--policy",
        policy.as_str(),
        "--punches",
        punches.as_str(),
        "--employee",
        "E004",
        "--date",
        "2026-10-01",
    ];
    let refused = kintai(&[&base[..], &["--format", "json"]].concat());
    assert_eq!(refused.exit, 4, "{refused:?}");
    assert_eq!(
        refused.output()["refused"],
        json!({"code": "AmbiguousBoundary", "dates": ["2026-09-27"], "anomalies": []})
    );
    let json_run = kintai(
        &[
            &base[..],
            &["--data-from", "2026-09-27", "--format", "json"],
        ]
        .concat(),
    );
    assert_eq!(json_run.exit, 0, "{json_run:?}");
    assert_eq!(json_run.output()["segments"].as_array().unwrap().len(), 4);
    let text = kintai(&[&base[..], &["--data-from", "2026-09-27"]].concat());
    assert_eq!(text.exit, 0, "{text:?}");
    assert!(
        text.stdout
            .contains("打刻がそろっている最初の日（data_from）: 2026-09-27"),
        "{}",
        text.stdout
    );
    let bad = kintai(&[&base[..], &["--data-from", "2026-09-31"]].concat());
    assert_eq!(failed_code(&bad, 2), "InvalidInput");
}

/// L2-13 前日に始まった勤務の続きがある暦日を指したときは、その続きは前日の労働日に数えていることを示す
/// （E002 は 10/01（木）22:00〜10/02 7:00 の夜勤。10/02 に始まった勤務はない）。
#[test]
fn continuation_from_the_previous_day_is_explained() {
    let continued = explain("E002", "2026-10-02", &[]);
    assert_eq!(continued.exit, 0, "{continued:?}");
    assert!(
        continued.stdout.contains(
            "区間: この日に始まった勤務はありません（前日に始まった勤務の続きは、前日の労働日に数えています）"
        ),
        "{}",
        continued.stdout
    );
    let json_run = explain("E002", "2026-10-02", &["--format", "json"]);
    assert_eq!(json_run.output()["segments"], json!([]));
    let none = explain("E002", "2026-10-10", &[]);
    assert!(
        none.stdout.contains("区間: この日の実労働はありません"),
        "{}",
        none.stdout
    );
}
