//! 年次有給休暇の規則（`docs/spec/leave.md`）: 出勤率、失効、消化の順、半日、5日取得義務、管理簿、対応外。
//!
//! AC-06.3〜06.10 を、手で作った従業員と取得の記録で確かめる。AC-06.8 は `leave-official.json` の数値例。

use kintai_core::Date;
use kintai_jp_labor::leave::{
    Attendance, CalendarDate, FiveDayStatus, GrantCheck, GrantInput, GrantRow, LSA_39_3, LSA_39_4,
    LSA_39_7, LeaveError, LeaveInput, LeaveNotice, LeaveResult, LeaveUnit, LeaveUnsupportedCode,
    REG_24NO3, REG_24NO5, TakenCheck, TakenInput, WorkingTerms, check_unsupported, compute,
};
use kintai_jp_labor::provenance::{CONFIRMED_ON, LABOR_STANDARDS_ACT, LABOR_STANDARDS_REGULATION};
use serde_json::Value;

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

fn full_time() -> WorkingTerms {
    WorkingTerms {
        weekly_scheduled_days: 5,
        weekly_scheduled_minutes: 2400,
        annual_scheduled_days: None,
    }
}

fn taken(list: &[(&str, LeaveUnit)]) -> Vec<TakenInput> {
    list.iter()
        .map(|&(day, unit)| TakenInput {
            date: date(day),
            unit,
        })
        .collect()
}

/// 法定の付与（`grants` なし）の入力。
fn statutory(
    as_of: &str,
    hire: &str,
    attendance: &[Attendance],
    records: &[(&str, LeaveUnit)],
) -> LeaveInput {
    LeaveInput {
        as_of: date(as_of),
        hire_date: date(hire),
        terms: full_time(),
        attendance: attendance.to_vec(),
        grants: None,
        taken: taken(records),
    }
}

/// 明示の付与の入力。
fn explicit(as_of: &str, grants: &[(&str, i64)], records: &[(&str, LeaveUnit)]) -> LeaveInput {
    LeaveInput {
        as_of: date(as_of),
        hire_date: date("2019-04-01"),
        terms: full_time(),
        attendance: vec![Attendance::Below, Attendance::Unknown],
        grants: Some(
            grants
                .iter()
                .map(|&(day, halves)| GrantInput {
                    date: date(day),
                    halves,
                })
                .collect(),
        ),
        taken: taken(records),
    }
}

fn lots(result: &LeaveResult) -> Vec<(String, i64, i64, i64, i64)> {
    result
        .lots
        .iter()
        .map(|lot| {
            (
                lot.grant_date.to_string(),
                lot.granted_halves,
                lot.used_halves,
                lot.remaining_halves,
                lot.expired_halves,
            )
        })
        .collect()
}

fn five_day(result: &LeaveResult) -> Vec<(String, i64, FiveDayStatus, i64)> {
    result
        .five_day
        .iter()
        .map(|f| {
            (
                f.base_date.to_string(),
                f.taken_halves,
                f.status,
                f.needed_halves,
            )
        })
        .collect()
}

fn owned(rows: &[(&str, i64, i64, i64, i64)]) -> Vec<(String, i64, i64, i64, i64)> {
    rows.iter()
        .map(|&(d, a, b, c, e)| (d.to_owned(), a, b, c, e))
        .collect()
}

use Attendance::{Below, Meets, Unknown};
use LeaveUnit::{Full, Half, Hours};

/// AC-06.3 出勤率 `below` の年は付与せず、勤続年数は進む（平成6年1月4日基発第1号の例: 初回10日、
/// 次の1年が8割未満で不付与、次は2年6か月の12日）。`meets` だけなら注意は付かない。
#[test]
fn ac06_3_below_skips_the_grant_but_service_continues() {
    let result = compute(&statutory(
        "2023-03-01",
        "2020-04-01",
        &[Meets, Below, Meets],
        &[],
    ))
    .unwrap();
    assert_eq!(
        lots(&result),
        owned(&[("2020-10-01", 20, 0, 0, 20), ("2022-10-01", 24, 0, 24, 0)])
    );
    assert_eq!(result.notices, []);
    assert_eq!(result.five_day.len(), 2);
    assert_eq!(result.register.len(), 2);
    // 最初の付与が below なら、最初のロットはない。
    let result = compute(&statutory("2021-10-01", "2020-04-01", &[Below, Meets], &[])).unwrap();
    assert_eq!(lots(&result), owned(&[("2021-10-01", 22, 0, 22, 0)]));
}

