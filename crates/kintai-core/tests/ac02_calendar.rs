//! AC-02.1: 日付の曜日・加減・月末の日数が、`chrono` と 2000〜2099 年の全日付で一致する
//!          （型の範囲 1900〜2200 年の全日付でも一致する）。
//! AC-02.6: 入力の範囲外の日付（2000年より前、2099年より後）を拒否する。計算の結果は2200年まで持てる。

use chrono::{Datelike, Days, NaiveDate};
use kintai_core::{CoreError, Date, Weekday, days_in_month, is_leap_year};

fn chrono_date(date: Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), date.day()).unwrap()
}

fn dates_between(first: Date, last: Date) -> impl Iterator<Item = Date> {
    (first.day_number()..=last.day_number()).map(|n| Date::from_day_number(n).unwrap())
}

fn chrono_days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    };
    u32::try_from(next.signed_duration_since(first).num_days()).unwrap()
}

/// 全日付で、年月日・曜日・月末の日数が chrono と一致する。日付の数を返す。
fn compare_every_date(first: Date, last: Date) -> usize {
    let mut expected = chrono_date(first);
    let mut count = 0;
    for date in dates_between(first, last) {
        assert_eq!(
            (date.year(), date.month(), date.day()),
            (expected.year(), expected.month(), expected.day()),
            "{date}"
        );
        assert_eq!(
            date.weekday().index_from_monday(),
            expected.weekday().num_days_from_monday(),
            "{date}"
        );
        assert_eq!(
            date.days_in_month(),
            chrono_days_in_month(expected.year(), expected.month()),
            "{date}"
        );
        expected = expected.succ_opt().unwrap();
        count += 1;
    }
    count
}

/// AC-02.1 2000〜2099年の全日付で、年月日・曜日・月末の日数が chrono と一致する。
#[test]
fn ac02_1_every_input_date_matches_chrono() {
    assert_eq!(compare_every_date(Date::INPUT_MIN, Date::INPUT_MAX), 36_525);
}

/// AC-02.1 型の範囲（1900〜2200年）の全日付でも一致する（年休の失効日など、計算の結果の日付）。
#[test]
fn ac02_1_every_representable_date_matches_chrono() {
    assert_eq!(compare_every_date(Date::MIN, Date::MAX), 109_938);
    assert_eq!(
        chrono_date(Date::MIN),
        NaiveDate::from_ymd_opt(1900, 1, 1).unwrap()
    );
    assert_eq!(
        chrono_date(Date::MAX),
        NaiveDate::from_ymd_opt(2200, 12, 31).unwrap()
    );
}

/// AC-02.1 日数の加減と差が、2000〜2099年の全日付で chrono と一致する。
/// 結果は型の範囲（1900〜2200年）まで作れ、その外は拒否する（AC-02.6）。
#[test]
fn ac02_1_add_days_matches_chrono_for_every_date() {
    let min = chrono_date(Date::MIN);
    let max = chrono_date(Date::MAX);
    let steps: [i64; 14] = [
        -40_000, -36_524, -400, -366, -31, -1, 0, 1, 7, 29, 365, 1000, 36_524, 40_000,
    ];
    for date in dates_between(Date::INPUT_MIN, Date::INPUT_MAX) {
        let base = chrono_date(date);
        for step in steps {
            let ours = date.checked_add_days(step);
            let theirs = if step >= 0 {
                base.checked_add_days(Days::new(step.unsigned_abs()))
            } else {
                base.checked_sub_days(Days::new(step.unsigned_abs()))
            }
            .unwrap();
            if theirs < min || theirs > max {
                assert_eq!(ours, Err(CoreError::OutOfRange), "{date} {step}");
            } else {
                let ours = ours.unwrap();
                assert_eq!(chrono_date(ours), theirs, "{date} {step}");
                assert_eq!(ours.days_since(date), step);
                assert_eq!(
                    ours.days_since(date),
                    theirs.signed_duration_since(base).num_days()
                );
            }
        }
    }
}

/// AC-02.1 うるう年の規則（4で割れ、100で割れず、400で割れるときを除く）と月の日数。
#[test]
fn ac02_1_leap_years_and_month_lengths() {
    for year in 1900..=2400 {
        let theirs = NaiveDate::from_ymd_opt(year, 2, 29).is_some();
        assert_eq!(is_leap_year(year), theirs, "{year}");
        for month in 1..=12 {
            assert_eq!(
                days_in_month(year, month),
                Some(chrono_days_in_month(year, month))
            );
        }
    }
    assert!(is_leap_year(2000));
    assert!(!is_leap_year(2100));
    assert!(!is_leap_year(1900));
    assert!(is_leap_year(2024));
    assert!(!is_leap_year(2026));
    assert_eq!(days_in_month(2026, 0), None);
    assert_eq!(days_in_month(2026, 13), None);
    assert_eq!(days_in_month(2024, 2), Some(29));
    assert_eq!(days_in_month(2100, 2), Some(28));
}

