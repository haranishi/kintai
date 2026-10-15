//! 変異試験（`tools/mutations/limits36.json`）で生き残った誤りを赤くする試験。
//!
//! - AC-05.1: 区分の結果から O と H を取り出すとき、月60時間超の分は O の内数で、H には入れない。
//!   `MonthHours::from_period` と `months_from_periods` の両方で確かめる（それまでの試験は、
//!   60時間超のない月だけだった）。
//! - `history` の協定月の初日の日が `months` の日と違えば不正な入力（`case-format-36-leave.md`）。
//!   それまでの試験は、日が後ろにずれた場合だけだった。

use kintai_core::{Date, MonthPeriod, MonthStartDay, Weekday};
use kintai_jp_labor::limits36::{
    Agreement, AgreementKind, HistoryMonth, Limits36Error, MonthHours, ValidAgreement, judge,
    months_from_periods,
};
use kintai_jp_labor::{ShiftInput, ShiftSet, WorkPolicy, WorkSystem, classify};

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

fn shift(start: &str, end: &str) -> ShiftInput {
    ShiftInput {
        start: start.parse().unwrap(),
        end: end.parse().unwrap(),
        breaks: Vec::new(),
    }
}

fn general() -> ValidAgreement {
    Agreement {
        kind: AgreementKind::General,
        year_start_month: 4,
        monthly_limit_minutes: Some(2700),
        annual_limit_minutes: Some(21_600),
        special_monthly_total_limit_minutes: None,
        special_annual_limit_minutes: None,
        special_months_max: None,
    }
    .validate()
    .unwrap()
}

/// AC-05.1 2026年10月の平日22日に 9:00〜21:00（法定内480分・時間外240分）、日曜（法定休日）に8時間。
/// 時間外は 240×22＝5280分で、月60時間（3600分）を超えた1680分が60時間超。O は60時間超を含む5280分、
/// H は法定休日の480分だけ（60時間超を足さない）。T＝O＋H、実労働＝法定内＋O＋H で二重に数えない。
#[test]
fn ac05_1_over60_minutes_stay_in_overtime_and_out_of_holiday() {
    let policy = WorkPolicy {
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
    .validate()
    .unwrap();
    let mut inputs = Vec::new();
    let mut day = date("2026-10-01");
    while day.month() == 10 {
        if !matches!(day.weekday(), Weekday::Saturday | Weekday::Sunday) {
            inputs.push(shift(&format!("{day}T09:00"), &format!("{day}T21:00")));
        }
        day = day.checked_add_days(1).unwrap();
    }
    assert_eq!(inputs.len(), 22);
    inputs.push(shift("2026-10-11T09:00", "2026-10-11T17:00"));
    let result = classify(&policy, &ShiftSet::new(&inputs).unwrap(), None).unwrap();
    assert_eq!(result.refused, None);
    let [period] = result.periods.as_slice() else {
        panic!("月の期間は1つ: {:?}", result.periods);
    };
    assert_eq!(
        (
            period.within,
            period.overtime,
            period.over60,
            period.holiday
        ),
        (10_560, 5280, 1680, 480)
    );

    let hours = MonthHours::from_period(period).unwrap();
    assert_eq!((hours.overtime, hours.holiday), (5280, 480));
    assert_eq!(hours.total(), Ok(5760));
    assert_eq!(period.total, period.within + hours.overtime + hours.holiday);

    let october = MonthPeriod::containing(date("2026-10-01"), MonthStartDay::new(1).unwrap());
    let filled = months_from_periods(&result.periods, october, october).unwrap();
    assert_eq!(
        filled,
        [MonthHours {
            start: date("2026-10-01"),
            overtime: 5280,
            holiday: 480,
        }]
    );
}

/// AC-05.6・05.7 の直近 n か月に使う `history` は、協定月の初日でなければ不正な入力。日が `months` の
/// 協定月の初日の日と違えば、前にずれても後にずれても誤り（`case-format-36-leave.md`）。
#[test]
fn history_day_must_equal_the_agreement_month_start_day() {
    let agreement = general();
    let months = [MonthHours {
        start: date("2026-04-16"),
        overtime: 0,
        holiday: 0,
    }];
    let history = |start: &str| {
        [HistoryMonth {
            start: date(start),
            total: 0,
        }]
    };
    for wrong in ["2026-03-01", "2026-03-15", "2026-03-17", "2026-03-28"] {
        assert_eq!(
            judge(&agreement, &months, &history(wrong)),
            Err(Limits36Error::HistoryNotAgreementMonth { index: 0 }),
            "{wrong}"
        );
    }
    let result = judge(&agreement, &months, &history("2026-03-16")).unwrap();
    assert_eq!(result.months[0].sum(2), Some(0));
}
