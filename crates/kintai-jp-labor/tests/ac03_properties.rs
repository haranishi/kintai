//! 性質の試験（proptest）。
//!
//! - AC-03.13: 区分の合計は、法定内＋時間外＋法定休日＝実労働。深夜は区分ごとの内数の和。
//! - AC-03.1: 合計は、休憩を除いた実労働の分と一致し、休憩を足しても実労働は増えない。
//! - AC-04.1: 月の期間の60時間超は、期間の時間外のうち3600分を超えた分。
//! - 区間の演算の実装が、1分ずつ数える素朴な実装（試験だけの実装）と全件一致する。
//! - 勤務の順序を入れ替えても、結果が同じ。
//! - AC-03.15: `data_from` を無作為に与え、`IncompleteWeek` も含めて素朴な実装と一致する。

mod common;

use std::collections::BTreeMap;

use common::naive_from;
use kintai_core::{Date, Instant, Week, Weekday};
use kintai_jp_labor::{
    AmbiguityMode, Classification, ShiftInput, ShiftSet, ValidPolicy, WorkPolicy, WorkSystem,
    classify,
};
use proptest::prelude::*;

#[derive(Clone, Debug)]
struct RawShift {
    gap: i64,
    len: i64,
    breaks: Vec<(i64, i64)>,
}

fn weekday() -> impl Strategy<Value = Weekday> {
    prop::sample::select(Weekday::ALL.to_vec())
}

fn work_policy() -> impl Strategy<Value = WorkPolicy> {
    (
        weekday(),
        weekday(),
        any::<bool>(),
        1_i64..=28,
        0_i64..=480,
        prop::bool::weighted(0.8),
    )
        .prop_map(
            |(week_start, legal_holiday, special, month_start_day, scheduled, use_default)| {
                WorkPolicy {
                    work_system: WorkSystem::Standard,
                    scheduled_minutes_per_day: scheduled,
                    week_start: Some(week_start),
                    weekly_limit_minutes: Some(if special { 2640 } else { 2400 }),
                    special_44h_confirmed: Some(special),
                    legal_holiday_weekday: Some(legal_holiday),
                    month_start_day,
                    ambiguity: Some(if use_default {
                        AmbiguityMode::UseDefault
                    } else {
                        AmbiguityMode::Reject
                    }),
                    worker_kind: None,
                    holiday_system: None,
                    holiday_swaps: 0,
                    night_window: None,
                }
            },
        )
}

fn raw_shift(max_gap: i64, min_len: i64) -> impl Strategy<Value = RawShift> {
    (
        0..max_gap,
        min_len..=1439_i64,
        prop::collection::vec((0_i64..240, 1_i64..120), 0..4),
    )
        .prop_map(|(gap, len, breaks)| RawShift { gap, len, breaks })
}

/// 始業の分 `base` から、重ならない勤務を並べる（接するものも作る）。休憩は勤務の内側。
fn build(base: i64, raws: &[RawShift]) -> Vec<ShiftInput> {
    let at = |m: i64| Instant::from_minutes(m).unwrap();
    let mut cursor = base;
    let mut shifts = Vec::new();
    for raw in raws {
        let start = cursor + raw.gap;
        let end = start + raw.len;
        let mut breaks = Vec::new();
        let mut inner = start + 1;
        for (gap, len) in &raw.breaks {
            let rest_start = inner + gap;
            let rest_end = rest_start + len;
            if rest_end >= end {
                break;
            }
            breaks.push((at(rest_start), at(rest_end)));
            inner = rest_end;
        }
        shifts.push(ShiftInput {
            start: at(start),
            end: at(end),
            breaks,
        });
        cursor = end;
    }
    shifts
}

/// まばらな勤務（週・月の境界や休日をいろいろ通る）と、詰まった勤務（60時間超に届く）。
/// `data_from` は、半分は無し、半分は最初の勤務の前後数日。
fn case() -> impl Strategy<Value = (WorkPolicy, Vec<ShiftInput>, Option<Date>)> {
    let room = 100_000;
    let base = prop_oneof![
        Instant::INPUT_MIN.minutes()..Instant::INPUT_MIN.minutes() + 60 * 1440,
        Instant::INPUT_MIN.minutes()..Instant::INPUT_MAX.minutes() - room,
        Instant::INPUT_MAX.minutes() - room..Instant::INPUT_MAX.minutes() - room + 1440,
    ];
    let raws = prop_oneof![
        prop::collection::vec(raw_shift(3000, 2), 0..12),
        prop::collection::vec(raw_shift(600, 600), 20..40),
    ];
    let data_from = prop::option::of(-10_i64..20);
    (work_policy(), base, raws, data_from).prop_map(|(policy, base, raws, offset)| {
        let first_day = Instant::from_minutes(base).unwrap().date();
        let data_from = offset.and_then(|days| first_day.checked_add_days(days).ok());
        (policy, build(base, &raws), data_from)
    })
}

