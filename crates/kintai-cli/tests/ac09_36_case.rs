//! `kintai case FILE` の36協定のケース（`docs/spec/case-format-36-leave.md`）。実際のバイナリを動かす。
//!
//! - AC-09.5: 36協定のケースが上の形で読まれ、範囲外の限度は不正な入力になる。
//! - AC-09.7: 出力のコードと順序が、規則のとおりに決まる。

mod common;

use common::{run_case, run_case_bytes};
use serde_json::{Value, json};

fn vectors() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/spec/vectors/limits36-official.json"
    );
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["vectors"].as_array().unwrap().clone()
}

/// 正しい36協定のケース（特別条項、2021-04 から3か月、履歴5か月）。
fn valid_case() -> Value {
    json!({
        "schema": "kintai.case36.v1",
        "agreement": {"kind": "special", "year_start_month": 4,
            "monthly_limit_minutes": 2700, "annual_limit_minutes": 21600,
            "special_monthly_total_limit_minutes": 5999,
            "special_annual_limit_minutes": 43200, "special_months_max": 6},
        "months": [
            {"start": "2021-04-01", "overtime": 3000, "holiday": 0},
            {"start": "2021-05-01", "overtime": 600, "holiday": 480},
            {"start": "2021-06-01", "overtime": 0, "holiday": 0}
        ],
        "history": [
            {"start": "2020-11-01", "total": 0}, {"start": "2020-12-01", "total": 0},
            {"start": "2021-01-01", "total": 0}, {"start": "2021-02-01", "total": 0},
            {"start": "2021-03-01", "total": 0}
        ]
    })
}

fn assert_invalid(case: &Value) {
    let run = run_case(case);
    assert_eq!(run.exit, 2, "{case}\n{run:?}");
    assert_eq!(run.error_code(), "InvalidInput", "{case}");
    assert_eq!(run.stdout, "", "{case}");
}

/// ケースの一部を書き換える（`null` を入れる、消す、値を替える）。
fn edited(edit: impl FnOnce(&mut Value)) -> Value {
    let mut case = valid_case();
    edit(&mut case);
    case
}

fn codes(findings: &Value) -> Vec<String> {
    findings
        .as_array()
        .unwrap()
        .iter()
        .map(|f| match f["n"].as_u64() {
            Some(n) => format!("{}:{n}", f["code"].as_str().unwrap()),
            None => f["code"].as_str().unwrap().to_owned(),
        })
        .collect()
}

/// 期待の指摘（文字列はコードだけ、オブジェクトはコードと n）を、出力の指摘と比べる。
fn same_findings(expected: &Value, actual: &Value) -> bool {
    let expected = expected.as_array().unwrap();
    let actual = actual.as_array().unwrap();
    expected.len() == actual.len()
        && expected.iter().zip(actual).all(|(want, got)| match want {
            Value::String(code) => got["code"] == *code,
            other => got["code"] == other["code"] && got["n"] == other["n"],
        })
}

/// AC-09.5 AC-09.7 数値例（公的な6件と導出の4件）を、バイナリで計算して期待の全項目と比べる。
#[test]
fn ac09_5_vectors_through_the_binary() {
    for vector in vectors() {
        let id = vector["id"].as_str().unwrap();
        let run = run_case(&vector["case"]);
        assert_eq!(run.exit, 0, "{id}: {run:?}");
        assert_eq!(run.stderr, "", "{id}");
        let out = run.output();
        assert_eq!(out["schema"], "kintai.limits36.v1");
        assert_eq!(out["notices"], json!(["OwnEmployerOnly"]));
        let months = out["months"].as_array().unwrap();
        let last = months.last().unwrap();
        let at = |start: &str| months.iter().find(|m| m["start"] == start).unwrap();
        for (key, expected) in vector["expect"].as_object().unwrap() {
            match key.as_str() {
                "total" | "special_count" | "year_overtime" => {
                    let got: Vec<&Value> = months.iter().map(|m| &m[key.as_str()]).collect();
                    let want: Vec<&Value> = expected.as_array().unwrap().iter().collect();
                    assert_eq!(got, want, "{id} {key}");
                }
                "sums_at_last" => {
                    for (n, value) in expected.as_object().unwrap() {
                        assert_eq!(&last["sums"][n], value, "{id} {n}");
                    }
                }
                "findings_at_last" => {
                    assert!(same_findings(expected, &last["findings"]), "{id}");
                }
                "year_overtime_known_at_last" => {
                    assert_eq!(&last["year_overtime_known"], expected, "{id}");
                }
                "special_count_last" => assert_eq!(&last["special_count"], expected, "{id}"),
                "findings_by_month" => {
                    let listed = expected.as_object().unwrap();
                    for month in months {
                        let start = month["start"].as_str().unwrap();
                        match listed.get(start) {
                            Some(want) => {
                                assert!(same_findings(want, &month["findings"]), "{id} {start}");
                            }
                            None => assert!(
                                codes(&month["findings"])
                                    .iter()
                                    .all(|c| c.starts_with("UnknownHistory")),
                                "{id} {start}"
                            ),
                        }
                    }
                }
                "sums_at" => {
                    for (start, sums) in expected.as_object().unwrap() {
                        for (n, value) in sums.as_object().unwrap() {
                            assert_eq!(&at(start)["sums"][n], value, "{id} {start} {n}");
                        }
                    }
                }
                "special_count_by_month" => {
                    for (start, count) in expected.as_object().unwrap() {
                        assert_eq!(&at(start)["special_count"], count, "{id} {start}");
                    }
                }
                "note" => {}
                other => panic!("{id}: 試験が知らない期待の項目 {other}"),
            }
        }
    }
}

