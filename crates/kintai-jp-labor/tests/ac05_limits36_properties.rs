//! 36協定の判定の性質の試験（proptest）。
//!
//! - AC-05.6・05.8: T を O と H にどう分けても、直近 n か月の合計と、T で決まる判定は変わらない。
//!   協定年の替わりで ΣO と発動月数が戻る。
//! - AC-05.2〜05.8・05.12・05.14・05.15: 仕様の文をそのまま書いた素朴な実装（協定年の初めを後ろ向きに探し、
//!   直近 n か月を月の表から足す）と、無作為の入力で全項目が一致する。

use std::collections::BTreeMap;

use kintai_core::Date;
use kintai_jp_labor::limits36::{
    Agreement, AgreementKind, Finding, HistoryMonth, Limits36Result, MonthHours, judge,
};
use proptest::prelude::*;

/// 無作為の協定（範囲の中）。
#[derive(Clone, Debug)]
struct Terms {
    kind: AgreementKind,
    year_start_month: i64,
    monthly: i64,
    annual: i64,
    special_total: i64,
    special_annual: i64,
    special_months: i64,
}

impl Terms {
    fn agreement(&self) -> Agreement {
        Agreement {
            kind: self.kind,
            year_start_month: self.year_start_month,
            monthly_limit_minutes: Some(self.monthly),
            annual_limit_minutes: Some(self.annual),
            special_monthly_total_limit_minutes: Some(self.special_total),
            special_annual_limit_minutes: Some(self.special_annual),
            special_months_max: Some(self.special_months),
        }
    }
}

/// 境界の値を多めに出す分（0〜`max`）。
fn minutes(max: i64, edges: &'static [i64]) -> impl Strategy<Value = i64> {
    prop_oneof![
        2 => prop::sample::select(edges.to_vec()),
        3 => 0..=max,
    ]
}

fn terms() -> impl Strategy<Value = Terms> {
    (
        prop::sample::select(vec![
            AgreementKind::None,
            AgreementKind::General,
            AgreementKind::Special,
        ]),
        1_i64..=12,
        minutes(2700, &[0, 1800, 2699, 2700]),
        minutes(21_600, &[0, 3000, 18_000, 21_600]),
        minutes(5999, &[0, 4800, 5100, 5999]),
        minutes(43_200, &[0, 8000, 21_600, 40_800, 43_200]),
        0_i64..=6,
    )
        .prop_map(
            |(
                kind,
                year_start_month,
                monthly,
                annual,
                special_total,
                special_annual,
                special_months,
            )| {
                Terms {
                    kind,
                    year_start_month,
                    monthly,
                    annual,
                    special_total,
                    special_annual,
                    special_months,
                }
            },
        )
}

/// 月の通し番号から協定月の初日。
fn month_start(index: i64, day: u32) -> Date {
    Date::from_ymd(
        i32::try_from(index.div_euclid(12)).unwrap(),
        u32::try_from(index.rem_euclid(12) + 1).unwrap(),
        day,
    )
    .unwrap()
}

fn index_of(date: Date) -> i64 {
    i64::from(date.year()) * 12 + i64::from(date.month()) - 1
}

/// 協定月の並び（O, H）と、最初の月の前8か月のうち無作為の月の履歴。
fn inputs() -> impl Strategy<Value = (Vec<MonthHours>, Vec<HistoryMonth>)> {
    let o = minutes(
        8000,
        &[0, 1, 1800, 1801, 2699, 2700, 2701, 3000, 4800, 5999, 6000],
    );
    let h = minutes(4000, &[0, 1, 1200, 3299, 3300]);
    (
        2021_i64 * 12..2024 * 12,
        1_u32..=28,
        prop::collection::vec((o, h), 0..30),
        prop::collection::vec(prop::option::of(0_i64..=7000), 8),
    )
        .prop_map(|(first, day, list, history)| {
            let months = list
                .iter()
                .enumerate()
                .map(|(i, &(overtime, holiday))| MonthHours {
                    start: month_start(first + i64::try_from(i).unwrap(), day),
                    overtime,
                    holiday,
                })
                .collect();
            let history = history
                .iter()
                .enumerate()
                .filter_map(|(back, total)| {
                    total.map(|total| HistoryMonth {
                        start: month_start(first - 1 - i64::try_from(back).unwrap(), day),
                        total,
                    })
                })
                .collect();
            (months, history)
        })
}

/// 1か月の期待（T, 直近 n か月の合計, 発動月数, ΣO, ΣO が分かるか, 指摘）。
type Expected = (i64, [Option<i64>; 5], u32, i64, bool, Vec<Finding>);

