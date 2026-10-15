//! 打刻の対の性質の試験（proptest）。AC-01.1・01.2・01.4・01.5、AC-10.4・10.5（打刻の側）。
//!
//! - 正しい打刻の列は必ず対になり、対にした勤務は区分の計算の入力の条件（休憩は内側・重ならない、
//!   勤務どうしは重ならない、1440分未満）を満たす。各勤務の「実労働＋休憩＝始業から終業」。
//! - `worker_favorable` の丸めでは、実際に働いた秒は、どれも丸めた実労働の区間に入る（実労働が減らない）。
//! - 秒が0の打刻を `reject` で対にすると、丸めは何もしない。
//! - 任意の打刻の列でパニックせず、結果は打刻の並びによらない。

use kintai_core::Instant;
use kintai_core::punch::{
    EmployeeId, PairedShift, PairingOutcome, Punch, PunchKind, PunchTime, SecondsPolicy, pair,
};
use proptest::prelude::*;

/// 2026-10-01T00:00:00 の、1970-01-01 からの秒。
fn base() -> i64 {
    "2026-10-01T00:00".parse::<Instant>().unwrap().minutes() * 60
}

fn time(seconds: i64) -> PunchTime {
    let minute = Instant::from_minutes(seconds.div_euclid(60)).unwrap();
    PunchTime::new(minute, u32::try_from(seconds.rem_euclid(60)).unwrap()).unwrap()
}

fn seconds_of(instant: Instant) -> i64 {
    instant.minutes() * 60
}

/// 勤務1つの作り方: 前の勤務の終業からの空き、（実労働, 休憩）の並び、最後の実労働。単位は秒。
#[derive(Clone, Debug)]
struct Plan {
    gap: i64,
    segments: Vec<(i64, i64)>,
    last: i64,
}

fn plans(unit: i64, gap: i64, work: i64, rest: i64) -> impl Strategy<Value = Vec<Plan>> {
    prop::collection::vec(
        (
            1..=gap,
            prop::collection::vec((1..=work, 1..=rest), 0..=3),
            1..=work,
        )
            .prop_map(move |(gap, segments, last)| Plan {
                gap: gap * unit,
                segments: segments
                    .into_iter()
                    .map(|(w, r)| (w * unit, r * unit))
                    .collect(),
                last: last * unit,
            }),
        1..=6,
    )
}

/// 秒の単位の作り方（勤務は最長 4×15000＋3×3000＝69000秒で、24時間に届かない）。
fn second_plans() -> impl Strategy<Value = Vec<Plan>> {
    plans(1, 100_000, 15_000, 3_000)
}

/// 分の単位の作り方（秒はすべて0。勤務は最長 4×250＋3×50＝1150分）。
fn minute_plans() -> impl Strategy<Value = Vec<Plan>> {
    plans(60, 2_000, 250, 50)
}

/// 秒の区間 `[開始, 終了)`。
type Span = (i64, i64);

/// 実際の勤務（始業, 終業, 休憩）（秒）。
type ActualShift = (i64, i64, Vec<Span>);

struct Built {
    punches: Vec<Punch>,
    /// 実際の実労働の区間（秒）。
    work: Vec<Span>,
    shifts: Vec<ActualShift>,
}

fn build(plans: &[Plan]) -> Built {
    let employee = EmployeeId::new("E001").unwrap();
    let mut punches = Vec::new();
    let mut work = Vec::new();
    let mut shifts = Vec::new();
    let mut add = |at: i64, kind| {
        punches.push(Punch {
            employee: employee.clone(),
            time: time(at),
            kind,
        });
    };
    let mut cursor = base();
    for plan in plans {
        cursor += plan.gap;
        let start = cursor;
        add(cursor, PunchKind::In);
        let mut segment_start = cursor;
        let mut breaks = Vec::new();
        for &(w, r) in &plan.segments {
            cursor += w;
            work.push((segment_start, cursor));
            add(cursor, PunchKind::BreakStart);
            let rest_start = cursor;
            cursor += r;
            breaks.push((rest_start, cursor));
            add(cursor, PunchKind::BreakEnd);
            segment_start = cursor;
        }
        cursor += plan.last;
        work.push((segment_start, cursor));
        add(cursor, PunchKind::Out);
        shifts.push((start, cursor, breaks));
    }
    Built {
        punches,
        work,
        shifts,
    }
}

