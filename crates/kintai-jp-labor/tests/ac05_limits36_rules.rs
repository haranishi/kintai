//! 36協定の上限の判定の規則と境界（`docs/spec/limits36.md`）。
//!
//! AC-05.1〜05.8、05.10、05.12〜05.15 を、手で作った協定月の並びで確かめる。

use kintai_core::{Date, MonthPeriod, MonthStartDay, Weekday};
use kintai_jp_labor::limits36::{
    Agreement, AgreementKind, Finding, HistoryMonth, LSA_36_1, LSA_36_4, LSA_36_6_ITEM2,
    LSA_36_6_ITEM3, Limits36Error, Limits36Notice, Limits36Result, MonthHours, MonthJudgement,
    ValidAgreement, check_history_minutes, check_month_minutes, judge, months_from_periods,
};
use kintai_jp_labor::provenance::{CONFIRMED_ON, LABOR_STANDARDS_ACT, LSA_36_5};
use kintai_jp_labor::{ShiftInput, ShiftSet, WorkPolicy, WorkSystem, classify};

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

fn agreement(kind: AgreementKind, limits: [Option<i64>; 5]) -> Agreement {
    let [
        monthly,
        annual,
        special_total,
        special_annual,
        special_months,
    ] = limits;
    Agreement {
        kind,
        year_start_month: 4,
        monthly_limit_minutes: monthly,
        annual_limit_minutes: annual,
        special_monthly_total_limit_minutes: special_total,
        special_annual_limit_minutes: special_annual,
        special_months_max: special_months,
    }
}

fn none() -> ValidAgreement {
    agreement(AgreementKind::None, [None; 5])
        .validate()
        .unwrap()
}

fn general(monthly: i64, annual: i64) -> ValidAgreement {
    agreement(
        AgreementKind::General,
        [Some(monthly), Some(annual), None, None, None],
    )
    .validate()
    .unwrap()
}

fn special(monthly: i64, total: i64, annual: i64, months: i64) -> ValidAgreement {
    agreement(
        AgreementKind::Special,
        [
            Some(monthly),
            Some(21_600),
            Some(total),
            Some(annual),
            Some(months),
        ],
    )
    .validate()
    .unwrap()
}

/// 特別条項の上限をすべて法定の最大にした協定（月の限度は2700分）。
fn special_max() -> ValidAgreement {
    special(2700, 5999, 43_200, 6)
}

/// `start` から隙間なく並べた協定月（O, H）。
fn months(start: &str, list: &[(i64, i64)]) -> Vec<MonthHours> {
    let first = date(start);
    list.iter()
        .enumerate()
        .map(|(i, &(overtime, holiday))| {
            let index = i64::from(first.year()) * 12 + i64::from(first.month()) - 1
                + i64::try_from(i).unwrap();
            let year = i32::try_from(index.div_euclid(12)).unwrap();
            let month = u32::try_from(index.rem_euclid(12) + 1).unwrap();
            MonthHours {
                start: Date::from_ymd(year, month, first.day()).unwrap(),
                overtime,
                holiday,
            }
        })
        .collect()
}

/// `first` の直前 `count` か月の履歴（T＝0）。
fn zero_history(first: &str, count: i64) -> Vec<HistoryMonth> {
    let first = date(first);
    (1..=count)
        .map(|back| {
            let index = i64::from(first.year()) * 12 + i64::from(first.month()) - 1 - back;
            HistoryMonth {
                start: Date::from_ymd(
                    i32::try_from(index.div_euclid(12)).unwrap(),
                    u32::try_from(index.rem_euclid(12) + 1).unwrap(),
                    first.day(),
                )
                .unwrap(),
                total: 0,
            }
        })
        .collect()
}

/// 履歴を5か月（T＝0）つけて判定する。
fn run(agreement: &ValidAgreement, start: &str, list: &[(i64, i64)]) -> Limits36Result {
    judge(agreement, &months(start, list), &zero_history(start, 5)).unwrap()
}

fn findings(result: &Limits36Result) -> Vec<Vec<Finding>> {
    result.months.iter().map(|m| m.findings.clone()).collect()
}

fn last(result: &Limits36Result) -> &MonthJudgement {
    result.months.last().unwrap()
}

fn shift(start: &str, end: &str) -> ShiftInput {
    ShiftInput {
        start: start.parse().unwrap(),
        end: end.parse().unwrap(),
        breaks: Vec::new(),
    }
}

fn policy() -> WorkPolicy {
    WorkPolicy {
        work_system: WorkSystem::Standard,
        scheduled_minutes_per_day: 480,
        week_start: None,
        weekly_limit_minutes: None,
        special_44h_confirmed: None,
        legal_holiday_weekday: Some(Weekday::Sunday),
        month_start_day: 1,
        ambiguity: None,
        worker_kind: None,
        holiday_system: None,
        holiday_swaps: 0,
        night_window: None,
    }
}