/// AC-02.1 曜日の起算日からの日数と、翌日の曜日。
#[test]
fn ac02_1_weekday_offsets() {
    for start in Weekday::ALL {
        for (offset, day) in
            (0..7).zip(std::iter::successors(Some(start), |d| Some(d.succ())).take(7))
        {
            assert_eq!(day.days_since(start), offset);
        }
    }
    assert_eq!(Weekday::Sunday.days_since(Weekday::Monday), 6);
    assert_eq!(Weekday::Monday.days_since(Weekday::Sunday), 1);
    assert_eq!(Weekday::Saturday.succ(), Weekday::Sunday);
    assert_eq!(Weekday::Sunday.succ(), Weekday::Monday);
    assert_eq!(Weekday::ALL.len(), 7);
    assert_eq!(Weekday::Sunday.index_from_monday(), 6);
    // 2026-10-07 は水曜日（公的な数値例の日付）。2000-01-01 は土曜日、1900-01-01 は月曜日。
    assert_eq!(
        Date::from_ymd(2026, 10, 7).unwrap().weekday(),
        Weekday::Wednesday
    );
    assert_eq!(Date::INPUT_MIN.weekday(), Weekday::Saturday);
    assert_eq!(Date::INPUT_MAX.weekday(), Weekday::Thursday);
    assert_eq!(Date::MIN.weekday(), Weekday::Monday);
    assert_eq!(Date::MAX.weekday(), Weekday::Wednesday);
}

/// AC-02.6 入力の範囲外（2000年より前、2099年より後）の日付の表記を拒否する。
#[test]
fn ac02_6_input_out_of_range_is_rejected() {
    for text in [
        "1999-12-31",
        "2100-01-01",
        "1900-01-01",
        "2200-12-31",
        "0000-01-01",
        "9999-12-31",
    ] {
        assert_eq!(text.parse::<Date>(), Err(CoreError::OutOfRange), "{text}");
        assert_eq!(Date::parse(text), Err(CoreError::OutOfRange), "{text}");
    }
    assert_eq!(Date::parse("2000-01-01"), Ok(Date::INPUT_MIN));
    assert_eq!(Date::parse("2099-12-31"), Ok(Date::INPUT_MAX));
    assert!(Date::INPUT_MIN.is_input_range());
    assert!(Date::INPUT_MAX.is_input_range());
    assert!(
        !Date::INPUT_MIN
            .checked_add_days(-1)
            .unwrap()
            .is_input_range()
    );
    assert!(
        !Date::INPUT_MAX
            .checked_add_days(1)
            .unwrap()
            .is_input_range()
    );
}

/// AC-02.6 計算の結果の日付は 1900〜2200年まで作れる（表記もできる）。その外は作れない。
#[test]
fn ac02_6_results_can_reach_2200() {
    let after = Date::INPUT_MAX.checked_add_days(1).unwrap();
    assert_eq!(after.to_string(), "2100-01-01");
    assert_eq!(
        Date::from_ymd(2150, 6, 1).unwrap().to_string(),
        "2150-06-01"
    );
    assert_eq!(
        Date::from_ymd(1999, 12, 31).unwrap().to_string(),
        "1999-12-31"
    );
    assert_eq!(Date::from_ymd(1900, 1, 1), Ok(Date::MIN));
    assert_eq!(Date::from_ymd(2200, 12, 31), Ok(Date::MAX));
    assert_eq!(Date::from_ymd(1899, 12, 31), Err(CoreError::OutOfRange));
    assert_eq!(Date::from_ymd(2201, 1, 1), Err(CoreError::OutOfRange));
    assert_eq!(
        Date::from_day_number(Date::MIN.day_number() - 1),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(
        Date::from_day_number(Date::MAX.day_number() + 1),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(Date::MIN.checked_add_days(-1), Err(CoreError::OutOfRange));
    assert_eq!(Date::MAX.checked_add_days(1), Err(CoreError::OutOfRange));
    assert_eq!(
        Date::MIN.checked_add_days(i64::MAX),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(
        Date::MAX.checked_add_days(i64::MIN),
        Err(CoreError::OutOfRange)
    );
    // 暦にない日付は、範囲の外でも「暦にない」として拒否する。
    assert_eq!(Date::from_ymd(1900, 2, 29), Err(CoreError::InvalidDate));
    assert_eq!(Date::from_ymd(2026, 2, 29), Err(CoreError::InvalidDate));
    assert_eq!(Date::from_ymd(2026, 13, 1), Err(CoreError::InvalidDate));
    assert_eq!(Date::from_ymd(2026, 1, 0), Err(CoreError::InvalidDate));
    assert_eq!(Date::from_ymd(2026, 4, 31), Err(CoreError::InvalidDate));
}

/// AC-02.1 並べ替えは暦の順、`Debug` と `Display` は決定的。
#[test]
fn ac02_1_ordering_and_display_are_deterministic() {
    let a = Date::from_ymd(2026, 12, 31).unwrap();
    let b = Date::from_ymd(2027, 1, 1).unwrap();
    assert!(a < b);
    assert!(Date::MIN < Date::INPUT_MIN && Date::INPUT_MAX < Date::MAX);
    assert_eq!(b.days_since(a), 1);
    assert_eq!(a.days_since(b), -1);
    assert_eq!(a.to_string(), "2026-12-31");
    assert_eq!(format!("{a:?}"), "Date(2026-12-31)");
    assert_eq!(Date::INPUT_MIN.day_number(), 10_957);
    assert_eq!(Date::MIN.day_number(), -25_567);
    assert!(
        CoreError::OutOfRange
            .to_string()
            .contains("2000-01-01〜2099-12-31")
    );
}