fn run(policy: &ValidPolicy, shifts: &[ShiftInput], data_from: Option<Date>) -> Classification {
    classify(policy, &ShiftSet::new(shifts).unwrap(), data_from).unwrap()
}

fn work_minutes(shifts: &[ShiftInput]) -> i64 {
    shifts
        .iter()
        .map(|s| {
            s.end.minutes_since(s.start)
                - s.breaks
                    .iter()
                    .map(|(a, b)| b.minutes_since(*a))
                    .sum::<i64>()
        })
        .sum()
}

/// 結果の中の関係（合計、内数、並び）を確かめる。
fn check_invariants(policy: &ValidPolicy, shifts: &[ShiftInput], result: &Classification) {
    if let Some(refusal) = &result.refused {
        assert_eq!(policy.ambiguity(), AmbiguityMode::Reject);
        assert_eq!(result.days, common::NO_DAYS);
        assert_eq!(result.periods, common::NO_PERIODS);
        assert_eq!(result.ambiguities, common::NO_AMBIGUITIES);
        assert_ne!(refusal.dates, common::NO_DATES);
        assert!(refusal.dates.windows(2).all(|w| w[0] < w[1]));
        return;
    }
    if policy.ambiguity() == AmbiguityMode::Reject {
        assert_eq!(result.ambiguities, common::NO_AMBIGUITIES);
    }
    assert!(result.ambiguities.windows(2).all(|w| w[0] < w[1]));
    assert!(result.days.windows(2).all(|w| w[0].date < w[1].date));
    assert!(result.periods.windows(2).all(|w| w[0].start < w[1].start));
    let mut week_within: BTreeMap<Week, i64> = BTreeMap::new();
    for d in &result.days {
        assert!(d.total > 0);
        assert_eq!(d.total, d.within + d.overtime + d.holiday, "AC-03.13");
        assert_eq!(d.night, d.night_within + d.night_overtime + d.night_holiday);
        assert!(d.over60 <= d.overtime);
        assert!(d.night_over60 <= d.night_overtime);
        assert!(d.night_over60 <= d.over60);
        assert!(d.night <= d.total);
        assert!(d.night_within <= d.within);
        assert!(d.night_overtime <= d.overtime);
        assert!(d.night_holiday <= d.holiday);
        assert!(d.within <= 480);
        assert_eq!(
            d.extra_within,
            (d.within - policy.scheduled_minutes_per_day()).max(0)
        );
        *week_within
            .entry(d.date.week(policy.week_start()))
            .or_default() += d.within;
    }
    for within in week_within.values() {
        assert!(*within <= policy.weekly_limit_minutes());
    }
    for p in &result.periods {
        let in_period: Vec<_> = result
            .days
            .iter()
            .filter(|d| d.date.month_period(policy.month_start_day()) == p.start)
            .collect();
        assert_ne!(in_period.len(), 0);
        assert_eq!(p.total, in_period.iter().map(|d| d.total).sum::<i64>());
        assert_eq!(p.within, in_period.iter().map(|d| d.within).sum::<i64>());
        assert_eq!(
            p.overtime,
            in_period.iter().map(|d| d.overtime).sum::<i64>()
        );
        assert_eq!(p.over60, in_period.iter().map(|d| d.over60).sum::<i64>());
        assert_eq!(p.holiday, in_period.iter().map(|d| d.holiday).sum::<i64>());
        assert_eq!(p.night, in_period.iter().map(|d| d.night).sum::<i64>());
        assert_eq!(p.over60, (p.overtime - 3600).max(0), "AC-04.1");
        assert_eq!(p.total, p.within + p.overtime + p.holiday);
    }
    assert_eq!(
        result.days.iter().map(|d| d.total).sum::<i64>(),
        work_minutes(shifts),
        "AC-03.1"
    );
}

