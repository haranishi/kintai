//! `kintai case FILE` の入出力（`docs/spec/case-format.md`）。実際のバイナリを動かす。
//!
//! - AC-09.1: 入力の検査（開始 < 終了、重なり、休憩が勤務の内側、必須項目、値の範囲）に失敗したら不正な入力。
//! - AC-09.2: 出力の合計の関係（total、night）が常に成り立つ。
//! - AC-09.3: 同じ入力は、毎回まったく同じ出力（順序・空白を含む）になる。
//! - AC-09.4: 終了コードが表のとおり（0・2・3・4）。

mod common;

use common::{
    assert_invalid, base_policy, case_with, kintai, official_cases, one_shift, run_case,
    run_case_bytes, valid_case, with_policy_item,
};
use serde_json::{Value, json};

/// 仕様の例と同じ1行（項目の並び、空白なし、末尾の改行）。
const EXAMPLE_1_OUTPUT: &str = concat!(
    r#"{"schema":"kintai.classify.v1","days":[{"date":"2026-10-07","total":1140,"within":480,"#,
    r#""extra_within":60,"overtime":660,"over60":0,"holiday":0,"night":420,"night_within":0,"#,
    r#""night_overtime":420,"night_over60":0,"night_holiday":0}],"periods":[{"start":"2026-10-01","#,
    r#""total":1140,"within":480,"overtime":660,"over60":0,"holiday":0,"night":420}],"#,
    r#""ambiguities":[],"refused":null}"#,
    "\n"
);

/// AC-09.3 出力は仕様の例の項目の並びで、インデントなしの1行＋末尾の改行。何度動かしても同じバイト列。
#[test]
fn ac09_3_output_is_one_deterministic_line() {
    let (_, vector) = &official_cases()[0];
    let first = run_case(&vector["case"]);
    assert_eq!(first.exit, 0, "{first:?}");
    assert_eq!(first.stdout, EXAMPLE_1_OUTPUT);
    assert_eq!(first.stderr, "");
    for _ in 0..3 {
        assert_eq!(run_case(&vector["case"]), first);
    }
    // 誤りの出力も毎回同じ。
    let broken = run_case_bytes(b"{");
    assert_eq!(run_case_bytes(b"{"), broken);
}

/// AC-09.2 AC-09.4 数値例（公的な7件と導出の例）を、バイナリで計算して期待値と比べる。
/// 終了コードは `expect.exit_code`（なければ0）。`refused` と `ambiguities` もあれば比べる。合計の関係も確かめる。
#[test]
fn ac09_2_official_vectors_through_the_binary() {
    for (id, vector) in official_cases() {
        let expect = &vector["expect"];
        let run = run_case(&vector["case"]);
        let exit = expect["exit_code"].as_i64().unwrap_or(0);
        assert_eq!(i64::from(run.exit), exit, "{id}: {run:?}");
        assert_eq!(run.stderr, "", "{id}");
        let out = run.output();
        assert_eq!(out["schema"], "kintai.classify.v1");
        assert_eq!(
            out["refused"],
            expect.get("refused").cloned().unwrap_or(Value::Null),
            "{id}"
        );
        if let Some(ambiguities) = expect.get("ambiguities") {
            assert_eq!(&out["ambiguities"], ambiguities, "{id}");
        }
        let expected_days = expect["days"].as_object().unwrap();
        let days = out["days"].as_array().unwrap();
        assert_eq!(days.len(), expected_days.len(), "{id}");
        for day in days {
            let date = day["date"].as_str().unwrap();
            let expected = &expected_days[date];
            for (field, value) in expected.as_object().unwrap() {
                assert_eq!(&day[field], value, "{id} {date} {field}");
            }
            check_day_sums(day);
        }
        let expected_periods = expect["periods"].as_object().unwrap();
        let periods = out["periods"].as_array().unwrap();
        assert_eq!(periods.len(), expected_periods.len(), "{id}");
        for period in periods {
            let start = period["start"].as_str().unwrap();
            for (field, value) in expected_periods[start].as_object().unwrap() {
                assert_eq!(&period[field], value, "{id} {start} {field}");
            }
        }
    }
}

