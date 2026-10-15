//! AC-05.9: 公的な数値例（`docs/spec/vectors/limits36-official.json`）の合計と、違反の判定がすべて一致する。
//!
//! `expect` に書かれた項目は全部検査する（知らない項目があれば試験を失敗にして、見落としを防ぐ）。
//! `findings_by_month` に書いていない月は、`UnknownHistory` 以外の指摘がないこと。
//! `UnknownHistory` は、直近 n か月の合計が `null` の n にだけ付くこと。

use kintai_jp_labor::limits36::{
    Agreement, AgreementKind, Finding, HistoryMonth, Limits36Notice, Limits36Result, MonthHours,
    MonthJudgement, judge,
};
use serde_json::Value;

fn vectors() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/spec/vectors/limits36-official.json"
    );
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(all["schema"], "kintai.vectors36.v1");
    all["vectors"].as_array().unwrap().clone()
}

fn limit(agreement: &Value, key: &str) -> Option<i64> {
    agreement.get(key).and_then(Value::as_i64)
}

/// ケースの JSON をライブラリの型にして判定する（ケースの形式の読み方は CLI の試験が確かめる）。
fn compute(case: &Value) -> Limits36Result {
    let agreement = &case["agreement"];
    let kind = match agreement["kind"].as_str().unwrap() {
        "none" => AgreementKind::None,
        "general" => AgreementKind::General,
        "special" => AgreementKind::Special,
        other => panic!("kind {other}"),
    };
    let valid = Agreement {
        kind,
        year_start_month: agreement["year_start_month"].as_i64().unwrap(),
        monthly_limit_minutes: limit(agreement, "monthly_limit_minutes"),
        annual_limit_minutes: limit(agreement, "annual_limit_minutes"),
        special_monthly_total_limit_minutes: limit(
            agreement,
            "special_monthly_total_limit_minutes",
        ),
        special_annual_limit_minutes: limit(agreement, "special_annual_limit_minutes"),
        special_months_max: limit(agreement, "special_months_max"),
    }
    .validate()
    .unwrap();
    let months: Vec<MonthHours> = case["months"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| MonthHours {
            start: m["start"].as_str().unwrap().parse().unwrap(),
            overtime: m["overtime"].as_i64().unwrap(),
            holiday: m["holiday"].as_i64().unwrap(),
        })
        .collect();
    let history: Vec<HistoryMonth> = case["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| HistoryMonth {
            start: h["start"].as_str().unwrap().parse().unwrap(),
            total: h["total"].as_i64().unwrap(),
        })
        .collect();
    judge(&valid, &months, &history).unwrap()
}

fn month<'a>(result: &'a Limits36Result, start: &str) -> &'a MonthJudgement {
    result
        .months
        .iter()
        .find(|m| m.start.to_string() == start)
        .unwrap_or_else(|| panic!("月 {start} がない"))
}

/// 期待の指摘の並び（文字列ならコードだけ、オブジェクトならコードと n）と比べる。
fn check_findings(id: &str, at: &str, expected: &Value, actual: &[Finding]) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        expected.len(),
        actual.len(),
        "{id} {at}: {expected:?} と {actual:?}"
    );
    for (want, got) in expected.iter().zip(actual) {
        match want {
            Value::String(code) => assert_eq!(code, got.code(), "{id} {at}"),
            Value::Object(object) => {
                assert_eq!(object["code"].as_str().unwrap(), got.code(), "{id} {at}");
                let n = object["n"].as_u64().map(|n| u32::try_from(n).unwrap());
                assert_eq!(n, got.n(), "{id} {at}");
            }
            other => panic!("{id}: 指摘の書き方 {other}"),
        }
    }
}

fn check_sums(id: &str, at: &str, expected: &Value, judged: &MonthJudgement) {
    for (n, value) in expected.as_object().unwrap() {
        let n: u32 = n.parse().unwrap();
        assert_eq!(judged.sum(n), value.as_i64(), "{id} {at} sums[{n}]");
    }
}

fn per_month_i64(id: &str, key: &str, expected: &Value, result: &Limits36Result) {
    let expected = expected.as_array().unwrap();
    assert_eq!(expected.len(), result.months.len(), "{id} {key}");
    for (want, got) in expected.iter().zip(&result.months) {
        let got = match key {
            "total" => got.total,
            "special_count" => i64::from(got.special_count),
            "year_overtime" => got.year_overtime,
            _ => unreachable!(),
        };
        assert_eq!(want.as_i64().unwrap(), got, "{id} {key}");
    }
}

