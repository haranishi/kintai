//! 説明（`explain`）の性質の試験（proptest）。無作為の方針と勤務で、すべての労働日について:
//!
//! - 区間の分を区分ごとに足すと、区分の結果のその日の値（法定内・所定外法定内・時間外・60時間超・法定休日・
//!   深夜の内訳）と一致する。
//! - 区間は時刻順で重ならず、続いている（接する）区間は、区分か深夜の属性が違う（つなぎ切っている）。
//! - 深夜の区間の分はすべて 22:00〜5:00、そうでない区間の分はすべて 5:00〜22:00 にある。
//! - 割増の下限は、表（`premium_floor`）のとおり。

use kintai_core::{Instant, MINUTES_PER_DAY, Weekday};
use kintai_jp_labor::explain::{Category, ExplainError, Segment, explain_day, premium_floor};
use kintai_jp_labor::{AmbiguityMode, ShiftInput, ShiftSet, WorkPolicy, WorkSystem, classify};
use proptest::prelude::*;

fn work_policy() -> impl Strategy<Value = WorkPolicy> {
    (
        prop::sample::select(Weekday::ALL.to_vec()),
        prop::sample::select(Weekday::ALL.to_vec()),
        prop::sample::select(vec![1_i64, 16]),
        0_i64..=480,
        prop::bool::weighted(0.8),
    )
        .prop_map(
            |(week_start, legal_holiday, month_start_day, scheduled, use_default)| WorkPolicy {
                work_system: WorkSystem::Standard,
                scheduled_minutes_per_day: scheduled,
                week_start: Some(week_start),
                weekly_limit_minutes: None,
                special_44h_confirmed: None,
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
            },
        )
}

/// 重ならない勤務（2023年3月から。60時間超の率の境界の 2023-04-01 をまたぐ）。
fn shifts(
    max_gap: i64,
    min_len: i64,
    count: std::ops::Range<usize>,
) -> impl Strategy<Value = Vec<ShiftInput>> {
    prop::collection::vec(
        (
            0..max_gap,
            min_len..=1439_i64,
            prop::collection::vec((0_i64..240, 1_i64..120), 0..3),
        ),
        count,
    )
    .prop_map(|raws| {
        let at = |m: i64| Instant::from_minutes(m).unwrap();
        let mut cursor = "2023-03-10T00:00".parse::<Instant>().unwrap().minutes();
        let mut out = Vec::new();
        for (gap, len, rests) in raws {
            let start = cursor + gap;
            let end = start + len;
            let mut breaks = Vec::new();
            let mut inner = start + 1;
            for (offset, rest) in rests {
                let (rest_start, rest_end) = (inner + offset, inner + offset + rest);
                if rest_end >= end {
                    break;
                }
                breaks.push((at(rest_start), at(rest_end)));
                inner = rest_end;
            }
            out.push(ShiftInput {
                start: at(start),
                end: at(end),
                breaks,
            });
            cursor = end;
        }
        out
    })
}

fn inputs() -> impl Strategy<Value = Vec<ShiftInput>> {
    prop_oneof![shifts(3000, 2, 0..12), shifts(600, 600, 20..45)]
}

fn is_night_minute(minute: i64) -> bool {
    let of_day = minute.rem_euclid(MINUTES_PER_DAY);
    !(300..1320).contains(&of_day)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn explanation_matches_the_classification(policy in work_policy(), inputs in inputs()) {
        let policy = policy.validate().unwrap();
        let shifts = ShiftSet::new(&inputs).unwrap();
        let result = classify(&policy, &shifts, None).unwrap();
        if result.refused.is_some() {
            for shift in shifts.shifts() {
                prop_assert_eq!(
                    explain_day(&policy, &shifts, &result, shift.labor_day()),
                    Err(ExplainError::Refused)
                );
            }
            return Ok(());
        }
        for day in &result.days {
            let explanation = explain_day(&policy, &shifts, &result, day.date).unwrap();
            let sum = |category: Category, night_only: bool| -> i64 {
                explanation
                    .segments
                    .iter()
                    .filter(|s| s.category == category && (!night_only || s.night))
                    .map(Segment::minutes)
                    .sum()
            };
            prop_assert_eq!(sum(Category::Within, false) + sum(Category::ExtraWithin, false), day.within);
            prop_assert_eq!(sum(Category::ExtraWithin, false), day.extra_within);
            prop_assert_eq!(sum(Category::Overtime, false) + sum(Category::Over60, false), day.overtime);
            prop_assert_eq!(sum(Category::Over60, false), day.over60);
            prop_assert_eq!(sum(Category::Holiday, false), day.holiday);
            prop_assert_eq!(sum(Category::Within, true) + sum(Category::ExtraWithin, true), day.night_within);
            prop_assert_eq!(sum(Category::Overtime, true) + sum(Category::Over60, true), day.night_overtime);
            prop_assert_eq!(sum(Category::Over60, true), day.night_over60);
            prop_assert_eq!(sum(Category::Holiday, true), day.night_holiday);
            let total: i64 = explanation.segments.iter().map(Segment::minutes).sum();
            prop_assert_eq!(total, day.total);

            for pair in explanation.segments.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                prop_assert!(a.interval.end() <= b.interval.start());
                if a.interval.end() == b.interval.start() {
                    prop_assert!(a.category != b.category || a.night != b.night);
                }
            }
            for segment in &explanation.segments {
                let start = segment.interval.start().minutes();
                let end = segment.interval.end().minutes();
                prop_assert!((start..end).all(|m| is_night_minute(m) == segment.night));
                prop_assert_eq!(segment.premium, premium_floor(segment.category, segment.night, day.date));
            }
        }
    }
}
