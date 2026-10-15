//! 対応外の拒否と、検査の順序（`docs/spec/unsupported.md`、`docs/spec/validation.md`）。実際のバイナリを動かす。
//!
//! - AC-11.1: 表のコードごとに、該当する指定があると計算を拒否して終了コード3にする。
//! - AC-11.2: 拒否のエラーに、コードと条文を含める。
//! - AC-11.3: `legal_holiday_weekday` のない方針は `LegalHolidayUnspecified` で拒否する。
//! - AC-11.4: 拒否した従業員の分を、黙って0や通常制として出さない（標準出力に何も出さない）。
//! - AC-13.1: 検査の順序（1→5）。同じ入力に複数の誤りがあっても最初のものを返す。
//! - AC-13.2: 対応外の指定の表の全行が、対応するコードを返す。表の順に見て最初に当たる。

mod common;

use common::{
    assert_invalid, assert_unsupported, base_policy, case_with, run_case, with_policy_item,
};
use serde_json::{Value, json};

/// `docs/spec/validation.md` の「対応外の指定」の表（項目、値、コード）。表の順。
const TABLE: [(&str, &str, &str); 24] = [
    ("work_system", "variable_monthly", "VariableMonthly"),
    ("work_system", "variable_yearly", "VariableYearly"),
    (
        "work_system",
        "variable_weekly_irregular",
        "VariableWeeklyIrregular",
    ),
    ("work_system", "flex", "Flex"),
    ("work_system", "discretionary", "DiscretionaryDeemed"),
    ("work_system", "offsite_deemed", "OffsiteDeemed"),
    ("work_system", "managerial", "Managerial"),
    ("work_system", "agriculture_exempt", "AgricultureExempt"),
    (
        "work_system",
        "surveillance_intermittent",
        "SurveillanceIntermittent",
    ),
    ("work_system", "night_duty_permit", "NightDutyPermit"),
    ("work_system", "highly_professional", "HighlyProfessional"),
    ("worker_kind", "minor", "Minor"),
    (
        "worker_kind",
        "pregnant_or_postpartum",
        "PregnantOrPostpartum",
    ),
    ("worker_kind", "research_development", "ResearchDevelopment"),
    ("worker_kind", "driver", "AutomobileDriver"),
    ("worker_kind", "physician", "Physician"),
    (
        "worker_kind",
        "disaster_recovery_construction",
        "DisasterRecoveryConstruction",
    ),
    ("worker_kind", "hazardous_work", "HazardousWork"),
    ("holiday_system", "four_weeks", "FourWeekHolidays"),
    (
        "holiday_system",
        "continuous_rotation",
        "ContinuousRotation",
    ),
    ("holiday_swaps", "", "HolidaySwap"),
    ("night_window", "23-06", "NightWindowOverride"),
    ("legal_holiday_weekday", "", "LegalHolidayUnspecified"),
    (
        "weekly_limit_minutes",
        "2640",
        "Weekly44WithoutConfirmation",
    ),
];

fn row_case(key: &str, value: &str) -> Value {
    match key {
        "weekly_limit_minutes" => with_policy_item(key, json!(2640)),
        "legal_holiday_weekday" => with_policy_item(key, Value::Null),
        // 要素の中身は検査しない（空でなければ対応外）。
        "holiday_swaps" => with_policy_item(key, json!([{"from": "2026-10-04"}, 7])),
        _ => with_policy_item(key, json!(value)),
    }
}

/// AC-11.1 AC-13.2 表の全行が、対応するコードと終了コード3を返す。標準出力には何も出さない（AC-11.4）。
#[test]
fn ac11_1_every_row_of_the_table() {
    for (key, value, code) in TABLE {
        assert_unsupported(&row_case(key, value), code);
    }
}

/// AC-11.2 拒否のエラーの説明に、条文（法令名と条項）を含める。
#[test]
fn ac11_2_message_cites_the_article() {
    let articles = [
        ("Flex", "労働基準法第32条の3"),
        ("VariableMonthly", "労働基準法第32条の2"),
        ("VariableYearly", "労働基準法第32条の4"),
        ("VariableWeeklyIrregular", "労働基準法第32条の5"),
        ("DiscretionaryDeemed", "労働基準法第38条の3"),
        ("OffsiteDeemed", "労働基準法第38条の2"),
        ("Managerial", "労働基準法第41条第2号"),
        ("AgricultureExempt", "労働基準法第41条第1号"),
        ("SurveillanceIntermittent", "労働基準法第41条第3号"),
        ("NightDutyPermit", "労働基準法施行規則第23条"),
        ("HighlyProfessional", "労働基準法第41条の2"),
        ("Minor", "労働基準法第60条"),
        ("PregnantOrPostpartum", "労働基準法第66条"),
        ("AutomobileDriver", "労働基準法附則第140条"),
        ("Physician", "労働基準法附則第141条"),
        ("DisasterRecoveryConstruction", "労働基準法附則第139条第1項"),
        ("HazardousWork", "労働基準法第36条第6項第1号"),
        ("ResearchDevelopment", "労働基準法第36条第11項"),
        ("FourWeekHolidays", "労働基準法第35条第2項"),
        ("ContinuousRotation", "労働基準法第35条"),
        ("HolidaySwap", "労働基準法第35条"),
        ("NightWindowOverride", "労働基準法第37条第4項"),
        ("Weekly44WithoutConfirmation", "労働基準法施行規則第25条の2"),
        ("LegalHolidayUnspecified", "労働基準法第35条"),
    ];
    for (key, value, code) in TABLE {
        let run = run_case(&row_case(key, value));
        let error: Value = serde_json::from_str(&run.stderr).unwrap();
        assert_eq!(error["error"]["code"], code);
        let message = error["error"]["message"].as_str().unwrap();
        let article = articles.iter().find(|(c, _)| *c == code).unwrap().1;
        assert!(message.contains(article), "{code}: {message}");
    }
}

