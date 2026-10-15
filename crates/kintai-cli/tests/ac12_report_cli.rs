//! `kintai report`（AC-12.2）と、決定性（AC-10.4）の試験。合成データは `tests/fixtures/synthetic/`。
//!
//! 期待の値は、合成データから手で数えたもの（各従業員の打刻の作り方は `policy.toml` のコメント）。
//! - E001: 平日21日（10/09 は年休）。7時間×19日＋10/05（8:59〜17:01、422分）＋10/08（750分。時間外270、深夜60）。
//! - E002: 夜勤17回×480分。深夜は1回360分。所定外法定内は1回60分。
//! - E003: 平日22日＋日曜10/18（法定休日360分）。10/14 は9時間で時間外60分。休憩の不足が2日。
//! - E004: 平日22日×660分。時間外は1日180分で3960分、そのうち3600分を超えた360分が60時間超。
//! - E005: 10/10（土）22:00〜翌5:00 が日曜と週の境界をまたぐので、確定集計を拒否（終了コード4）。

mod common;

use common::{
    csv_with_employees, failed_code, fixture_path, fixture_text, kintai, policy_with_employees,
    temp_dir, write_file,
};
use serde_json::{Value, json};

fn report(extra: &[&str]) -> common::Run {
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let mut args = vec![
        "report",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
    ];
    args.extend_from_slice(extra);
    kintai(&args)
}

fn report_with(policy: &str, punches: &str, extra: &[&str]) -> common::Run {
    let dir = temp_dir("report");
    let policy = write_file(&dir, "policy.toml", policy);
    let punches = write_file(&dir, "punches.csv", punches);
    let mut args = vec![
        "report",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
    ];
    args.extend_from_slice(extra);
    kintai(&args)
}

fn employee<'a>(out: &'a Value, id: &str) -> &'a Value {
    out["employees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["employee_id"] == id)
        .unwrap_or_else(|| panic!("{id} がない: {out}"))
}

/// AC-12.2・AC-12.9 JSON: 従業員ごとの台帳の項目（`docs/spec/cli-output.md` の項目名。並びは
/// `ac12_output_shape.rs`）。拒否した人は値を出さず `refused`。メタ情報が必ずある。
#[test]
fn ac12_2_report_json_has_the_ledger_items() {
    let run = report(&["--format", "json"]);
    assert_eq!(run.exit, 4, "{run:?}");
    assert_eq!(run.stderr, "");
    assert!(run.stdout.ends_with('\n') && !run.stdout.trim_end().contains('\n'));
    let out = run.output();
    assert_eq!(out["schema"], "kintai.report.v1");
    assert_eq!(out["month"], "2026-10");
    assert_eq!(out["period_start"], "2026-10-01");
    assert_eq!(out["period_end"], "2026-10-31");
    assert_eq!(out["data_from"], "2026-09-27");
    assert_eq!(out["rounding"], "monthly_30min");
    let ids: Vec<&str> = out["employees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["employee_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["E001", "E002", "E003", "E004", "E005"]);
    assert_eq!(
        employee(&out, "E001")["ledger"],
        json!([{"period_start": "2026-10-01", "work_days": 21, "work_minutes": 9152,
            "overtime_minutes": 270, "over60_minutes": 0, "holiday_minutes": 0,
            "night_minutes": 60, "within_minutes": 8882, "extra_within_minutes": 62,
            "rounded": {"overtime": 5, "holiday": 0, "night": 1}}])
    );
    let e002 = &employee(&out, "E002")["ledger"][0];
    assert_eq!(
        (e002["work_days"].as_i64(), e002["night_minutes"].as_i64()),
        (Some(17), Some(6120))
    );
    assert_eq!(e002["rounded"]["night"], 102);
    let e003 = employee(&out, "E003");
    assert_eq!(e003["ledger"][0]["holiday_minutes"], 360);
    assert_eq!(
        e003["ledger"][0]["overtime_minutes"], 60,
        "AC-08.2 法定休日の労働は延長に入らない"
    );
    assert_eq!(e003["ledger"][0]["work_days"], 23);
    assert_eq!(
        e003["break_shortages"],
        json!([
            {"date": "2026-10-14", "work_minutes": 540, "break_minutes": 0, "required_minutes": 60, "shortage_minutes": 60},
            {"date": "2026-10-21", "work_minutes": 420, "break_minutes": 15, "required_minutes": 45, "shortage_minutes": 30}
        ])
    );
    let e004 = &employee(&out, "E004")["ledger"][0];
    assert_eq!(
        (
            e004["overtime_minutes"].as_i64(),
            e004["over60_minutes"].as_i64(),
            e004["work_minutes"].as_i64()
        ),
        (Some(3960), Some(360), Some(14520))
    );
    let e005 = employee(&out, "E005");
    assert_eq!(e005["ledger"], json!([]));
    assert_eq!(
        e005["refused"],
        json!({"code": "AmbiguousBoundary", "dates": ["2026-10-10"], "anomalies": []})
    );
    // AC-08.1 合計の関係（深夜は足さない）。
    for entry in out["employees"].as_array().unwrap() {
        for row in entry["ledger"].as_array().unwrap() {
            let n = |key: &str| row[key].as_i64().unwrap();
            assert_eq!(
                n("work_minutes"),
                n("within_minutes") + n("overtime_minutes") + n("holiday_minutes")
            );
            assert!(n("over60_minutes") <= n("overtime_minutes"));
        }
    }
    assert_eq!(
        out["notices"],
        json!([
            "NoHolidaySwap",
            "NoCrossEmployerAggregation",
            "NoEmergencyWork",
            "WeeklyAssignmentIsProductRule"
        ])
    );
    assert_eq!(out["table_checked_on"], "2026-10-15");
    assert_eq!(out["review_status"], "unreviewed");
}