/// 期待の1項目を確かめる。
fn check_expect(id: &str, key: &str, expected: &Value, result: &Limits36Result) {
    let last = result.months.last().unwrap();
    let last_start = last.start.to_string();
    match key {
        "total" | "special_count" | "year_overtime" => {
            per_month_i64(id, key, expected, result);
        }
        "sums_at_last" => check_sums(id, &last_start, expected, last),
        "findings_at_last" => check_findings(id, &last_start, expected, &last.findings),
        "year_overtime_known_at_last" => {
            assert_eq!(
                expected.as_bool().unwrap(),
                last.year_overtime_known,
                "{id}"
            );
        }
        "special_count_last" => {
            assert_eq!(
                expected.as_i64().unwrap(),
                i64::from(last.special_count),
                "{id}"
            );
        }
        "findings_by_month" => {
            let listed = expected.as_object().unwrap();
            for judged in &result.months {
                let at = judged.start.to_string();
                match listed.get(&at) {
                    Some(want) => check_findings(id, &at, want, &judged.findings),
                    None => assert!(
                        judged
                            .findings
                            .iter()
                            .all(|f| matches!(f, Finding::UnknownHistory { .. })),
                        "{id} {at}: 書いていない月に指摘がある {:?}",
                        judged.findings
                    ),
                }
            }
            for at in listed.keys() {
                month(result, at);
            }
        }
        "sums_at" => {
            for (at, sums) in expected.as_object().unwrap() {
                check_sums(id, at, sums, month(result, at));
            }
        }
        "special_count_by_month" => {
            for (at, count) in expected.as_object().unwrap() {
                assert_eq!(
                    count.as_i64().unwrap(),
                    i64::from(month(result, at).special_count),
                    "{id} {at}"
                );
            }
        }
        "note" => {}
        other => panic!("{id}: 試験が知らない期待の項目 {other}"),
    }
}

/// AC-05.9 公的な数値例（p.16〜17、p.21、p.4）と導出の例の、期待の全項目が一致する。
#[test]
fn ac05_9_official_vectors_match_every_expectation() {
    let all = vectors();
    assert_eq!(all.len(), 10, "数値例の件数");
    for vector in &all {
        let id = vector["id"].as_str().unwrap();
        let result = compute(&vector["case"]);
        assert_eq!(
            result.notices,
            vec![Limits36Notice::OwnEmployerOnly],
            "{id}"
        );
        assert_eq!(
            result.months.len(),
            vector["case"]["months"].as_array().unwrap().len()
        );
        for (key, expected) in vector["expect"].as_object().unwrap() {
            check_expect(id, key, expected, &result);
        }
        // 合計が null の n にだけ UnknownHistory が付く（どの月も）。
        for judged in &result.months {
            for n in 2..=6 {
                let unknown = judged.findings.contains(&Finding::UnknownHistory { n });
                assert_eq!(
                    unknown,
                    judged.sum(n).is_none(),
                    "{id} {} n={n}",
                    judged.start
                );
            }
        }
    }
}

/// AC-05.9 p.16〜17 の表の各月の T と、2021/9 の直近2〜6か月の合計（115・160・220・300・380時間）を時間で確かめる。
#[test]
fn ac05_9_leaflet_table_in_hours() {
    let table = &vectors()[0];
    assert_eq!(table["id"], "mhlw-36-leaflet-p16-table1");
    let result = compute(&table["case"]);
    let hours: Vec<i64> = result.months.iter().map(|m| m.total / 60).collect();
    assert_eq!(hours, [80, 80, 60, 45, 35, 80]);
    let last = result.months.last().unwrap();
    let sums: Vec<i64> = (2..=6).map(|n| last.sum(n).unwrap() / 60).collect();
    assert_eq!(sums, [115, 160, 220, 300, 380]);
    // 平均は 57.5・53.3・55.0・60.0・63.3 時間で、どれも80時間以内。割らずに合計で比べる。
    for (n, sum) in (2..=6).zip(&sums) {
        assert!(sum * 60 <= 4800 * n);
    }
}