/// AC-05.1 O と H を区分の結果から取り出し、T＝O＋H とする。法定休日の労働は8時間を超えても H だけで、O に重ねない。
/// 法定外休日（土曜）の労働は、日・週の法定時間を超えた分だけが O になる。
#[test]
fn ac05_1_overtime_and_holiday_from_classification() {
    let policy = policy().validate().unwrap();
    let shifts = ShiftSet::new(&[
        // 土曜（法定外休日）600分: 法定内480・時間外120。週（9/27〜10/3）はこの日だけ。
        shift("2026-10-03T09:00", "2026-10-03T19:00"),
        // 日曜（法定休日）720分: すべて法定休日。
        shift("2026-10-04T09:00", "2026-10-04T21:00"),
        // 月曜 660分: 法定内480・時間外180。
        shift("2026-10-05T09:00", "2026-10-05T20:00"),
    ])
    .unwrap();
    let result = classify(&policy, &shifts, None).unwrap();
    assert_eq!(result.periods.len(), 1);
    let period = &result.periods[0];
    let hours = MonthHours::from_period(period).unwrap();
    assert_eq!(hours.start, date("2026-10-01"));
    assert_eq!((hours.overtime, hours.holiday), (300, 720));
    assert_eq!(hours.total().unwrap(), 1020);
    // 二重に数えない: 実労働＝法定内＋O＋H。
    assert_eq!(period.total, period.within + hours.overtime + hours.holiday);
    assert_eq!(period.total, 600 + 720 + 660);
}

/// AC-05.1 区分の結果の期間から、範囲の協定月を隙間なく並べる。実労働のない月は O も H も0。
#[test]
fn ac05_1_months_from_periods_fill_gaps_with_zero() {
    let policy = policy().validate().unwrap();
    let shifts = ShiftSet::new(&[
        shift("2026-10-05T09:00", "2026-10-05T20:00"),
        shift("2026-12-07T09:00", "2026-12-07T19:00"),
    ])
    .unwrap();
    let result = classify(&policy, &shifts, None).unwrap();
    let day = MonthStartDay::new(1).unwrap();
    let first = MonthPeriod::containing(date("2026-09-15"), day);
    let last = MonthPeriod::containing(date("2027-01-15"), day);
    let filled = months_from_periods(&result.periods, first, last).unwrap();
    let got: Vec<(String, i64, i64)> = filled
        .iter()
        .map(|m| (m.start.to_string(), m.overtime, m.holiday))
        .collect();
    assert_eq!(
        got,
        [
            ("2026-09-01".to_owned(), 0, 0),
            ("2026-10-01".to_owned(), 180, 0),
            ("2026-11-01".to_owned(), 0, 0),
            ("2026-12-01".to_owned(), 120, 0),
            ("2027-01-01".to_owned(), 0, 0),
        ]
    );
    // 範囲の外の期間は使わない。
    let only_nov = months_from_periods(
        &result.periods,
        MonthPeriod::containing(date("2026-11-02"), day),
        MonthPeriod::containing(date("2026-11-02"), day),
    )
    .unwrap();
    assert_eq!(only_nov.len(), 1);
    assert_eq!((only_nov[0].overtime, only_nov[0].holiday), (0, 0));
    // 起算日がそろっていなければ誤り。
    let other = MonthPeriod::containing(date("2026-11-20"), MonthStartDay::new(16).unwrap());
    assert_eq!(
        months_from_periods(&result.periods, first, other),
        Err(Limits36Error::MismatchedStartDay)
    );
}

/// AC-05.2 協定なしでは、O または H がある月は違反。どちらも0の月は違反でない。
#[test]
fn ac05_2_no_agreement() {
    let result = run(&none(), "2021-04-01", &[(0, 0), (1, 0), (0, 1), (0, 0)]);
    assert_eq!(
        findings(&result),
        [
            vec![],
            vec![Finding::NoAgreement],
            vec![Finding::NoAgreement],
            vec![]
        ]
    );
    // 協定なしでも、月100時間と平均80時間は常に見る。
    let heavy = run(&none(), "2021-04-01", &[(0, 6000)]);
    assert_eq!(
        last(&heavy).findings,
        [Finding::NoAgreement, Finding::MonthlyTotalReached100h]
    );
}

/// AC-05.3 一般条項の月の限度: 法定の45時間を超えた月と、協定の限度だけを超えた月を別のコードにする。
#[test]
fn ac05_3_general_monthly_limits() {
    let agreement = general(1800, 21_600);
    let result = run(
        &agreement,
        "2021-04-01",
        &[(1800, 0), (1801, 0), (2700, 0), (2701, 0)],
    );
    assert_eq!(
        findings(&result),
        [
            vec![],
            vec![Finding::GeneralMonthlyLimitExceeded],
            vec![Finding::GeneralMonthlyLimitExceeded],
            vec![Finding::MonthlyOvertimeExceeded],
        ]
    );
    // 協定の限度が法定と同じなら、45時間ちょうどまでは適合。
    let legal = run(
        &general(2700, 21_600),
        "2021-04-01",
        &[(2700, 0), (2701, 0)],
    );
    assert_eq!(
        findings(&legal),
        [vec![], vec![Finding::MonthlyOvertimeExceeded]]
    );
    // H は月の限度に数えない（O だけを見る）。
    let holiday = run(&general(1800, 21_600), "2021-04-01", &[(1800, 2000)]);
    assert_eq!(last(&holiday).findings, []);
}

