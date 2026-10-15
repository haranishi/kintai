//! 打刻の対（`docs/spec/punch.md`）の境界の試験。AC-01.1〜01.5、AC-08.7（従業員IDの形）、AC-10.4（決定性）。
//! データはすべて合成（従業員IDは E001 など）。

use kintai_core::punch::{
    Anomaly, AnomalyKind, EmployeeId, EmployeePairing, LONG_SHIFT_MINUTES, MAX_SHIFTS_PER_MONTH,
    PairedShift, PairingOutcome, Punch, PunchError, PunchKind, PunchTime, SecondsPolicy, pair,
};
use kintai_core::{CoreError, Instant};

use PunchKind::{BreakEnd, BreakStart, In, Out};

fn punch(employee: &str, time: &str, kind: PunchKind) -> Punch {
    Punch {
        employee: EmployeeId::new(employee).unwrap(),
        time: time.parse().unwrap(),
        kind,
    }
}

fn at(text: &str) -> Instant {
    text.parse().unwrap()
}

/// 1人分の打刻を対にして、勤務を返す（異常があれば試験の失敗）。
fn paired(punches: &[Punch]) -> Vec<PairedShift> {
    paired_with(punches, SecondsPolicy::WorkerFavorable)
}

fn paired_with(punches: &[Punch], seconds: SecondsPolicy) -> Vec<PairedShift> {
    let result = pair(punches, seconds).unwrap();
    assert_eq!(result.len(), 1, "{result:?}");
    match &result[0].outcome {
        PairingOutcome::Paired(shifts) => shifts.clone(),
        PairingOutcome::Refused(anomalies) => panic!("異常がある: {anomalies:?}"),
    }
}

/// 1人分の打刻を対にして、異常を返す（異常がなければ試験の失敗）。
fn refused(punches: &[Punch]) -> Vec<Anomaly> {
    let result = pair(punches, SecondsPolicy::WorkerFavorable).unwrap();
    assert_eq!(result.len(), 1, "{result:?}");
    match &result[0].outcome {
        PairingOutcome::Refused(anomalies) => anomalies.clone(),
        PairingOutcome::Paired(shifts) => panic!("異常がない: {shifts:?}"),
    }
}

fn kinds(anomalies: &[Anomaly]) -> Vec<AnomalyKind> {
    anomalies.iter().map(|anomaly| anomaly.kind).collect()
}

/// 勤務を「開始〜終了」と休憩の文字列にする（比べやすくするため）。
fn describe(shift: &PairedShift) -> (String, String, Vec<(String, String)>) {
    (
        shift.span().start().to_string(),
        shift.span().end().to_string(),
        shift
            .breaks()
            .iter()
            .map(|rest| (rest.start().to_string(), rest.end().to_string()))
            .collect(),
    )
}

fn shift(
    start: &str,
    end: &str,
    breaks: &[(&str, &str)],
) -> (String, String, Vec<(String, String)>) {
    (
        start.to_string(),
        end.to_string(),
        breaks
            .iter()
            .map(|(s, e)| ((*s).to_string(), (*e).to_string()))
            .collect(),
    )
}

// ---- AC-01.1 対にする ----

/// AC-01.1: `in` → `break_start` → `break_end` → `out` が、休憩つきの勤務1つになる。
#[test]
fn ac01_1_in_break_out_makes_one_shift() {
    let shifts = paired(&[
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-07 12:00", BreakStart),
        punch("E001", "2026-10-07 13:00", BreakEnd),
        punch("E001", "2026-10-07 18:00", Out),
    ]);
    assert_eq!(
        shifts.iter().map(describe).collect::<Vec<_>>(),
        vec![shift(
            "2026-10-07T09:00",
            "2026-10-07T18:00",
            &[("2026-10-07T12:00", "2026-10-07T13:00")]
        )]
    );
}

