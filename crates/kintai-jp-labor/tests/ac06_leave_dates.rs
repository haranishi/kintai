//! AC-06.2: 付与日・失効日・5日の期間が、日付の規則（民法143条2項）で決まる。
//! 2023-08-29入社の例（g0〜g4、失効、期間）と、`leave-official.json` の `derived-first-grant-dates` が一致する。

use kintai_core::{CoreError, Date};
use kintai_jp_labor::leave::{
    CalendarDate, expiry_date, first_grant_date, five_day_window_end, grant_date,
    next_period_first_day, period_last_day,
};
use serde_json::Value;

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

/// AC-06.2 仕様の例: 雇入れ2023-08-29 → g0＝2024-02-29、g1＝2025-03-01、g2＝2026-03-01、g3＝2027-03-01、
/// g4＝2028-02-29。g0 の失効日は2026-03-01、5日の期間は2024-02-29〜2025-02-28。
#[test]
fn ac06_2_hire_2023_08_29() {
    let hire = date("2023-08-29");
    let grants: Vec<String> = (0..5)
        .map(|k| grant_date(hire, k).unwrap().to_string())
        .collect();
    assert_eq!(
        grants,
        [
            "2024-02-29",
            "2025-03-01",
            "2026-03-01",
            "2027-03-01",
            "2028-02-29"
        ]
    );
    let g0 = first_grant_date(hire).unwrap();
    assert_eq!(g0, date("2024-02-29"));
    assert_eq!(expiry_date(g0).unwrap(), date("2026-03-01"));
    assert_eq!(five_day_window_end(g0).unwrap(), date("2025-02-28"));
    // g3 の期間の終わりは 2028-03-01 の前日（2028年はうるう年）。
    assert_eq!(
        five_day_window_end(date("2027-03-01")).unwrap(),
        date("2028-02-29")
    );
    assert_eq!(expiry_date(date("2028-02-29")).unwrap(), date("2030-03-01"));
    // g0 から数える。前回の付与日から1年ずつ足すと、2月29日が一度ずれて戻らない（g4 が 2028-03-01 になる）。
    let mut chained = g0;
    for _ in 0..4 {
        chained = next_period_first_day(chained, 12).unwrap();
    }
    assert_eq!(chained, date("2028-03-01"));
    assert_ne!(chained, grant_date(hire, 4).unwrap());
}

/// AC-06.2 `derived-first-grant-dates`（応当日がない月は翌月1日）。
#[test]
fn ac06_2_first_grant_dates_vector() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/spec/vectors/leave-official.json"
    );
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let vector = all["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "derived-first-grant-dates")
        .unwrap();
    let cases = vector["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let hire = date(case["hire_date"].as_str().unwrap());
        assert_eq!(
            first_grant_date(hire).unwrap().to_string(),
            case["first_grant"].as_str().unwrap(),
            "{case}"
        );
    }
}

/// AC-06.2 期間の終わりと次の初日の例（応当日の前日、応当日がない月の末日、年をまたぐ）。
#[test]
fn ac06_2_period_examples() {
    let cases = [
        ("2024-04-01", 6, "2024-09-30", "2024-10-01"),
        ("2024-12-31", 6, "2025-06-30", "2025-07-01"),
        ("2024-01-31", 1, "2024-02-29", "2024-03-01"),
        ("2023-01-31", 1, "2023-02-28", "2023-03-01"),
        ("2024-01-30", 1, "2024-02-29", "2024-03-01"),
        ("2024-01-29", 1, "2024-02-28", "2024-02-29"),
        ("2024-02-29", 12, "2025-02-28", "2025-03-01"),
        ("2024-02-29", 48, "2028-02-28", "2028-02-29"),
        ("2024-03-01", 12, "2025-02-28", "2025-03-01"),
        ("2027-03-01", 12, "2028-02-29", "2028-03-01"),
        ("2024-10-01", 24, "2026-09-30", "2026-10-01"),
        ("2099-12-31", 24, "2101-12-30", "2101-12-31"),
        ("2024-05-15", 0, "2024-05-14", "2024-05-15"),
    ];
    for (start, months, last, next) in cases {
        assert_eq!(
            period_last_day(date(start), months).unwrap(),
            date_any(last),
            "{start} {months}"
        );
        assert_eq!(
            next_period_first_day(date(start), months).unwrap(),
            date_any(next),
            "{start} {months}"
        );
    }
}

/// 入力の範囲（2000〜2099年）の外も含めて、日付の型を作る。
fn date_any(text: &str) -> Date {
    CalendarDate::parse(text).unwrap().to_date().unwrap()
}