/// AC-05.3 一般条項の年の限度: 協定の年限度だけを超えた月と、法定の360時間を超えた月を別のコードにする。
#[test]
fn ac05_3_general_annual_limits() {
    // 協定の年限度18000分。1800分×10か月＝18000分（ちょうど）は適合、11か月目から協定の限度超過。
    let agreement = general(2700, 18_000);
    let mut list = vec![(1800, 0); 10];
    list.push((1, 0));
    list.push((2700, 0));
    let result = run(&agreement, "2021-04-01", &list);
    let got = findings(&result);
    assert!(got[..10].iter().all(Vec::is_empty), "{got:?}");
    assert_eq!(got[10], [Finding::GeneralAnnualLimitExceeded]);
    assert_eq!(result.months[11].year_overtime, 20_701);
    assert_eq!(got[11], [Finding::GeneralAnnualLimitExceeded]);
    // 法定の21600分ちょうどは協定の限度超過、21601分から法定の超過（別のコード）。
    let mut list = vec![(2700, 0); 8];
    list.push((0, 0));
    list.push((1, 0));
    let result = run(&general(2700, 21_600), "2021-04-01", &list);
    assert_eq!(result.months[8].year_overtime, 21_600);
    assert_eq!(result.months[8].findings, []);
    assert_eq!(result.months[9].findings, [Finding::AnnualOvertimeExceeded]);
    let result = run(&general(2700, 18_000), "2021-04-01", &list);
    assert_eq!(
        result.months[8].findings,
        [Finding::GeneralAnnualLimitExceeded]
    );
    assert_eq!(result.months[9].findings, [Finding::AnnualOvertimeExceeded]);
}

/// AC-05.4 特別条項: 発動月の数、年の ΣO、発動月の T の上限を検査する。
#[test]
fn ac05_4_special_clause_limits() {
    // 回数: 協定6回。7回目の発動月だけ SpecialCountExceeded。
    let agreement = special(2700, 5999, 43_200, 6);
    let mut list = vec![(2701, 0); 7];
    list.push((0, 0));
    let result = run(&agreement, "2021-04-01", &list);
    let counts: Vec<u32> = result.months.iter().map(|m| m.special_count).collect();
    assert_eq!(counts, [1, 2, 3, 4, 5, 6, 7, 7]);
    let got = findings(&result);
    assert!(got[..6].iter().all(Vec::is_empty), "{got:?}");
    assert_eq!(got[6], [Finding::SpecialCountExceeded]);
    assert_eq!(got[7], []);
    // 回数0の協定では、最初の発動月から超過。
    let zero = run(&special(2700, 5999, 43_200, 0), "2021-04-01", &[(2701, 0)]);
    assert_eq!(last(&zero).findings, [Finding::SpecialCountExceeded]);

    // 発動月の T の上限: 5100分ちょうどは適合、5101分は超過。発動月でない月は見ない
    // （月ごとに別に判定する。続けると直近2か月の合計の上限にも当たるため）。
    let agreement = special(2700, 5100, 43_200, 6);
    for (month, expected) in [
        ((3000, 2100), vec![]),
        ((3000, 2101), vec![Finding::SpecialMonthlyTotalExceeded]),
        ((2700, 2401), vec![]),
    ] {
        let result = run(&agreement, "2021-04-01", &[month]);
        assert_eq!(last(&result).findings, expected, "{month:?}");
    }

    // 年の上限: ΣO が協定の年限度（8000分）ちょうどは適合、超えた月から毎月。
    let agreement = special(2700, 5999, 8000, 6);
    let result = run(
        &agreement,
        "2021-04-01",
        &[(2700, 0), (2700, 0), (2600, 0), (1, 0), (0, 0)],
    );
    assert_eq!(result.months[2].year_overtime, 8000);
    assert_eq!(
        findings(&result),
        [
            vec![],
            vec![],
            vec![],
            vec![Finding::SpecialAnnualExceeded],
            vec![Finding::SpecialAnnualExceeded],
        ]
    );
    // 720時間は通常分を含む総量: 一般条項の年限度（21600分）を超えても、特別条項の年限度以内なら適合。
    let result = run(&special_max(), "2021-04-01", &[(2700, 0); 12]);
    assert_eq!(last(&result).year_overtime, 32_400);
    assert!(findings(&result).iter().all(Vec::is_empty));
}

/// AC-05.5 T が6000分ちょうどは違反、5999分は適合（協定の種類によらない）。
///
/// 月ごとに別に判定する（前の5か月の履歴は T＝0）。続けて並べると、直近2か月の合計の上限にも当たるため。
#[test]
fn ac05_5_hundred_hours_boundary() {
    for agreement in [special_max(), general(2700, 21_600)] {
        let below = run(&agreement, "2021-04-01", &[(2700, 3299)]);
        assert_eq!(last(&below).total, 5999);
        assert_eq!(last(&below).findings, []);
        let at = run(&agreement, "2021-04-01", &[(2700, 3300)]);
        assert_eq!(last(&at).total, 6000);
        assert_eq!(last(&at).findings, [Finding::MonthlyTotalReached100h]);
    }
    // 発動月で、特別条項の月の上限（5999分）と同時に。
    let below = run(&special_max(), "2021-04-01", &[(5999, 0)]);
    assert_eq!(last(&below).findings, []);
    let at = run(&special_max(), "2021-04-01", &[(6000, 0)]);
    assert_eq!(
        last(&at).findings,
        [
            Finding::SpecialMonthlyTotalExceeded,
            Finding::MonthlyTotalReached100h
        ]
    );
}

