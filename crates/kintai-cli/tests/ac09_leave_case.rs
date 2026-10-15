//! `kintai case FILE` の年休のケース（`docs/spec/case-format-36-leave.md`）。実際のバイナリを動かす。
//!
//! - AC-09.6: 年休のケースが上の形で読まれ、`hours` は拒否される（`LeaveHourly`、終了コード3）。
//! - AC-09.7: 出力のコードと順序が、規則のとおりに決まる。検査の順序（形と型 → 列挙と範囲 → 対応外 →
//!   日付の書式と整合 → 計算）も確かめる。

mod common;

use common::{run_case, run_case_bytes};
use serde_json::{Value, json};

fn vectors() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/spec/vectors/leave-official.json"
    );
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["vectors"].as_array().unwrap().clone()
}

/// 仕様の例の入力。
fn example() -> Value {
    json!({"schema": "kintai.caseleave.v1", "as_of": "2026-03-15",
     "employee": {"hire_date": "2024-04-01", "weekly_scheduled_days": 5,
       "weekly_scheduled_minutes": 2400, "annual_scheduled_days": null,
       "attendance": ["meets", "meets"], "grants": null},
     "taken": [{"date": "2025-05-01", "unit": "full"}, {"date": "2025-06-02", "unit": "half"}]})
}

fn edited(edit: impl FnOnce(&mut Value)) -> Value {
    let mut case = example();
    edit(&mut case);
    case
}

fn assert_invalid(case: &Value) {
    let run = run_case(case);
    assert_eq!(run.exit, 2, "{case}\n{run:?}");
    assert_eq!(run.error_code(), "InvalidInput", "{case}");
    assert_eq!(run.stdout, "", "{case}");
}

fn assert_unsupported(case: &Value, code: &str) {
    let run = run_case(case);
    assert_eq!(run.exit, 3, "{case}\n{run:?}");
    assert_eq!(run.error_code(), code, "{case}");
    assert_eq!(run.stdout, "", "{case}");
}

fn assert_ok(case: &Value) -> Value {
    let run = run_case(case);
    assert_eq!(run.exit, 0, "{case}\n{run:?}");
    assert_eq!(run.stderr, "");
    run.output()
}

