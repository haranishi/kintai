//! エラーの形と終了コード（AC-12.8）、`case` の3種類（AC-12.6）、`retention`（AC-08.5 をコマンドから）、
//! 方針ファイルの検査（AC-12.1 をコマンドから）、入力の上限（AC-10.6）、エラーの文（AC-10.7）の試験。

mod common;

use common::{
    Run, failed_code, fixture_path, fixture_text, kintai, run_case, temp_dir, valid_case,
    write_file,
};
use serde_json::json;

fn report_with_policy(policy: &str) -> Run {
    let dir = temp_dir("errors");
    let policy = write_file(&dir, "policy.toml", policy);
    let punches = fixture_path("punches.csv");
    kintai(&[
        "report",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
    ])
}

/// AC-12.8 引数の誤りは、どのコマンドでも標準エラーに JSON で、終了コード2。
/// 表の `P` `C` `L` は、合成データの方針・打刻・休暇のファイルに置き換える。
#[test]
fn ac12_8_argument_errors_are_json_with_exit_2() {
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let leaves = fixture_path("leaves.csv");
    let cases = [
        "",
        "unknown-command",
        "report",
        "report --policy P --punches C",
        "report --policy P --punches C --month 2026-13",
        "report --policy P --punches C --month 2026-10 --format xml",
        "report --policy P --punches C --month 2026-10 --surprise",
        "report --policy P --punches C --month 2026-10 --data-from 2026-10-32",
        "report --policy /no/such/file.toml --punches C --month 2026-10",
        "check --policy P --punches C",
        "check --policy P --punches C --month 2026-10 --format json",
        "check --policy P --punches C --month 2026-10 --data-from 2026-02-30",
        "explain --policy P --punches C --employee E001 --date 2026-10-08 --data-from 2026-1-1",
        "leave --policy P --leaves L",
        "explain --policy P --punches C --employee E001",
        "retention --kind payslip --completed 2026-10-31 --wage-due 2026-11-25",
        "retention --kind wage-ledger --completed 2026-10-31",
        "case",
        "case /no/such/case.json",
    ];
    for case in cases {
        let words: Vec<&str> = case
            .split_whitespace()
            .map(|word| match word {
                "P" => policy.as_str(),
                "C" => punches.as_str(),
                "L" => leaves.as_str(),
                other => other,
            })
            .collect();
        let run = kintai(&words);
        assert_eq!(failed_code(&run, 2), "InvalidInput", "{case}");
    }
}

/// AC-12.8 対応外は3、拒否は4、違反は1、成功は0。2と3は標準出力が空で、標準エラーが JSON。
#[test]
fn ac12_8_exit_codes_follow_the_table() {
    let flex =
        fixture_text("policy.toml").replace("work_system = \"standard\"", "work_system = \"flex\"");
    assert_eq!(failed_code(&report_with_policy(&flex), 3), "Flex");
    let refused = report_with_policy(&fixture_text("policy.toml"));
    assert_eq!((refused.exit, refused.stderr.as_str()), (4, ""));
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let history = fixture_path("history.csv");
    let violation = kintai(&[
        "check",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
        "--history",
        &history,
        "--fail-on-violation",
    ]);
    assert_eq!((violation.exit, violation.stderr.as_str()), (1, ""));
    let ok = kintai(&[
        "retention",
        "--kind",
        "attendance",
        "--completed",
        "2026-04-30",
        "--wage-due",
        "2026-05-15",
    ]);
    assert_eq!((ok.exit, ok.stderr.as_str()), (0, ""));
}