/// AC-09.1 同じ名前の項目が2回ある JSON は不正な入力（手順1。対応外より先）。
#[test]
fn ac09_1_duplicate_names_are_invalid() {
    let duplicated = [
        r#"{"schema":"kintai.case.v1","schema":"kintai.case.v1","policy":{"work_system":"standard","scheduled_minutes_per_day":480,"legal_holiday_weekday":"sun","month_start_day":1},"shifts":[]}"#,
        r#"{"schema":"kintai.case.v1","policy":{"work_system":"flex","work_system":"flex","scheduled_minutes_per_day":480,"legal_holiday_weekday":"sun","month_start_day":1},"shifts":[]}"#,
        r#"{"schema":"kintai.case.v1","policy":{"work_system":"standard","scheduled_minutes_per_day":480,"legal_holiday_weekday":"sun","month_start_day":1},"shifts":[{"start":"2026-10-05T09:00","end":"2026-10-05T17:00","end":"2026-10-05T18:00"}]}"#,
    ];
    for text in duplicated {
        let run = run_case_bytes(text.as_bytes());
        assert_eq!(run.exit, 2, "{text}");
        assert_eq!(run.error_code(), "InvalidInput");
    }
}

fn number(value: &Value, field: &str) -> i64 {
    value[field].as_i64().unwrap()
}

/// AC-09.2 日ごとの total と night の関係、内数の関係。
fn check_day_sums(day: &Value) {
    let n = |field| number(day, field);
    assert_eq!(
        n("total"),
        n("within") + n("overtime") + n("holiday"),
        "{day}"
    );
    assert_eq!(
        n("night"),
        n("night_within") + n("night_overtime") + n("night_holiday"),
        "{day}"
    );
    assert!(n("over60") <= n("overtime"), "{day}");
    assert!(n("night_over60") <= n("night_overtime"), "{day}");
    assert!(n("extra_within") <= n("within"), "{day}");
    let keys: Vec<&str> = day
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = vec![
        "date",
        "total",
        "within",
        "extra_within",
        "overtime",
        "over60",
        "holiday",
        "night",
        "night_within",
        "night_overtime",
        "night_over60",
        "night_holiday",
    ];
    expected.sort_unstable();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, expected);
}

/// AC-09.2 曖昧さを含む `use_default` の出力でも、合計の関係が成り立ち、記録が並ぶ。
#[test]
fn ac09_2_use_default_output() {
    let mut policy = base_policy();
    policy["ambiguity"] = json!("use_default");
    let case = case_with(
        policy,
        json!([
            {"start": "2026-10-31T22:00", "end": "2026-11-01T02:00"},
            {"start": "2026-10-05T09:00", "end": "2026-10-05T19:00",
             "breaks": [["2026-10-05T12:00", "2026-10-05T13:00"]]}
        ]),
    );
    let run = run_case(&case);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = run.output();
    for day in out["days"].as_array().unwrap() {
        check_day_sums(day);
    }
    assert_eq!(
        out["ambiguities"],
        json!([
            {"date": "2026-10-31", "code": "HolidayBoundary"},
            {"date": "2026-10-31", "code": "WeekBoundary"},
            {"date": "2026-10-31", "code": "MonthBoundary"}
        ])
    );
    assert_eq!(out["days"][0]["date"], "2026-10-05", "日付順");
}

/// AC-09.4 終了コード: 0 成功、2 不正な入力、3 対応外、4 曖昧さによる拒否。
#[test]
fn ac09_4_exit_codes() {
    assert_eq!(run_case(&valid_case()).exit, 0);
    assert_eq!(run_case_bytes(b"not json").exit, 2);
    assert_eq!(
        run_case(&with_policy_item("work_system", json!("flex"))).exit,
        3
    );
    let refused = run_case(&case_with(
        base_policy(),
        json!([{"start": "2026-10-10T22:00", "end": "2026-10-11T02:00"}]),
    ));
    assert_eq!(refused.exit, 4);
    assert_eq!(refused.stderr, "", "拒否はエラーでなく出力で返す");
    let out = refused.output();
    assert_eq!(
        out["refused"],
        json!({"code": "AmbiguousBoundary", "dates": ["2026-10-10"]})
    );
    assert_eq!(out["days"], json!([]));
    assert_eq!(out["periods"], json!([]));
    assert_eq!(out["ambiguities"], json!([]));
    // 勤務がなければ、空の結果で成功。
    let empty = run_case(&case_with(base_policy(), json!([])));
    assert_eq!(empty.exit, 0);
    assert_eq!(
        empty.stdout,
        "{\"schema\":\"kintai.classify.v1\",\"days\":[],\"periods\":[],\"ambiguities\":[],\"refused\":null}\n"
    );
}