/// AC-06.3 `unknown`（足りない付与も）は付与して、注意を1回だけ付ける。付与の数より長い出勤率の余りは使わない。
/// `grants` を明示したときは出勤率を使わず、注意も付けない。
#[test]
fn ac06_3_unknown_grants_with_one_notice() {
    let result = compute(&statutory("2023-03-01", "2020-04-01", &[Unknown], &[])).unwrap();
    assert_eq!(result.lots.len(), 3);
    assert_eq!(result.notices, [LeaveNotice::AttendanceUnconfirmed]);
    assert_eq!(result.notices[0].code(), "AttendanceUnconfirmed");
    let long = [Meets, Meets, Meets, Below, Unknown, Below];
    let result = compute(&statutory("2023-03-01", "2020-04-01", &long, &[])).unwrap();
    assert_eq!(result.lots.len(), 3);
    assert_eq!(result.notices, [], "余りの below・unknown は使わない");
    // as_of より前に付与がなければ、注意もない。
    let result = compute(&statutory("2020-09-30", "2020-04-01", &[], &[])).unwrap();
    assert_eq!((result.lots.len(), result.notices.len()), (0, 0));
    // 明示の付与では、出勤率（below・unknown）を使わない。
    let result = compute(&explicit("2020-06-01", &[("2019-04-01", 20)], &[])).unwrap();
    assert_eq!(lots(&result), owned(&[("2019-04-01", 20, 0, 20, 0)]));
    assert_eq!(result.notices, []);
}

/// AC-06.4 ロットは失効日の当日に失効する（前日までは使え、残数に入る）。
#[test]
fn ac06_4_lot_expires_on_the_expiry_day() {
    let before = compute(&statutory("2026-09-30", "2024-04-01", &[Meets, Meets], &[])).unwrap();
    assert_eq!(before.lots[0].expires, date("2026-10-01"));
    assert_eq!(
        (
            before.lots[0].remaining_halves,
            before.lots[0].expired_halves
        ),
        (20, 0)
    );
    assert_eq!(before.balance_halves, 42);
    let on = compute(&statutory(
        "2026-10-01",
        "2024-04-01",
        &[Meets, Meets, Meets],
        &[],
    ))
    .unwrap();
    assert_eq!(
        (on.lots[0].remaining_halves, on.lots[0].expired_halves),
        (0, 20)
    );
    assert_eq!(on.balance_halves, 22 + 24);
    // 失効日の前日の取得は古いロットから。当日の取得は、古いロットを使えない。
    let records = [("2026-09-30", Full), ("2026-10-01", Full)];
    let result = compute(&statutory(
        "2026-12-01",
        "2024-04-01",
        &[Meets; 3],
        &records,
    ))
    .unwrap();
    assert_eq!(
        lots(&result),
        owned(&[
            ("2024-10-01", 20, 2, 0, 18),
            ("2025-10-01", 22, 2, 20, 0),
            ("2026-10-01", 24, 0, 24, 0),
        ])
    );
}

/// AC-06.4 付与日の当日から使える（付与日 ≤ 日付 < 失効日）。付与日の前日には、そのロットは使えない。
#[test]
fn ac06_4_lot_is_usable_from_the_grant_day() {
    let on_grant_day = compute(&statutory(
        "2024-12-01",
        "2024-04-01",
        &[Meets],
        &[("2024-10-01", Full)],
    ))
    .unwrap();
    assert_eq!(lots(&on_grant_day), owned(&[("2024-10-01", 20, 2, 18, 0)]));
    assert_eq!(on_grant_day.five_day[0].taken_halves, 2);
    assert_eq!(
        compute(&statutory(
            "2024-12-01",
            "2024-04-01",
            &[Meets],
            &[("2024-09-30", Half)]
        )),
        Err(LeaveError::InsufficientBalance { index: 0 })
    );
    // 2つ目のロットの付与日の当日は、古いロットが残っていれば古いロットから。
    let records = [("2025-10-01", Full)];
    let result = compute(&statutory(
        "2025-12-01",
        "2024-04-01",
        &[Meets, Meets],
        &records,
    ))
    .unwrap();
    assert_eq!(result.lots[0].used_halves, 2);
    assert_eq!(result.lots[1].used_halves, 0);
}

