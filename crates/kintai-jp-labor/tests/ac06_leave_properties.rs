//! 年休の計算の性質の試験（proptest）。
//!
//! - AC-06.4・06.5: 残数は常に非負で、付与−使用−失効に等しい。`balance_halves` は残数の合計。
//! - AC-06.6・06.7: 取得を足しても、どの5日の期間の取得の合計も減らない（単調増加）。
//! - AC-06.3〜06.10: 仕様の文をそのまま書いた素朴な実装（半日ずつ消化、期間は取得の一覧から絞る、
//!   付与表は試験の中の数字）と、無作為の入力で、結果と誤りが全件一致する。

use kintai_core::Date;
use kintai_jp_labor::leave::{
    Attendance, FiveDayStatus, GrantInput, LeaveError, LeaveInput, LeaveNotice, LeaveResult,
    LeaveUnit, LeaveUnsupportedCode, TakenInput, WorkingTerms, compute, expiry_date,
    five_day_window_end, grant_date, next_period_first_day,
};
use proptest::prelude::*;

fn day(base: i64, offset: i64) -> Date {
    Date::from_day_number(base + offset).unwrap()
}

/// 2018-01-01 の日数。
const BASE: i64 = 17_532;

fn attendance() -> impl Strategy<Value = Attendance> {
    prop::sample::select(vec![
        Attendance::Meets,
        Attendance::Meets,
        Attendance::Below,
        Attendance::Unknown,
    ])
}

fn unit() -> impl Strategy<Value = LeaveUnit> {
    prop_oneof![
        24 => Just(LeaveUnit::Full),
        16 => Just(LeaveUnit::Half),
        1 => Just(LeaveUnit::Hours),
    ]
}

fn terms() -> impl Strategy<Value = WorkingTerms> {
    (
        0_i64..=7,
        prop::sample::select(vec![0_i64, 600, 1500, 1799, 1800, 2400]),
        prop::option::of(prop::sample::select(vec![
            0_i64, 47, 48, 72, 73, 120, 121, 168, 169, 216, 217, 300,
        ])),
    )
        .prop_map(|(days, minutes, annual)| WorkingTerms {
            weekly_scheduled_days: days,
            weekly_scheduled_minutes: minutes,
            annual_scheduled_days: annual,
        })
}

fn input() -> impl Strategy<Value = LeaveInput> {
    (
        0_i64..1800,
        0_i64..2600,
        terms(),
        prop::collection::vec(attendance(), 0..10),
        prop::option::weighted(
            0.3,
            prop::collection::vec(
                (
                    -200_i64..2600,
                    prop_oneof![1 => 1_i64..20, 3 => 20_i64..=40],
                ),
                0..6,
            ),
        ),
        prop::collection::vec(
            (prop_oneof![1 => -50_i64..2700, 4 => 190_i64..2700], unit()),
            0..20,
        ),
    )
        .prop_map(
            |(hire, span, terms, attendance, grants, taken)| LeaveInput {
                as_of: day(BASE, hire + span),
                hire_date: day(BASE, hire),
                terms,
                attendance,
                grants: grants.map(|list| {
                    list.into_iter()
                        .map(|(offset, halves)| GrantInput {
                            date: day(BASE, hire + offset),
                            halves,
                        })
                        .collect()
                }),
                taken: taken
                    .into_iter()
                    .map(|(offset, unit)| TakenInput {
                        date: day(BASE, hire + offset),
                        unit,
                    })
                    .collect(),
            },
        )
}

/// 付与日数（日）の行。表の外は `None`。数字は条文の表から写した（試験の中だけ）。
fn naive_table(terms: &WorkingTerms) -> Option<[i64; 7]> {
    const NORMAL: [i64; 7] = [10, 11, 12, 14, 16, 18, 20];
    const ROWS: [[i64; 7]; 4] = [
        [7, 8, 9, 10, 12, 13, 15],
        [5, 6, 6, 8, 9, 10, 11],
        [3, 4, 4, 5, 6, 6, 7],
        [1, 2, 2, 2, 3, 3, 3],
    ];
    if terms.weekly_scheduled_days >= 5 || terms.weekly_scheduled_minutes >= 1800 {
        return Some(NORMAL);
    }
    if let Some(annual) = terms.annual_scheduled_days {
        return match annual {
            217.. => Some(NORMAL),
            169..=216 => Some(ROWS[0]),
            121..=168 => Some(ROWS[1]),
            73..=120 => Some(ROWS[2]),
            48..=72 => Some(ROWS[3]),
            _ => None,
        };
    }
    match terms.weekly_scheduled_days {
        4 => Some(ROWS[0]),
        3 => Some(ROWS[1]),
        2 => Some(ROWS[2]),
        1 => Some(ROWS[3]),
        _ => None,
    }
}