/// AC-09.1 勤務の検査（開始 < 終了、重なり、休憩が勤務の内側、長さ）。
#[test]
fn ac09_1_shift_checks() {
    let shifts = [
        json!([{"start": "2026-10-05T17:00", "end": "2026-10-05T09:00"}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T09:00"}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00"},
               {"start": "2026-10-05T16:59", "end": "2026-10-05T20:00"}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00",
                "breaks": [["2026-10-05T09:00", "2026-10-05T10:00"]]}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00",
                "breaks": [["2026-10-05T16:00", "2026-10-05T17:00"]]}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00",
                "breaks": [["2026-10-05T18:00", "2026-10-05T19:00"]]}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00",
                "breaks": [["2026-10-05T13:00", "2026-10-05T12:00"]]}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00",
                "breaks": [["2026-10-05T12:00", "2026-10-05T13:00"],
                           ["2026-10-05T12:30", "2026-10-05T13:30"]]}]),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-06T09:00"}]),
    ];
    for s in shifts {
        assert_invalid(&case_with(base_policy(), s));
    }
    // 接する勤務・接する休憩はよい。
    let touching = case_with(
        base_policy(),
        json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T13:00",
                "breaks": [["2026-10-05T11:00", "2026-10-05T11:30"],
                           ["2026-10-05T11:30", "2026-10-05T12:00"]]},
               {"start": "2026-10-05T13:00", "end": "2026-10-05T18:00"}]),
    );
    assert_eq!(run_case(&touching).exit, 0);
}

/// AC-09.1 日時の表記（2月30日、25時、24:00、60分、秒つき、範囲外の年）。
#[test]
fn ac09_1_datetime_notation() {
    for (start, end) in [
        ("2026-02-30T09:00", "2026-02-30T17:00"),
        ("2026-10-05T09:00", "2026-10-05T25:00"),
        ("2026-10-05T09:00", "2026-10-05T24:00"),
        ("2026-10-05T09:60", "2026-10-05T17:00"),
        ("2026-10-05T09:00:00", "2026-10-05T17:00:00"),
        ("1999-12-31T09:00", "1999-12-31T17:00"),
        ("2099-12-31T20:00", "2100-01-01T00:00"),
        ("2026-10-05", "2026-10-06"),
        ("2026-10-05T09:00+09:00", "2026-10-05T17:00+09:00"),
    ] {
        assert_invalid(&case_with(
            base_policy(),
            json!([{"start": start, "end": end}]),
        ));
    }
    // 区切りの空白は読める。
    let spaced = case_with(
        base_policy(),
        json!([{"start": "2026-10-05 09:00", "end": "2026-10-05 17:00"}]),
    );
    assert_eq!(run_case(&spaced).exit, 0);
}

/// AC-09.1 必須項目の欠け（法定休日の曜日は対応外の扱い。別の試験）。
#[test]
fn ac09_1_required_items() {
    for key in [
        "work_system",
        "scheduled_minutes_per_day",
        "month_start_day",
    ] {
        assert_invalid(&with_policy_item(key, Value::Null));
        let mut policy = base_policy();
        policy[key] = Value::Null;
        assert_invalid(&case_with(policy, one_shift()));
    }
    for key in ["schema", "policy", "shifts"] {
        let mut case = valid_case();
        case.as_object_mut().unwrap().remove(key);
        assert_invalid(&case);
    }
    for key in ["start", "end"] {
        let mut shifts = one_shift();
        shifts[0].as_object_mut().unwrap().remove(key);
        assert_invalid(&case_with(base_policy(), shifts));
    }
}

/// AC-09.1 値の範囲と列挙の値。
#[test]
fn ac09_1_value_ranges() {
    let invalid = [
        ("scheduled_minutes_per_day", json!(-1)),
        ("scheduled_minutes_per_day", json!(481)),
        ("month_start_day", json!(0)),
        ("month_start_day", json!(29)),
        ("month_start_day", json!(18_446_744_073_709_551_615_u64)),
        ("weekly_limit_minutes", json!(2500)),
        ("weekly_limit_minutes", json!(2401)),
        ("week_start", json!("Sun")),
        ("week_start", json!("sunday")),
        ("legal_holiday_weekday", json!("日")),
        ("ambiguity", json!("ignore")),
        ("work_system", json!("normal")),
        ("worker_kind", json!("adult")),
        ("holiday_system", json!("monthly")),
        ("night_window", json!("22-5")),
    ];
    for (key, value) in invalid {
        assert_invalid(&with_policy_item(key, value));
    }
    let valid = [
        ("scheduled_minutes_per_day", json!(0)),
        ("scheduled_minutes_per_day", json!(480)),
        ("month_start_day", json!(28)),
        ("weekly_limit_minutes", json!(2400)),
        ("week_start", json!("sat")),
        ("ambiguity", json!("use_default")),
        ("worker_kind", json!("standard")),
        ("holiday_system", json!("weekly")),
        ("night_window", json!("22-05")),
        ("special_44h_confirmed", json!(true)),
    ];
    for (key, value) in valid {
        let run = run_case(&with_policy_item(key, value.clone()));
        assert_eq!(run.exit, 0, "{key}={value}: {run:?}");
    }
}