/// AC-05.6 直近 n か月の T の合計が 4800×n ちょうどは適合、1分超は違反（n＝2〜6のそれぞれ）。丸めた平均を使わない。
#[test]
fn ac05_6_multi_month_sum_boundary_for_each_n() {
    for n in 2..=6_u32 {
        let width = usize::try_from(n).unwrap();
        for extra in [0, 1] {
            // n か月前の月に 4800×n＋extra、その後の月は0。直近 n か月の合計だけが境界に乗る。
            let mut list = vec![(0, 0); width];
            list[0] = (0, 4800 * i64::from(n) + extra);
            let result = run(&special_max(), "2021-04-01", &list);
            let judged = last(&result);
            assert_eq!(judged.sum(n), Some(4800 * i64::from(n) + extra));
            let expected = if extra == 0 {
                vec![]
            } else {
                vec![Finding::MultiMonthSumExceeded { n }]
            };
            assert_eq!(judged.findings, expected, "n={n} extra={extra}");
        }
    }
    // 2か月平均80時間ちょうど（160時間）と、平均80時間を1分だけ超える3か月（240時間＋1分）。
    let result = run(
        &special_max(),
        "2021-04-01",
        &[(0, 4801), (4800, 0), (4800, 0)],
    );
    assert_eq!(last(&result).sum(2), Some(9600));
    assert_eq!(last(&result).sum(3), Some(14_401));
    assert_eq!(
        last(&result).findings,
        [Finding::MultiMonthSumExceeded { n: 3 }]
    );
}

/// AC-05.7 履歴が足りない n は合計が `null` で `UnknownHistory`。足りない月を0分とみなさない。
#[test]
fn ac05_7_unknown_history_is_not_zero() {
    let agreement = general(2700, 21_600);
    // 履歴なし: 最初の月は n＝2〜6 がすべて不明。
    let list = months("2021-04-01", &[(2000, 0), (2000, 0)]);
    let result = judge(&agreement, &list, &[]).unwrap();
    assert_eq!(result.months[0].sums, [None; 5]);
    assert_eq!(
        result.months[0].findings,
        (2..=6)
            .map(|n| Finding::UnknownHistory { n })
            .collect::<Vec<_>>()
    );
    assert_eq!(result.months[1].sum(2), Some(4000));
    assert_eq!(result.months[1].sum(3), None);
    // 歯抜けの履歴: 3か月前が無いと、n＝4 以上が不明（2・3か月前があっても、0とみなして足さない）。
    let mut history = zero_history("2021-04-01", 5);
    history.retain(|h| h.start != date("2021-01-01"));
    history[0].total = 4000;
    let result = judge(&agreement, &list[..1], &history).unwrap();
    let judged = &result.months[0];
    assert_eq!(judged.sum(2), Some(6000));
    assert_eq!(judged.sum(3), Some(6000));
    assert_eq!(judged.sum(4), None);
    assert_eq!(
        judged.findings,
        [
            Finding::UnknownHistory { n: 4 },
            Finding::UnknownHistory { n: 5 },
            Finding::UnknownHistory { n: 6 },
        ]
    );
    // 履歴の並びは問わない。6か月より前の履歴は使わない。
    let mut shuffled = zero_history("2021-04-01", 7);
    shuffled.reverse();
    shuffled[0].total = 44_640;
    shuffled[1].total = 44_640;
    let result = judge(&agreement, &list[..1], &shuffled).unwrap();
    assert_eq!(result.months[0].sum(6), Some(2000));
    assert_eq!(result.months[0].findings, []);
    // 範囲外の n は None。
    assert_eq!(result.months[0].sum(1), None);
    assert_eq!(result.months[0].sum(7), None);
}

/// AC-05.8 協定年の替わりで ΣO と発動月数は0に戻り、直近 n か月の合計は戻らない。
#[test]
fn ac05_8_year_reset() {
    let agreement = special(2700, 5999, 43_200, 6);
    let result = run(&agreement, "2021-01-01", &[(3000, 0); 6]);
    let year_overtime: Vec<i64> = result.months.iter().map(|m| m.year_overtime).collect();
    assert_eq!(year_overtime, [3000, 6000, 9000, 3000, 6000, 9000]);
    let counts: Vec<u32> = result.months.iter().map(|m| m.special_count).collect();
    assert_eq!(counts, [1, 2, 3, 1, 2, 3]);
    let known: Vec<bool> = result
        .months
        .iter()
        .map(|m| m.year_overtime_known)
        .collect();
    assert_eq!(known, [false, false, false, true, true, true]);
    // 4月（新しい協定年の最初の月）の直近2か月は、3月と4月の T。
    assert_eq!(result.months[3].sum(2), Some(6000));
    assert_eq!(result.months[3].sum(6), Some(12_000));
    // 協定年の起算月が1月なら、1月で戻る（起算月の設定どおり）。
    let january = Agreement {
        year_start_month: 1,
        ..agreement_general_plain()
    }
    .validate()
    .unwrap();
    let result = run(&january, "2021-11-01", &[(100, 0); 4]);
    let year_overtime: Vec<i64> = result.months.iter().map(|m| m.year_overtime).collect();
    assert_eq!(year_overtime, [100, 200, 100, 200]);
}

fn agreement_general_plain() -> Agreement {
    agreement(
        AgreementKind::General,
        [Some(2700), Some(21_600), None, None, None],
    )
}