/// AC-01.1: 休憩が2つ、日をまたぐ夜勤、勤務が複数日。打刻の並びは問わない（時刻順に並べ直す）。
#[test]
fn ac01_1_several_shifts_in_any_input_order() {
    let mut punches = vec![
        punch("E001", "2026-10-08T05:00", Out),
        punch("E001", "2026-10-07T22:00", In),
        punch("E001", "2026-10-08T01:00", BreakStart),
        punch("E001", "2026-10-08T01:30", BreakEnd),
        punch("E001", "2026-10-09T09:00", In),
        punch("E001", "2026-10-09T10:00", BreakStart),
        punch("E001", "2026-10-09T10:10", BreakEnd),
        punch("E001", "2026-10-09T15:00", BreakStart),
        punch("E001", "2026-10-09T15:15", BreakEnd),
        punch("E001", "2026-10-09T17:00", Out),
    ];
    let expected = vec![
        shift(
            "2026-10-07T22:00",
            "2026-10-08T05:00",
            &[("2026-10-08T01:00", "2026-10-08T01:30")],
        ),
        shift(
            "2026-10-09T09:00",
            "2026-10-09T17:00",
            &[
                ("2026-10-09T10:00", "2026-10-09T10:10"),
                ("2026-10-09T15:00", "2026-10-09T15:15"),
            ],
        ),
    ];
    assert_eq!(
        paired(&punches).iter().map(describe).collect::<Vec<_>>(),
        expected
    );
    punches.reverse();
    assert_eq!(
        paired(&punches).iter().map(describe).collect::<Vec<_>>(),
        expected
    );
}

/// AC-01.1: 従業員ごとに対にし、結果は従業員ID順（バイトの順）。
#[test]
fn ac01_1_employees_are_separate_and_sorted_by_id() {
    let punches = [
        punch("E2", "2026-10-07 09:00", In),
        punch("E10", "2026-10-07 10:00", In),
        punch("E2", "2026-10-07 17:00", Out),
        punch("E10", "2026-10-07 11:00", Out),
        punch("A9", "2026-10-07 08:00", In),
        punch("A9", "2026-10-07 08:30", Out),
    ];
    let result = pair(&punches, SecondsPolicy::default()).unwrap();
    let ids: Vec<&str> = result.iter().map(|r| r.employee.as_str()).collect();
    assert_eq!(ids, ["A9", "E10", "E2"]);
    for EmployeePairing { outcome, .. } in &result {
        assert!(matches!(outcome, PairingOutcome::Paired(shifts) if shifts.len() == 1));
    }
}

/// AC-01.1: 打刻がなければ、結果は空。
#[test]
fn ac01_1_no_punches() {
    assert_eq!(pair(&[], SecondsPolicy::Reject).unwrap(), Vec::new());
}

// ---- AC-01.2 異常 ----

