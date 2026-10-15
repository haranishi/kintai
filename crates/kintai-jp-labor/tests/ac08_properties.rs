//! 月次の集計の性質の試験（proptest）。
//!
//! - AC-08.1: 台帳の項目は、区分の結果の日ごとの値を、労働日の月の期間ごとに足したもの。
//!   労働時間＝法定内＋時間外＋休日、深夜は足さない、60時間超は時間外の内数。
//! - AC-08.3: 丸めた時間は、元の分と1時間未満の差（30分未満切り捨て・30分以上切り上げ）で、元の分は変えない。

use kintai_core::{Instant, Weekday};
use kintai_jp_labor::report::{Rounding, ledger, ledger_for, round_to_hours};
use kintai_jp_labor::{AmbiguityMode, ShiftInput, ShiftSet, WorkPolicy, WorkSystem, classify};
use proptest::prelude::*;

fn work_policy() -> impl Strategy<Value = WorkPolicy> {
    (
        prop::sample::select(Weekday::ALL.to_vec()),
        prop::sample::select(Weekday::ALL.to_vec()),
        prop::sample::select(vec![1_i64, 16, 28]),
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

/// 重ならない勤務を並べる（前の勤務の終業からの空き, 長さ, 休憩の（空き, 長さ）の並び）。
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
        let mut cursor = "2026-09-20T00:00".parse::<Instant>().unwrap().minutes();
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

/// まばらな勤務（境界をいろいろ通る）と、詰まった勤務（60時間超に届く）。
fn inputs() -> impl Strategy<Value = Vec<ShiftInput>> {
    prop_oneof![shifts(3000, 2, 0..12), shifts(600, 600, 20..40)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// AC-08.3: 丸めた時間と元の分の差は、30分未満切り捨て・30分以上切り上げの範囲（1時間未満）。
    #[test]
    fn ac08_3_rounding_is_within_half_an_hour(minutes in 0_i64..50_000_000) {
        let hours = round_to_hours(minutes);
        let difference = hours * 60 - minutes;
        prop_assert!(-30 < difference && difference <= 30, "{minutes} → {hours}");
        prop_assert!(hours * 60 <= minutes || hours * 60 - minutes < 60);
        prop_assert!(round_to_hours(minutes) <= round_to_hours(minutes + 1));
    }

    /// AC-08.1・08.3: 台帳の項目は区分の結果と一致し、丸めは元の分を変えない。
    #[test]
    fn ac08_1_ledger_is_the_sum_of_days(policy in work_policy(), inputs in inputs()) {
        let policy = policy.validate().unwrap();
        let result = classify(&policy, &ShiftSet::new(&inputs).unwrap(), None).unwrap();
        if result.refused.is_some() {
            prop_assert!(ledger(&policy, &result, Rounding::None).is_err());
            return Ok(());
        }
        let rows = ledger(&policy, &result, Rounding::None).unwrap();
        let rounded = ledger(&policy, &result, Rounding::Monthly30Min).unwrap();
        prop_assert_eq!(rows.len(), result.periods.len());
        for ((row, period), with_hours) in rows.iter().zip(&result.periods).zip(&rounded) {
            prop_assert_eq!(row.period, period.start);
            prop_assert_eq!(
                (row.work_minutes, row.within_minutes, row.overtime_minutes, row.over60_minutes, row.holiday_minutes, row.night_minutes),
                (period.total, period.within, period.overtime, period.over60, period.holiday, period.night)
            );
            let days: Vec<_> = result.days.iter().filter(|d| period.start.contains(d.date)).collect();
            prop_assert_eq!(row.work_days, i64::try_from(days.len()).unwrap());
            prop_assert_eq!(row.extra_within_minutes, days.iter().map(|d| d.extra_within).sum::<i64>());
            prop_assert_eq!(row.work_minutes, row.within_minutes + row.overtime_minutes + row.holiday_minutes);
            prop_assert!(row.over60_minutes <= row.overtime_minutes);
            prop_assert!(row.extra_within_minutes <= row.within_minutes);
            prop_assert!(row.night_minutes <= row.work_minutes);
            prop_assert_eq!(row.rounded, None);
            // 丸めても分は同じで、丸めた時間は各合計から求めたもの。
            let mut plain = *with_hours;
            plain.rounded = None;
            prop_assert_eq!(&plain, row);
            let hours = with_hours.rounded.unwrap();
            prop_assert_eq!(
                (hours.overtime, hours.holiday, hours.night),
                (round_to_hours(row.overtime_minutes), round_to_hours(row.holiday_minutes), round_to_hours(row.night_minutes))
            );
            // 1つの期間だけを求めても同じ。
            prop_assert_eq!(&ledger_for(&policy, &result, row.period, Rounding::None).unwrap(), row);
        }
        let total: i64 = rows.iter().map(|r| r.work_minutes).sum();
        prop_assert_eq!(total, result.days.iter().map(|d| d.total).sum::<i64>());
    }
}