/// AC-12.2・AC-12.9 CSV: 決まった列（`docs/spec/cli-output.md`）、UTF-8・LF・見出し行。拒否した人の行は出さない。
/// メタ情報は各行の末尾の列。
#[test]
fn ac12_2_report_csv_has_fixed_columns() {
    let run = report(&["--format", "csv"]);
    assert_eq!(run.exit, 4, "{run:?}");
    assert!(!run.stdout.contains('\r'));
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(
        lines[0],
        "employee_id,period_start,work_days,work_minutes,overtime_minutes,over60_minutes,holiday_minutes,night_minutes,within_minutes,extra_within_minutes,table_checked_on,review_status,notices"
    );
    assert_eq!(lines.len(), 5, "E005（拒否）の行は出さない: {}", run.stdout);
    assert_eq!(
        lines[4],
        "E004,2026-10-01,22,14520,3960,360,0,0,10560,1320,2026-10-15,unreviewed,NoHolidaySwap;NoCrossEmployerAggregation;NoEmergencyWork;WeeklyAssignmentIsProductRule"
    );
    assert!(run.stdout.ends_with('\n'));
    // AC-08.7 どの値も、表計算ソフトが式として読む文字（= + - @）で始まらない。
    for line in &lines[1..] {
        for cell in line.split(',') {
            assert!(!cell.starts_with(['=', '+', '-', '@']), "{cell}");
        }
    }
}

/// AC-12.2・AC-12.10 text: 人が読む表。末尾に注意・確認日・確認の状態と、参考値の1行。
#[test]
fn ac12_2_report_text_is_a_table_with_the_meta() {
    let run = report(&[]);
    assert_eq!(run.exit, 4, "{run:?}");
    let text = &run.stdout;
    assert!(text.contains("従業員  労働日数  労働時間  延長"), "{text}");
    assert!(
        text.contains("  E004          22     14520  3960"),
        "{text}"
    );
    assert!(
        text.contains("E005: 確定集計を拒否（AmbiguousBoundary。日付: 2026-10-10"),
        "{text}"
    );
    assert!(
        text.contains("E003 2026-10-14: 実労働540分に対して休憩が0分です"),
        "{text}"
    );
    assert!(text.contains("table_checked_on）: 2026-10-15"));
    assert!(text.contains("review_status）: unreviewed"));
    assert!(
        text.ends_with("この出力は参考値です（専門家の確認前。review_status: unreviewed）。\n")
    );
}