/// AC-01.2: 遷移の表の外にある打刻は、それぞれの異常になる。
#[test]
fn ac01_2_transitions_outside_the_table() {
    type Case<'a> = (&'a [(&'a str, PunchKind)], &'a [AnomalyKind]);
    let cases: [Case; 9] = [
        (
            &[("09:00", In), ("10:00", In), ("18:00", Out)],
            &[AnomalyKind::DuplicateIn],
        ),
        (
            &[
                ("09:00", In),
                ("10:00", BreakStart),
                ("10:30", In),
                ("18:00", Out),
            ],
            &[AnomalyKind::DuplicateIn],
        ),
        (&[("18:00", Out)], &[AnomalyKind::OutWithoutIn]),
        (
            &[("12:00", BreakStart)],
            &[AnomalyKind::BreakStartWithoutIn],
        ),
        (
            &[
                ("09:00", In),
                ("12:00", BreakStart),
                ("12:10", BreakStart),
                ("13:00", BreakEnd),
                ("18:00", Out),
            ],
            &[AnomalyKind::BreakStartWithoutIn],
        ),
        (&[("13:00", BreakEnd)], &[AnomalyKind::BreakEndWithoutStart]),
        (
            &[("09:00", In), ("13:00", BreakEnd), ("18:00", Out)],
            &[AnomalyKind::BreakEndWithoutStart],
        ),
        (
            &[("09:00", In), ("12:00", BreakStart), ("18:00", Out)],
            &[AnomalyKind::OutDuringBreak],
        ),
        (
            &[("09:00", In), ("12:00", BreakStart)],
            &[AnomalyKind::OpenShift],
        ),
    ];
    for (punches, expected) in cases {
        let punches: Vec<Punch> = punches
            .iter()
            .map(|(time, kind)| punch("E001", &format!("2026-10-07 {time}"), *kind))
            .collect();
        assert_eq!(kinds(&refused(&punches)), expected, "{punches:?}");
    }
}

/// AC-01.2: 異常の時点と番号は、原因の打刻（`OpenShift` はその勤務の `in`）を指す。
#[test]
fn ac01_2_anomaly_points_at_the_punch() {
    let punches = [
        punch("E001", "2026-10-07 08:00", Out),
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-07 07:00:15", BreakEnd),
    ];
    let anomalies = refused(&punches);
    assert_eq!(
        anomalies,
        vec![
            Anomaly {
                kind: AnomalyKind::BreakEndWithoutStart,
                time: "2026-10-07 07:00:15".parse().unwrap(),
                index: 2,
            },
            Anomaly {
                kind: AnomalyKind::OutWithoutIn,
                time: "2026-10-07 08:00".parse().unwrap(),
                index: 0,
            },
            Anomaly {
                kind: AnomalyKind::OpenShift,
                time: "2026-10-07 09:00".parse().unwrap(),
                index: 1,
            },
        ]
    );
}

/// AC-01.2: 勤務中のまま打刻が終わると `OpenShift`。休憩中のままでも同じ。
#[test]
fn ac01_2_open_shift_at_the_end() {
    assert_eq!(
        kinds(&refused(&[punch("E001", "2026-10-31 22:00", In)])),
        [AnomalyKind::OpenShift]
    );
}

/// AC-01.2: 24時間以上の勤務は `LongShift`。23時間59分は異常でない。境界は秒を分にしたあとの長さ。
#[test]
fn ac01_2_long_shift_at_24_hours() {
    assert_eq!(LONG_SHIFT_MINUTES, 1440);
    let ok = paired(&[
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-08 08:59", Out),
    ]);
    assert_eq!(ok[0].span().len_minutes(), 1439);

    let exact = refused(&[
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-08 09:00", Out),
    ]);
    assert_eq!(
        exact,
        vec![Anomaly {
            kind: AnomalyKind::LongShift,
            time: "2026-10-07 09:00".parse().unwrap(),
            index: 0,
        }]
    );

    // 実際は23時間59分59秒でも、終業を切り上げると24時間になる（区分の計算に1440分の勤務を渡さない）。
    let rounded = refused(&[
        punch("E001", "2026-10-07 09:00:00", In),
        punch("E001", "2026-10-08 08:59:59", Out),
    ]);
    assert_eq!(kinds(&rounded), [AnomalyKind::LongShift]);

    // 何日も続く勤務も同じ。
    let days = refused(&[
        punch("E001", "2026-10-01 09:00", In),
        punch("E001", "2026-10-04 18:00", Out),
    ]);
    assert_eq!(kinds(&days), [AnomalyKind::LongShift]);
}

/// AC-01.2: 同じ従業員の同時刻（秒まで同じ）の打刻は `SameTimestamp`。1組に1件で、番号は小さい方。
#[test]
fn ac01_2_same_timestamp() {
    // 終業と次の始業が同じ時刻（接する勤務を打刻で書いたもの）。
    let touching = refused(&[
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-07 18:00", In),
        punch("E001", "2026-10-07 18:00", Out),
        punch("E001", "2026-10-07 22:00", Out),
    ]);
    assert_eq!(
        touching,
        vec![Anomaly {
            kind: AnomalyKind::SameTimestamp,
            time: "2026-10-07 18:00".parse().unwrap(),
            index: 1,
        }]
    );
    // 同じ打刻を2回（二重の記録）。
    let twice = refused(&[
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-07 18:00", Out),
    ]);
    assert!(kinds(&twice).contains(&AnomalyKind::SameTimestamp));
    // 秒が違えば同時刻ではない。
    let shifts = paired(&[
        punch("E001", "2026-10-07 09:00:00", In),
        punch("E001", "2026-10-07 18:00:00", Out),
        punch("E001", "2026-10-07 18:00:01", In),
        punch("E001", "2026-10-07 22:00:00", Out),
    ]);
    assert_eq!(shifts.len(), 2);
    // 別の従業員の同時刻は異常でない。
    let result = pair(
        &[
            punch("E001", "2026-10-07 09:00", In),
            punch("E002", "2026-10-07 09:00", In),
            punch("E001", "2026-10-07 18:00", Out),
            punch("E002", "2026-10-07 18:00", Out),
        ],
        SecondsPolicy::Reject,
    )
    .unwrap();
    assert!(
        result
            .iter()
            .all(|r| matches!(r.outcome, PairingOutcome::Paired(_)))
    );
}

/// AC-01.2: 異常は時刻の順に、すべて一覧する（最初の1つで止めない）。
#[test]
fn ac01_2_lists_every_anomaly_in_time_order() {
    let anomalies = refused(&[
        punch("E001", "2026-10-01 18:00", Out),
        punch("E001", "2026-10-02 09:00", In),
        punch("E001", "2026-10-03 09:00", In),
        punch("E001", "2026-10-03 18:00", Out),
        punch("E001", "2026-10-04 12:00", BreakEnd),
        punch("E001", "2026-10-05 09:00", In),
    ]);
    assert_eq!(
        kinds(&anomalies),
        [
            AnomalyKind::OutWithoutIn,
            AnomalyKind::DuplicateIn,
            AnomalyKind::BreakEndWithoutStart,
            AnomalyKind::OpenShift,
        ]
    );
    let times: Vec<PunchTime> = anomalies.iter().map(|a| a.time).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
}

// ---- AC-01.3 異常のある従業員は確定集計を拒否する ----

/// AC-01.3: 異常のある従業員は勤務を返さない（正しい日の勤務も返さない）。ほかの従業員は対にする。
#[test]
fn ac01_3_anomalous_employee_gets_no_shifts() {
    let punches = [
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-07 18:00", Out),
        punch("E001", "2026-10-08 18:00", Out),
        punch("E002", "2026-10-07 09:00", In),
        punch("E002", "2026-10-07 18:00", Out),
    ];
    let result = pair(&punches, SecondsPolicy::WorkerFavorable).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].employee.as_str(), "E001");
    assert_eq!(
        result[0].outcome,
        PairingOutcome::Refused(vec![Anomaly {
            kind: AnomalyKind::OutWithoutIn,
            time: "2026-10-08 18:00".parse().unwrap(),
            index: 2,
        }])
    );
    assert_eq!(result[1].employee.as_str(), "E002");
    assert!(matches!(&result[1].outcome, PairingOutcome::Paired(shifts) if shifts.len() == 1));
}

