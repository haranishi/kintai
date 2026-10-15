//! 打刻のそろっていない週（`data_from` と `IncompleteWeek`。`docs/spec/classify.md` の境界、
//! `docs/spec/validation.md` の曖昧さの定義）。
//!
//! - AC-03.15: `data_from` が週の起算日より後のとき `IncompleteWeek` になり、既定で拒否、
//!   `use_default` で記録つきの計算になる。`data_from` が無いか、週の起算日以前なら付かない。
//! - AC-03.16: 週48時間の例で、前月の日を含む週の時間外480分が、`data_from` があれば出て、
//!   月の分だけを渡すと拒否される（仕様が名指す数値例 `derived-incomplete-week-*` は、この周の時点で
//!   数値例のファイルにないので、同じ形の手書きの例で確かめる）。
//! - AC-13.3: 曖昧さの4つ目のコード `IncompleteWeek` が、定義どおりに付く（日付は週の起算日）。
//! - AC-13.4: 記録と `refused.dates` の並び（日付の昇順、同じ日付ではコードの順）。

mod common;

use common::{date, day, labor, period, policy, run_from, shift};
use kintai_core::Weekday;
use kintai_jp_labor::{Ambiguity, AmbiguityCode, AmbiguityMode, RecordDate, RefusalCode};

/// 2026-09-27（日）起算の週の、月曜（9/28）から土曜（10/3）までの 9:00〜17:00。
fn week_of_48_hours() -> Vec<kintai_jp_labor::ShiftInput> {
    [
        "2026-09-28",
        "2026-09-29",
        "2026-09-30",
        "2026-10-01",
        "2026-10-02",
        "2026-10-03",
    ]
    .iter()
    .map(|d| shift(&format!("{d}T09:00"), &format!("{d}T17:00")))
    .collect()
}

fn week_start(text: &str) -> RecordDate {
    RecordDate::WeekStart(date(text).week(Weekday::Sunday))
}

/// AC-03.16 前月の日を含む週を全部渡し、`data_from` を週の起算日にすると、時間外480分が出る。
#[test]
fn ac03_16_full_week_with_data_from_shows_overtime() {
    let p = policy(Weekday::Sunday);
    let result = run_from(&p, &week_of_48_hours(), Some("2026-09-27"));
    assert_eq!(result.refused, None);
    let sat = day(&result, "2026-10-03");
    assert_eq!((sat.within, sat.overtime), (0, 480));
    assert_eq!(period(&result, "2026-09-01").within, 1440);
    let october = period(&result, "2026-10-01");
    assert_eq!((october.within, october.overtime), (960, 480));
}

/// AC-03.16 月の分だけ（10/1〜10/3）を `data_from` = 10/1 で渡すと、週が欠けているので拒否する。
#[test]
fn ac03_16_month_only_input_is_refused() {
    let p = policy(Weekday::Sunday);
    let october_only: Vec<_> = week_of_48_hours().into_iter().skip(3).collect();
    let result = run_from(&p, &october_only, Some("2026-10-01"));
    let refusal = result.refused.unwrap();
    assert_eq!(refusal.code, RefusalCode::AmbiguousBoundary);
    assert_eq!(refusal.dates, vec![week_start("2026-09-27")]);
    assert_eq!(refusal.dates[0].to_string(), "2026-09-27");
    assert_eq!(result.days, common::NO_DAYS);
    // data_from がなければ、入力がすべてだとみなして計算する（週の超過は出ない）。
    let unchecked = run_from(&p, &october_only, None);
    assert_eq!(period(&unchecked, "2026-10-01").overtime, 0);
}

/// AC-03.15 `use_default` では、記録を付けて計算する。
#[test]
fn ac03_15_use_default_records_and_computes() {
    let mut p = policy(Weekday::Sunday);
    p.ambiguity = Some(AmbiguityMode::UseDefault);
    let october_only: Vec<_> = week_of_48_hours().into_iter().skip(3).collect();
    let result = run_from(&p, &october_only, Some("2026-10-01"));
    assert_eq!(result.refused, None);
    assert_eq!(
        result.ambiguities,
        vec![Ambiguity {
            date: week_start("2026-09-27"),
            code: AmbiguityCode::IncompleteWeek,
        }]
    );
    assert_eq!(period(&result, "2026-10-01").within, 1440);
}

/// AC-03.15 `data_from` が無い、起算日と同じ、起算日より前なら付かない。起算日より後なら付く。
#[test]
fn ac03_15_only_when_data_from_is_after_week_start() {
    let p = policy(Weekday::Sunday);
    let shifts = [shift("2026-10-07T09:00", "2026-10-07T17:00")]; // 水。週は 10/4（日）から
    for data_from in [
        None,
        Some("2026-10-04"),
        Some("2026-09-01"),
        Some("2026-10-03"),
    ] {
        let result = run_from(&p, &shifts, data_from);
        assert_eq!(result.refused, None, "{data_from:?}");
    }
    for data_from in ["2026-10-05", "2026-10-07", "2026-10-10", "2026-12-01"] {
        let result = run_from(&p, &shifts, Some(data_from));
        assert_eq!(
            result.refused.unwrap().dates,
            vec![week_start("2026-10-04")],
            "{data_from}"
        );
    }
}