/// 次の初日を、日を1日ずつ進めて求める素朴な読み: n か月後の月の1日から、起算日と同じ日に着くか、
/// その月を出るまで進める。着いた日が次の初日（応当日。なければ翌月1日）。
fn naive_next_first_day(start: Date, months: u32) -> Date {
    let index = i64::from(start.year()) * 12 + i64::from(start.month()) - 1 + i64::from(months);
    let year = i32::try_from(index.div_euclid(12)).unwrap();
    let month = u32::try_from(index.rem_euclid(12) + 1).unwrap();
    let mut day = Date::from_ymd(year, month, 1).unwrap();
    while day.month() == month && day.day() < start.day() {
        day = day.checked_add_days(1).unwrap();
    }
    day
}

/// AC-06.2 2000〜2099年の全日付と n＝1・6・12・24・36 で、素朴な読みと一致する。
/// 期間の終わりは次の初日の前日。範囲を問わない暦日の型でも同じ答え。
#[test]
fn ac06_2_every_date_matches_the_day_by_day_reading() {
    let mut day = Date::INPUT_MIN;
    let mut checked = 0;
    while day <= Date::INPUT_MAX {
        for months in [1, 6, 12, 24, 36] {
            let next = next_period_first_day(day, months).unwrap();
            assert_eq!(next, naive_next_first_day(day, months), "{day} {months}");
            let last = period_last_day(day, months).unwrap();
            assert_eq!(last.checked_add_days(1).unwrap(), next);
            let loose = CalendarDate::from_date(day);
            assert_eq!(
                loose.next_period_first_day(months),
                Some(CalendarDate::from_date(next))
            );
            assert_eq!(
                loose.period_last_day(months),
                Some(CalendarDate::from_date(last))
            );
            checked += 1;
        }
        day = day.checked_add_days(1).unwrap();
    }
    assert_eq!(checked, 36_525 * 5);
}

/// AC-06.2 範囲を問わない暦日: 書式と暦だけを見る（0000年は読まない）。入力の範囲の日付は `Date` と同じ。
#[test]
fn ac06_2_calendar_date_reads_format_but_not_range() {
    for ok in [
        "2024-02-29",
        "1999-12-31",
        "2100-01-01",
        "0001-01-01",
        "9999-12-31",
    ] {
        let parsed = CalendarDate::parse(ok).unwrap();
        assert_eq!(parsed.to_string(), ok);
    }
    for wrong in [
        "2023-02-29",
        "2024-13-01",
        "2024-00-10",
        "2024-04-31",
        "2024-1-01",
        " 2024-01-01",
        "2024-01-01 ",
        "2024/01/01",
        "20240101",
        "0000-01-01",
        "２０２４-01-01",
        "",
    ] {
        assert_eq!(CalendarDate::parse(wrong), None, "{wrong}");
    }
    let parsed = CalendarDate::parse("2100-01-01").unwrap();
    assert_eq!((parsed.year(), parsed.month(), parsed.day()), (2100, 1, 1));
    assert_eq!(parsed.to_date().unwrap(), date_any("2100-01-01"));
    assert_eq!(
        CalendarDate::parse("2201-01-01").unwrap().to_date(),
        Err(CoreError::OutOfRange)
    );
    // 入力の範囲の全日付で、Date の表記と同じ値・同じ順。
    let mut day = Date::INPUT_MIN;
    let mut previous = None;
    while day <= Date::INPUT_MAX {
        let loose = CalendarDate::parse(&day.to_string()).unwrap();
        assert_eq!(loose, CalendarDate::from_date(day));
        assert_eq!(loose.to_date().unwrap(), day);
        if let Some(before) = previous {
            assert!(before < loose);
        }
        previous = Some(loose);
        day = day.checked_add_days(1).unwrap();
    }
    // 9999年の後も計算はできる（比べるだけに使う）。
    let far = CalendarDate::parse("9999-12-31").unwrap();
    assert_eq!(
        far.next_period_first_day(1).unwrap().to_string(),
        "10000-01-31"
    );
}

/// AC-06.2 日付の型の範囲（2200年まで）を超える計算は誤りになる。
#[test]
fn ac06_2_results_beyond_the_type_range_are_errors() {
    assert_eq!(
        next_period_first_day(Date::MAX, 1),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(
        grant_date(date("2024-04-01"), u32::MAX),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(expiry_date(Date::MAX), Err(CoreError::OutOfRange));
    assert!(five_day_window_end(date("2099-12-31")).is_ok());
}