/// AC-12.1 方針ファイルの検査をコマンドから: 必須・知らない項目・範囲・型は2、対応外は3（ID つき）。
#[test]
fn ac12_1_policy_file_errors_through_the_command() {
    let base = fixture_text("policy.toml");
    let invalid = [
        base.replace("schema = \"kintai.policy.v1\"\n", ""),
        base.replace("[workplace]\n", "[workplace]\nmystery = true\n"),
        base.replace(
            "month_start_day = 1\nambiguity",
            "month_start_day = 0\nambiguity",
        ),
        base.replace("rounding = \"monthly_30min\"", "rounding = \"daily_floor\""),
        base.replace("seconds = \"worker_favorable\"", "seconds = \"round\""),
        base.replace(
            "scheduled_minutes_per_day = 420",
            "scheduled_minutes_per_day = \"420\"",
        ),
        base.replace("special_months_max = 6", "special_months_max = 7"),
        base.replace(
            "worker_kind = \"standard\"",
            "worker_kind = \"construction\"",
        ),
        base.replace("hire_date = \"2024-04-01\"", "hire_date = \"2024-02-30\""),
        format!("{base}\n[extra]\nx = 1\n"),
        String::from("not toml at all ==="),
        String::new(),
    ];
    for text in &invalid {
        let run = report_with_policy(text);
        assert_eq!(failed_code(&run, 2), "InvalidInput", "{text}");
    }
    let unsupported = [
        (
            base.replace(
                "work_system = \"standard\"",
                "work_system = \"variable_monthly\"",
            ),
            "VariableMonthly",
        ),
        (
            base.replace("worker_kind = \"standard\"", "worker_kind = \"minor\""),
            "Minor",
        ),
        (
            base.replace("legal_holiday_weekday = \"sun\"\n", ""),
            "LegalHolidayUnspecified",
        ),
        (
            base.replace("weekly_limit_minutes = 2400", "weekly_limit_minutes = 2640"),
            "Weekly44WithoutConfirmation",
        ),
        (
            base.replace(
                "[workplace]\n",
                "[workplace]\nholiday_swaps = [\"2026-10-04\"]\n",
            ),
            "HolidaySwap",
        ),
        (
            base.replace("[workplace]\n", "[workplace]\nnight_window = \"23-06\"\n"),
            "NightWindowOverride",
        ),
        (
            base.replace(
                "[workplace]\n",
                "[workplace]\nholiday_system = \"four_weeks\"\n",
            ),
            "FourWeekHolidays",
        ),
        (
            base.replace(
                "year_start_month = 4\nmonth_start_day = 1",
                "year_start_month = 4\nmonth_start_day = 2",
            ),
            "MonthStartDayMismatch",
        ),
    ];
    for (text, code) in &unsupported {
        let run = report_with_policy(text);
        assert_eq!(failed_code(&run, 3), *code, "{text}");
    }
    let minor = report_with_policy(&unsupported[1].0);
    assert!(
        minor.stderr.contains("E001"),
        "エラーに従業員ID: {}",
        minor.stderr
    );
    // AC-08.4 丸めに日ごとの切り捨ての値はない（範囲外の値は不正な入力。上の daily_floor）。
}

/// AC-12.6 `case` は3種類のケースを処理し、`schema` が未知なら不正な入力。
#[test]
fn ac12_6_case_handles_three_schemas() {
    assert_eq!(
        run_case(&valid_case()).output()["schema"],
        "kintai.classify.v1"
    );
    let limits = run_case(&json!({
        "schema": "kintai.case36.v1",
        "agreement": {"kind": "general", "year_start_month": 4, "monthly_limit_minutes": 2700, "annual_limit_minutes": 21600},
        "months": [{"start": "2026-04-01", "overtime": 0, "holiday": 0}],
        "history": []
    }));
    assert_eq!(limits.output()["schema"], "kintai.limits36.v1");
    let leave = run_case(&json!({
        "schema": "kintai.caseleave.v1", "as_of": "2026-10-31",
        "employee": {"hire_date": "2024-04-01", "weekly_scheduled_days": 5, "weekly_scheduled_minutes": 2400, "attendance": []},
        "taken": []
    }));
    assert_eq!(leave.output()["schema"], "kintai.leave.v1");
    for schema in ["kintai.case.v2", "kintai.policy.v1", ""] {
        let mut case = valid_case();
        case["schema"] = json!(schema);
        assert_eq!(failed_code(&run_case(&case), 2), "InvalidInput", "{schema}");
    }
}