// ---- AC-01.4 秒の丸め（worker_favorable） ----

/// AC-01.4: `in` と `break_end` は切り捨て、`out` と `break_start` は切り上げる。秒が0ならそのまま。
#[test]
fn ac01_4_start_side_floors_and_end_side_ceils() {
    let shifts = paired(&[
        punch("E001", "2026-10-07 09:00:59", In),
        punch("E001", "2026-10-07 12:00:01", BreakStart),
        punch("E001", "2026-10-07 12:59:59", BreakEnd),
        punch("E001", "2026-10-07 18:00:01", Out),
    ]);
    assert_eq!(
        describe(&shifts[0]),
        shift(
            "2026-10-07T09:00",
            "2026-10-07T18:01",
            &[("2026-10-07T12:01", "2026-10-07T12:59")]
        )
    );
    let exact = paired(&[
        punch("E001", "2026-10-07 09:00:00", In),
        punch("E001", "2026-10-07 18:00:00", Out),
    ]);
    assert_eq!(
        describe(&exact[0]),
        shift("2026-10-07T09:00", "2026-10-07T18:00", &[])
    );
}

/// AC-01.4: 丸めで長さが0以下になる休憩は、無かったものとする（実労働を減らさない側）。
#[test]
fn ac01_4_break_shorter_than_a_minute_disappears() {
    let shifts = paired(&[
        punch("E001", "2026-10-07 09:00", In),
        punch("E001", "2026-10-07 12:00:10", BreakStart),
        punch("E001", "2026-10-07 12:00:50", BreakEnd),
        punch("E001", "2026-10-07 12:30:10", BreakStart),
        punch("E001", "2026-10-07 12:31:05", BreakEnd),
        punch("E001", "2026-10-07 18:00", Out),
    ]);
    // 12:00:10〜12:00:50 は 12:01〜12:00 で消える。12:30:10〜12:31:05 は 12:31〜12:31 で消える。
    assert_eq!(
        describe(&shifts[0]),
        shift("2026-10-07T09:00", "2026-10-07T18:00", &[])
    );
}