fn paired_shifts(punches: &[Punch], seconds: SecondsPolicy) -> Vec<PairedShift> {
    let result = pair(punches, seconds).unwrap();
    assert_eq!(result.len(), 1);
    match &result[0].outcome {
        PairingOutcome::Paired(shifts) => shifts.clone(),
        PairingOutcome::Refused(anomalies) => panic!("異常がある: {anomalies:?}"),
    }
}

/// 区分の計算の入力の条件と、「実労働＋休憩＝始業から終業」。
fn assert_valid(shifts: &[PairedShift]) {
    for shift in shifts {
        let span = shift.span();
        assert!(span.len_minutes() < 1440, "{shift:?}");
        let mut previous_end = span.start();
        for rest in shift.breaks() {
            // 休憩は始業の後・終業の前（端に接しない）にあり、前の休憩と接しない。
            assert!(previous_end < rest.start(), "{shift:?}");
            assert!(rest.end() < span.end(), "{shift:?}");
            previous_end = rest.end();
        }
        // 1分ずつ数えた実労働と休憩の合計が、始業から終業の長さに等しい。
        let (mut work, mut rest) = (0, 0);
        for minute in span.start().minutes()..span.end().minutes() {
            let instant = Instant::from_minutes(minute).unwrap();
            if shift.breaks().iter().any(|b| b.contains(instant)) {
                rest += 1;
            } else {
                work += 1;
            }
        }
        let break_total: i64 = shift.breaks().iter().map(|b| b.len_minutes()).sum();
        assert_eq!(rest, break_total);
        assert_eq!(work + rest, span.len_minutes());
    }
    for pair in shifts.windows(2) {
        assert!(pair[0].span().end() <= pair[1].span().start(), "{pair:?}");
    }
}