/// AC-08.5 保存期限が種類ごとに出る（賃金台帳・重要な書類・年休管理簿は遅い方、状況の記録は完了の日から）。
/// AC-12.10 末尾は参考値の1行。
#[test]
fn ac08_5_retention_through_the_command() {
    let retention = |kind: &str, completed: &str, due: &str| {
        let run = kintai(&[
            "retention",
            "--kind",
            kind,
            "--completed",
            completed,
            "--wage-due",
            due,
        ]);
        assert_eq!(run.exit, 0, "{run:?}");
        run.stdout
    };
    let wage = retention("wage-ledger", "2026-04-30", "2026-05-15");
    assert!(wage.contains("起算日（start）: 2026-05-15"), "{wage}");
    assert!(wage.contains("statutory_current: 2029-05-15"));
    assert!(wage.contains("statutory_base: 2031-05-15"));
    assert!(wage.contains("product_default: 2031-05-15"));
    assert!(wage.contains("労働基準法附則第143条第1項"));
    let status = retention("status-record", "2026-04-30", "2026-05-15");
    assert!(status.contains("起算日（start）: 2026-04-30"), "{status}");
    assert!(status.contains("statutory_base: 2029-04-30"), "{status}");
    assert!(status.contains("product_default: 2031-04-30"));
    let register = retention("leave-register", "2026-09-30", "2026-09-25");
    assert!(
        register.contains("statutory_current: 2029-09-30"),
        "{register}"
    );
    assert!(register.contains("付与の期間中も保存します"));
    let leap = retention("attendance", "2027-02-28", "2027-02-28");
    assert!(
        leap.contains("product_default: 2032-02-29"),
        "民法の期間の数え方: {leap}"
    );
    assert!(
        leap.ends_with("この出力は参考値です（専門家の確認前。review_status: unreviewed）。\n")
    );
}

/// AC-10.7 エラーの文に、入力の行の中身（値・知らない項目の名前・従業員ID）が入らない。
#[test]
fn ac10_7_error_messages_do_not_echo_input_rows() {
    let marker = "QQSECRET9";
    let dir = temp_dir("errors-echo");
    let policy = fixture_path("policy.toml");
    let punches_cases = [
        format!("employee_id,timestamp,kind\nE001,{marker},in\n"),
        format!("employee_id,timestamp,kind\nE001,2026-10-05 09:00,{marker}\n"),
        format!("employee_id,timestamp,kind\n{marker}x,2026-10-05 09:00,in\n"),
        format!("employee_id,timestamp,{marker}\n"),
        format!("employee_id,timestamp,kind\nE001,2026-10-05 09:00,in,{marker}\n"),
    ];
    for text in punches_cases {
        let punches = write_file(&dir, "punches.csv", &text);
        let run = kintai(&[
            "report",
            "--policy",
            &policy,
            "--punches",
            &punches,
            "--month",
            "2026-10",
        ]);
        assert_eq!(failed_code(&run, 2), "InvalidInput", "{text}");
        assert!(!run.stderr.contains(marker), "{}", run.stderr);
    }
    let policy_cases = [
        fixture_text("policy.toml")
            .replace("[workplace]\n", &format!("[workplace]\n{marker} = 1\n")),
        fixture_text("policy.toml").replace("id = \"E004\"", &format!("id = \"{marker}!\"")),
        fixture_text("policy.toml").replace(
            "ambiguity = \"reject\"",
            &format!("ambiguity = \"{marker}\""),
        ),
        fixture_text("policy.toml").replace(
            "hire_date = \"2026-04-01\"",
            &format!("hire_date = \"{marker}\""),
        ),
    ];
    for text in policy_cases {
        let run = report_with_policy(&text);
        assert_eq!(failed_code(&run, 2), "InvalidInput");
        assert!(!run.stderr.contains(marker), "{}", run.stderr);
    }
}