/// AC-05.10 協定の限度が法定の最大を超える設定は不正な入力。境界（最大ちょうど・0）は受け付ける。
#[test]
fn ac05_10_agreement_limits_over_the_legal_maximum_are_invalid() {
    let fields = [
        ("monthly_limit_minutes", 2700),
        ("annual_limit_minutes", 21_600),
        ("special_monthly_total_limit_minutes", 5999),
        ("special_annual_limit_minutes", 43_200),
        ("special_months_max", 6),
    ];
    for (index, (field, max)) in fields.iter().enumerate() {
        for kind in [
            AgreementKind::None,
            AgreementKind::General,
            AgreementKind::Special,
        ] {
            let mut limits = [Some(2700), Some(21_600), Some(5999), Some(43_200), Some(6)];
            limits[index] = Some(max + 1);
            assert_eq!(
                agreement(kind, limits).validate(),
                Err(Limits36Error::AgreementOutOfRange {
                    field,
                    expected: match *field {
                        "monthly_limit_minutes" => "0〜2700",
                        "annual_limit_minutes" => "0〜21600",
                        "special_monthly_total_limit_minutes" => "0〜5999",
                        "special_annual_limit_minutes" => "0〜43200",
                        _ => "0〜6",
                    },
                }),
                "{field} {kind:?}"
            );
            limits[index] = Some(-1);
            assert!(agreement(kind, limits).validate().is_err(), "{field} 負");
            limits[index] = Some(*max);
            assert!(agreement(kind, limits).validate().is_ok(), "{field} 最大");
            limits[index] = Some(0);
            assert!(agreement(kind, limits).validate().is_ok(), "{field} 0");
        }
    }
    // 起算月は1〜12（種類によらず必須）。
    for month in [0, 13, -1] {
        let mut wrong = agreement(AgreementKind::None, [None; 5]);
        wrong.year_start_month = month;
        assert_eq!(
            wrong.validate(),
            Err(Limits36Error::AgreementOutOfRange {
                field: "year_start_month",
                expected: "1〜12",
            })
        );
    }
    // 種類ごとの必須。
    assert_eq!(
        agreement(AgreementKind::General, [Some(2700), None, None, None, None]).validate(),
        Err(Limits36Error::MissingLimit {
            field: "annual_limit_minutes"
        })
    );
    assert_eq!(
        agreement(
            AgreementKind::Special,
            [Some(2700), Some(21_600), Some(5999), Some(43_200), None]
        )
        .validate(),
        Err(Limits36Error::MissingLimit {
            field: "special_months_max"
        })
    );
    assert!(agreement(AgreementKind::None, [None; 5]).validate().is_ok());
    // 使わない項目は範囲だけ見て、使わない。
    let general_with_unused = agreement(
        AgreementKind::General,
        [Some(1800), Some(18_000), Some(5000), Some(1), Some(0)],
    )
    .validate()
    .unwrap();
    assert_eq!(general_with_unused.kind(), AgreementKind::General);
    assert_eq!(general_with_unused.special_months_max(), None);
    let result = run(&general_with_unused, "2021-04-01", &[(1800, 3000)]);
    assert_eq!(last(&result).findings, []);
}

/// AC-05.10 検査済みの協定から、種類と限度を読める。
#[test]
fn ac05_10_valid_agreement_accessors() {
    let valid = special(1800, 5100, 40_800, 6);
    assert_eq!(valid.kind(), AgreementKind::Special);
    assert_eq!(valid.year_start_month(), 4);
    assert_eq!(valid.monthly_limit_minutes(), Some(1800));
    assert_eq!(valid.annual_limit_minutes(), Some(21_600));
    assert_eq!(valid.special_monthly_total_limit_minutes(), Some(5100));
    assert_eq!(valid.special_annual_limit_minutes(), Some(40_800));
    assert_eq!(valid.special_months_max(), Some(6));
    let plain = none();
    assert_eq!(plain.kind(), AgreementKind::None);
    assert_eq!(plain.monthly_limit_minutes(), None);
    assert_eq!(plain.annual_limit_minutes(), None);
    assert_eq!(plain.special_monthly_total_limit_minutes(), None);
    assert_eq!(plain.special_annual_limit_minutes(), None);
    assert_eq!(general(2000, 3000).monthly_limit_minutes(), Some(2000));
}

/// AC-05.12 発動月は O が2700分を超える月。協定の月限度が小さいとき、その限度を超え2700分以下の月は
/// 発動月に数えず `GeneralMonthlyLimitExceeded`（一般条項でも特別条項でも）。
#[test]
fn ac05_12_activation_month_is_over_45_hours() {
    for agreement in [special(1800, 5999, 43_200, 6), general(1800, 21_600)] {
        let result = run(&agreement, "2021-04-01", &[(2000, 0), (2700, 0), (2701, 0)]);
        let counts: Vec<u32> = result.months.iter().map(|m| m.special_count).collect();
        assert_eq!(counts, [0, 0, 1], "{:?}", agreement.kind());
        assert_eq!(
            result.months[0].findings,
            [Finding::GeneralMonthlyLimitExceeded]
        );
        assert_eq!(
            result.months[1].findings,
            [Finding::GeneralMonthlyLimitExceeded]
        );
        let third = &result.months[2].findings;
        assert!(!third.contains(&Finding::GeneralMonthlyLimitExceeded));
    }
    // 特別条項の発動月（2701分）は、協定の月限度の超過に当たらない。
    let result = run(&special(1800, 5999, 43_200, 6), "2021-04-01", &[(2701, 0)]);
    assert_eq!(last(&result).findings, []);
}