/// AC-09.7 出力は仕様の例の項目の並びで、インデントなしの1行＋末尾の改行（仕様の例の入力）。
/// `n` のない指摘には `n` の項目を作らない。何度動かしても同じバイト列。
#[test]
fn ac09_7_output_is_one_deterministic_line() {
    let example = json!({
        "schema": "kintai.case36.v1",
        "agreement": {"kind": "special", "year_start_month": 4,
          "monthly_limit_minutes": 2700, "annual_limit_minutes": 21600,
          "special_monthly_total_limit_minutes": 5940,
          "special_annual_limit_minutes": 43200, "special_months_max": 6},
        "months": [{"start": "2021-04-01", "overtime": 4800, "holiday": 0}],
        "history": [{"start": "2021-03-01", "total": 600}]
    });
    let expected = concat!(
        r#"{"schema":"kintai.limits36.v1","months":[{"start":"2021-04-01","total":4800,"#,
        r#""sums":{"2":5400,"3":null,"4":null,"5":null,"6":null},"special_count":1,"#,
        r#""year_overtime":4800,"year_overtime_known":true,"findings":[{"code":"UnknownHistory","n":3},"#,
        r#"{"code":"UnknownHistory","n":4},{"code":"UnknownHistory","n":5},{"code":"UnknownHistory","n":6}]}],"#,
        r#""notices":["OwnEmployerOnly"]}"#,
        "\n"
    );
    let first = run_case(&example);
    assert_eq!(first.exit, 0, "{first:?}");
    assert_eq!(first.stdout, expected);
    for _ in 0..3 {
        assert_eq!(run_case(&example), first);
    }
    let none = json!({
        "schema": "kintai.case36.v1",
        "agreement": {"kind": "none", "year_start_month": 1},
        "months": [{"start": "2022-01-01", "overtime": 1, "holiday": 0}],
        "history": []
    });
    let out = run_case(&none).output();
    assert_eq!(
        out["months"][0]["findings"][0],
        json!({"code": "NoAgreement"})
    );
    assert_eq!(out["months"][0]["year_overtime_known"], json!(true));
}

/// AC-09.7 指摘はコードの表の順で、同じコードは n の昇順。
#[test]
fn ac09_7_findings_order_in_output() {
    let case = json!({
        "schema": "kintai.case36.v1",
        "agreement": {"kind": "general", "year_start_month": 4,
            "monthly_limit_minutes": 2700, "annual_limit_minutes": 21600},
        "months": [
            {"start": "2021-04-01", "overtime": 5500, "holiday": 500},
            {"start": "2021-05-01", "overtime": 5500, "holiday": 500},
            {"start": "2021-06-01", "overtime": 5500, "holiday": 500},
            {"start": "2021-07-01", "overtime": 5500, "holiday": 500}
        ],
        "history": []
    });
    let out = run_case(&case).output();
    assert_eq!(
        codes(&out["months"][3]["findings"]),
        [
            "MonthlyOvertimeExceeded",
            "AnnualOvertimeExceeded",
            "MonthlyTotalReached100h",
            "MultiMonthSumExceeded:2",
            "MultiMonthSumExceeded:3",
            "MultiMonthSumExceeded:4",
            "UnknownHistory:5",
            "UnknownHistory:6",
        ]
    );
    // 協定年の途中から: ΣO が分からない年は偽。
    assert_eq!(out["months"][0]["year_overtime_known"], json!(true));
}

