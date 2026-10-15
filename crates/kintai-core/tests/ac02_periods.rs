//! AC-02.7: 週の起算日と月の期間の初日が、起算日1と16、月をまたぐ、年をまたぐ場合で正しい。

use chrono::{Datelike, NaiveDate};
use kintai_core::{CoreError, Date, MonthPeriod, MonthStartDay, Week, Weekday};

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

fn start_day(day: u32) -> MonthStartDay {
    MonthStartDay::new(day).unwrap()
}

/// AC-02.7 週の初日（日曜起算・月曜起算）。月と年をまたぐ。
#[test]
fn ac02_7_week_first_day() {
    // 2026-10-07 は水曜。日曜起算の週は 10-04 から、月曜起算は 10-05 から。
    let wed = date("2026-10-07");
    assert_eq!(
        wed.week(Weekday::Sunday).first_day(),
        Ok(date("2026-10-04"))
    );
    assert_eq!(
        wed.week(Weekday::Monday).first_day(),
        Ok(date("2026-10-05"))
    );
    // 起算日そのもの
    assert_eq!(
        date("2026-10-04").week(Weekday::Sunday).first_day(),
        Ok(date("2026-10-04"))
    );
    // 月をまたぐ: 2026-11-01（日）の前日 10-31（土）は前の週。
    let sat = date("2026-10-31");
    let sun = date("2026-11-01");
    assert_ne!(sat.week(Weekday::Sunday), sun.week(Weekday::Sunday));
    assert_eq!(
        sat.week(Weekday::Monday),
        sun.week(Weekday::Monday),
        "月曜起算なら同じ週"
    );
    // 年をまたぐ: 2026-12-31（木）と 2027-01-02（土）は日曜起算で同じ週（初日 2026-12-27）。
    let week = date("2027-01-02").week(Weekday::Sunday);
    assert_eq!(week.first_day(), Ok(date("2026-12-27")));
    assert!(week.contains(date("2026-12-31")));
    assert!(week.contains(date("2026-12-27")));
    assert!(!week.contains(date("2027-01-03")));
    assert!(!week.contains(date("2026-12-26")));
    assert_eq!(week.start_weekday(), Weekday::Sunday);
    assert_eq!(week.to_string(), "2026-12-27");
    assert_eq!(week.first_day_number(), date("2026-12-27").day_number());
    assert_eq!(Week::containing(date("2026-12-31"), Weekday::Sunday), week);
}

/// AC-02.7 週の初日が入力の範囲（2000-01-01）より前になる週も表せる。型の範囲の外は `Date` にできない。
#[test]
fn ac02_7_week_before_range() {
    let week = Date::INPUT_MIN.week(Weekday::Sunday); // 2000-01-01 は土曜
    assert_eq!(week.to_string(), "1999-12-26");
    assert_eq!(week.first_day(), Ok(Date::from_ymd(1999, 12, 26).unwrap()));
    assert!(week.contains(Date::INPUT_MIN));
    assert_eq!(
        Date::INPUT_MIN.week(Weekday::Saturday).first_day(),
        Ok(Date::INPUT_MIN)
    );
    // 1900-01-01（月曜）の日曜起算の週は 1899-12-31 から。型の範囲の外なので Date にはできない。
    let earliest = Date::MIN.week(Weekday::Sunday);
    assert_eq!(earliest.to_string(), "1899-12-31");
    assert_eq!(earliest.first_day(), Err(CoreError::OutOfRange));
}

/// AC-02.7 全日付・全起算曜日で、週の初日が chrono の曜日から求めたものと一致する。
#[test]
fn ac02_7_week_first_day_for_every_date() {
    for n in Date::MIN.day_number()..=Date::MAX.day_number() {
        let day = Date::from_day_number(n).unwrap();
        let theirs = NaiveDate::from_ymd_opt(day.year(), day.month(), day.day()).unwrap();
        for start in Weekday::ALL {
            let offset =
                (theirs.weekday().num_days_from_monday() + 7 - start.index_from_monday()) % 7;
            let week = day.week(start);
            assert_eq!(week.first_day_number(), n - i64::from(offset));
            assert!(week.contains(day));
        }
    }
}

