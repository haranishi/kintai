//! 月60時間超の時間外の手書きの例（`docs/spec/over60.md`）。
//! 2026-07-05 は日曜（法定休日）。月〜金の 5:00〜22:00 は 1020分（法定内480＋時間外540）で深夜を含まない。

mod common;

use common::{day, period, policy, run, shift};
use kintai_core::Weekday;
use kintai_jp_labor::ShiftInput;

/// 月〜金（`monday` から5日）を 5:00〜22:00、土曜を 5:00〜20:00 にした1週。時間外は 2700＋900＝3600分。
fn week_of_3600_overtime(days: [&str; 6]) -> Vec<ShiftInput> {
    let mut shifts: Vec<ShiftInput> = days[..5]
        .iter()
        .map(|d| shift(&format!("{d}T05:00"), &format!("{d}T22:00")))
        .collect();
    shifts.push(shift(
        &format!("{}T05:00", days[5]),
        &format!("{}T20:00", days[5]),
    ));
    shifts
}

const JULY_WEEK_2: [&str; 6] = [
    "2026-07-06",
    "2026-07-07",
    "2026-07-08",
    "2026-07-09",
    "2026-07-10",
    "2026-07-11",
];
const JULY_WEEK_3: [&str; 6] = [
    "2026-07-13",
    "2026-07-14",
    "2026-07-15",
    "2026-07-16",
    "2026-07-17",
    "2026-07-18",
];
const AUGUST_WEEK_2: [&str; 6] = [
    "2026-08-03",
    "2026-08-04",
    "2026-08-05",
    "2026-08-06",
    "2026-08-07",
    "2026-08-08",
];

/// AC-04.2 3600分ちょうどまでは60時間以下、3601分目から60時間超。
#[test]
fn ac04_2_threshold_3600_and_3601() {
    let p = policy(Weekday::Sunday);
    let mut shifts = week_of_3600_overtime(JULY_WEEK_2);
    let exact = run(&p, &shifts);
    let july = period(&exact, "2026-07-01");
    assert_eq!((july.overtime, july.over60), (3600, 0));
    assert_eq!(day(&exact, "2026-07-11").overtime, 900);

    shifts.push(shift("2026-07-13T05:00", "2026-07-13T13:01")); // 481分: 法定内480＋時間外1
    let one_more = run(&p, &shifts);
    let mon = day(&one_more, "2026-07-13");
    assert_eq!((mon.within, mon.overtime, mon.over60), (480, 1, 1));
    let july = period(&one_more, "2026-07-01");
    assert_eq!((july.overtime, july.over60), (3601, 1));
}

/// AC-04.1 月の期間ごとに累計し、期間が替わると累計が0に戻る。
#[test]
fn ac04_1_resets_each_period() {
    let p = policy(Weekday::Sunday);
    let mut shifts = week_of_3600_overtime(JULY_WEEK_2);
    shifts.extend(week_of_3600_overtime(JULY_WEEK_3));
    shifts.extend(week_of_3600_overtime(AUGUST_WEEK_2));
    shifts.push(shift("2026-08-10T05:00", "2026-08-10T13:01"));
    let result = run(&p, &shifts);
    let july = period(&result, "2026-07-01");
    assert_eq!((july.overtime, july.over60), (7200, 3600));
    let august = period(&result, "2026-08-01");
    assert_eq!((august.overtime, august.over60), (3601, 1));
    assert_eq!(day(&result, "2026-08-08").over60, 0, "8月は0から数え直す");
    assert_eq!(day(&result, "2026-08-10").over60, 1);
}