/// 勤務 `index` の実労働の区間の内側に、休憩を1つ足す（足せなければ `None`）。
fn with_extra_break(
    shifts: &[ShiftInput],
    index: usize,
    pick: usize,
) -> Option<(Vec<ShiftInput>, i64)> {
    let target = shifts.get(index)?;
    let mut rests: Vec<(i64, i64)> = target
        .breaks
        .iter()
        .map(|(a, b)| (a.minutes(), b.minutes()))
        .collect();
    rests.sort_unstable();
    let mut gaps = Vec::new();
    let mut cursor = target.start.minutes() + 1;
    for (a, b) in &rests {
        gaps.push((cursor, *a));
        cursor = *b;
    }
    gaps.push((cursor, target.end.minutes() - 1));
    let usable: Vec<(i64, i64)> = gaps.into_iter().filter(|(a, b)| b - a >= 1).collect();
    if usable.is_empty() {
        return None;
    }
    let (a, b) = usable[pick % usable.len()];
    let start = a + (b - a) / 3;
    let end = (start + 1 + (b - a) / 3).min(b);
    let mut changed = shifts.to_vec();
    changed[index].breaks.push((
        Instant::from_minutes(start).unwrap(),
        Instant::from_minutes(end).unwrap(),
    ));
    Some((changed, end - start))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// AC-03.13 区分の合計と内数の関係。区間の演算の実装と、1分ずつ数える実装が全件一致する。
    #[test]
    fn ac03_13_totals_and_naive_agreement((policy, shifts, data_from) in case()) {
        let valid = policy.validate().unwrap();
        let result = run(&valid, &shifts, data_from);
        check_invariants(&valid, &shifts, &result);
        prop_assert_eq!(&result, &naive_from(&valid, &shifts, data_from));
        // 同じ入力は同じ結果（AC-09.3 の土台）。
        prop_assert_eq!(&result, &run(&valid, &shifts, data_from));
    }

    /// 勤務の順序を入れ替えても、結果が同じ。
    #[test]
    fn ac03_13_order_does_not_matter(
        (policy, shifts, shuffled, data_from) in case().prop_flat_map(|(p, s, d)| {
            let copy = s.clone();
            (Just(p), Just(s), Just(copy).prop_shuffle(), Just(d))
        })
    ) {
        let valid = policy.validate().unwrap();
        prop_assert_eq!(run(&valid, &shifts, data_from), run(&valid, &shuffled, data_from));
    }

    /// AC-03.1 休憩を足すと、実労働はちょうどその分だけ減り、増えることはない。
    #[test]
    fn ac03_1_adding_a_break_never_adds_work(
        (policy, shifts, data_from) in case(), index in 0_usize..40, pick in 0_usize..8,
    ) {
        let mut valid_policy = policy;
        valid_policy.ambiguity = Some(AmbiguityMode::UseDefault);
        let valid = valid_policy.validate().unwrap();
        if let Some((changed, rest)) = with_extra_break(&shifts, index % shifts.len().max(1), pick) {
            let before = run(&valid, &shifts, data_from);
            let after = run(&valid, &changed, data_from);
            let total = |r: &Classification| r.days.iter().map(|d| d.total).sum::<i64>();
            prop_assert_eq!(total(&after), total(&before) - rest);
            prop_assert!(total(&after) <= total(&before));
            prop_assert_eq!(&after, &naive_from(&valid, &changed, data_from));
            // 休憩を足しても、曖昧さの記録（始業と最後の分で決まる）は変わらない。
            prop_assert_eq!(before.ambiguities, after.ambiguities);
        }
    }
}

/// 範囲の端（2000年1月）で、月の期間の初日が1999年12月になる場合も計算できる。
#[test]
fn month_period_before_2000_is_reported() {
    let mut policy = common::policy(Weekday::Sunday);
    policy.month_start_day = 16;
    let shifts = [common::shift("2000-01-04T09:00", "2000-01-04T19:00")];
    let valid = policy.validate().unwrap();
    let result = run(&valid, &shifts, None);
    assert_eq!(result.periods[0].start.to_string(), "1999-12-16");
    assert_eq!(result.periods[0].overtime, 120);
    assert_eq!(result, naive_from(&valid, &shifts, None));
    assert_eq!(result.days[0].date, Date::from_ymd(2000, 1, 4).unwrap());
}

/// 範囲の端（2099年12月31日）の最後の分まで働く勤務も計算できる。
#[test]
fn last_minute_of_the_range() {
    let policy = common::policy(Weekday::Monday);
    let shifts = [common::shift("2099-12-31T20:00", "2099-12-31T23:59")];
    let valid = policy.validate().unwrap();
    let result = run(&valid, &shifts, None);
    assert_eq!(result.days[0].total, 239);
    assert_eq!(result.days[0].night, 119);
    assert_eq!(result, naive_from(&valid, &shifts, None));
}

/// 計算の誤りの説明（検査済みの勤務では起きないが、説明は決まっている）。
#[test]
fn classify_error_message() {
    let error = kintai_jp_labor::ClassifyError::Overflow;
    assert_eq!(error.to_string(), "分の合計が大きすぎます");
    let _: &dyn std::error::Error = &error;
}