/// AC-06.4 使えるロットの古いものから消化する。1日の取得は、残り半日のロットと次のロットにまたがってよい。
#[test]
fn ac06_4_oldest_lot_first_and_a_full_day_may_span_two_lots() {
    // 2024-10-01 の20半日のうち19半日を半日で取り、残り1半日。次に1日を取ると、1半日ずつ2つのロットから。
    let mut records: Vec<(&str, LeaveUnit)> = Vec::new();
    let days: Vec<String> = (1..=19).map(|d| format!("2025-11-{d:02}")).collect();
    for day in &days {
        records.push((day.as_str(), Half));
    }
    records.push(("2025-12-01", Full));
    let result = compute(&statutory(
        "2026-01-01",
        "2024-04-01",
        &[Meets, Meets],
        &records,
    ))
    .unwrap();
    assert_eq!(
        lots(&result),
        owned(&[("2024-10-01", 20, 20, 0, 0), ("2025-10-01", 22, 1, 21, 0)])
    );
    assert_eq!(result.balance_halves, 21);
}

/// AC-06.5 半日は0.5日（1半日）として、残数と5日の取得に数える。
#[test]
fn ac06_5_half_day_counts_as_one_half() {
    let records = [
        ("2025-05-01", Half),
        ("2025-05-02", Half),
        ("2025-05-08", Full),
    ];
    let result = compute(&statutory("2025-06-01", "2024-04-01", &[Meets], &records)).unwrap();
    assert_eq!(lots(&result), owned(&[("2024-10-01", 20, 4, 16, 0)]));
    assert_eq!(result.five_day[0].taken_halves, 4);
    assert_eq!(result.five_day[0].needed_halves, 6);
    assert_eq!(Half.halves(), Some(1));
    assert_eq!(Full.halves(), Some(2));
    assert_eq!(Hours.halves(), None);
    assert_eq!(
        [Full.code(), Half.code(), Hours.code()],
        ["full", "half", "hours"]
    );
}