/// 丸めた実労働の区間（秒。重ならず、接するものはつないだもの）。
fn rounded_work(shifts: &[PairedShift]) -> Vec<(i64, i64)> {
    let mut pieces: Vec<(i64, i64)> = Vec::new();
    for shift in shifts {
        let mut cursor = seconds_of(shift.span().start());
        for rest in shift.breaks() {
            pieces.push((cursor, seconds_of(rest.start())));
            cursor = seconds_of(rest.end());
        }
        pieces.push((cursor, seconds_of(shift.span().end())));
    }
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in pieces {
        match merged.last_mut() {
            Some(last) if last.1 >= start => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// 任意の打刻（3人、10分刻みの時刻に秒を0・20・40、種別は無作為）。同時刻も24時間超えも起きる。
fn arbitrary_punches() -> impl Strategy<Value = Vec<Punch>> {
    prop::collection::vec((0..3_usize, 0..200_i64, 0..3_i64, 0..4_usize), 0..40).prop_map(|items| {
        let ids = ["E001", "E002", "x.y-z_9"];
        items
            .into_iter()
            .map(|(who, step, second, kind)| Punch {
                employee: EmployeeId::new(ids[who]).unwrap(),
                time: time(base() + step * 600 + second * 20),
                kind: PunchKind::ALL[kind],
            })
            .collect()
    })
}

/// 番号を除いた、1人の結果（勤務か、異常の種類と時点）。
type Projected = (String, Result<Vec<PairedShift>, Vec<(String, PunchTime)>>);

/// 番号を除いた結果（並びを変えた入力と比べるため）。
fn without_index(punches: &[Punch]) -> Vec<Projected> {
    pair(punches, SecondsPolicy::WorkerFavorable)
        .unwrap()
        .into_iter()
        .map(|employee| {
            let outcome = match employee.outcome {
                PairingOutcome::Paired(shifts) => Ok(shifts),
                PairingOutcome::Refused(anomalies) => Err(anomalies
                    .iter()
                    .map(|a| (a.kind.code().to_string(), a.time))
                    .collect()),
            };
            (employee.employee.to_string(), outcome)
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// AC-01.1・01.4: 正しい打刻の列は対になり、条件を満たし、実際に働いた秒はすべて丸めた実労働に入る。
    #[test]
    fn ac01_4_worker_favorable_never_loses_work(plans in second_plans()) {
        let built = build(&plans);
        let shifts = paired_shifts(&built.punches, SecondsPolicy::WorkerFavorable);
        assert_valid(&shifts);
        let covered = rounded_work(&shifts);
        for &(start, end) in &built.work {
            let inside = covered.iter().find(|(s, e)| *s <= start && start < *e);
            prop_assert!(
                inside.is_some_and(|(_, e)| end <= *e),
                "実労働 {start}..{end} が丸めた区間 {covered:?} に入らない"
            );
        }
        let actual: i64 = built.work.iter().map(|(s, e)| e - s).sum();
        let rounded: i64 = covered.iter().map(|(s, e)| e - s).sum();
        prop_assert!(rounded >= actual);
        // 丸めで始業が遅くなることはない（切り捨て）。
        prop_assert_eq!(seconds_of(shifts[0].span().start()), built.shifts[0].0 - built.shifts[0].0.rem_euclid(60));
    }

    /// AC-01.1・01.5: 秒が0の打刻は、`reject` でも `worker_favorable` でも、打刻どおりの勤務と休憩になる。
    #[test]
    fn ac01_5_whole_minutes_are_kept_as_is(plans in minute_plans()) {
        let built = build(&plans);
        let rejected = paired_shifts(&built.punches, SecondsPolicy::Reject);
        let favorable = paired_shifts(&built.punches, SecondsPolicy::WorkerFavorable);
        prop_assert_eq!(&rejected, &favorable);
        assert_valid(&rejected);
        prop_assert_eq!(rejected.len(), built.shifts.len());
        for (shift, (start, end, breaks)) in rejected.iter().zip(&built.shifts) {
            prop_assert_eq!(seconds_of(shift.span().start()), *start);
            prop_assert_eq!(seconds_of(shift.span().end()), *end);
            let got: Vec<(i64, i64)> = shift
                .breaks()
                .iter()
                .map(|b| (seconds_of(b.start()), seconds_of(b.end())))
                .collect();
            prop_assert_eq!(&got, breaks);
        }
    }

    /// AC-01.2・AC-10.5（打刻の側）: 任意の打刻でパニックせず、勤務は条件を満たし、異常は時刻の順。
    #[test]
    fn ac10_5_arbitrary_punches_do_not_panic(punches in arbitrary_punches()) {
        let result = pair(&punches, SecondsPolicy::WorkerFavorable).unwrap();
        let ids: Vec<&EmployeeId> = result.iter().map(|r| &r.employee).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        prop_assert_eq!(ids, sorted);
        for employee in &result {
            match &employee.outcome {
                PairingOutcome::Paired(shifts) => assert_valid(shifts),
                PairingOutcome::Refused(anomalies) => {
                    prop_assert!(!anomalies.is_empty());
                    let times: Vec<PunchTime> = anomalies.iter().map(|a| a.time).collect();
                    let mut in_order = times.clone();
                    in_order.sort();
                    prop_assert_eq!(times, in_order);
                    for anomaly in anomalies {
                        prop_assert!(anomaly.index < punches.len());
                    }
                }
            }
        }
    }

    /// AC-10.4: 打刻の並びを変えても、勤務と異常（種類と時点）は同じ。
    #[test]
    fn ac10_4_result_does_not_depend_on_input_order(
        (punches, shuffled) in arbitrary_punches()
            .prop_flat_map(|punches| (Just(punches.clone()), Just(punches).prop_shuffle()))
    ) {
        prop_assert_eq!(without_index(&punches), without_index(&shuffled));
    }
}