/// AC-09.5 `months` は空でもよい（出力の `months` も空）。そのとき履歴は日が1〜28であることだけを見る。
#[test]
fn ac09_5_empty_months() {
    let case = edited(|case| {
        case["months"] = json!([]);
        case["history"] =
            json!([{"start": "2021-03-16", "total": 5}, {"start": "2021-03-01", "total": 0}]);
    });
    let run = run_case(&case);
    assert_eq!(run.exit, 0, "{run:?}");
    assert_eq!(
        run.stdout,
        "{\"schema\":\"kintai.limits36.v1\",\"months\":[],\"notices\":[\"OwnEmployerOnly\"]}\n"
    );
    assert_invalid(&edited(|case| {
        case["months"] = json!([]);
        case["history"] = json!([{"start": "2021-03-29", "total": 0}]);
    }));
}

/// AC-09.5 協定の限度が法定の最大を超えると不正な入力。最大ちょうどと0は受け付ける。
#[test]
fn ac09_5_agreement_limits_out_of_range() {
    let limits = [
        ("monthly_limit_minutes", 2700),
        ("annual_limit_minutes", 21_600),
        ("special_monthly_total_limit_minutes", 5999),
        ("special_annual_limit_minutes", 43_200),
        ("special_months_max", 6),
    ];
    for (field, max) in limits {
        assert_invalid(&edited(|case| case["agreement"][field] = json!(max + 1)));
        assert_invalid(&edited(|case| case["agreement"][field] = json!(-1)));
        for ok in [max, 0] {
            let run = run_case(&edited(|case| case["agreement"][field] = json!(ok)));
            assert_eq!(run.exit, 0, "{field}={ok} {run:?}");
        }
        // 協定なしでも、ある項目は範囲を見る。
        assert_invalid(&json!({
            "schema": "kintai.case36.v1",
            "agreement": {"kind": "none", "year_start_month": 4, field: max + 1},
            "months": [], "history": []
        }));
    }
    for month in [0, 13] {
        assert_invalid(&edited(|case| {
            case["agreement"]["year_start_month"] = json!(month);
        }));
    }
}

/// AC-09.5 必須の項目（トップレベルの4つ、協定の kind と起算月、種類ごとの限度）がない、または `null`。
/// 任意の項目の `null` は、項目がないのと同じ。
#[test]
fn ac09_5_required_items_and_null() {
    for key in ["schema", "agreement", "months", "history"] {
        assert_invalid(&edited(|case| {
            case.as_object_mut().unwrap().remove(key);
        }));
        assert_invalid(&edited(|case| case[key] = Value::Null));
    }
    for key in [
        "kind",
        "year_start_month",
        "special_months_max",
        "monthly_limit_minutes",
    ] {
        assert_invalid(&edited(|case| {
            case["agreement"].as_object_mut().unwrap().remove(key);
        }));
        assert_invalid(&edited(|case| case["agreement"][key] = Value::Null));
    }
    for key in ["start", "overtime", "holiday"] {
        assert_invalid(&edited(|case| {
            case["months"][0].as_object_mut().unwrap().remove(key);
        }));
        assert_invalid(&edited(|case| case["months"][0][key] = Value::Null));
    }
    for key in ["start", "total"] {
        assert_invalid(&edited(|case| case["history"][0][key] = Value::Null));
    }
    // 協定なしの限度の null は、項目なしと同じ。
    let run = run_case(&json!({
        "schema": "kintai.case36.v1",
        "agreement": {"kind": "none", "year_start_month": 4, "monthly_limit_minutes": null,
            "special_months_max": null},
        "months": [{"start": "2021-04-01", "overtime": 0, "holiday": 0}],
        "history": []
    }));
    assert_eq!(run.exit, 0, "{run:?}");
}