/// AC-06.6 5日取得義務は10日（20半日）以上の付与だけ。期間の中は `InProgress`（期間の最後の日を含む）、
/// 過ぎて5日に満たなければ `NotMet`、ちょうど5日で `Met`。10日未満は `NotApplicable`。
#[test]
fn ac06_6_five_day_obligation() {
    // 10半日ちょうどで Met、9半日なら残り1。
    let ten: Vec<(String, LeaveUnit)> = (1..=5).map(|d| (format!("2025-05-0{d}"), Full)).collect();
    let records: Vec<(&str, LeaveUnit)> = ten.iter().map(|(d, u)| (d.as_str(), *u)).collect();
    let result = compute(&statutory("2025-06-01", "2024-04-01", &[Meets], &records)).unwrap();
    assert_eq!(
        five_day(&result),
        [("2024-10-01".to_owned(), 10, FiveDayStatus::Met, 0)]
    );
    let mut nine = records.clone();
    nine[4].1 = Half;
    let result = compute(&statutory("2025-06-01", "2024-04-01", &[Meets], &nine)).unwrap();
    assert_eq!(
        five_day(&result),
        [("2024-10-01".to_owned(), 9, FiveDayStatus::InProgress, 1)]
    );
    // 期間の最後の日（2025-09-30）は InProgress、翌日から NotMet。
    let last = compute(&statutory("2025-09-30", "2024-04-01", &[Meets], &nine)).unwrap();
    assert_eq!(last.five_day[0].status, FiveDayStatus::InProgress);
    assert_eq!(last.five_day[0].window_end, date("2025-09-30"));
    let after = compute(&statutory("2025-10-01", "2024-04-01", &[Meets], &nine)).unwrap();
    assert_eq!(after.five_day[0].status, FiveDayStatus::NotMet);
    assert_eq!(after.five_day[0].needed_halves, 1);
    assert_eq!(FiveDayStatus::NotMet.code(), "NotMet");
    // 比例付与（週4日）: 3年6か月（2023-10-01）の10日から対象。それまでは NotApplicable（期間に取得があっても）。
    let input = LeaveInput {
        terms: WorkingTerms {
            weekly_scheduled_days: 4,
            weekly_scheduled_minutes: 1500,
            annual_scheduled_days: None,
        },
        ..statutory(
            "2024-11-01",
            "2020-04-01",
            &[Meets; 5],
            &[("2023-01-10", Full), ("2023-12-01", Full)],
        )
    };
    let result = compute(&input).unwrap();
    let got: Vec<(i64, FiveDayStatus, i64, i64)> = result
        .lots
        .iter()
        .zip(&result.five_day)
        .map(|(lot, f)| {
            (
                lot.granted_halves,
                f.status,
                f.needed_halves,
                f.taken_halves,
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (14, FiveDayStatus::NotApplicable, 0, 0),
            (16, FiveDayStatus::NotApplicable, 0, 0),
            (18, FiveDayStatus::NotApplicable, 0, 2),
            (20, FiveDayStatus::NotMet, 8, 2),
            (24, FiveDayStatus::InProgress, 10, 0),
        ]
    );
    for status in [
        FiveDayStatus::Met,
        FiveDayStatus::InProgress,
        FiveDayStatus::NotApplicable,
    ] {
        assert_ne!(status.code(), "");
    }
}

/// AC-06.7 繰越分（古いロット）から消化した取得も、新しい付与の5日の期間に数える。
/// 期間が重なるときは、重なる日の取得を両方の期間に数える（導出）。
#[test]
fn ac06_7_carry_over_and_overlapping_windows() {
    // 2026-05-07 の半日は 2024-10-01 のロットから消化するが、2025-10-01 の期間に入る。
    let records = [("2025-06-02", Full), ("2026-05-07", Half)];
    let result = compute(&statutory(
        "2026-06-01",
        "2024-04-01",
        &[Meets, Meets],
        &records,
    ))
    .unwrap();
    assert_eq!(result.lots[0].used_halves, 3);
    assert_eq!(result.lots[1].used_halves, 0);
    assert_eq!(result.five_day[1].taken_halves, 1);
    assert_eq!(
        result.register[1]
            .taken
            .iter()
            .map(|t| t.date)
            .collect::<Vec<_>>(),
        [date("2026-05-07")]
    );
    // 解説p.9 ケース2（按分なし）: 2019-10-01 と 2020-04-01 の期間が重なり、2020-05-01 の取得は両方に入る。
    let input = explicit(
        "2020-10-15",
        &[("2019-10-01", 20), ("2020-04-01", 22)],
        &[("2020-03-02", Full), ("2020-05-01", Full)],
    );
    let result = compute(&input).unwrap();
    assert_eq!(
        five_day(&result),
        [
            ("2019-10-01".to_owned(), 4, FiveDayStatus::NotMet, 6),
            ("2020-04-01".to_owned(), 2, FiveDayStatus::InProgress, 8),
        ]
    );
    assert_eq!(result.register[0].taken.len(), 2);
    assert_eq!(result.register[1].taken.len(), 1);
}

fn leave_vectors() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/spec/vectors/leave-official.json"
    );
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(all["schema"], "kintai.vectorsleave.v1");
    all["vectors"].as_array().unwrap().clone()
}

/// ケースの JSON をライブラリの型にする（ケースの形式の読み方は CLI の試験が確かめる）。
fn input_from_json(case: &Value) -> LeaveInput {
    let employee = &case["employee"];
    let attendance = employee["attendance"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| match a.as_str().unwrap() {
            "meets" => Meets,
            "below" => Below,
            _ => Unknown,
        })
        .collect();
    let grants = employee["grants"].as_array().map(|grants| {
        grants
            .iter()
            .map(|g| GrantInput {
                date: date(g["date"].as_str().unwrap()),
                halves: g["halves"].as_i64().unwrap(),
            })
            .collect()
    });
    let records = case["taken"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| TakenInput {
            date: date(t["date"].as_str().unwrap()),
            unit: if t["unit"] == "full" { Full } else { Half },
        })
        .collect();
    LeaveInput {
        as_of: date(case["as_of"].as_str().unwrap()),
        hire_date: date(employee["hire_date"].as_str().unwrap()),
        terms: WorkingTerms {
            weekly_scheduled_days: employee["weekly_scheduled_days"].as_i64().unwrap(),
            weekly_scheduled_minutes: employee["weekly_scheduled_minutes"].as_i64().unwrap(),
            annual_scheduled_days: employee["annual_scheduled_days"].as_i64(),
        },
        attendance,
        grants,
        taken: records,
    }
}