/// 素朴な実装の結果（ロット、5日、管理簿の日付と単位、注意）。
#[derive(Debug, PartialEq, Eq)]
struct Summary {
    lots: Vec<LotRow>,
    balance: i64,
    five_day: Vec<FiveDayRow>,
    register: Vec<RegisterRow>,
    notices: Vec<LeaveNotice>,
}

/// ロット（付与日、付与、失効日、使用、残数、失効）。
type LotRow = (Date, i64, Date, i64, i64, i64);
/// 5日（基準日、期間の終わり、取得、状態、あと何半日）。
type FiveDayRow = (Date, Date, i64, FiveDayStatus, i64);
/// 管理簿（基準日、付与、取得の日付と単位、取得の計）。
type RegisterRow = (Date, i64, Vec<(Date, LeaveUnit)>, i64);

fn summary(result: &LeaveResult) -> Summary {
    Summary {
        lots: result
            .lots
            .iter()
            .map(|l| {
                (
                    l.grant_date,
                    l.granted_halves,
                    l.expires,
                    l.used_halves,
                    l.remaining_halves,
                    l.expired_halves,
                )
            })
            .collect(),
        balance: result.balance_halves,
        five_day: result
            .five_day
            .iter()
            .map(|f| {
                (
                    f.base_date,
                    f.window_end,
                    f.taken_halves,
                    f.status,
                    f.needed_halves,
                )
            })
            .collect(),
        register: result
            .register
            .iter()
            .map(|r| {
                (
                    r.base_date,
                    r.granted_halves,
                    r.taken.iter().map(|t| (t.date, t.unit)).collect(),
                    r.taken_halves,
                )
            })
            .collect(),
        notices: result.notices.clone(),
    }
}

fn halves(unit: LeaveUnit) -> i64 {
    match unit {
        LeaveUnit::Full => 2,
        _ => 1,
    }
}

/// 素朴な実装の対応外の判定（時間単位 → 分割付与 → 表の外）。`as_of` より後の取得と付与は入れない。
fn naive_unsupported(input: &LeaveInput) -> Option<LeaveUnsupportedCode> {
    let as_of = input.as_of;
    if input
        .taken
        .iter()
        .any(|t| t.unit == LeaveUnit::Hours && t.date <= as_of)
    {
        return Some(LeaveUnsupportedCode::LeaveHourly);
    }
    if let Some(grants) = &input.grants {
        let mut sorted: Vec<&GrantInput> = grants.iter().filter(|g| g.date <= as_of).collect();
        sorted.sort_by_key(|g| g.date);
        for pair in sorted.windows(2) {
            if pair[0].halves < 20
                && pair[1].date < next_period_first_day(pair[0].date, 12).unwrap()
            {
                return Some(LeaveUnsupportedCode::LeaveSplitGrant);
            }
        }
    }
    naive_table(&input.terms)
        .is_none()
        .then_some(LeaveUnsupportedCode::LeaveOutOfTable)
}

/// 素朴な実装のロット（付与日、半日の数）と、出勤率が未確認の付与があったか。
fn naive_lots(input: &LeaveInput, table: [i64; 7]) -> (Vec<(Date, i64)>, bool) {
    let as_of = input.as_of;
    if let Some(grants) = &input.grants {
        let mut sorted: Vec<&GrantInput> = grants.iter().filter(|g| g.date <= as_of).collect();
        sorted.sort_by_key(|g| g.date);
        return (sorted.iter().map(|g| (g.date, g.halves)).collect(), false);
    }
    let mut lots = Vec::new();
    let mut unconfirmed = false;
    for k in 0..200_u32 {
        let date = grant_date(input.hire_date, k).unwrap();
        if date > as_of {
            break;
        }
        let index = usize::try_from(k).unwrap();
        match input.attendance.get(index) {
            Some(Attendance::Below) => continue,
            Some(Attendance::Meets) => {}
            _ => unconfirmed = true,
        }
        lots.push((date, table[index.min(6)] * 2));
    }
    (lots, unconfirmed)
}

/// 素朴な実装の消化: 日付順（同じ日は入力の順）に、半日ずつ、使えるロットの古いものから。
fn naive_used(
    lots: &[(Date, i64)],
    counted: &[(usize, &TakenInput)],
) -> Result<Vec<i64>, LeaveError> {
    let mut used = vec![0_i64; lots.len()];
    for &(index, record) in counted {
        for _ in 0..halves(record.unit) {
            let slot = lots
                .iter()
                .zip(used.iter_mut())
                .find(|((date, granted), used)| {
                    *date <= record.date
                        && record.date < expiry_date(*date).unwrap()
                        && **used < *granted
                });
            match slot {
                Some((_, used)) => *used += 1,
                None => return Err(LeaveError::InsufficientBalance { index }),
            }
        }
    }
    Ok(used)
}