/// AC-09.5 型の誤り（整数の欄は JSON の整数だけ）、知らない項目、同じ名前の項目、定義にない種類。
#[test]
fn ac09_5_types_unknown_items_and_duplicates() {
    for value in [
        json!("10"),
        json!(1.0),
        json!(true),
        json!([1]),
        json!(18_446_744_073_709_551_615_u64),
    ] {
        assert_invalid(&edited(|case| {
            case["months"][0]["overtime"] = value.clone();
        }));
        assert_invalid(&edited(|case| {
            case["agreement"]["year_start_month"] = value.clone();
        }));
    }
    assert_invalid(&edited(|case| {
        case["months"][0]["start"] = json!(20_210_401);
    }));
    assert_invalid(&edited(|case| case["months"] = json!({})));
    assert_invalid(&edited(|case| case["history"] = json!([["2021-03-01", 0]])));
    assert_invalid(&edited(|case| case["agreement"] = json!([])));
    assert_invalid(&edited(|case| case["agreement"]["kind"] = json!("both")));
    assert_invalid(&edited(|case| case["agreement"]["kind"] = json!("Special")));
    assert_invalid(&edited(|case| case["extra"] = json!(1)));
    assert_invalid(&edited(|case| {
        case["agreement"]["month_start_day"] = json!(1);
    }));
    assert_invalid(&edited(|case| case["months"][0]["note"] = json!("x")));
    assert_invalid(&edited(|case| case["history"][0]["overtime"] = json!(0)));
    // -0 と指数は整数でない（バイト列で書く）。
    let text = serde_json::to_string(&valid_case()).unwrap();
    for replacement in ["\"overtime\":-0", "\"overtime\":3e3", "\"overtime\":3000.0"] {
        let broken = text.replacen("\"overtime\":3000", replacement, 1);
        assert_ne!(broken, text);
        let run = run_case_bytes(broken.as_bytes());
        assert_eq!(
            (run.exit, run.error_code().as_str()),
            (2, "InvalidInput"),
            "{replacement}"
        );
    }
    // 同じ名前の項目（serde_json は項目を名前の順に書く）。
    for (from, to) in [
        (
            "\"kind\":\"special\"",
            "\"kind\":\"special\",\"kind\":\"special\"",
        ),
        (
            "\"holiday\":0,\"overtime\":3000",
            "\"holiday\":0,\"holiday\":0,\"overtime\":3000",
        ),
        ("\"history\":", "\"history\":[],\"history\":"),
    ] {
        let broken = text.replacen(from, to, 1);
        assert_ne!(broken, text);
        let run = run_case_bytes(broken.as_bytes());
        assert_eq!(
            (run.exit, run.error_code().as_str()),
            (2, "InvalidInput"),
            "{to}"
        );
    }
    // 入れ子が深すぎる JSON は読めない（手順1。境界の127段・128段は区分のケースの試験と strict_json が見る）。
    let inner = "[".repeat(200) + &"]".repeat(200);
    let deep = text.replacen(
        "{\"agreement\"",
        &format!("{{\"x\":{inner},\"agreement\""),
        1,
    );
    let run = run_case_bytes(deep.as_bytes());
    assert_eq!((run.exit, run.error_code().as_str()), (2, "InvalidInput"));
}

/// AC-09.5 分の範囲: O と H は0以上で O＋H は44640分以下、履歴の T は0〜44640分。
#[test]
fn ac09_5_minutes_ranges() {
    assert_invalid(&edited(|case| case["months"][0]["overtime"] = json!(-1)));
    assert_invalid(&edited(|case| case["months"][0]["holiday"] = json!(-1)));
    assert_invalid(&edited(|case| {
        case["months"][0]["overtime"] = json!(44_000);
        case["months"][0]["holiday"] = json!(641);
    }));
    assert_invalid(&edited(|case| case["history"][0]["total"] = json!(44_641)));
    assert_invalid(&edited(|case| case["history"][0]["total"] = json!(-1)));
    let run = run_case(&edited(|case| {
        case["months"][0]["overtime"] = json!(44_000);
        case["months"][0]["holiday"] = json!(640);
        case["history"][0]["total"] = json!(44_640);
    }));
    assert_eq!(run.exit, 0, "{run:?}");
}

/// AC-09.5 日付の書式と整合: 表記の誤り、入力の範囲の外、日が29〜31、隙間、履歴の月の誤り・重複。
#[test]
fn ac09_5_dates_must_be_valid_and_consistent() {
    for start in [
        "2021-4-01",
        "2021-02-30",
        "2100-04-01",
        "1999-04-01",
        "2021-04-29",
        "2021/04/01",
    ] {
        assert_invalid(&edited(|case| case["months"][0]["start"] = json!(start)));
    }
    assert_invalid(&edited(|case| {
        case["months"][1]["start"] = json!("2021-06-01");
    }));
    assert_invalid(&edited(|case| {
        case["months"][2]["start"] = json!("2021-06-02");
    }));
    assert_invalid(&edited(|case| {
        case["history"][0]["start"] = json!("2021-04-01");
    }));
    assert_invalid(&edited(|case| {
        case["history"][0]["start"] = json!("2020-11-02");
    }));
    assert_invalid(&edited(|case| {
        case["history"][0]["start"] = json!("2021-03-01");
    }));
    assert_invalid(&edited(|case| {
        case["history"][0]["start"] = json!("2020-13-01");
    }));
    // 履歴の並びは問わない。
    let run = run_case(&edited(|case| {
        let list = case["history"].as_array_mut().unwrap();
        list.reverse();
    }));
    assert_eq!(run.exit, 0, "{run:?}");
}