/// 仕様の文をそのまま書いた素朴な実装。
fn naive(terms: &Terms, months: &[MonthHours], history: &[HistoryMonth]) -> Vec<Expected> {
    let mut totals: BTreeMap<i64, i64> = history
        .iter()
        .map(|h| (index_of(h.start), h.total))
        .collect();
    for m in months {
        totals.insert(index_of(m.start), m.overtime + m.holiday);
    }
    let Some(first) = months.first().map(|m| index_of(m.start)) else {
        return Vec::new();
    };
    months
        .iter()
        .map(|m| {
            let here = index_of(m.start);
            let o = m.overtime;
            let t = m.overtime + m.holiday;
            let sums: [Option<i64>; 5] = std::array::from_fn(|j| {
                let n = i64::try_from(j).unwrap() + 2;
                (0..n).map(|k| totals.get(&(here - k)).copied()).sum()
            });
            // 協定年の初めは、起算月の協定月のうち、この月以前で最も近いもの。
            let mut year_start = here;
            while year_start.rem_euclid(12) + 1 != terms.year_start_month {
                year_start -= 1;
            }
            let in_year: Vec<&MonthHours> = months
                .iter()
                .filter(|x| (year_start..=here).contains(&index_of(x.start)))
                .collect();
            let year_overtime: i64 = in_year.iter().map(|x| x.overtime).sum();
            let count =
                u32::try_from(in_year.iter().filter(|x| x.overtime > 2700).count()).unwrap();
            let mut findings = Vec::new();
            match terms.kind {
                AgreementKind::None => {
                    if o > 0 || m.holiday > 0 {
                        findings.push(Finding::NoAgreement);
                    }
                }
                AgreementKind::General => {
                    if o > 2700 {
                        findings.push(Finding::MonthlyOvertimeExceeded);
                    }
                    if o <= 2700 && o > terms.monthly {
                        findings.push(Finding::GeneralMonthlyLimitExceeded);
                    }
                    if year_overtime > 21_600 {
                        findings.push(Finding::AnnualOvertimeExceeded);
                    }
                    if year_overtime <= 21_600 && year_overtime > terms.annual {
                        findings.push(Finding::GeneralAnnualLimitExceeded);
                    }
                }
                AgreementKind::Special => {
                    if o <= 2700 && o > terms.monthly {
                        findings.push(Finding::GeneralMonthlyLimitExceeded);
                    }
                    if o > 2700 && i64::from(count) > terms.special_months {
                        findings.push(Finding::SpecialCountExceeded);
                    }
                    if o > 2700 && t > terms.special_total {
                        findings.push(Finding::SpecialMonthlyTotalExceeded);
                    }
                    if year_overtime > terms.special_annual {
                        findings.push(Finding::SpecialAnnualExceeded);
                    }
                }
            }
            if t >= 6000 {
                findings.push(Finding::MonthlyTotalReached100h);
            }
            for (n, sum) in (2_u32..).zip(sums) {
                if sum.is_some_and(|s| s > 4800 * i64::from(n)) {
                    findings.push(Finding::MultiMonthSumExceeded { n });
                }
            }
            for (n, sum) in (2_u32..).zip(sums) {
                if sum.is_none() {
                    findings.push(Finding::UnknownHistory { n });
                }
            }
            (t, sums, count, year_overtime, year_start >= first, findings)
        })
        .collect()
}

fn actual(result: &Limits36Result) -> Vec<Expected> {
    result
        .months
        .iter()
        .map(|m| {
            (
                m.total,
                m.sums,
                m.special_count,
                m.year_overtime,
                m.year_overtime_known,
                m.findings.clone(),
            )
        })
        .collect()
}

/// T で決まる部分（合計と、100時間・平均・不明の判定）。
fn total_parts(result: &Limits36Result) -> Vec<(i64, [Option<i64>; 5], Vec<Finding>)> {
    result
        .months
        .iter()
        .map(|m| {
            let by_total = m
                .findings
                .iter()
                .copied()
                .filter(|f| {
                    matches!(
                        f,
                        Finding::MonthlyTotalReached100h
                            | Finding::MultiMonthSumExceeded { .. }
                            | Finding::UnknownHistory { .. }
                    )
                })
                .collect();
            (m.total, m.sums, by_total)
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// AC-05.2〜05.8 AC-05.12 AC-05.14 AC-05.15 素朴な実装と全項目が一致する。
    #[test]
    fn ac05_matches_a_naive_reading_of_the_spec(terms in terms(), (months, history) in inputs()) {
        let agreement = terms.agreement().validate().unwrap();
        let result = judge(&agreement, &months, &history).unwrap();
        prop_assert_eq!(actual(&result), naive(&terms, &months, &history));
        for month in &result.months {
            let mut sorted = month.findings.clone();
            sorted.sort();
            sorted.dedup();
            prop_assert_eq!(&sorted, &month.findings, "コードの順で重複なし");
        }
    }

    /// AC-05.6 T を O と H に別の比で分けても、直近 n か月の合計と、T で決まる判定は変わらない。
    #[test]
    fn ac05_6_splitting_t_keeps_the_sums(
        terms in terms(),
        (months, history) in inputs(),
        ratios in prop::collection::vec(0_i64..=100, 30),
    ) {
        let agreement = terms.agreement().validate().unwrap();
        let resplit: Vec<MonthHours> = months
            .iter()
            .zip(&ratios)
            .map(|(m, ratio)| {
                let total = m.overtime + m.holiday;
                let overtime = total * ratio / 100;
                MonthHours { start: m.start, overtime, holiday: total - overtime }
            })
            .collect();
        let before = judge(&agreement, &months, &history).unwrap();
        let after = judge(&agreement, &resplit, &history).unwrap();
        prop_assert_eq!(total_parts(&before), total_parts(&after));
    }

    /// AC-05.8 協定年の最初の月で、ΣO はその月の O、発動月数は0か1に戻り、ΣO が分かる。
    /// 年の途中の月は、前の月に O を足したもの。
    #[test]
    fn ac05_8_year_start_resets_the_running_totals(terms in terms(), (months, history) in inputs()) {
        let agreement = terms.agreement().validate().unwrap();
        let result = judge(&agreement, &months, &history).unwrap();
        let mut previous: Option<(i64, u32)> = None;
        for (m, judged) in months.iter().zip(&result.months) {
            let activated = u32::from(m.overtime > 2700);
            if i64::from(m.start.month()) == terms.year_start_month {
                prop_assert_eq!(judged.year_overtime, m.overtime);
                prop_assert_eq!(judged.special_count, activated);
                prop_assert!(judged.year_overtime_known);
            } else if let Some((sum, count)) = previous {
                prop_assert_eq!(judged.year_overtime, sum + m.overtime);
                prop_assert_eq!(judged.special_count, count + activated);
            }
            previous = Some((judged.year_overtime, judged.special_count));
        }
    }
}