/// 仕様の文をそのまま書いた素朴な実装。
fn naive(input: &LeaveInput) -> Result<Summary, LeaveError> {
    let as_of = input.as_of;
    if let Some(code) = naive_unsupported(input) {
        return Err(LeaveError::Unsupported(code));
    }
    let table = naive_table(&input.terms).unwrap();
    let (lots, unconfirmed) = naive_lots(input, table);
    let mut counted: Vec<(usize, &TakenInput)> = input
        .taken
        .iter()
        .enumerate()
        .filter(|(_, t)| t.date <= as_of)
        .collect();
    counted.sort_by_key(|(_, t)| t.date);
    let used = naive_used(&lots, &counted)?;
    let mut summary = Summary {
        lots: Vec::new(),
        balance: 0,
        five_day: Vec::new(),
        register: Vec::new(),
        notices: if unconfirmed {
            vec![LeaveNotice::AttendanceUnconfirmed]
        } else {
            vec![]
        },
    };
    for (&(date, granted), &used) in lots.iter().zip(&used) {
        let expires = expiry_date(date).unwrap();
        let (remaining, lapsed) = if as_of >= expires {
            (0, granted - used)
        } else {
            (granted - used, 0)
        };
        summary
            .lots
            .push((date, granted, expires, used, remaining, lapsed));
        summary.balance += remaining;
        let end = five_day_window_end(date).unwrap();
        let in_window: Vec<(Date, LeaveUnit)> = counted
            .iter()
            .filter(|(_, t)| date <= t.date && t.date <= end)
            .map(|(_, t)| (t.date, t.unit))
            .collect();
        let taken: i64 = in_window.iter().map(|(_, unit)| halves(*unit)).sum();
        let status = if granted < 20 {
            FiveDayStatus::NotApplicable
        } else if taken >= 10 {
            FiveDayStatus::Met
        } else if as_of > end {
            FiveDayStatus::NotMet
        } else {
            FiveDayStatus::InProgress
        };
        let needed = match status {
            FiveDayStatus::Met | FiveDayStatus::NotApplicable => 0,
            _ => 10 - taken,
        };
        summary.five_day.push((date, end, taken, status, needed));
        summary.register.push((date, granted, in_window, taken));
    }
    Ok(summary)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// AC-06.3〜06.10 素朴な実装と、結果も誤りも一致する。
    #[test]
    fn ac06_matches_a_naive_reading_of_the_spec(input in input()) {
        let ours = compute(&input).map(|result| summary(&result));
        prop_assert_eq!(ours, naive(&input));
    }

    /// AC-06.4 AC-06.5 残数は非負で、付与−使用−失効に等しい。失効と残数は同時に正にならない。
    #[test]
    fn ac06_remaining_is_granted_minus_used_minus_expired(input in input()) {
        if let Ok(result) = compute(&input) {
            let mut balance = 0;
            for lot in &result.lots {
                prop_assert!(lot.used_halves >= 0 && lot.remaining_halves >= 0 && lot.expired_halves >= 0);
                prop_assert_eq!(
                    lot.remaining_halves,
                    lot.granted_halves - lot.used_halves - lot.expired_halves
                );
                prop_assert!(lot.remaining_halves == 0 || lot.expired_halves == 0);
                balance += lot.remaining_halves;
            }
            prop_assert_eq!(balance, result.balance_halves);
        }
    }

    /// AC-06.6 AC-06.7 取得を1件足しても、どの期間の取得の合計も減らない（計算できる入力どうしで）。
    #[test]
    fn ac06_adding_a_record_never_lowers_window_sums(
        input in input(),
        offset in -50_i64..2700,
        half in any::<bool>(),
    ) {
        let Ok(before) = compute(&input) else { return Ok(()); };
        let mut more = input.clone();
        more.taken.push(TakenInput {
            date: day(BASE, more.hire_date.day_number() - BASE + offset),
            unit: if half { LeaveUnit::Half } else { LeaveUnit::Full },
        });
        let Ok(after) = compute(&more) else { return Ok(()); };
        prop_assert_eq!(before.five_day.len(), after.five_day.len());
        for (b, a) in before.five_day.iter().zip(&after.five_day) {
            prop_assert!(a.taken_halves >= b.taken_halves);
            prop_assert!(a.needed_halves <= b.needed_halves);
        }
        for (b, a) in before.register.iter().zip(&after.register) {
            prop_assert!(a.taken_halves >= b.taken_halves);
        }
        prop_assert!(after.balance_halves <= before.balance_halves);
    }
}
