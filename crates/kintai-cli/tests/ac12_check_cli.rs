//! `kintai check`（AC-12.3）の試験。合成データは `tests/fixtures/synthetic/`。
//!
//! 合成データの10月の違反と警告は3件: E004 の直近2か月の合計 9660分 ＞ 9600分（`MultiMonthSumExceeded`）、
//! E003 の休憩の不足2日。E005 は曖昧さで拒否。打刻CSVは 2026-09-27（日）から始まるので、判定する協定月は
//! 2026-10 だけで、それより前は履歴CSVの T。

mod common;

use common::{
    csv_with_employees, failed_code, fixture_path, fixture_text, kintai, policy_with_employees,
    temp_dir, write_file,
};

fn check(extra: &[&str]) -> common::Run {
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let mut args = vec![
        "check",
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

/// 違反も警告もない合成データ: E001・E002 の打刻と、E003 の日曜（9/27）の打刻だけ（打刻CSVが日曜から始まる）。
fn clean_files(history: bool) -> (String, String, Option<String>) {
    let dir = temp_dir("check-clean");
    let all = fixture_text("punches.csv");
    let mut punches = csv_with_employees(&all, &["E001", "E002"]);
    for row in all.lines().filter(|row| row.starts_with("E003,2026-09-27")) {
        punches.push_str(row);
        punches.push('\n');
    }
    let policy = write_file(
        &dir,
        "policy.toml",
        policy_with_employees(&["E001", "E002", "E003"]),
    );
    let punches = write_file(&dir, "punches.csv", punches);
    let history = history.then(|| {
        write_file(
            &dir,
            "history.csv",
            csv_with_employees(&fixture_text("history.csv"), &["E001", "E002", "E003"]),
        )
    });
    (policy, punches, history)
}

fn run_clean(history: bool, extra: &[&str]) -> common::Run {
    let (policy, punches, history) = clean_files(history);
    let mut args = vec![
        "check",
        "--policy",
        policy.as_str(),
        "--punches",
        punches.as_str(),
        "--month",
        "2026-10",
    ];
    if let Some(history) = &history {
        args.extend_from_slice(&["--history", history.as_str()]);
    }
    args.extend_from_slice(extra);
    kintai(&args)
}

/// AC-12.3 違反と警告があれば、`--fail-on-violation` で終了コード1。なければ拒否（E005）の4。
#[test]
fn ac12_3_fail_on_violation_exits_1() {
    let history = fixture_path("history.csv");
    let with_flag = check(&["--history", &history, "--fail-on-violation"]);
    assert_eq!(with_flag.exit, 1, "{with_flag:?}");
    assert_eq!(with_flag.stderr, "");
    assert!(with_flag.stdout.contains("違反と警告: 3件（36協定の判定 1件、休憩の不足 2件、打刻の異常 0件）。確定集計を拒否した従業員: 1人"), "{}", with_flag.stdout);
    assert!(with_flag.stdout.contains("MultiMonthSumExceeded(n=2)"));
    assert!(with_flag.stdout.contains("9660"));
    assert!(
        with_flag
            .stdout
            .contains("E005: 確定集計を拒否（AmbiguousBoundary")
    );
    let without = check(&["--history", &history]);
    assert_eq!(without.exit, 4, "{without:?}");
    assert_eq!(
        without.stdout, with_flag.stdout,
        "出力は終了コードによらず同じ"
    );
}

/// AC-12.3 違反も警告もなければ、`--fail-on-violation` でも終了コード0。AC-12.10 末尾は参考値の1行。
#[test]
fn ac12_3_clean_data_exits_0() {
    let run = run_clean(true, &["--fail-on-violation"]);
    assert_eq!(run.exit, 0, "{run:?}");
    assert!(run.stdout.contains("違反と警告: 0件"), "{}", run.stdout);
    assert!(run.stdout.contains("OwnEmployerOnly"));
    assert!(
        run.stdout
            .ends_with("この出力は参考値です（専門家の確認前。review_status: unreviewed）。\n")
    );
}

/// AC-12.3 履歴がないと直近 n か月が分からず `UnknownHistory`（0分とみなさない）。これも違反に数える。
#[test]
fn ac12_3_unknown_history_is_a_violation() {
    let run = run_clean(false, &["--fail-on-violation"]);
    assert_eq!(run.exit, 1, "{run:?}");
    for n in 2..=6 {
        assert!(
            run.stdout.contains(&format!("UnknownHistory(n={n})")),
            "{n}: {}",
            run.stdout
        );
    }
    assert!(run.stdout.contains("不明"));
    assert_eq!(run_clean(false, &[]).exit, 0, "フラグがなければ0");
}

/// AC-12.3 警告（休憩の不足）だけでも、`--fail-on-violation` で終了コード1。
#[test]
fn ac12_3_warning_only_exits_1() {
    let dir = temp_dir("check-warning");
    let policy = write_file(&dir, "policy.toml", policy_with_employees(&["E003"]));
    let punches = write_file(
        &dir,
        "punches.csv",
        csv_with_employees(&fixture_text("punches.csv"), &["E003"]),
    );
    let history = write_file(
        &dir,
        "history.csv",
        csv_with_employees(&fixture_text("history.csv"), &["E003"]),
    );
    let run = kintai(&[
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
    assert_eq!(run.exit, 1, "{run:?}");
    assert!(
        run.stdout
            .contains("違反と警告: 2件（36協定の判定 0件、休憩の不足 2件、打刻の異常 0件）"),
        "{}",
        run.stdout
    );
}

/// 打刻の異常は、違反（`--fail-on-violation` で1）であり、拒否（フラグなしで4）でもある。
#[test]
fn punch_anomaly_is_a_violation_and_a_refusal() {
    let (policy, punches, history) = clean_files(true);
    let text = std::fs::read_to_string(&punches).unwrap();
    let broken = text.replace("E002,2026-10-30 07:00,out\n", "");
    std::fs::write(&punches, broken).unwrap();
    let history = history.unwrap();
    let base = [
        "check",
        "--policy",
        policy.as_str(),
        "--punches",
        punches.as_str(),
        "--month",
        "2026-10",
        "--history",
        history.as_str(),
    ];
    let with_flag = kintai(&[&base[..], &["--fail-on-violation"]].concat());
    assert_eq!(with_flag.exit, 1, "{with_flag:?}");
    assert!(
        with_flag.stdout.contains("PunchAnomaly"),
        "{}",
        with_flag.stdout
    );
    assert!(
        with_flag.stdout.contains("OpenShift 2026-10-29"),
        "{}",
        with_flag.stdout
    );
    let without = kintai(&base);
    assert_eq!(without.exit, 4, "{without:?}");
}

/// 対象の月が、打刻CSVの最初の日付以後に始まる協定月より前なら、判定できない（不正な入力）。
#[test]
fn month_before_the_first_agreement_month_is_invalid() {
    let run = check(&["--month", "2026-09"]);
    // `--month` が2回あるので、まず余分な引数になる。
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let run = kintai(&[
        "check",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-09",
    ]);
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    assert!(run.stderr.contains("より前に始まります"), "{}", run.stderr);
}

/// 履歴の誤りは、履歴CSVの行番号つきの不正な入力。方針にない従業員IDも不正な入力。
#[test]
fn history_errors_name_the_line() {
    let dir = temp_dir("check-history");
    let cases = [
        ("E001,2026-10-01,0\n", "2行目の列 month_start"),
        (
            "E001,2026-09-01,0\nE001,2026-09-01,10\n",
            "3行目の列 month_start",
        ),
        ("E001,2026-09-02,0\n", "2行目の列 month_start"),
        ("ZZ9,2026-09-01,0\n", "2行目の列 employee_id"),
        ("E001,2026-09-01,44641\n", "2行目の列 total_minutes"),
    ];
    for (rows, expected) in cases {
        let history = write_file(
            &dir,
            "history.csv",
            format!("employee_id,month_start,total_minutes\n{rows}"),
        );
        let run = check(&["--history", &history]);
        assert_eq!(failed_code(&run, 2), "InvalidInput", "{rows}");
        assert!(run.stderr.contains(expected), "{rows}: {}", run.stderr);
        assert!(!run.stderr.contains("ZZ9"));
    }
}

/// 協定なし（`kind = "none"`）では、時間外か休日労働のある月が `NoAgreement`。
#[test]
fn no_agreement_flags_any_overtime() {
    let (policy, punches, history) = clean_files(true);
    let text = std::fs::read_to_string(&policy).unwrap();
    std::fs::write(
        &policy,
        text.replace("kind = \"special\"", "kind = \"none\""),
    )
    .unwrap();
    let history = history.unwrap();
    let run = kintai(&[
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
    assert_eq!(run.exit, 1, "{run:?}");
    assert!(run.stdout.contains("NoAgreement"), "{}", run.stdout);
}

/// 方針の `MonthStartDayMismatch`（協定の起算日が違う）は対応外で、計算しない。
#[test]
fn month_start_day_mismatch_is_unsupported() {
    let (policy, punches, _) = clean_files(false);
    let text = std::fs::read_to_string(&policy).unwrap();
    let changed = text.replace(
        "year_start_month = 4\nmonth_start_day = 1",
        "year_start_month = 4\nmonth_start_day = 16",
    );
    assert_ne!(text, changed);
    std::fs::write(&policy, changed).unwrap();
    let run = kintai(&[
        "check",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
    ]);
    assert_eq!(failed_code(&run, 3), "MonthStartDayMismatch");
    assert!(
        run.stderr.contains("平成21年基発0529001号"),
        "{}",
        run.stderr
    );
}

/// `use_default` では拒否せず、判定する期間に重なる曖昧さの記録を出す。週の記録（`IncompleteWeek`）は、
/// 週の起算日が期間の前でも、週の7日のどれかが期間に入れば出す。
#[test]
fn use_default_lists_ambiguities_in_the_range() {
    let dir = temp_dir("check-default");
    let policy = policy_with_employees(&["E001", "E005"])
        .replace("ambiguity = \"reject\"", "ambiguity = \"use_default\"");
    let policy = write_file(&dir, "policy.toml", policy);
    // E003 の日曜の打刻を除くので、打刻CSVは月曜（9/28）から始まり、9/27 の週がそろっていない。
    let punches = write_file(
        &dir,
        "punches.csv",
        csv_with_employees(&fixture_text("punches.csv"), &["E001", "E005"]),
    );
    let run = kintai(&[
        "check",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
    ]);
    assert_eq!(run.exit, 0, "{run:?}");
    for record in [
        "  E001 2026-09-27 IncompleteWeek",
        "  E005 2026-09-27 IncompleteWeek",
        "  E005 2026-10-10 HolidayBoundary",
        "  E005 2026-10-10 WeekBoundary",
    ] {
        assert!(run.stdout.contains(record), "{record}\n{}", run.stdout);
    }
    assert!(!run.stdout.contains("確定集計を拒否（"), "{}", run.stdout);
}

/// E001・E002 の打刻だけ（月曜 9/28 から始まる）で `check` を動かす。
fn check_monday_start(extra: &[&str], from_october_5th: bool) -> common::Run {
    let dir = temp_dir("check-data-from");
    let mut punches = csv_with_employees(&fixture_text("punches.csv"), &["E001", "E002"]);
    if from_october_5th {
        let mut lines = punches.lines();
        let header = lines.next().unwrap().to_owned();
        let kept: Vec<&str> = lines
            .filter(|row| row.split(',').nth(1).unwrap() >= "2026-10-05")
            .collect();
        punches = format!("{header}\n{}\n", kept.join("\n"));
    }
    let policy = write_file(
        &dir,
        "policy.toml",
        policy_with_employees(&["E001", "E002"]),
    );
    let punches = write_file(&dir, "punches.csv", punches);
    let history = write_file(
        &dir,
        "history.csv",
        csv_with_employees(&fixture_text("history.csv"), &["E001", "E002"]),
    );
    let mut args = vec![
        "check",
        "--policy",
        policy.as_str(),
        "--punches",
        punches.as_str(),
        "--month",
        "2026-10",
        "--history",
        history.as_str(),
    ];
    args.extend_from_slice(extra);
    kintai(&args)
}

/// `--data-from`（`report` と同じ意味）: 打刻CSVが月曜から始まると、既定では最初の週が `IncompleteWeek`
/// で拒否される。日曜からそろっていることを `--data-from` で伝えると避けられる。
#[test]
fn data_from_avoids_the_incomplete_week_in_check() {
    let refused = check_monday_start(&[], false);
    assert_eq!(refused.exit, 4, "{refused:?}");
    assert!(
        refused
            .stdout
            .contains("E001: 確定集計を拒否（AmbiguousBoundary。日付: 2026-09-27"),
        "{}",
        refused.stdout
    );
    assert!(
        refused
            .stdout
            .contains("打刻がそろっている最初の日（data_from）: 2026-09-28")
    );
    let avoided = check_monday_start(&["--data-from", "2026-09-27", "--fail-on-violation"], false);
    assert_eq!(avoided.exit, 0, "{avoided:?}");
    assert!(
        avoided.stdout.contains("違反と警告: 0件"),
        "{}",
        avoided.stdout
    );
    assert!(
        avoided
            .stdout
            .contains("打刻がそろっている最初の日（data_from）: 2026-09-27")
    );
}

/// 判定する最初の協定月も `--data-from` 以後で決まる（既定は打刻CSVの最初の日付）。打刻が 10/05 から
/// しかないと、既定では10月の協定月がそろわず判定できないが、9/27 からそろっている（その間は勤務なし）と
/// 伝えれば、10月を判定する。
#[test]
fn data_from_sets_the_first_agreement_month() {
    let without = check_monday_start(&[], true);
    assert_eq!(common::failed_code(&without, 2), "InvalidInput");
    assert!(without.stderr.contains("--data-from"), "{}", without.stderr);
    let with = check_monday_start(&["--data-from", "2026-09-27", "--fail-on-violation"], true);
    assert_eq!(with.exit, 0, "{with:?}");
    assert!(
        with.stdout
            .contains("判定する協定月: 2026-10-01 から 2026-10-01 まで"),
        "{}",
        with.stdout
    );
    let late = check_monday_start(&["--data-from", "2026-10-02"], true);
    assert_eq!(
        common::failed_code(&late, 2),
        "InvalidInput",
        "10/02 以後に始まる協定月は11月"
    );
}

/// L2-M21 休憩の不足は、判定する期間（10/01〜10/31）の労働日だけを数える。翌の期間の初日（11/01）の
/// 休憩の不足は、10月の `check` に入れない。
#[test]
fn break_shortage_on_the_next_period_start_is_not_counted() {
    let dir = temp_dir("check-next-period");
    let punches = format!(
        "{}E003,2026-11-01 09:00,in\nE003,2026-11-01 18:00,out\n",
        fixture_text("punches.csv")
    );
    let punches = write_file(&dir, "punches.csv", punches);
    let policy = fixture_path("policy.toml");
    let history = fixture_path("history.csv");
    let run = kintai(&[
        "check",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
        "--history",
        &history,
    ]);
    assert_eq!(run.exit, 4, "{run:?}");
    assert!(run.stdout.contains("休憩の不足 2件"), "{}", run.stdout);
    assert!(!run.stdout.contains("E003 2026-11-01"), "{}", run.stdout);
}
