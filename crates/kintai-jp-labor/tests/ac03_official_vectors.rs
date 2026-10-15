//! AC-03.10: 公的な数値例（`vectors/official-examples.json`）がすべて一致する。
//! AC-04.6: 公的な数値例（4380分＝3600＋780、休日120分、27日に60分）が一致する。

mod common;

use common::{
    DAY_FIELDS, PERIOD_FIELDS, day, day_field, naive_from, period_field, policy_from_json,
    shifts_from_json, vectors,
};
use kintai_jp_labor::{Classification, ShiftSet, classify};
use serde_json::Value;

/// 出典が公的な数値例として、この周の時点で入っている7件。
const KNOWN_IDS: [&str; 7] = [
    "mhlw-check-roudou-ex1",
    "mhlw-check-roudou-ex2",
    "tokyo-aramashi-week-excess-1",
    "tokyo-aramashi-week-excess-2",
    "nagasaki-no28-case1",
    "nagasaki-no28-case2",
    "mhlw-60h-leaflet-31day-month",
];

fn compute(case: &Value) -> Classification {
    let policy = policy_from_json(&case["policy"]).validate().unwrap();
    let inputs = shifts_from_json(&case["shifts"]);
    let data_from = case["data_from"].as_str().map(common::date);
    let result = classify(&policy, &ShiftSet::new(&inputs).unwrap(), data_from).unwrap();
    // 1分ずつ数える素朴な実装とも一致する。
    assert_eq!(naive_from(&policy, &inputs, data_from), result);
    result
}

/// `expect` の日ごと・月の期間ごとの全項目と、あれば曖昧さと拒否を比べる。
fn check(id: &str, expect: &Value, result: &Classification) {
    let expected_days = expect["days"].as_object().map_or(0, |days| {
        for (date, expected) in days {
            let ours = day(result, date);
            for field in DAY_FIELDS {
                assert_eq!(
                    day_field(ours, field),
                    expected[field].as_i64().unwrap(),
                    "{id} {date} {field}"
                );
            }
        }
        days.len()
    });
    assert_eq!(result.days.len(), expected_days, "{id}: 日数");
    let expected_periods = expect["periods"].as_object().map_or(0, |periods| {
        for (start, expected) in periods {
            let ours = result
                .periods
                .iter()
                .find(|p| p.start.to_string() == *start)
                .unwrap_or_else(|| panic!("{id}: 期間 {start} がない"));
            for field in PERIOD_FIELDS {
                assert_eq!(
                    period_field(ours, field),
                    expected[field].as_i64().unwrap(),
                    "{id} {start} {field}"
                );
            }
        }
        periods.len()
    });
    assert_eq!(result.periods.len(), expected_periods, "{id}: 期間の数");
    let ambiguities: Vec<(String, &str)> = result
        .ambiguities
        .iter()
        .map(|a| (a.date.to_string(), a.code.code()))
        .collect();
    let expected_ambiguities: Vec<(String, &str)> = expect["ambiguities"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|a| {
                    (
                        a["date"].as_str().unwrap().to_owned(),
                        a["code"].as_str().unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(ambiguities, expected_ambiguities, "{id}: 曖昧さ");
    match expect.get("refused") {
        Some(Value::Object(refused)) => {
            let ours = result.refused.as_ref().unwrap();
            assert_eq!(ours.code.code(), refused["code"], "{id}");
            let dates: Vec<String> = ours.dates.iter().map(ToString::to_string).collect();
            let expected: Vec<String> = refused["dates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(dates, expected, "{id}: refused.dates");
        }
        _ => assert_eq!(result.refused, None, "{id}"),
    }
}

/// AC-03.10 数値例の、日ごと・月の期間ごとの全項目が期待値と一致する（7件の公的な例を含む全件）。
#[test]
fn ac03_10_every_official_vector_matches() {
    let all = vectors();
    assert_eq!(all["schema"], "kintai.vectors.v1");
    let list = all["vectors"].as_array().unwrap();
    for id in KNOWN_IDS {
        assert!(list.iter().any(|v| v["id"] == id), "{id} がない");
    }
    for vector in list {
        let id = vector["id"].as_str().unwrap();
        let case = &vector["case"];
        assert_eq!(case["schema"], "kintai.case.v1", "{id}");
        let result = compute(case);
        check(id, &vector["expect"], &result);
    }
}

/// AC-04.6 月60時間超の数値例: 時間外4380分＝60時間以下3600分＋60時間超780分、休日120分、27日に60分。
#[test]
fn ac04_6_sixty_hour_leaflet_example() {
    let all = vectors();
    let vector = all["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "mhlw-60h-leaflet-31day-month")
        .unwrap();
    let result = compute(&vector["case"]);
    let july = &result.periods[0];
    assert_eq!(july.start.to_string(), "2026-07-01");
    assert_eq!(july.overtime, 4380);
    assert_eq!(july.overtime - july.over60, 3600);
    assert_eq!(july.over60, 780);
    assert_eq!(july.holiday, 120);
    // 27日の時間外の3時間目（60分）から60時間超。その前の日（24日）までは0。
    assert_eq!(day(&result, "2026-07-24").over60, 0);
    assert_eq!(day(&result, "2026-07-27").over60, 60);
    assert_eq!(day(&result, "2026-07-28").over60, 180);
    // 週ごとの時間外は 9・19・15・15・15 時間、累計は 9・28・43・58・73 時間。
    let weekly: Vec<i64> = [
        ("2026-06-28", "2026-07-04"),
        ("2026-07-05", "2026-07-11"),
        ("2026-07-12", "2026-07-18"),
        ("2026-07-19", "2026-07-25"),
        ("2026-07-26", "2026-08-01"),
    ]
    .iter()
    .map(|(from, to)| {
        let from = common::date(from);
        let to = common::date(to);
        result
            .days
            .iter()
            .filter(|d| from <= d.date && d.date <= to)
            .map(|d| d.overtime)
            .sum()
    })
    .collect();
    assert_eq!(weekly, vec![540, 1140, 900, 900, 900]);
    let cumulative: Vec<i64> = weekly
        .iter()
        .scan(0, |acc, w| {
            *acc += w;
            Some(*acc)
        })
        .collect();
    assert_eq!(cumulative, vec![540, 1680, 2580, 3480, 4380]);
}