/// AC-02.7 月の期間（起算日1）。月末と年末をまたぐ。
#[test]
fn ac02_7_month_period_start_day_1() {
    let first = MonthStartDay::FIRST;
    let period = date("2026-10-31").month_period(first);
    assert_eq!(period.first_day(), Ok(date("2026-10-01")));
    assert_eq!(period.next_first_day(), Ok(date("2026-11-01")));
    assert_eq!(period.to_string(), "2026-10-01");
    assert_ne!(period, date("2026-11-01").month_period(first));
    let december = date("2026-12-31").month_period(first);
    assert_eq!(december.next_first_day(), Ok(date("2027-01-01")));
    assert_eq!((december.year(), december.month()), (2026, 12));
    assert_eq!(december.start_day(), first);
    assert!(december < date("2027-01-01").month_period(first));
}

/// AC-02.7 月の期間（起算日16）。期間が月をまたぎ、年をまたぐ。
#[test]
fn ac02_7_month_period_start_day_16() {
    let sixteen = start_day(16);
    let cases = [
        ("2026-10-15", "2026-09-16", "2026-10-16"),
        ("2026-10-16", "2026-10-16", "2026-11-16"),
        ("2026-10-31", "2026-10-16", "2026-11-16"),
        ("2027-01-10", "2026-12-16", "2027-01-16"),
        ("2026-12-16", "2026-12-16", "2027-01-16"),
        ("2024-02-29", "2024-02-16", "2024-03-16"),
        ("2026-03-15", "2026-02-16", "2026-03-16"),
    ];
    for (day, first, next) in cases {
        let period = date(day).month_period(sixteen);
        assert_eq!(period.first_day(), Ok(date(first)), "{day}");
        assert_eq!(period.next_first_day(), Ok(date(next)), "{day}");
        assert_eq!(period.to_string(), first);
        assert!(period.contains(date(day)));
        assert!(period.contains(date(first)));
        assert!(!period.contains(date(next)));
        assert_eq!(
            period.next_first_day_number() - period.first_day_number(),
            date(next).days_since(date(first))
        );
    }
}

/// AC-02.7 全日付で、月の期間の初日 ≤ 日付 < 次の期間の初日。起算日1〜28のすべて。
#[test]
fn ac02_7_month_period_contains_every_date() {
    for day_of_month in 1..=28 {
        let sd = start_day(day_of_month);
        for n in Date::MIN.day_number()..=Date::MAX.day_number() {
            let day = Date::from_day_number(n).unwrap();
            let period = MonthPeriod::containing(day, sd);
            assert!(period.first_day_number() <= n, "{day} {day_of_month}");
            assert!(n < period.next_first_day_number(), "{day} {day_of_month}");
            assert!(period.contains(day));
        }
    }
}

/// AC-02.7 入力の範囲の端では、期間の初日が入力の範囲の外になる（型の範囲の中なので Date にできる）。
#[test]
fn ac02_7_month_period_at_range_edges() {
    let period = date("2000-01-05").month_period(start_day(16));
    assert_eq!(period.to_string(), "1999-12-16");
    assert_eq!(
        period.first_day(),
        Ok(Date::from_ymd(1999, 12, 16).unwrap())
    );
    assert_eq!(period.next_first_day(), Ok(date("2000-01-16")));
    let last = Date::INPUT_MAX.month_period(start_day(16));
    assert_eq!(last.first_day(), Ok(date("2099-12-16")));
    assert_eq!(
        last.next_first_day(),
        Ok(Date::from_ymd(2100, 1, 16).unwrap())
    );
    // 型の範囲の端では、Date にできない（表示はできる）。
    let very_last = Date::MAX.month_period(start_day(16));
    assert_eq!(very_last.next_first_day(), Err(CoreError::OutOfRange));
    let very_first = Date::MIN.month_period(start_day(16));
    assert_eq!(very_first.to_string(), "1899-12-16");
    assert_eq!(very_first.first_day(), Err(CoreError::OutOfRange));
}

/// AC-02.7 起算日は 1〜28 だけを受け付ける。
#[test]
fn ac02_7_month_start_day_range() {
    assert_eq!(MonthStartDay::new(0), Err(CoreError::InvalidMonthStartDay));
    assert_eq!(MonthStartDay::new(29), Err(CoreError::InvalidMonthStartDay));
    assert_eq!(MonthStartDay::new(1), Ok(MonthStartDay::FIRST));
    assert_eq!(MonthStartDay::new(28).unwrap().get(), 28);
}