/// AC-05.13 出力の `notices` に `OwnEmployerOnly` が必ず付く（月が空でも、協定なしでも）。
#[test]
fn ac05_13_own_employer_only_notice_is_always_there() {
    for agreement in [none(), general(2700, 21_600), special_max()] {
        for list in [&[][..], &[(0, 0)][..], &[(5000, 1000)][..]] {
            let result = judge(&agreement, &months("2021-04-01", list), &[]).unwrap();
            assert_eq!(result.notices, vec![Limits36Notice::OwnEmployerOnly]);
            assert_eq!(result.notices[0].code(), "OwnEmployerOnly");
        }
    }
    let empty = judge(&none(), &[], &zero_history("2021-04-01", 3)).unwrap();
    assert_eq!(empty.months, []);
}

/// AC-05.14 年の累計の違反は超えた月以降の毎月（O が0の月も）に付き、協定年が替われば付かない。
/// `SpecialCountExceeded` は発動月だけに付く。
#[test]
fn ac05_14_cumulative_findings_and_count_only_on_activation_months() {
    // 一般条項の協定の年限度3000分: 2か月目に超え、3・4か月目（O＝0）も付く。翌年度の4月は付かない。
    let mut list = vec![(2000, 0), (1001, 0)];
    list.extend(std::iter::repeat_n((0, 0), 10));
    list.push((0, 0));
    let result = run(&general(2700, 3000), "2021-04-01", &list);
    let got = findings(&result);
    assert_eq!(got[0], []);
    for (i, f) in got.iter().enumerate().take(12).skip(1) {
        assert_eq!(f, &[Finding::GeneralAnnualLimitExceeded], "{i}");
    }
    assert_eq!(got[12], [], "協定年が替わった");
    assert_eq!(result.months[12].year_overtime, 0);
    // 法定の年360時間の超過も、O が0の月まで毎月。
    let mut list = vec![(2700, 0); 8];
    list.push((1, 0));
    list.extend([(0, 0); 3]);
    let result = run(&general(2700, 21_600), "2021-04-01", &list);
    let got = findings(&result);
    for f in &got[8..] {
        assert_eq!(f, &[Finding::AnnualOvertimeExceeded]);
    }
    // 特別条項の年の上限の超過も同じ。
    let result = run(
        &special(2700, 5999, 3000, 6),
        "2021-04-01",
        &[(2701, 0), (300, 0), (0, 0)],
    );
    assert_eq!(
        findings(&result),
        [
            vec![],
            vec![Finding::SpecialAnnualExceeded],
            vec![Finding::SpecialAnnualExceeded]
        ]
    );
    // 回数1の協定: 2回目と3回目の発動月だけに付き、その間の発動月でない月には付かない。
    let result = run(
        &special(2700, 5999, 43_200, 1),
        "2021-04-01",
        &[(3000, 0), (3000, 0), (0, 0), (3000, 0)],
    );
    assert_eq!(
        findings(&result),
        [
            vec![],
            vec![Finding::SpecialCountExceeded],
            vec![],
            vec![Finding::SpecialCountExceeded]
        ]
    );
}

/// AC-05.15 協定年の途中から始まる入力: `year_overtime_known` が偽で、下限値に同じ規則を当てて確実な違反だけを出す。
#[test]
fn ac05_15_mid_year_start_uses_lower_bounds() {
    // 4月起算で10月から: 協定の年限度3000分を、下限値（10・11月の和）がすでに超える。
    let result = run(&general(2700, 3000), "2021-10-01", &[(2000, 0), (1001, 0)]);
    assert!(result.months.iter().all(|m| !m.year_overtime_known));
    assert_eq!(result.months[1].year_overtime, 3001);
    assert_eq!(
        findings(&result),
        [vec![], vec![Finding::GeneralAnnualLimitExceeded]]
    );
    // 下限値が21600分を超えれば、法定の超過。5月から11か月（2700分×9＝24300分）。
    let result = run(&general(2700, 21_600), "2021-05-01", &[(2700, 0); 11]);
    let got = findings(&result);
    assert!(got[..8].iter().all(Vec::is_empty), "{got:?}");
    for f in &got[8..] {
        assert_eq!(f, &[Finding::AnnualOvertimeExceeded]);
    }
    assert!(result.months.iter().all(|m| !m.year_overtime_known));
    // 回数の下限値: 協定2回で、途中から3回目の発動月。
    let result = run(
        &special(2700, 5999, 43_200, 2),
        "2021-09-01",
        &[(3000, 0); 3],
    );
    assert_eq!(
        findings(&result),
        [vec![], vec![], vec![Finding::SpecialCountExceeded]]
    );
    // 協定年の初月から入れれば真になる（次の4月から）。
    let result = run(&general(2700, 21_600), "2022-02-01", &[(100, 0); 4]);
    let known: Vec<bool> = result
        .months
        .iter()
        .map(|m| m.year_overtime_known)
        .collect();
    assert_eq!(known, [false, false, true, true]);
    assert_eq!(result.months[2].year_overtime, 100);
}