/// AC-11.3 法定休日の曜日がない、または `null` の方針。
#[test]
fn ac11_3_missing_or_null_legal_holiday() {
    assert_unsupported(
        &with_policy_item("legal_holiday_weekday", Value::Null),
        "LegalHolidayUnspecified",
    );
    let mut policy = base_policy();
    policy["legal_holiday_weekday"] = Value::Null;
    assert_unsupported(&case_with(policy, json!([])), "LegalHolidayUnspecified");
}

/// AC-11.4 拒否のときは、勤務があっても日ごとの分を出さない（0としても出さない）。
#[test]
fn ac11_4_nothing_is_printed_for_refused_workers() {
    let case = with_policy_item("work_system", json!("managerial"));
    let run = run_case(&case);
    assert_eq!(run.exit, 3);
    assert_eq!(run.stdout, "");
}

fn bad_shift() -> Value {
    json!([{"start": "2026-10-05T17:00", "end": "2026-10-05T09:00"}])
}

/// AC-13.1 手順1（型・知らない項目）は手順3（対応外）より先。勤務の中の型の誤りも手順1。
#[test]
fn ac13_1_shape_errors_come_first() {
    let mut policy = base_policy();
    policy["work_system"] = json!("flex");
    policy["unknown_item"] = json!(1);
    assert_invalid(&case_with(policy, json!([])));

    let mut policy = base_policy();
    policy["work_system"] = json!("flex");
    assert_invalid(&case_with(
        policy.clone(),
        json!([{"start": 900, "end": "2026-10-05T17:00"}]),
    ));
    assert_invalid(&case_with(
        policy,
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00", "rest": []}]),
    ));
}

/// AC-13.1 手順2（列挙の値・数値の範囲）は手順3より先。
#[test]
fn ac13_1_value_errors_come_before_unsupported() {
    for (key, value) in [
        ("night_window", json!("21-04")),
        ("month_start_day", json!(0)),
        ("scheduled_minutes_per_day", json!(600)),
        ("weekly_limit_minutes", json!(1)),
        ("week_start", json!("monday")),
        ("ambiguity", json!("maybe")),
    ] {
        let mut policy = base_policy();
        policy["work_system"] = json!("flex");
        policy
            .as_object_mut()
            .unwrap()
            .remove("legal_holiday_weekday");
        policy[key] = value;
        assert_invalid(&case_with(policy, json!([])));
    }
}

/// AC-13.1 手順3（対応外）は手順4（勤務の検査）より先。日時の表記の誤りも手順4。
#[test]
fn ac13_1_unsupported_comes_before_shift_errors() {
    let shifts = [
        bad_shift(),
        json!([{"start": "2026-02-30T09:00", "end": "2026-02-30T17:00"}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T24:00"}]),
        json!([{"start": "1999-12-31T09:00", "end": "1999-12-31T17:00"}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-06T09:00"}]),
    ];
    for s in shifts {
        let mut policy = base_policy();
        policy["work_system"] = json!("flex");
        assert_unsupported(&case_with(policy, s.clone()), "Flex");
        let mut policy = base_policy();
        policy
            .as_object_mut()
            .unwrap()
            .remove("legal_holiday_weekday");
        assert_unsupported(&case_with(policy, s), "LegalHolidayUnspecified");
    }
}

/// AC-13.1 手順4（勤務の検査）は手順5（曖昧さによる拒否）より先。
#[test]
fn ac13_1_shift_errors_come_before_refusal() {
    let case = case_with(
        base_policy(),
        json!([
            {"start": "2026-10-10T22:00", "end": "2026-10-11T02:00"},
            {"start": "2026-10-10T23:00", "end": "2026-10-11T01:00"}
        ]),
    );
    assert_invalid(&case);
}

/// AC-13.2 表の順に見て、最初に当たったものを返す。
#[test]
fn ac13_2_first_row_wins() {
    let mut policy = base_policy();
    policy["work_system"] = json!("managerial");
    policy["worker_kind"] = json!("minor");
    policy["holiday_system"] = json!("four_weeks");
    policy["holiday_swaps"] = json!(["2026-10-04"]);
    policy["night_window"] = json!("23-06");
    policy["weekly_limit_minutes"] = json!(2640);
    policy
        .as_object_mut()
        .unwrap()
        .remove("legal_holiday_weekday");
    let order = [
        ("work_system", "Managerial", json!("standard")),
        ("worker_kind", "Minor", json!("standard")),
        ("holiday_system", "FourWeekHolidays", json!("weekly")),
        ("holiday_swaps", "HolidaySwap", json!([])),
        ("night_window", "NightWindowOverride", json!("22-05")),
        (
            "legal_holiday_weekday",
            "LegalHolidayUnspecified",
            json!("sun"),
        ),
    ];
    for (key, code, fixed) in order {
        assert_unsupported(&case_with(policy.clone(), json!([])), code);
        policy[key] = fixed;
    }
    assert_unsupported(
        &case_with(policy.clone(), json!([])),
        "Weekly44WithoutConfirmation",
    );
    policy["special_44h_confirmed"] = json!(true);
    assert_eq!(run_case(&case_with(policy, json!([]))).exit, 0);
}