/// AC-10.6 入力の上限: 方針ファイルの1行は4096バイトまで、日付は2000〜2099年、ファイルは256MiBまで。
#[test]
fn ac10_6_input_limits_are_invalid_input() {
    let long = format!("{}# {}\n", fixture_text("policy.toml"), "x".repeat(4095));
    let run = report_with_policy(&long);
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    assert!(run.stderr.contains("行が長すぎます"), "{}", run.stderr);
    let fits = format!("{}# {}\n", fixture_text("policy.toml"), "x".repeat(4094));
    assert_eq!(
        report_with_policy(&fits).exit,
        4,
        "4096バイトちょうどは読める"
    );
    let early = fixture_text("policy.toml")
        .replace("hire_date = \"2024-04-01\"", "hire_date = \"1999-12-31\"");
    assert_eq!(failed_code(&report_with_policy(&early), 2), "InvalidInput");
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    for month in ["1999-12", "2100-01"] {
        let run = kintai(&[
            "report",
            "--policy",
            &policy,
            "--punches",
            &punches,
            "--month",
            month,
        ]);
        assert_eq!(failed_code(&run, 2), "InvalidInput", "{month}");
    }
    // 256MiB を1バイト超えるファイル（中身のない大きなファイル。多くのファイルシステムでは場所をとらない）。
    let dir = temp_dir("errors-size");
    let path = dir.join("huge.toml");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(256 * 1024 * 1024 + 1).unwrap();
    drop(file);
    let run = kintai(&[
        "report",
        "--policy",
        path.to_str().unwrap(),
        "--punches",
        &punches,
        "--month",
        "2026-10",
    ]);
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    assert!(run.stderr.contains("大きすぎます"), "{}", run.stderr);
    std::fs::remove_file(&path).unwrap();
}

/// `--help` は、すべてのコマンドを説明する。`--version` は版とハッシュ。
#[test]
fn help_lists_every_command() {
    let help = kintai(&["--help"]);
    assert_eq!(help.exit, 0);
    for command in [
        "report",
        "check",
        "leave",
        "explain",
        "retention",
        "case",
        "demo",
    ] {
        assert!(
            help.stdout.contains(&format!("kintai {command}")),
            "{command}"
        );
    }
    let version = kintai(&["--version"]);
    assert!(version.stdout.starts_with("kintai 0.0.1 ("), "{version:?}");
}

/// AC-10.7・L2-02 秒の切り上げで勤務が日時の範囲の外（2100-01-01T00:00）になるときも、エラーの文に
/// 従業員ID（名前に見えるIDもありうる）を出さず、その打刻の行番号を示す。
#[test]
fn l2_02_out_of_range_shift_error_hides_the_employee_id() {
    let marker = "Tanaka.Taro-1980";
    let dir = temp_dir("errors-2099");
    let policy = write_file(
        &dir,
        "policy.toml",
        fixture_text("policy.toml").replace("id = \"E001\"", &format!("id = \"{marker}\"")),
    );
    let punches = write_file(
        &dir,
        "punches.csv",
        format!(
            "employee_id,timestamp,kind\n{marker},2099-12-31 20:00,in\n{marker},2099-12-31 23:59:30,out\n"
        ),
    );
    for command in ["report", "check"] {
        let run = kintai(&[
            command,
            "--policy",
            &policy,
            "--punches",
            &punches,
            "--month",
            "2099-12",
            "--data-from",
            "2099-12-01",
        ]);
        assert_eq!(failed_code(&run, 2), "InvalidInput", "{command}");
        assert!(!run.stderr.contains(marker), "{command}: {}", run.stderr);
        assert!(
            run.stderr.contains("3行目の列 timestamp"),
            "{command}: {}",
            run.stderr
        );
    }
}

/// L2-11 `retention` の先頭の3行は `名前: 日付` だけ（`docs/spec/cli-output.md`）。説明はその後の行。
#[test]
fn l2_11_retention_starts_with_three_date_lines() {
    let run = kintai(&[
        "retention",
        "--kind",
        "attendance",
        "--completed",
        "2026-10-31",
        "--wage-due",
        "2026-11-25",
    ]);
    assert_eq!(run.exit, 0, "{run:?}");
    let first: Vec<&str> = run.stdout.lines().take(3).collect();
    assert_eq!(
        first,
        [
            "statutory_current: 2029-11-25",
            "statutory_base: 2031-11-25",
            "product_default: 2031-11-25"
        ]
    );
    assert!(
        run.stdout
            .contains("statutory_current は現行の法定の最低（当分の間3年）")
    );
}