/// AC-05.9 の前提: 指摘の順はコードの表の順、同じコードは n の昇順（別実装との突き合わせでも、この順で比べる）。
#[test]
fn ac05_findings_are_in_code_table_order() {
    // 一般条項: 4か月目に、月の超過・年の超過・100時間・2〜4か月の合計の超過・5〜6か月の不明。
    let result = judge(
        &general(2700, 21_600),
        &months("2021-04-01", &[(5500, 500); 4]),
        &[],
    )
    .unwrap();
    assert_eq!(
        last(&result).findings,
        [
            Finding::MonthlyOvertimeExceeded,
            Finding::AnnualOvertimeExceeded,
            Finding::MonthlyTotalReached100h,
            Finding::MultiMonthSumExceeded { n: 2 },
            Finding::MultiMonthSumExceeded { n: 3 },
            Finding::MultiMonthSumExceeded { n: 4 },
            Finding::UnknownHistory { n: 5 },
            Finding::UnknownHistory { n: 6 },
        ]
    );
    // 特別条項: 回数・月の上限・年の上限・100時間・合計・不明。
    let result = judge(
        &special(2700, 5000, 10_000, 1),
        &months("2021-04-01", &[(5500, 500); 2]),
        &[],
    )
    .unwrap();
    assert_eq!(
        last(&result).findings,
        [
            Finding::SpecialCountExceeded,
            Finding::SpecialMonthlyTotalExceeded,
            Finding::SpecialAnnualExceeded,
            Finding::MonthlyTotalReached100h,
            Finding::MultiMonthSumExceeded { n: 2 },
            Finding::UnknownHistory { n: 3 },
            Finding::UnknownHistory { n: 4 },
            Finding::UnknownHistory { n: 5 },
            Finding::UnknownHistory { n: 6 },
        ]
    );
    let codes: Vec<&str> = last(&result).findings.iter().map(|f| f.code()).collect();
    assert_eq!(codes[0], "SpecialCountExceeded");
    assert_eq!(last(&result).findings[4].n(), Some(2));
    assert_eq!(last(&result).findings[0].n(), None);
}

/// 協定月の日付の整合（手順4）と、分の範囲（手順2）の誤り。
#[test]
fn ac05_months_and_history_must_be_consistent() {
    let agreement = general(2700, 21_600);
    let ok = months("2021-04-01", &[(0, 0); 3]);
    // 協定月の初日は1〜28日。
    let day29 = months("2021-04-29", &[(0, 0)]);
    assert_eq!(
        judge(&agreement, &day29, &[]),
        Err(Limits36Error::MonthStartDay { index: 0 })
    );
    // 隙間、逆順、日の違い。
    let mut gap = ok.clone();
    gap.remove(1);
    assert_eq!(
        judge(&agreement, &gap, &[]),
        Err(Limits36Error::NotConsecutive { index: 1 })
    );
    let mut reversed = ok.clone();
    reversed.reverse();
    assert_eq!(
        judge(&agreement, &reversed, &[]),
        Err(Limits36Error::NotConsecutive { index: 1 })
    );
    let mut other_day = ok.clone();
    other_day[2].start = date("2021-06-02");
    assert_eq!(
        judge(&agreement, &other_day, &[]),
        Err(Limits36Error::NotConsecutive { index: 2 })
    );
    // 起算日16日の協定月も並べられる（年をまたぐ）。
    let sixteenth = months("2021-11-16", &[(100, 0); 4]);
    assert_eq!(sixteenth[2].start, date("2022-01-16"));
    let result = judge(&agreement, &sixteenth, &zero_history("2021-11-16", 5)).unwrap();
    assert_eq!(result.months[3].sum(6), Some(400));
    // 履歴: 協定月の初日でない、months の最初の月以後、同じ月の2回。
    let history = |start: &str| HistoryMonth {
        start: date(start),
        total: 0,
    };
    assert_eq!(
        judge(&agreement, &ok, &[history("2021-03-02")]),
        Err(Limits36Error::HistoryNotAgreementMonth { index: 0 })
    );
    assert_eq!(
        judge(
            &agreement,
            &ok,
            &[history("2021-03-01"), history("2021-04-01")]
        ),
        Err(Limits36Error::HistoryNotBefore { index: 1 })
    );
    assert_eq!(
        judge(&agreement, &ok, &[history("2021-06-01")]),
        Err(Limits36Error::HistoryNotBefore { index: 0 })
    );
    assert_eq!(
        judge(
            &agreement,
            &ok,
            &[
                history("2021-02-01"),
                history("2021-03-01"),
                history("2021-02-01")
            ]
        ),
        Err(Limits36Error::DuplicateHistory { index: 2 })
    );
    // months が空なら、履歴は日が1〜28であることだけを見る（日の違いと同じ月の2回は見ない）。
    assert!(
        judge(
            &agreement,
            &[],
            &[
                history("2021-03-16"),
                history("2021-03-01"),
                history("2021-03-01")
            ]
        )
        .is_ok()
    );
    assert_eq!(
        judge(
            &agreement,
            &[],
            &[history("2021-03-01"), history("2021-03-29")]
        ),
        Err(Limits36Error::HistoryNotAgreementMonth { index: 1 })
    );
}