/// AC-01.4: 同じ1分の中で終業と次の始業があると、丸めた勤務が重なる。前の勤務の終わりを次の始業に合わせる。
#[test]
fn ac01_4_shifts_ending_and_starting_in_one_minute() {
    let shifts = paired(&[
        punch("E001", "2026-10-07 09:00:00", In),
        punch("E001", "2026-10-07 18:00:30", Out),
        punch("E001", "2026-10-07 18:00:45", In),
        punch("E001", "2026-10-07 22:00:00", Out),
    ]);
    assert_eq!(
        shifts.iter().map(describe).collect::<Vec<_>>(),
        vec![
            shift("2026-10-07T09:00", "2026-10-07T18:00", &[]),
            shift("2026-10-07T18:00", "2026-10-07T22:00", &[]),
        ]
    );
}

/// AC-01.4: 前の勤務の最後の休憩が切った終わりに届くなら、その休憩の始まりで前の勤務を終える
/// （休憩が勤務の端に接しないように）。1分の中に収まる勤務は、次の勤務に吸収される。
#[test]
fn ac01_4_clipping_keeps_breaks_inside() {
    let shifts = paired(&[
        punch("E001", "2026-10-07 09:00:00", In),
        punch("E001", "2026-10-07 17:00:00", BreakStart),
        punch("E001", "2026-10-07 18:00:10", BreakEnd),
        punch("E001", "2026-10-07 18:00:20", Out),
        punch("E001", "2026-10-07 18:00:40", In),
        punch("E001", "2026-10-07 20:00:00", Out),
    ]);
    assert_eq!(
        shifts.iter().map(describe).collect::<Vec<_>>(),
        vec![
            shift("2026-10-07T09:00", "2026-10-07T17:00", &[]),
            shift("2026-10-07T18:00", "2026-10-07T20:00", &[]),
        ]
    );

    let absorbed = paired(&[
        punch("E001", "2026-10-07 18:00:05", In),
        punch("E001", "2026-10-07 18:00:15", Out),
        punch("E001", "2026-10-07 18:00:25", In),
        punch("E001", "2026-10-07 18:00:35", Out),
        punch("E001", "2026-10-07 18:00:45", In),
        punch("E001", "2026-10-07 19:00:00", Out),
    ]);
    assert_eq!(
        absorbed.iter().map(describe).collect::<Vec<_>>(),
        vec![shift("2026-10-07T18:00", "2026-10-07T19:00", &[])]
    );
}

// ---- AC-01.5 reject ----

/// AC-01.5: `reject` では、秒が0でない打刻を不正な入力にする（最も小さい番号を返す）。秒が0なら対にする。
#[test]
fn ac01_5_reject_nonzero_seconds() {
    let punches = [
        punch("E002", "2026-10-07 09:00:00", In),
        punch("E001", "2026-10-07 18:00:01", Out),
        punch("E001", "2026-10-07 09:00:00", In),
        punch("E002", "2026-10-07 18:00:59", Out),
    ];
    assert_eq!(
        pair(&punches, SecondsPolicy::Reject),
        Err(PunchError::NonZeroSeconds { index: 1 })
    );
    assert!(pair(&punches, SecondsPolicy::WorkerFavorable).is_ok());

    let zero = [
        punch("E001", "2026-10-07 09:00:00", In),
        punch("E001", "2026-10-07 18:00", Out),
    ];
    assert_eq!(
        paired_with(&zero, SecondsPolicy::Reject)
            .iter()
            .map(describe)
            .collect::<Vec<_>>(),
        vec![shift("2026-10-07T09:00", "2026-10-07T18:00", &[])]
    );
    let message = PunchError::NonZeroSeconds { index: 1 }.to_string();
    assert!(message.contains("reject"), "{message}");
}

/// AC-01.5: 方針の値は `worker_favorable`（既定）と `reject` だけ。
#[test]
fn ac01_5_seconds_policy_codes() {
    assert_eq!(SecondsPolicy::default(), SecondsPolicy::WorkerFavorable);
    for policy in [SecondsPolicy::WorkerFavorable, SecondsPolicy::Reject] {
        assert_eq!(SecondsPolicy::from_code(policy.code()), Some(policy));
    }
    for other in ["", "Reject", "floor", "round", "daily_floor", " reject"] {
        assert_eq!(SecondsPolicy::from_code(other), None, "{other}");
    }
}

// ---- 時点の表記 ----