/// AC-09.1 型の誤り、知らない項目、`schema` の誤り、JSON として読めないもの。
#[test]
fn ac09_1_shape_and_types() {
    let mut cases = vec![
        with_policy_item("scheduled_minutes_per_day", json!("480")),
        with_policy_item("scheduled_minutes_per_day", json!(480.0)),
        with_policy_item("month_start_day", json!(true)),
        with_policy_item("week_start", json!(0)),
        with_policy_item("special_44h_confirmed", json!("true")),
        with_policy_item("legal_holiday_weekday", json!(7)),
        with_policy_item("unknown_item", json!(1)),
        case_with(json!([]), one_shift()),
        case_with(base_policy(), json!({})),
        case_with(base_policy(), json!([[]])),
        case_with(
            base_policy(),
            json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00", "note": ""}]),
        ),
        case_with(
            base_policy(),
            json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00",
                    "breaks": [["2026-10-05T12:00"]]}]),
        ),
        case_with(
            base_policy(),
            json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00",
                    "breaks": [["2026-10-05T12:00", "2026-10-05T12:30", "2026-10-05T13:00"]]}]),
        ),
        case_with(
            base_policy(),
            json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00",
                    "breaks": {"start": "2026-10-05T12:00"}}]),
        ),
        case_with(
            base_policy(),
            json!([{"start": 0, "end": "2026-10-05T17:00"}]),
        ),
        json!(["kintai.case.v1", base_policy(), one_shift()]),
    ];
    let mut unknown_top = valid_case();
    unknown_top["note"] = json!("x");
    cases.push(unknown_top);
    for schema in [
        json!("kintai.case.v2"),
        json!("kintai.agreement36.v1"),
        json!(1),
    ] {
        let mut case = valid_case();
        case["schema"] = schema;
        cases.push(case);
    }
    for case in &cases {
        assert_invalid(case);
    }
    for bytes in [
        &b""[..],
        b"{",
        b"[]",
        b"null",
        b"\"kintai.case.v1\"",
        b"\xff\xfe",
        b"{\"schema\": \"kintai.case.v1\"} trailing",
    ] {
        let run = run_case_bytes(bytes);
        assert_eq!(run.exit, 2, "{bytes:?}");
        assert_eq!(run.error_code(), "InvalidInput");
    }
}

/// 任意の項目の `null` は既定と同じ。
#[test]
fn null_optional_items_mean_defaults() {
    let mut policy = base_policy();
    for key in [
        "week_start",
        "weekly_limit_minutes",
        "special_44h_confirmed",
        "ambiguity",
        "worker_kind",
        "holiday_system",
        "night_window",
    ] {
        policy[key] = Value::Null;
    }
    let shifts = json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00", "breaks": null}]);
    let with_nulls = run_case(&case_with(policy, shifts));
    assert_eq!(with_nulls.exit, 0, "{with_nulls:?}");
    assert_eq!(with_nulls.stdout, run_case(&valid_case()).stdout);
}

/// ファイルが開けないとき、引数の誤り、知らないコマンド。
#[test]
fn command_line_errors() {
    let missing = kintai(&["case", "/nonexistent/kintai-case.json"]);
    assert_eq!(
        (missing.exit, missing.error_code()),
        (2, "InvalidInput".to_owned())
    );
    let no_file = kintai(&["case"]);
    assert_eq!(no_file.exit, 2);
    let extra = kintai(&["case", "a.json", "b.json"]);
    assert_eq!(extra.exit, 2);
    let unknown = kintai(&["report"]);
    assert_eq!(
        (unknown.exit, unknown.error_code()),
        (2, "InvalidInput".to_owned())
    );
    let nothing = kintai(&[]);
    assert_eq!(nothing.exit, 2);
    assert_eq!(nothing.stdout, "");
}

/// `--version` は版と git の短いハッシュ（取れなければ unknown）、`--help` は使い方。
#[test]
fn version_and_help() {
    let version = kintai(&["--version"]);
    assert_eq!(version.exit, 0);
    let line = version.stdout.trim_end();
    let rest = line
        .strip_prefix(concat!("kintai ", env!("CARGO_PKG_VERSION"), " ("))
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{line}"));
    assert!(
        rest == "unknown" || (rest.len() == 7 && rest.bytes().all(|b| b.is_ascii_hexdigit())),
        "{rest}"
    );
    assert_eq!(kintai(&["-V"]).stdout, version.stdout);
    let help = kintai(&["--help"]);
    assert_eq!(help.exit, 0);
    assert!(help.stdout.contains("kintai case FILE"), "{}", help.stdout);
    assert_eq!(kintai(&["-h"]).stdout, help.stdout);
    assert_eq!(help.stderr, "");
}