/// 分の範囲: O と H は0以上で O＋H は44640分以下、履歴の T は0〜44640分（手順2）。
#[test]
fn ac05_minutes_ranges() {
    let agreement = special_max();
    let ok = |o, h| judge(&agreement, &months("2021-04-01", &[(o, h)]), &[]);
    assert!(ok(44_640, 0).is_ok());
    assert!(ok(0, 44_640).is_ok());
    assert!(ok(22_320, 22_320).is_ok());
    assert_eq!(
        ok(44_640, 1),
        Err(Limits36Error::MonthTotalTooLarge { index: 0 })
    );
    assert_eq!(
        ok(i64::MAX, i64::MAX),
        Err(Limits36Error::MonthTotalTooLarge { index: 0 })
    );
    assert_eq!(
        ok(-1, 0),
        Err(Limits36Error::MinutesOutOfRange {
            list: "months",
            index: 0,
            field: "overtime"
        })
    );
    assert_eq!(
        ok(0, -1),
        Err(Limits36Error::MinutesOutOfRange {
            list: "months",
            index: 0,
            field: "holiday"
        })
    );
    let history = |total| {
        judge(
            &agreement,
            &[],
            &[HistoryMonth {
                start: date("2021-03-01"),
                total,
            }],
        )
    };
    assert!(history(0).is_ok());
    assert!(history(44_640).is_ok());
    for total in [-1, 44_641] {
        assert_eq!(
            history(total),
            Err(Limits36Error::MinutesOutOfRange {
                list: "history",
                index: 0,
                field: "total"
            })
        );
    }
    // 日付を読む前に呼べる同じ検査。
    assert!(check_month_minutes(3, 44_000, 640).is_ok());
    assert_eq!(
        check_month_minutes(3, 44_000, 641),
        Err(Limits36Error::MonthTotalTooLarge { index: 3 })
    );
    assert!(check_history_minutes(1, 44_640).is_ok());
    assert!(check_history_minutes(1, 44_641).is_err());
}

/// 誤りの文に、入力の値を含めない（場所と規則だけ）。
#[test]
fn ac05_error_messages_name_the_place_not_the_value() {
    let errors = [
        Limits36Error::AgreementOutOfRange {
            field: "monthly_limit_minutes",
            expected: "0〜2700",
        },
        Limits36Error::MissingLimit {
            field: "annual_limit_minutes",
        },
        Limits36Error::MinutesOutOfRange {
            list: "months",
            index: 7,
            field: "overtime",
        },
        Limits36Error::MonthTotalTooLarge { index: 7 },
        Limits36Error::MonthStartDay { index: 7 },
        Limits36Error::NotConsecutive { index: 7 },
        Limits36Error::HistoryNotAgreementMonth { index: 7 },
        Limits36Error::HistoryNotBefore { index: 7 },
        Limits36Error::DuplicateHistory { index: 7 },
        Limits36Error::DateOutOfRange,
        Limits36Error::MismatchedStartDay,
        Limits36Error::Overflow,
    ];
    for error in errors {
        let text = error.to_string();
        assert_ne!(text, "");
        assert!(!text.contains("2021"), "{text}");
    }
    assert!(
        Limits36Error::NotConsecutive { index: 7 }
            .to_string()
            .contains("months[7]")
    );
}

/// 指摘の根拠の条文（労働基準法第36条）。条文の写しにある文言と対応する。
#[test]
fn ac05_findings_cite_article_36() {
    let statutes = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/legal/sources/statutes-2026-10-15.txt"
    ))
    .unwrap();
    let start = statutes
        .find("第三十六条 （時間外及び休日の労働）")
        .unwrap();
    let article = statutes[start..].split("\n第三十七条").next().unwrap();
    assert!(article.contains("一箇月について四十五時間及び一年について三百六十時間"));
    assert!(article.contains("百時間未満"));
    assert!(article.contains("七百二十時間を超えない"));
    assert!(article.contains("一年について六箇月以内"));
    assert!(article.contains("平均時間八十時間を超えないこと"));
    for provenance in [LSA_36_1, LSA_36_4, LSA_36_6_ITEM2, LSA_36_6_ITEM3] {
        assert_eq!(provenance.law, LABOR_STANDARDS_ACT);
        assert_eq!(provenance.confirmed_on, CONFIRMED_ON);
        assert!(provenance.citation().starts_with("労働基準法第36条"));
    }
    let all = [
        Finding::NoAgreement,
        Finding::MonthlyOvertimeExceeded,
        Finding::GeneralMonthlyLimitExceeded,
        Finding::AnnualOvertimeExceeded,
        Finding::GeneralAnnualLimitExceeded,
        Finding::SpecialCountExceeded,
        Finding::SpecialMonthlyTotalExceeded,
        Finding::SpecialAnnualExceeded,
        Finding::MonthlyTotalReached100h,
        Finding::MultiMonthSumExceeded { n: 2 },
        Finding::UnknownHistory { n: 2 },
    ];
    for finding in all {
        assert!(!finding.provenances().is_empty(), "{finding:?}");
    }
    assert_eq!(Finding::SpecialAnnualExceeded.provenances(), [LSA_36_5]);
    assert_eq!(
        Finding::MonthlyTotalReached100h.provenances(),
        [LSA_36_6_ITEM2]
    );
    assert_eq!(
        Finding::UnknownHistory { n: 6 }.provenances(),
        [LSA_36_6_ITEM3]
    );
}