/// AC-01.4・01.5: 打刻の時点は `YYYY-MM-DD HH:MM[:SS]`（区切りは空白か `T`）。往復で一致し、不正な表記は拒否する。
#[test]
fn ac01_punch_time_notation() {
    for (text, shown) in [
        ("2026-10-07 09:00", "2026-10-07T09:00:00"),
        ("2026-10-07T09:00", "2026-10-07T09:00:00"),
        ("2026-10-07 09:00:05", "2026-10-07T09:00:05"),
        ("2026-10-07T23:59:59", "2026-10-07T23:59:59"),
        ("2000-01-01 00:00:00", "2000-01-01T00:00:00"),
        ("2099-12-31 23:59:59", "2099-12-31T23:59:59"),
    ] {
        let time = PunchTime::parse(text).unwrap();
        assert_eq!(time.to_string(), shown);
        assert_eq!(PunchTime::parse(shown).unwrap(), time);
    }
    for (text, error) in [
        ("2026-10-07 09:00:60", CoreError::InvalidTime),
        ("2026-10-07 09:60:00", CoreError::InvalidTime),
        ("2026-10-07 25:00:00", CoreError::InvalidTime),
        ("2026-10-07 24:00:00", CoreError::Hour24),
        ("2026-02-30 09:00:00", CoreError::InvalidDate),
        ("1999-12-31 23:59:59", CoreError::OutOfRange),
        ("2100-01-01 00:00:00", CoreError::OutOfRange),
        ("2026-10-07 09:00:5", CoreError::InvalidFormat),
        ("2026-10-07 09:00.05", CoreError::InvalidFormat),
        ("2026-10-07 09:00:0a", CoreError::InvalidFormat),
        ("2026-10-07 9:00:00", CoreError::InvalidFormat),
        ("2026/10/07 09:00:00", CoreError::InvalidFormat),
        (" 2026-10-07 09:00", CoreError::InvalidFormat),
        ("2026-10-07 09:00:00Z", CoreError::InvalidFormat),
        ("2026-10-07", CoreError::InvalidFormat),
        ("２０２６-10-07 09:00", CoreError::InvalidFormat),
        ("", CoreError::InvalidFormat),
    ] {
        assert_eq!(PunchTime::parse(text), Err(error), "{text}");
    }
}

/// AC-01.4: 切り上げと切り捨て。2099-12-31T23:59:59 の切り上げは 2100-01-01T00:00（型の範囲の中）。
#[test]
fn ac01_4_floor_and_ceil() {
    let time = PunchTime::parse("2026-10-07 09:00:01").unwrap();
    assert_eq!(time.floor(), at("2026-10-07T09:00"));
    assert_eq!(time.ceil().unwrap(), at("2026-10-07T09:01"));
    assert_eq!(time.second(), 1);
    let zero = PunchTime::parse("2026-10-07 09:00").unwrap();
    assert_eq!(zero.ceil().unwrap(), zero.floor());
    let last = PunchTime::parse("2099-12-31 23:59:59").unwrap();
    assert_eq!(last.ceil().unwrap().to_string(), "2100-01-01T00:00");
    assert_eq!(
        PunchTime::new(at("2026-10-07T09:00"), 60),
        Err(CoreError::InvalidTime)
    );
    assert_eq!(
        PunchTime::new(Instant::from_minutes(0).unwrap(), 0),
        Err(CoreError::OutOfRange)
    );
}

// ---- 入力の上限 ----

/// AC-10.6（打刻の側）: 1人が1つの暦の月に始める勤務は1000件まで。1001件目は不正な入力。月が替われば数え直す。
#[test]
fn ac10_6_at_most_1000_shifts_per_month() {
    assert_eq!(MAX_SHIFTS_PER_MONTH, 1000);
    let mut punches = Vec::new();
    for n in 0..MAX_SHIFTS_PER_MONTH {
        // 10月の中に、40分ごとに20分の勤務を1000件。
        let start = at("2026-10-01T00:00")
            .checked_add_minutes(i64::try_from(n).unwrap() * 40)
            .unwrap();
        let end = start.checked_add_minutes(20).unwrap();
        punches.push(Punch {
            employee: EmployeeId::new("E001").unwrap(),
            time: PunchTime::new(start, 0).unwrap(),
            kind: In,
        });
        punches.push(Punch {
            employee: EmployeeId::new("E001").unwrap(),
            time: PunchTime::new(end, 0).unwrap(),
            kind: Out,
        });
    }
    // 11月の勤務は別に数える。
    punches.push(punch("E001", "2026-11-01 09:00", In));
    punches.push(punch("E001", "2026-11-01 10:00", Out));
    assert_eq!(paired(&punches).len(), MAX_SHIFTS_PER_MONTH + 1);

    punches.push(punch("E001", "2026-10-31 23:00", In));
    punches.push(punch("E001", "2026-10-31 23:30", Out));
    let index = punches.len() - 2;
    assert_eq!(
        pair(&punches, SecondsPolicy::WorkerFavorable),
        Err(PunchError::TooManyShifts { index })
    );
}