/// `--employee` で1人に絞る。拒否のない人だけなら終了コード0。
#[test]
fn employee_filter_computes_one_person() {
    let run = report(&["--format", "json", "--employee", "E001"]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    assert_eq!(out["employees"].as_array().unwrap().len(), 1);
    assert_eq!(out["employees"][0]["employee_id"], "E001");
    let refused = report(&["--format", "json", "--employee", "E005"]);
    assert_eq!(refused.exit, 4);
    assert_eq!(
        failed_code(&report(&["--employee", "E999"]), 2),
        "InvalidInput"
    );
}

/// 打刻のない月は、全員0の行（拒否は打刻の全体で見るので、E005 は打刻のない月でも拒否のまま）。
#[test]
fn month_without_punches_gives_zero_rows() {
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let run = kintai(&[
        "report",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-12",
        "--format",
        "json",
        "--employee",
        "E001",
    ]);
    assert_eq!(run.exit, 0, "{run:?}");
    let ledger = &run.output()["employees"][0]["ledger"][0];
    assert_eq!(ledger["period_start"], "2026-12-01");
    assert_eq!(
        (
            ledger["work_days"].as_i64(),
            ledger["work_minutes"].as_i64()
        ),
        (Some(0), Some(0))
    );
}

/// AC-10.4 同じ入力は、毎回まったく同じバイト列（3つの形とも）。
#[test]
fn ac10_4_same_input_same_bytes() {
    for format in ["text", "json", "csv"] {
        let first = report(&["--format", format]);
        let second = report(&["--format", format]);
        assert_eq!(first, second, "{format}");
    }
}

/// AC-10.4 打刻の行の並びを変えても、出力は変わらない（従業員はID順、日付は昇順に並べ直す）。
#[test]
fn ac10_4_row_order_does_not_change_the_output() {
    let text = fixture_text("punches.csv");
    let mut lines = text.lines();
    let header = lines.next().unwrap();
    let mut rows: Vec<&str> = lines.collect();
    rows.reverse();
    let shuffled = format!("{header}\n{}\n", rows.join("\n"));
    let original = report(&["--format", "json"]);
    let reversed = report_with(
        &fixture_text("policy.toml"),
        &shuffled,
        &["--format", "json"],
    );
    assert_eq!(original, reversed);
}

/// `--data-from` が週の起算日より後なら、その週は `IncompleteWeek`（AC-03.15 をコマンドから）。
#[test]
fn data_from_after_the_week_start_refuses_the_week() {
    let run = report(&[
        "--format",
        "json",
        "--data-from",
        "2026-10-01",
        "--employee",
        "E004",
    ]);
    assert_eq!(run.exit, 4, "{run:?}");
    assert_eq!(
        run.output()["employees"][0]["refused"],
        json!({"code": "AmbiguousBoundary", "dates": ["2026-09-27"], "anomalies": []})
    );
    let explicit = report(&[
        "--format",
        "json",
        "--data-from",
        "2026-09-27",
        "--employee",
        "E004",
    ]);
    assert_eq!(explicit.exit, 0, "{explicit:?}");
}

/// 打刻の異常のある従業員は、その人だけ `PunchAnomaly` で拒否（種類・日付・行番号）。ほかの人は出す。
#[test]
fn punch_anomaly_refuses_only_that_employee() {
    let text = fixture_text("punches.csv");
    let target = "E002,2026-10-30 07:00,out";
    let line = text.lines().position(|row| row == target).unwrap();
    let broken = text.replace(&format!("{target}\n"), "");
    let run = report_with(&fixture_text("policy.toml"), &broken, &["--format", "json"]);
    assert_eq!(run.exit, 4, "{run:?}");
    let out = run.output();
    let refused = &employee(&out, "E002")["refused"];
    assert_eq!(refused["code"], "PunchAnomaly");
    assert_eq!(refused["dates"], json!(["2026-10-29"]));
    // OpenShift の行は、その勤務の in の行（消した行の4行前）。
    assert_eq!(
        refused["anomalies"],
        json!([{"kind": "OpenShift", "date": "2026-10-29", "line": line + 1 - 3}])
    );
    assert_eq!(employee(&out, "E002")["ledger"], json!([]));
    assert_eq!(employee(&out, "E001")["ledger"][0]["work_minutes"], 9152);
}

/// `use_default` では、曖昧さの記録を付けて計算する（終了コード0）。
#[test]
fn use_default_records_the_ambiguities() {
    let policy = fixture_text("policy.toml")
        .replace("ambiguity = \"reject\"", "ambiguity = \"use_default\"");
    let run = report_with(&policy, &fixture_text("punches.csv"), &["--format", "json"]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    let e005 = employee(&out, "E005");
    assert_eq!(e005["refused"], Value::Null);
    assert_eq!(
        e005["ambiguities"],
        json!([{"date": "2026-10-10", "code": "HolidayBoundary"}, {"date": "2026-10-10", "code": "WeekBoundary"}])
    );
    assert!(e005["ledger"][0]["holiday_minutes"].as_i64().unwrap() > 0);
}

/// 秒の方針が `reject` なら、秒のある打刻は不正な入力（行番号つき。AC-01.5 をコマンドから）。
#[test]
fn seconds_reject_makes_the_seconds_invalid() {
    let policy = fixture_text("policy.toml")
        .replace("seconds = \"worker_favorable\"", "seconds = \"reject\"");
    let text = fixture_text("punches.csv");
    let line = text
        .lines()
        .position(|row| row.contains("08:59:40"))
        .unwrap()
        + 1;
    let run = report_with(&policy, &text, &[]);
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    assert!(
        run.stderr.contains(&format!("{line}行目の列 timestamp")),
        "{}",
        run.stderr
    );
}

/// 打刻CSVの従業員IDは、方針の `[[employee]]` にあること（なければ不正な入力。IDは文に出さない）。
#[test]
fn punch_ids_must_be_in_the_policy() {
    let text = format!(
        "{}ZZ9,2026-10-05 09:00,in\nZZ9,2026-10-05 10:00,out\n",
        fixture_text("punches.csv")
    );
    let run = report_with(&fixture_text("policy.toml"), &text, &[]);
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    assert!(run.stderr.contains("442行目"), "{}", run.stderr);
    assert!(!run.stderr.contains("ZZ9"), "{}", run.stderr);
}

/// `rounding = "none"` では、丸めた時間を出さない（`null`）。
#[test]
fn rounding_none_has_no_rounded_hours() {
    let policy =
        fixture_text("policy.toml").replace("rounding = \"monthly_30min\"", "rounding = \"none\"");
    let run = report_with(
        &policy,
        &fixture_text("punches.csv"),
        &["--format", "json", "--employee", "E001"],
    );
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    assert_eq!(out["rounding"], "none");
    assert_eq!(out["employees"][0]["ledger"][0]["rounded"], Value::Null);
}

/// 方針の従業員のうち打刻がない人も、0の行で出す（黙って除外しない）。打刻を E001 だけにすると
/// 打刻CSVの最初の日付が月曜（9/28）になり、既定の `data_from` では最初の週が `IncompleteWeek` になるので、
/// 日曜からそろっていることを `--data-from` で示す。
#[test]
fn employees_without_punches_get_zero_rows() {
    let punches = csv_with_employees(&fixture_text("punches.csv"), &["E001"]);
    let policy = policy_with_employees(&["E001", "E004"]);
    let default = report_with(&policy, &punches, &["--format", "json"]);
    assert_eq!(default.exit, 4, "{default:?}");
    let run = report_with(
        &policy,
        &punches,
        &["--format", "json", "--data-from", "2026-09-27"],
    );
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    assert_eq!(employee(&out, "E004")["ledger"][0]["work_minutes"], 0);
    assert_eq!(employee(&out, "E001")["ledger"][0]["work_minutes"], 9152);
}

/// L2-M19 `use_default` で、対象の月の期間をまたぐ週（9/27〜10/3）の `IncompleteWeek` の記録は、週の起算日
/// （9/27）が期間の前でも、週の7日のどれかが期間に入るので10月の `report` に出す。
#[test]
fn week_record_spanning_the_period_start_is_reported() {
    let policy = fixture_text("policy.toml")
        .replace("ambiguity = \"reject\"", "ambiguity = \"use_default\"");
    let run = report_with(
        &policy,
        &fixture_text("punches.csv"),
        &[
            "--format",
            "json",
            "--data-from",
            "2026-09-28",
            "--employee",
            "E001",
        ],
    );
    assert_eq!(run.exit, 0, "{run:?}");
    assert_eq!(
        run.output()["employees"][0]["ambiguities"],
        json!([{"date": "2026-09-27", "code": "IncompleteWeek"}])
    );
}