/// AC-09.6 AC-09.7 数値例（解説p.8・p.9 と導出の例）を、バイナリで計算して期待の全項目と比べる。
#[test]
fn ac09_6_vectors_through_the_binary() {
    let mut checked = 0;
    for vector in vectors() {
        let id = vector["id"].as_str().unwrap();
        let Some(case) = vector.get("case") else {
            continue;
        };
        let out = assert_ok(case);
        assert_eq!(out["schema"], "kintai.leave.v1");
        for (key, expected) in vector["expect"].as_object().unwrap() {
            match key.as_str() {
                "lots" | "five_day" => {
                    let rows = out[key.as_str()].as_array().unwrap();
                    let want = expected.as_array().unwrap();
                    assert_eq!(rows.len(), want.len(), "{id} {key}");
                    for (row, want) in rows.iter().zip(want) {
                        for (field, value) in want.as_object().unwrap() {
                            if field != "note" {
                                assert_eq!(&row[field], value, "{id} {key} {field}");
                            }
                        }
                    }
                }
                "balance_halves" | "notices" => assert_eq!(&out[key.as_str()], expected, "{id}"),
                other => panic!("{id}: 試験が知らない期待の項目 {other}"),
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 4);
}

/// AC-09.7 出力は仕様の例の項目の並びで、1行＋末尾の改行。管理簿は付与ごとに1行（取得がなくても）。
#[test]
fn ac09_7_output_is_one_deterministic_line() {
    let expected = concat!(
        r#"{"schema":"kintai.leave.v1","lots":[{"grant_date":"2024-10-01","granted_halves":20,"#,
        r#""expires":"2026-10-01","used_halves":3,"remaining_halves":17,"expired_halves":0},"#,
        r#"{"grant_date":"2025-10-01","granted_halves":22,"expires":"2027-10-01","used_halves":0,"#,
        r#""remaining_halves":22,"expired_halves":0}],"balance_halves":39,"five_day":["#,
        r#"{"base_date":"2024-10-01","window_end":"2025-09-30","taken_halves":3,"status":"NotMet","#,
        r#""needed_halves":7},{"base_date":"2025-10-01","window_end":"2026-09-30","taken_halves":0,"#,
        r#""status":"InProgress","needed_halves":10}],"register":[{"base_date":"2024-10-01","#,
        r#""granted_halves":20,"taken":[{"date":"2025-05-01","unit":"full"},{"date":"2025-06-02","#,
        r#""unit":"half"}],"taken_halves":3},{"base_date":"2025-10-01","granted_halves":22,"#,
        r#""taken":[],"taken_halves":0}],"notices":[]}"#,
        "\n"
    );
    let first = run_case(&example());
    assert_eq!(first.exit, 0, "{first:?}");
    assert_eq!(first.stdout, expected);
    for _ in 0..3 {
        assert_eq!(run_case(&example()), first);
    }
    // 出勤率が未確認の付与があれば、注意を1回。
    let out = assert_ok(&edited(|case| case["employee"]["attendance"] = json!([])));
    assert_eq!(out["notices"], json!(["AttendanceUnconfirmed"]));
    // 管理簿の取得は日付の昇順（同じ日は入力の順）。
    let out = assert_ok(&edited(|case| {
        case["taken"] = json!([
            {"date": "2025-06-02", "unit": "half"},
            {"date": "2025-05-01", "unit": "full"},
            {"date": "2025-05-01", "unit": "half"}
        ]);
    }));
    assert_eq!(
        out["register"][0]["taken"],
        json!([
            {"date": "2025-05-01", "unit": "full"},
            {"date": "2025-05-01", "unit": "half"},
            {"date": "2025-06-02", "unit": "half"}
        ])
    );
}

/// AC-09.6 取得の `unit` が `hours` なら、`LeaveHourly` で拒否（終了コード3。説明に条文）。
#[test]
fn ac09_6_hours_are_rejected() {
    let case = edited(|case| case["taken"][1]["unit"] = json!("hours"));
    assert_unsupported(&case, "LeaveHourly");
    let run = run_case(&case);
    let error: Value = serde_json::from_str(&run.stderr).unwrap();
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("労働基準法第39条第4項")
    );
    // as_of より後の時間単位は、判定に入れない（数えもしない）。
    let later = edited(|case| {
        case["taken"]
            .as_array_mut()
            .unwrap()
            .push(json!({"date": "2026-04-01", "unit": "hours"}));
    });
    let out = assert_ok(&later);
    assert_eq!(out["balance_halves"], 39);
}

/// AC-09.7 対応外の順: 時間単位 → 分割付与 → 表の外。
#[test]
fn ac09_7_unsupported_order() {
    let all_three = edited(|case| {
        case["employee"]["weekly_scheduled_days"] = json!(0);
        case["employee"]["weekly_scheduled_minutes"] = json!(0);
        case["employee"]["grants"] =
            json!([{"date": "2024-04-01", "halves": 10}, {"date": "2024-10-01", "halves": 10}]);
        case["taken"][0]["unit"] = json!("hours");
    });
    assert_unsupported(&all_three, "LeaveHourly");
    let mut split_and_table = all_three.clone();
    split_and_table["taken"][0]["unit"] = json!("full");
    assert_unsupported(&split_and_table, "LeaveSplitGrant");
    let mut table_only = split_and_table.clone();
    table_only["employee"]["grants"] = json!([{"date": "2024-04-01", "halves": 20}]);
    assert_unsupported(&table_only, "LeaveOutOfTable");
    let mut annual = table_only.clone();
    annual["employee"]["annual_scheduled_days"] = json!(47);
    assert_unsupported(&annual, "LeaveOutOfTable");
    annual["employee"]["annual_scheduled_days"] = json!(48);
    assert_ok(&annual);
}

/// AC-09.7 検査の順: 列挙と範囲（手順2）は対応外（手順3）より先。対応外は日付の検査（手順4）より先。
#[test]
fn ac09_7_validation_order() {
    let hours = |case: &mut Value| case["taken"][0]["unit"] = json!("hours");
    // 手順2が先: 定義にない単位・出勤率、範囲外の週の日数・付与の日数。
    assert_invalid(&edited(|case| {
        hours(case);
        case["taken"][1]["unit"] = json!("day");
    }));
    assert_invalid(&edited(|case| {
        hours(case);
        case["employee"]["attendance"] = json!(["yes"]);
    }));
    assert_invalid(&edited(|case| {
        hours(case);
        case["employee"]["weekly_scheduled_days"] = json!(8);
    }));
    assert_invalid(&edited(|case| {
        hours(case);
        case["employee"]["grants"] = json!([{"date": "2024-10-01", "halves": 0}]);
    }));
    // 手順3が先: 書式の誤りの日付の時間単位、雇入れ日の書式の誤りと表の外。
    assert_unsupported(
        &edited(|case| {
            hours(case);
            case["taken"][0]["date"] = json!("2025-02-30");
        }),
        "LeaveHourly",
    );
    assert_unsupported(
        &edited(|case| {
            case["employee"]["weekly_scheduled_days"] = json!(0);
            case["employee"]["weekly_scheduled_minutes"] = json!(0);
            case["employee"]["hire_date"] = json!("2024-4-01");
        }),
        "LeaveOutOfTable",
    );
    // as_of が書式の誤りなら、どの記録も判定に入れる（as_of より後の日付の時間単位でも拒否）。
    assert_unsupported(
        &edited(|case| {
            case["as_of"] = json!("2026-02-30");
            case["taken"][0] = json!({"date": "2027-01-01", "unit": "hours"});
        }),
        "LeaveHourly",
    );
    // 書式は正しく範囲の外（2100年）で as_of より後: 判定に入れず、手順4で不正な入力。
    assert_invalid(&edited(|case| {
        case["taken"][0] = json!({"date": "2100-01-01", "unit": "hours"});
    }));
    // 書式の誤りの付与日があれば、分割付与を判定せず、手順4で不正な入力。
    assert_invalid(&edited(|case| {
        case["employee"]["grants"] = json!([
            {"date": "2024-04-01", "halves": 10},
            {"date": "2024-10-01", "halves": 10},
            {"date": "2024-02-30", "halves": 20}
        ]);
    }));
    // as_of より後の付与は、分割付与の判定に入れない。
    let later = assert_ok(&edited(|case| {
        case["employee"]["grants"] = json!([
            {"date": "2025-10-01", "halves": 22},
            {"date": "2026-03-16", "halves": 10},
            {"date": "2026-05-01", "halves": 10}
        ]);
        case["taken"] = json!([{"date": "2025-11-01", "unit": "full"}]);
    }));
    assert_eq!(later["lots"].as_array().unwrap().len(), 1);
}

/// AC-09.6 必須の項目がない、または `null`（`taken` も）。任意の項目の `null` は指定なし。
#[test]
fn ac09_6_required_items_and_null() {
    for key in ["schema", "as_of", "employee", "taken"] {
        assert_invalid(&edited(|case| {
            case.as_object_mut().unwrap().remove(key);
        }));
        assert_invalid(&edited(|case| case[key] = Value::Null));
    }
    for key in [
        "hire_date",
        "weekly_scheduled_days",
        "weekly_scheduled_minutes",
        "attendance",
    ] {
        assert_invalid(&edited(|case| {
            case["employee"].as_object_mut().unwrap().remove(key);
        }));
        assert_invalid(&edited(|case| case["employee"][key] = Value::Null));
    }
    for key in ["date", "unit"] {
        assert_invalid(&edited(|case| case["taken"][0][key] = Value::Null));
    }
    let without = edited(|case| {
        let employee = case["employee"].as_object_mut().unwrap();
        employee.remove("annual_scheduled_days");
        employee.remove("grants");
    });
    assert_eq!(assert_ok(&without), assert_ok(&example()));
    assert_ok(&edited(|case| case["taken"] = json!([])));
    assert_ok(&edited(|case| case["employee"]["attendance"] = json!([])));
}

/// AC-09.6 型の誤り、知らない項目、同じ名前の項目、範囲外、定義にない値。
#[test]
fn ac09_6_types_ranges_and_enums() {
    for value in [json!("5"), json!(5.0), json!(true), json!(null)] {
        assert_invalid(&edited(|case| {
            case["employee"]["weekly_scheduled_days"] = value.clone();
        }));
    }
    assert_invalid(&edited(|case| {
        case["employee"]["annual_scheduled_days"] = json!("200");
    }));
    assert_invalid(&edited(|case| {
        case["employee"]["grants"] = json!({"date": "2024-10-01"});
    }));
    assert_invalid(&edited(|case| {
        case["employee"]["grants"] = json!([{"date": "2024-10-01"}]);
    }));
    assert_invalid(&edited(|case| {
        case["employee"]["attendance"] = json!("meets");
    }));
    assert_invalid(&edited(|case| case["employee"]["attendance"] = json!([1])));
    assert_invalid(&edited(|case| case["taken"] = json!({})));
    assert_invalid(&edited(|case| case["as_of"] = json!(20_260_315)));
    assert_invalid(&edited(|case| case["extra"] = json!(0)));
    assert_invalid(&edited(|case| case["employee"]["name"] = json!("x")));
    assert_invalid(&edited(|case| case["taken"][0]["hours"] = json!(2)));
    assert_invalid(&edited(|case| {
        case["employee"]["grants"] = json!([{"date": "2024-10-01", "halves": 20, "note": "x"}]);
    }));
    for (field, wrong, ok) in [
        ("weekly_scheduled_days", -1, 7),
        ("weekly_scheduled_days", 8, 0),
        ("weekly_scheduled_minutes", 10_081, 10_080),
        ("annual_scheduled_days", 367, 366),
        ("annual_scheduled_days", -1, 217),
    ] {
        assert_invalid(&edited(|case| case["employee"][field] = json!(wrong)));
        let run = run_case(&edited(|case| case["employee"][field] = json!(ok)));
        assert!(run.exit == 0 || run.exit == 3, "{field}={ok} {run:?}");
    }
    for halves in [0, 201, -5] {
        assert_invalid(&edited(|case| {
            case["employee"]["grants"] = json!([{"date": "2024-10-01", "halves": halves}]);
        }));
    }
    assert_ok(&edited(|case| {
        case["employee"]["grants"] = json!([{"date": "2024-10-01", "halves": 200}]);
    }));
    assert_invalid(&edited(|case| {
        case["employee"]["attendance"] = json!(["meets", "Below"]);
    }));
    assert_invalid(&edited(|case| case["taken"][0]["unit"] = json!("FULL")));
    // 同じ名前の項目と -0（バイト列で書く。serde_json は項目を名前の順に書く）。
    let text = serde_json::to_string(&example()).unwrap();
    for (from, to) in [
        ("\"as_of\":", "\"as_of\":\"2026-03-15\",\"as_of\":"),
        ("\"unit\":\"full\"", "\"unit\":\"full\",\"unit\":\"full\""),
        (
            "\"weekly_scheduled_days\":5",
            "\"weekly_scheduled_days\":-0",
        ),
        (
            "\"weekly_scheduled_days\":5",
            "\"weekly_scheduled_days\":5e0",
        ),
    ] {
        let broken = text.replacen(from, to, 1);
        assert_ne!(broken, text, "{from}");
        let run = run_case_bytes(broken.as_bytes());
        assert_eq!(
            (run.exit, run.error_code().as_str()),
            (2, "InvalidInput"),
            "{to}"
        );
    }
}

/// AC-09.6 日付の書式と入力の範囲（手順4）。残数が足りない取得（手順5）も不正な入力。
#[test]
fn ac09_6_dates_and_balance() {
    assert_invalid(&edited(|case| {
        case["employee"]["hire_date"] = json!("2024-02-30");
    }));
    assert_invalid(&edited(|case| case["as_of"] = json!("2100-01-01")));
    assert_invalid(&edited(|case| case["as_of"] = json!("1999-12-31")));
    assert_invalid(&edited(|case| {
        case["taken"][0]["date"] = json!("2024/05/01");
    }));
    assert_invalid(&edited(|case| {
        case["employee"]["grants"] = json!([{"date": "2019-13-01", "halves": 20}]);
    }));
    // 最初の付与（2024-10-01）より前の取得は、使える残数がない。
    assert_invalid(&edited(|case| {
        case["taken"][0]["date"] = json!("2024-09-30");
    }));
    // as_of より後の取得は、足りなくても数えない。
    let out = assert_ok(&edited(|case| {
        let list = case["taken"].as_array_mut().unwrap();
        for day in 1..=30 {
            list.push(json!({"date": format!("2026-04-{day:02}"), "unit": "full"}));
        }
    }));
    assert_eq!(out["balance_halves"], 39);
}
