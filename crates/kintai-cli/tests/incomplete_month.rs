//! Monthly totals and daily premiums require the beginning of the monthly period.
mod common;

use common::{kintai, policy_with_employees, temp_dir, write_file};
use std::fmt::Write;

fn punches() -> (String, String) {
    let mut full = String::from("employee_id,timestamp,kind\n");
    // Two complete Mon-Sat weeks: (6 * 12h - 40h) * 2 = 64h overtime.
    for day in [2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14] {
        for (time, kind) in [
            ("08:00", "in"),
            ("12:00", "break_start"),
            ("13:00", "break_end"),
            ("21:00", "out"),
        ] {
            writeln!(full, "E001,2026-11-{day:02}T{time},{kind}").unwrap();
        }
    }
    let last = "E001,2026-11-16T08:00,in\nE001,2026-11-16T12:00,break_start\nE001,2026-11-16T13:00,break_end\nE001,2026-11-16T18:00,out\n";
    full.push_str(last);
    (full, format!("employee_id,timestamp,kind\n{last}"))
}

/// AC-04.1 / AC-12.2: a complete week does not supply the missing monthly overtime.
#[test]
fn incomplete_month_refuses_report_and_daily_premium() {
    let dir = temp_dir("incomplete-month");
    let policy = write_file(&dir, "policy.toml", policy_with_employees(&["E001"]));
    let (full, tail) = punches();
    let full = write_file(&dir, "full.csv", full);
    let tail = write_file(&dir, "tail.csv", tail);
    for (command, date_key, date) in [
        ("report", "--month", "2026-11"),
        ("explain", "--date", "2026-11-16"),
    ] {
        let args = |csv, from| {
            kintai(&[
                command,
                "--policy",
                &policy,
                "--punches",
                csv,
                date_key,
                date,
                "--employee",
                "E001",
                "--data-from",
                from,
                "--format",
                "json",
            ])
        };
        let complete = args(&full, "2026-11-01");
        assert_eq!(complete.exit, 0, "{complete:?}");
        let output = complete.output();
        if command == "report" {
            assert_eq!(output["employees"][0]["ledger"][0]["over60_minutes"], 300);
        } else {
            assert_eq!(output["day"]["over60"], 60);
        }
        let incomplete = args(&tail, "2026-11-15");
        assert_eq!(incomplete.exit, 4, "{incomplete:?}");
        let output = incomplete.output();
        let employee = if command == "report" {
            &output["employees"][0]
        } else {
            &output
        };
        assert_eq!(employee["refused"]["code"], "IncompleteMonth");
        if command == "report" {
            assert_eq!(employee["ledger"], serde_json::json!([]));
        } else {
            assert!(employee["day"].is_null());
            assert_eq!(employee["segments"], serde_json::json!([]));
        }
        // Explicitly certifying that the earlier dates had no work is sufficient.
        assert_eq!(args(&tail, "2026-11-01").exit, 0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

/// AC-12.2: honor a non-calendar monthly boundary, with or without --data-from.
#[test]
fn incomplete_month_uses_policy_start_day_and_inferred_data_from() {
    let dir = temp_dir("incomplete-month-offset");
    let policy = write_file(
        &dir,
        "policy.toml",
        policy_with_employees(&["E001"]).replace("month_start_day = 1\n", "month_start_day = 16\n"),
    );
    let csv = write_file(
        &dir,
        "punches.csv",
        "employee_id,timestamp,kind\nE001,2026-11-22T09:00,in\nE001,2026-11-22T10:00,out\n",
    );
    for from in [None, Some("2026-11-22"), Some("2026-11-15")] {
        let mut args = vec![
            "report",
            "--policy",
            &policy,
            "--punches",
            &csv,
            "--month",
            "2026-11",
            "--format",
            "json",
        ];
        if let Some(from) = from {
            args.extend(["--data-from", from]);
        }
        let run = kintai(&args);
        assert_eq!(
            run.exit,
            if from == Some("2026-11-15") { 0 } else { 4 },
            "{run:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