// ---- AC-08.7 従業員ID ----

/// AC-08.7: 従業員IDは `[A-Za-z0-9][A-Za-z0-9._-]{0,63}` だけ。式として実行される先頭の文字を拒否する。
#[test]
fn ac08_7_employee_id_format() {
    let longest = format!("E{}", "9".repeat(63));
    for ok in ["E001", "a", "0", "A.b_c-d", "e.g_2-x", longest.as_str()] {
        let id = EmployeeId::new(ok).unwrap();
        assert_eq!(id.as_str(), ok);
        assert_eq!(id.to_string(), ok);
        assert_eq!(ok.parse::<EmployeeId>().unwrap(), id);
    }
    let too_long = format!("E{}", "9".repeat(64));
    for bad in [
        "",
        too_long.as_str(),
        "=SUM(A1)",
        "+1",
        "-E001",
        "@E001",
        "_E001",
        ".E001",
        "E 001",
        "E001 ",
        "E001\n",
        "E\t1",
        "E,001",
        "E\"1",
        "E/1",
        "E=1",
        "E+1",
        "E@1",
        "E:1",
        "E;1",
        "E\\1",
        "従業員",
        "Ｅ001",
    ] {
        assert_eq!(
            EmployeeId::new(bad),
            Err(PunchError::InvalidEmployeeId),
            "{bad:?}"
        );
    }
    // 並べ替えはバイトの順（大文字が小文字より前、数字はさらに前）。
    let mut ids: Vec<EmployeeId> = ["b", "B", "a10", "a9", "1"]
        .iter()
        .map(|t| EmployeeId::new(t).unwrap())
        .collect();
    ids.sort();
    let sorted: Vec<&str> = ids.iter().map(EmployeeId::as_str).collect();
    assert_eq!(sorted, ["1", "B", "a10", "a9", "b"]);
}

/// 種別の値と、異常のコード。
#[test]
fn ac01_codes() {
    for kind in PunchKind::ALL {
        assert_eq!(PunchKind::from_code(kind.code()), Some(kind));
    }
    assert_eq!(
        PunchKind::ALL.map(PunchKind::code),
        ["in", "out", "break_start", "break_end"]
    );
    for other in ["IN", "In", " in", "in ", "break-start", "start", ""] {
        assert_eq!(PunchKind::from_code(other), None, "{other}");
    }
    assert_eq!(
        AnomalyKind::ALL.map(AnomalyKind::code),
        [
            "DuplicateIn",
            "OutWithoutIn",
            "BreakStartWithoutIn",
            "BreakEndWithoutStart",
            "OutDuringBreak",
            "OpenShift",
            "SameTimestamp",
            "LongShift",
        ]
    );
}

/// AC-10.4: 同じ打刻の集まりは、並びによらず、まったく同じ結果になる（異常の番号は入力の並びを指す）。
#[test]
fn ac10_4_same_input_same_result() {
    let punches = vec![
        punch("E002", "2026-10-07 09:00", In),
        punch("E001", "2026-10-07 22:00:30", In),
        punch("E002", "2026-10-07 12:00", BreakStart),
        punch("E001", "2026-10-08 05:00:10", Out),
        punch("E002", "2026-10-07 13:00", BreakEnd),
        punch("E002", "2026-10-07 18:00", Out),
    ];
    let first = pair(&punches, SecondsPolicy::WorkerFavorable).unwrap();
    for _ in 0..3 {
        assert_eq!(
            pair(&punches, SecondsPolicy::WorkerFavorable).unwrap(),
            first
        );
    }
    let mut reversed = punches.clone();
    reversed.reverse();
    assert_eq!(
        pair(&reversed, SecondsPolicy::WorkerFavorable).unwrap(),
        first
    );
}