/// AC-04.3 法定休日の労働は数えず、法定外休日（土曜）で法定時間を超えた分は数える。
#[test]
fn ac04_3_legal_holiday_is_not_counted() {
    let p = policy(Weekday::Sunday);
    let mut shifts = vec![shift("2026-07-05T05:00", "2026-07-05T22:00")]; // 日曜（法定休日）1020分
    shifts.extend(week_of_3600_overtime(JULY_WEEK_2));
    shifts.push(shift("2026-07-13T05:00", "2026-07-13T13:01"));
    let result = run(&p, &shifts);
    assert_eq!(day(&result, "2026-07-05").holiday, 1020);
    assert_eq!(
        day(&result, "2026-07-11").over60,
        0,
        "日曜の1020分を数えていれば、土曜で超えていた"
    );
    assert_eq!(day(&result, "2026-07-13").over60, 1, "土曜の900分は数える");
    let july = period(&result, "2026-07-01");
    assert_eq!((july.overtime, july.over60, july.holiday), (3601, 1, 1020));
}

/// AC-04.4 時刻の順に数え、同じ日の中では後ろの分から60時間超になる。
/// AC-04.5 60時間超で深夜の分が `night_over60` に別に出る。
#[test]
fn ac04_4_later_minutes_exceed_first() {
    let p = policy(Weekday::Sunday);
    let mut shifts = week_of_3600_overtime(JULY_WEEK_2);
    shifts[5] = shift("2026-07-11T05:00", "2026-07-11T19:00"); // 土曜を840分にして、累計3540分
    shifts.push(shift("2026-07-13T13:00", "2026-07-13T23:00")); // 法定内 13〜21時、時間外 21〜23時
    let result = run(&p, &shifts);
    let mon = day(&result, "2026-07-13");
    assert_eq!((mon.overtime, mon.over60), (120, 60));
    // 後ろの60分（22〜23時）が60時間超。そこは深夜なので night_over60 = 60。
    assert_eq!((mon.night_overtime, mon.night_over60), (60, 60));
}

/// AC-04.5 60時間超に達した後の、深夜の時間外。
#[test]
fn ac04_5_night_over60() {
    let p = policy(Weekday::Sunday);
    let mut shifts = week_of_3600_overtime(JULY_WEEK_2);
    shifts.push(shift("2026-07-14T15:00", "2026-07-15T01:00")); // 法定内 15〜23時、時間外 23〜1時
    let result = run(&p, &shifts);
    let tue = day(&result, "2026-07-14");
    assert_eq!((tue.overtime, tue.over60), (120, 120));
    assert_eq!(
        (
            tue.night,
            tue.night_within,
            tue.night_overtime,
            tue.night_over60
        ),
        (180, 60, 120, 120)
    );
    assert!(tue.night_over60 <= tue.over60);
}

/// AC-04.7 月の起算日が16日のとき、15日までと16日からで期間が分かれる。
#[test]
fn ac04_7_month_start_day_16() {
    let mut p = policy(Weekday::Sunday);
    p.month_start_day = 16;
    let mut shifts = week_of_3600_overtime(JULY_WEEK_2);
    shifts.push(shift("2026-07-15T05:00", "2026-07-15T13:01")); // 6/16〜7/15 の期間
    shifts.push(shift("2026-07-16T05:00", "2026-07-16T13:01")); // 7/16〜8/15 の期間
    let result = run(&p, &shifts);
    assert_eq!(result.periods.len(), 2);
    let june = period(&result, "2026-06-16");
    assert_eq!((june.overtime, june.over60), (3601, 1));
    let july = period(&result, "2026-07-16");
    assert_eq!((july.overtime, july.over60), (1, 0));
    assert_eq!(day(&result, "2026-07-15").over60, 1);
    assert_eq!(day(&result, "2026-07-16").over60, 0);
}

/// AC-04.7 起算日1でも、月の初日で期間が替わる。
#[test]
fn ac04_7_month_start_day_1_splits_on_first() {
    let p = policy(Weekday::Sunday);
    let result = run(
        &p,
        &[
            shift("2026-06-30T09:00", "2026-06-30T19:00"),
            shift("2026-07-01T09:00", "2026-07-01T19:00"),
        ],
    );
    assert_eq!(period(&result, "2026-06-01").overtime, 120);
    assert_eq!(period(&result, "2026-07-01").overtime, 120);
}