/// AC-13.3 週の起算日は方針の `week_start` による。勤務のない週には付かない。
#[test]
fn ac13_3_incomplete_week_follows_week_start() {
    let mut p = policy(Weekday::Sunday);
    p.week_start = Some(Weekday::Wednesday);
    p.ambiguity = Some(AmbiguityMode::UseDefault);
    // 10/7 は水曜。水曜起算の週は 10/7 から。data_from = 10/7 なら欠けていない。
    let shifts = [
        shift("2026-10-07T09:00", "2026-10-07T17:00"),
        shift("2026-10-13T09:00", "2026-10-13T17:00"), // 火曜。週は 10/7 から
    ];
    let complete = run_from(&p, &shifts, Some("2026-10-07"));
    assert_eq!(complete.ambiguities, common::NO_AMBIGUITIES);
    // data_from = 10/8 なら、10/7 起算の週（勤務あり）だけが欠けている。
    let incomplete = run_from(&p, &shifts, Some("2026-10-08"));
    assert_eq!(
        incomplete.ambiguities,
        vec![Ambiguity {
            date: RecordDate::WeekStart(date("2026-10-07").week(Weekday::Wednesday)),
            code: AmbiguityCode::IncompleteWeek,
        }]
    );
    // 勤務が10/14 以降だけなら、10/7 起算の週は勤務がないので付かない。
    let later = [shift("2026-10-14T09:00", "2026-10-14T17:00")];
    assert_eq!(
        run_from(&p, &later, Some("2026-10-08")).ambiguities,
        common::NO_AMBIGUITIES
    );
}

/// AC-13.3 週の起算日が 2000-01-01 より前でも記録できる（日付は 1999-12-26）。
#[test]
fn ac13_3_week_start_before_2000() {
    let p = policy(Weekday::Sunday);
    let shifts = [shift("2000-01-01T09:00", "2000-01-01T17:00")]; // 土曜
    let result = run_from(&p, &shifts, Some("2000-01-01"));
    let refusal = result.refused.unwrap();
    assert_eq!(refusal.dates.len(), 1);
    assert_eq!(refusal.dates[0].to_string(), "1999-12-26");
    assert_eq!(
        refusal.dates[0].day_number(),
        date("2000-01-01").day_number() - 6
    );
}

/// AC-13.4 同じ日付では表の順（休日の境界 → 打刻のそろっていない週）。`refused.dates` は重複なし。
#[test]
fn ac13_4_same_date_is_ordered_by_code() {
    let mut p = policy(Weekday::Sunday);
    p.ambiguity = Some(AmbiguityMode::UseDefault);
    // 日 10/4 22:00 → 月 10/5 2:00。労働日 10/4 は法定休日の暦日をまたぐ。週の起算日も 10/4。
    let shifts = [
        shift("2026-10-08T09:00", "2026-10-08T17:00"),
        shift("2026-10-04T22:00", "2026-10-05T02:00"),
    ];
    let result = run_from(&p, &shifts, Some("2026-10-05"));
    assert_eq!(
        result.ambiguities,
        vec![
            Ambiguity {
                date: labor("2026-10-04"),
                code: AmbiguityCode::HolidayBoundary,
            },
            Ambiguity {
                date: week_start("2026-10-04"),
                code: AmbiguityCode::IncompleteWeek,
            },
        ]
    );
    p.ambiguity = Some(AmbiguityMode::Reject);
    let refused = run_from(&p, &shifts, Some("2026-10-05")).refused.unwrap();
    let dates: Vec<String> = refused.dates.iter().map(ToString::to_string).collect();
    assert_eq!(dates, vec!["2026-10-04"]);
}

/// AC-13.4 週の起算日の記録と、後の労働日の記録が、日付の昇順に並ぶ。
#[test]
fn ac13_4_week_start_sorts_before_later_labor_days() {
    let mut p = policy(Weekday::Sunday);
    p.ambiguity = Some(AmbiguityMode::UseDefault);
    let shifts = [shift("2026-10-10T22:00", "2026-10-11T02:00")]; // 土 → 日
    let result = run_from(&p, &shifts, Some("2026-10-06"));
    let codes: Vec<(String, &str)> = result
        .ambiguities
        .iter()
        .map(|a| (a.date.to_string(), a.code.code()))
        .collect();
    assert_eq!(
        codes,
        vec![
            ("2026-10-04".to_owned(), "IncompleteWeek"),
            ("2026-10-10".to_owned(), "HolidayBoundary"),
            ("2026-10-10".to_owned(), "WeekBoundary"),
        ]
    );
    assert_eq!(
        result.ambiguities[0].date.day_number() + 6,
        labor("2026-10-10").day_number()
    );
}