/// 期待の配列（`note` 以外の項目）と、結果の配列を比べる。
fn check_rows(id: &str, key: &str, expected: &Value, actual: &[Value]) {
    let expected = expected.as_array().unwrap();
    assert_eq!(expected.len(), actual.len(), "{id} {key}");
    for (want, got) in expected.iter().zip(actual) {
        for (field, value) in want.as_object().unwrap() {
            if field != "note" {
                assert_eq!(&got[field], value, "{id} {key} {field}");
            }
        }
    }
}

/// AC-06.8 公的な数値例（解説p.8・p.9 の期間の日付）と導出の例が、期待の全項目で一致する。
#[test]
fn ac06_8_official_vectors() {
    let mut checked = 0;
    for vector in leave_vectors() {
        let id = vector["id"].as_str().unwrap();
        let Some(case) = vector.get("case") else {
            continue;
        };
        let result = compute(&input_from_json(case)).unwrap();
        let lots: Vec<Value> = result
            .lots
            .iter()
            .map(|l| {
                serde_json::json!({
                    "grant_date": l.grant_date.to_string(), "granted_halves": l.granted_halves,
                    "expires": l.expires.to_string(), "used_halves": l.used_halves,
                    "remaining_halves": l.remaining_halves, "expired_halves": l.expired_halves,
                })
            })
            .collect();
        let five: Vec<Value> = result
            .five_day
            .iter()
            .map(|f| {
                serde_json::json!({
                    "base_date": f.base_date.to_string(), "window_end": f.window_end.to_string(),
                    "taken_halves": f.taken_halves, "status": f.status.code(),
                    "needed_halves": f.needed_halves,
                })
            })
            .collect();
        for (key, expected) in vector["expect"].as_object().unwrap() {
            match key.as_str() {
                "lots" => check_rows(id, key, expected, &lots),
                "five_day" => check_rows(id, key, expected, &five),
                "balance_halves" => {
                    assert_eq!(expected.as_i64().unwrap(), result.balance_halves, "{id}");
                }
                "notices" => {
                    let codes: Vec<&str> = result.notices.iter().map(|n| n.code()).collect();
                    let want: Vec<&str> = expected
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap())
                        .collect();
                    assert_eq!(codes, want, "{id}");
                }
                other => panic!("{id}: 試験が知らない期待の項目 {other}"),
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 4);
}

fn unsupported(input: &LeaveInput) -> Option<LeaveUnsupportedCode> {
    match compute(input) {
        Err(LeaveError::Unsupported(code)) => Some(code),
        Ok(_) => None,
        Err(other) => panic!("{other:?}"),
    }
}

fn out_of_table() -> WorkingTerms {
    WorkingTerms {
        weekly_scheduled_days: 0,
        weekly_scheduled_minutes: 0,
        annual_scheduled_days: None,
    }
}

/// AC-06.9 対応外は、時間単位 → 分割付与 → 表の外（48日未満、週0日）の順に見て、最初のものを返す。
#[test]
fn ac06_9_unsupported_in_order() {
    let split = [("2019-04-01", 10), ("2019-07-01", 10)];
    let mut all_three = explicit("2020-01-01", &split, &[("2019-08-01", Hours)]);
    all_three.terms = out_of_table();
    assert_eq!(
        unsupported(&all_three),
        Some(LeaveUnsupportedCode::LeaveHourly)
    );
    let mut two = explicit("2020-01-01", &split, &[]);
    two.terms = out_of_table();
    assert_eq!(
        unsupported(&two),
        Some(LeaveUnsupportedCode::LeaveSplitGrant)
    );
    // grants を明示しても、表の外は対応外。
    let mut one = explicit("2020-01-01", &[("2019-04-01", 20)], &[]);
    one.terms = out_of_table();
    assert_eq!(
        unsupported(&one),
        Some(LeaveUnsupportedCode::LeaveOutOfTable)
    );
    one.terms.annual_scheduled_days = Some(47);
    assert_eq!(
        unsupported(&one),
        Some(LeaveUnsupportedCode::LeaveOutOfTable)
    );
    one.terms.annual_scheduled_days = Some(48);
    assert_eq!(unsupported(&one), None);
    // 法定の付与でも同じ。
    let mut statutory_out = statutory("2021-01-01", "2020-04-01", &[], &[]);
    statutory_out.terms = out_of_table();
    assert_eq!(
        unsupported(&statutory_out),
        Some(LeaveUnsupportedCode::LeaveOutOfTable)
    );
    let codes: Vec<&str> = LeaveUnsupportedCode::ALL.iter().map(|c| c.code()).collect();
    assert_eq!(codes, ["LeaveHourly", "LeaveSplitGrant", "LeaveOutOfTable"]);
}

/// AC-06.9 分割付与: 10日（20半日）未満の付与の次の付与が、`次の初日(その付与日, 12)` より前にある。
/// 付与日の昇順に安定に並べて見る（同じ日は入力の順）。
#[test]
fn ac06_9_split_grant_definition() {
    let check = |grants: &[(&str, i64)]| unsupported(&explicit("2021-12-01", grants, &[]));
    assert_eq!(
        check(&[("2019-04-01", 19), ("2020-03-31", 2)]),
        Some(LeaveUnsupportedCode::LeaveSplitGrant)
    );
    // 次の初日（2020-04-01）ちょうどは分割でない。20半日以上の付与の次も分割でない。
    assert_eq!(check(&[("2019-04-01", 19), ("2020-04-01", 2)]), None);
    assert_eq!(check(&[("2019-04-01", 20), ("2019-07-01", 2)]), None);
    // 入力の順は問わない（付与日の昇順に並べる）。
    assert_eq!(
        check(&[("2019-07-01", 10), ("2019-04-01", 10)]),
        Some(LeaveUnsupportedCode::LeaveSplitGrant)
    );
    // 同じ日の付与は入力の順: 小さい付与が先なら分割、大きい付与が先なら分割でない。
    assert_eq!(
        check(&[("2019-04-01", 10), ("2019-04-01", 30)]),
        Some(LeaveUnsupportedCode::LeaveSplitGrant)
    );
    assert_eq!(check(&[("2019-04-01", 30), ("2019-04-01", 10)]), None);
    // 法定の付与は分割にならない（週1日の2日でも）。
    let mut small = statutory("2023-01-01", "2020-04-01", &[], &[]);
    small.terms.weekly_scheduled_days = 1;
    small.terms.weekly_scheduled_minutes = 300;
    assert_eq!(unsupported(&small), None);
}

/// AC-06.9 対応外の判定に、`as_of` より後の取得と付与は入れない。
#[test]
fn ac06_9_records_after_as_of_are_not_judged() {
    let hourly_later = statutory("2025-01-01", "2024-04-01", &[], &[("2025-01-02", Hours)]);
    assert_eq!(unsupported(&hourly_later), None);
    let hourly_on = statutory("2025-01-01", "2024-04-01", &[], &[("2025-01-01", Hours)]);
    assert_eq!(
        unsupported(&hourly_on),
        Some(LeaveUnsupportedCode::LeaveHourly)
    );
    let split_later = explicit("2019-06-30", &[("2019-04-01", 10), ("2019-07-01", 10)], &[]);
    assert_eq!(unsupported(&split_later), None);
    let split_on = explicit("2019-07-01", &[("2019-04-01", 10), ("2019-07-01", 10)], &[]);
    assert_eq!(
        unsupported(&split_on),
        Some(LeaveUnsupportedCode::LeaveSplitGrant)
    );
}

/// AC-06.9 書式の誤りの日付は入れたまま判定する（時間単位）。書式の誤りの付与日があれば分割付与を判定しない。
/// 書式が正しければ、入力の範囲の外の日付も `as_of` と比べる。
#[test]
fn ac06_9_unsupported_checks_before_date_validation() {
    let terms = full_time().validate().unwrap();
    let loose = |text: &str| CalendarDate::parse(text);
    let hours = |text: &str| TakenCheck {
        date: loose(text),
        unit: Hours,
    };
    let as_of = loose("2026-01-01");
    // 書式の誤り（2月30日）の時間単位は入れたまま → 対応外。
    assert_eq!(
        check_unsupported(as_of, &terms, None, &[hours("2025-02-30")]),
        Err(LeaveUnsupportedCode::LeaveHourly)
    );
    // as_of が書式の誤りなら、どの記録も入れる。
    assert_eq!(
        check_unsupported(loose("2026-13-01"), &terms, None, &[hours("2030-01-01")]),
        Err(LeaveUnsupportedCode::LeaveHourly)
    );
    // 書式は正しく範囲の外（2100年）: as_of より後なので入れない。範囲の誤りは手順4。
    assert_eq!(
        check_unsupported(as_of, &terms, None, &[hours("2100-01-01")]),
        Ok(GrantRow::Normal)
    );
    assert_eq!(
        check_unsupported(as_of, &terms, None, &[hours("1999-12-31")]),
        Err(LeaveUnsupportedCode::LeaveHourly)
    );
    let grant = |text: &str, halves| GrantCheck {
        date: loose(text),
        halves,
    };
    let split = [grant("2019-04-01", 10), grant("2019-07-01", 10)];
    assert_eq!(
        check_unsupported(as_of, &terms, Some(&split), &[]),
        Err(LeaveUnsupportedCode::LeaveSplitGrant)
    );
    // 書式の誤りの付与日があれば、並べられないので分割付与を判定しない。
    let broken = [
        grant("2019-04-01", 10),
        grant("2019-07-01", 10),
        grant("2019-02-30", 20),
    ];
    assert_eq!(
        check_unsupported(as_of, &terms, Some(&broken), &[]),
        Ok(GrantRow::Normal)
    );
    // 範囲の外でも書式が正しければ並べる（1999年の小さい付与の次が2000年の付与）。
    let old = [grant("1999-10-01", 10), grant("2000-01-01", 10)];
    assert_eq!(
        check_unsupported(as_of, &terms, Some(&old), &[]),
        Err(LeaveUnsupportedCode::LeaveSplitGrant)
    );
    // 表の外は最後。
    let zero = WorkingTerms {
        weekly_scheduled_days: 0,
        weekly_scheduled_minutes: 0,
        annual_scheduled_days: None,
    }
    .validate()
    .unwrap();
    assert_eq!(
        check_unsupported(as_of, &zero, Some(&broken), &[]),
        Err(LeaveUnsupportedCode::LeaveOutOfTable)
    );
}

/// AC-06.9 対応外の説明に、扱いの名前と根拠の条文が入る。
#[test]
fn ac06_9_unsupported_codes_cite_the_law() {
    assert_eq!(LeaveUnsupportedCode::LeaveHourly.provenances(), [LSA_39_4]);
    assert_eq!(
        LeaveUnsupportedCode::LeaveSplitGrant.provenances(),
        [LSA_39_7, REG_24NO5]
    );
    assert_eq!(
        LeaveUnsupportedCode::LeaveOutOfTable.provenances(),
        [LSA_39_3, REG_24NO3]
    );
    for code in LeaveUnsupportedCode::ALL {
        let text = code.to_string();
        assert!(text.contains(code.work_style()), "{text}");
        assert!(text.contains(&code.citation()), "{text}");
        assert!(text.contains(code.difference()), "{text}");
        for provenance in code.provenances() {
            assert_eq!(provenance.confirmed_on, CONFIRMED_ON);
            assert!(
                provenance.law == LABOR_STANDARDS_ACT
                    || provenance.law == LABOR_STANDARDS_REGULATION
            );
        }
    }
    assert_eq!(
        LeaveUnsupportedCode::LeaveOutOfTable.citation(),
        "労働基準法第39条第3項、労働基準法施行規則第24条の3"
    );
}

/// AC-06.10 管理簿: 付与ごとに基準日・付与日数・5日の期間に入る取得（日付の昇順、同じ日は入力の順）・計。
/// `as_of` より後の付与と取得は出ない。
#[test]
fn ac06_10_register_rows() {
    let records = [
        ("2025-08-01", Full),
        ("2025-05-01", Half),
        ("2025-05-01", Full),
        ("2026-02-01", Half),
        ("2026-04-01", Full),
    ];
    let result = compute(&statutory(
        "2026-03-01",
        "2024-04-01",
        &[Meets, Meets],
        &records,
    ))
    .unwrap();
    assert_eq!(result.register.len(), 2);
    let first = &result.register[0];
    assert_eq!(first.base_date, date("2024-10-01"));
    assert_eq!(first.granted_halves, 20);
    let rows: Vec<(String, LeaveUnit)> = first
        .taken
        .iter()
        .map(|t| (t.date.to_string(), t.unit))
        .collect();
    assert_eq!(
        rows,
        [
            ("2025-05-01".to_owned(), Half),
            ("2025-05-01".to_owned(), Full),
            ("2025-08-01".to_owned(), Full),
        ]
    );
    assert_eq!(first.taken_halves, 5);
    let second = &result.register[1];
    assert_eq!(second.taken.len(), 1, "2026-04-01 は as_of より後");
    assert_eq!(second.taken_halves, 1);
    // as_of より後の付与のロットは出ない（as_of 当日の付与は出る）。
    assert!(
        result
            .lots
            .iter()
            .all(|lot| lot.grant_date <= date("2026-03-01"))
    );
    let on_grant_day = compute(&statutory("2025-10-01", "2024-04-01", &[], &[])).unwrap();
    assert_eq!(on_grant_day.lots.len(), 2);
}

/// AC-06.10 残数が足りない取得は不正な入力（入力の何番目か）。`as_of` より後の取得は検査しない。
#[test]
fn ac06_10_insufficient_balance_is_invalid() {
    let eleven: Vec<String> = (1..=11).map(|d| format!("2025-06-{d:02}")).collect();
    let records: Vec<(&str, LeaveUnit)> = eleven.iter().map(|d| (d.as_str(), Full)).collect();
    assert_eq!(
        compute(&statutory("2025-07-01", "2024-04-01", &[Meets], &records)),
        Err(LeaveError::InsufficientBalance { index: 10 })
    );
    // 最初の付与より前の取得。
    assert_eq!(
        compute(&statutory(
            "2025-07-01",
            "2024-04-01",
            &[],
            &[("2024-09-30", Half)]
        )),
        Err(LeaveError::InsufficientBalance { index: 0 })
    );
    // 並びは日付順に直して消化する（入力の番号で返す）。
    let shuffled = [("2025-06-02", Full), ("2024-05-01", Half)];
    assert_eq!(
        compute(&statutory("2025-07-01", "2024-04-01", &[], &shuffled)),
        Err(LeaveError::InsufficientBalance { index: 1 })
    );
    // as_of より後なら、足りなくても数えず、誤りにしない。
    assert!(compute(&statutory("2025-05-31", "2024-04-01", &[Meets], &records)).is_ok());
    // 失効したロットは使えない（2回目と3回目は below で付与なし。失効日の当日には使えるロットがない）。
    assert_eq!(
        compute(&statutory(
            "2026-12-01",
            "2024-04-01",
            &[Meets, Below, Below],
            &[("2026-10-01", Half)]
        )),
        Err(LeaveError::InsufficientBalance { index: 0 })
    );
    // 3回目が未確認なら付与されるので、同じ日の取得は新しいロットから。
    let unknown_third = compute(&statutory(
        "2026-12-01",
        "2024-04-01",
        &[Meets, Below],
        &[("2026-10-01", Half)],
    ))
    .unwrap();
    assert_eq!(unknown_third.lots[1].used_halves, 1);
}

/// 明示の付与の日数は1〜200半日。同じ日の付与は2つのロット（入力の順）。
#[test]
fn ac06_10_explicit_grants() {
    for halves in [0, 201, -1] {
        assert_eq!(
            compute(&explicit(
                "2020-01-01",
                &[("2019-04-01", 20), ("2019-10-01", halves)],
                &[]
            )),
            Err(LeaveError::GrantHalvesOutOfRange { index: 1 })
        );
    }
    let result = compute(&explicit(
        "2020-01-01",
        &[("2019-10-01", 200), ("2019-04-01", 1)],
        &[],
    ));
    assert_eq!(
        result,
        Err(LeaveError::Unsupported(
            LeaveUnsupportedCode::LeaveSplitGrant
        ))
    );
    let result = compute(&explicit(
        "2020-01-01",
        &[("2019-04-01", 24), ("2019-04-01", 20)],
        &[("2019-05-01", Full)],
    ))
    .unwrap();
    assert_eq!(
        lots(&result),
        owned(&[("2019-04-01", 24, 2, 22, 0), ("2019-04-01", 20, 0, 20, 0)])
    );
    assert_eq!(result.balance_halves, 42);
}

/// 誤りの文に、入力の値を含めない。
#[test]
fn ac06_error_messages() {
    for error in [
        LeaveError::OutOfRange {
            field: "weekly_scheduled_days",
            expected: "0〜7",
        },
        LeaveError::GrantHalvesOutOfRange { index: 3 },
        LeaveError::Unsupported(LeaveUnsupportedCode::LeaveHourly),
        LeaveError::InsufficientBalance { index: 3 },
        LeaveError::DateOutOfRange,
        LeaveError::Overflow,
    ] {
        let text = error.to_string();
        assert_ne!(text, "");
        assert!(!text.contains("2025"), "{text}");
    }
    assert!(
        LeaveError::InsufficientBalance { index: 3 }
            .to_string()
            .contains("taken[3]")
    );
}
