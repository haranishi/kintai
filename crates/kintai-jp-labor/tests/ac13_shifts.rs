//! 勤務の検査と、曖昧さの厳密な定義（`docs/spec/validation.md`）。
//!
//! - AC-13.3: 曖昧さのコード（休日・週・月の境界）が、定義どおりの勤務で付く。24:00ちょうどに終わる勤務は付かない。
//!   4つ目の `IncompleteWeek` は `ac03_incomplete_week.rs`。
//! - AC-13.4: 記録と `refused.dates` の並びが定義どおり。
//! - AC-13.5: 勤務の検査の各条件が不正な入力になる。接する勤務はよい。

mod common;

use common::{at, date, labor, policy, shift, shift_with_breaks};
use kintai_core::{Interval, Weekday};
use kintai_jp_labor::{
    Ambiguity, AmbiguityCode, AmbiguityMode, ShiftError, ShiftInput, ShiftSet, classify,
    find_ambiguities,
};

fn codes(
    legal: Weekday,
    week_start: Weekday,
    month_start_day: i64,
    s: ShiftInput,
) -> Vec<AmbiguityCode> {
    let mut p = policy(legal);
    p.week_start = Some(week_start);
    p.month_start_day = month_start_day;
    let valid = p.validate().unwrap();
    find_ambiguities(&valid, &ShiftSet::new(&[s]).unwrap(), None)
        .into_iter()
        .map(|a| a.code)
        .collect()
}

/// AC-13.3 3つのコードが、定義どおりの勤務で付く。
#[test]
fn ac13_3_codes_follow_the_definition() {
    use AmbiguityCode::{HolidayBoundary, MonthBoundary, WeekBoundary};
    use Weekday::{Monday, Sunday, Wednesday};
    // 土 → 日（日曜が法定休日。月曜起算なので週は同じ）: 休日だけ。
    let s = || shift("2026-10-10T23:00", "2026-10-11T01:00");
    assert_eq!(codes(Sunday, Monday, 1, s()), vec![HolidayBoundary]);
    // 日曜起算なら週もまたぐ。
    assert_eq!(
        codes(Sunday, Sunday, 1, s()),
        vec![HolidayBoundary, WeekBoundary]
    );
    // 法定休日が水曜なら、休日の境界は付かない。
    assert_eq!(codes(Wednesday, Sunday, 1, s()), vec![WeekBoundary]);
    // 日 → 月（始業の日が法定休日でも、D に法定休日の暦日があれば付く）。
    let sun_to_mon = shift("2026-10-11T23:00", "2026-10-12T01:00");
    assert_eq!(codes(Sunday, Sunday, 1, sun_to_mon), vec![HolidayBoundary]);
    // 月の期間の境界（起算日16: 10/15 → 10/16）。
    let mid_month = || shift("2026-10-15T23:00", "2026-10-16T01:00");
    assert_eq!(codes(Sunday, Sunday, 16, mid_month()), vec![MonthBoundary]);
    assert_eq!(
        codes(Sunday, Sunday, 1, mid_month()),
        Vec::<AmbiguityCode>::new()
    );
    // 暦日をまたいでも、どの境界にも当たらなければ記録はない。
    let plain = shift("2026-10-07T20:00", "2026-10-08T06:00");
    assert_eq!(codes(Sunday, Sunday, 1, plain), common::NO_CODES);
}

/// AC-13.3 24:00ちょうど（翌日の0:00）に終わる勤務はまたがない。1分過ぎればまたぐ。
#[test]
fn ac13_3_midnight_end_does_not_cross() {
    use Weekday::Sunday;
    let to_midnight = shift("2026-10-10T20:00", "2026-10-11T00:00");
    assert_eq!(codes(Sunday, Sunday, 1, to_midnight), common::NO_CODES);
    let past = shift("2026-10-10T20:00", "2026-10-11T00:01");
    assert_eq!(
        codes(Sunday, Sunday, 1, past),
        vec![AmbiguityCode::HolidayBoundary, AmbiguityCode::WeekBoundary]
    );
    // 法定休日の 0:00 から始まる勤務は、またがない（休日の暦日1つだけ）。
    let from_midnight = shift("2026-10-11T00:00", "2026-10-11T08:00");
    assert_eq!(codes(Sunday, Sunday, 1, from_midnight), common::NO_CODES);
}

/// AC-13.4 記録は（労働日、コード）の組ごとに1件、労働日の昇順、同じ労働日では表の順。
/// `refused.dates` は記録のある労働日の重複のない昇順。
#[test]
fn ac13_4_ordering_of_records_and_refused_dates() {
    let mut p = policy(Weekday::Sunday);
    p.ambiguity = Some(AmbiguityMode::UseDefault);
    let shifts = [
        // 入力の並びは時刻順でない。
        shift("2026-10-31T22:00", "2026-11-01T02:00"), // 休日・週・月
        shift("2026-10-10T00:30", "2026-10-10T08:00"), // 土 当日だけ
        shift("2026-10-10T22:00", "2026-10-11T02:00"), // 土→日: 休日・週（同じ労働日 10/10 の2つめの勤務）
        shift("2026-10-03T22:00", "2026-10-04T02:00"), // 土→日: 休日・週
    ];
    let valid = p.validate().unwrap();
    let set = ShiftSet::new(&shifts).unwrap();
    let expected = vec![
        Ambiguity {
            date: labor("2026-10-03"),
            code: AmbiguityCode::HolidayBoundary,
        },
        Ambiguity {
            date: labor("2026-10-03"),
            code: AmbiguityCode::WeekBoundary,
        },
        Ambiguity {
            date: labor("2026-10-10"),
            code: AmbiguityCode::HolidayBoundary,
        },
        Ambiguity {
            date: labor("2026-10-10"),
            code: AmbiguityCode::WeekBoundary,
        },
        Ambiguity {
            date: labor("2026-10-31"),
            code: AmbiguityCode::HolidayBoundary,
        },
        Ambiguity {
            date: labor("2026-10-31"),
            code: AmbiguityCode::WeekBoundary,
        },
        Ambiguity {
            date: labor("2026-10-31"),
            code: AmbiguityCode::MonthBoundary,
        },
    ];
    assert_eq!(find_ambiguities(&valid, &set, None), expected);
    assert_eq!(classify(&valid, &set, None).unwrap().ambiguities, expected);

    p.ambiguity = Some(AmbiguityMode::Reject);
    let valid = p.validate().unwrap();
    let refused = classify(&valid, &set, None).unwrap().refused.unwrap();
    assert_eq!(refused.code.code(), "AmbiguousBoundary");
    assert_eq!(
        refused.dates,
        vec![
            labor("2026-10-03"),
            labor("2026-10-10"),
            labor("2026-10-31")
        ]
    );
    assert_eq!(AmbiguityCode::HolidayBoundary.code(), "HolidayBoundary");
    assert_eq!(AmbiguityCode::WeekBoundary.code(), "WeekBoundary");
    assert_eq!(AmbiguityCode::MonthBoundary.code(), "MonthBoundary");
}

/// AC-13.4 同じ労働日の2つの勤務が同じコードに当たっても、記録は1件。
#[test]
fn ac13_4_one_record_per_labor_day_and_code() {
    let mut p = policy(Weekday::Wednesday);
    p.month_start_day = 16;
    p.ambiguity = Some(AmbiguityMode::UseDefault);
    let shifts = [
        shift("2026-10-15T00:00", "2026-10-15T01:00"),
        shift("2026-10-15T23:00", "2026-10-16T01:00"),
    ];
    let valid = p.validate().unwrap();
    let found = find_ambiguities(&valid, &ShiftSet::new(&shifts).unwrap(), None);
    assert_eq!(
        found,
        vec![Ambiguity {
            date: labor("2026-10-15"),
            code: AmbiguityCode::MonthBoundary
        }]
    );
}

fn error(shifts: &[ShiftInput]) -> ShiftError {
    ShiftSet::new(shifts).unwrap_err()
}

/// AC-13.5 勤務の検査の各条件が不正な入力になる。
#[test]
fn ac13_5_shift_checks() {
    // 開始が終了以上
    assert_eq!(
        error(&[shift("2026-10-05T09:00", "2026-10-05T09:00")]),
        ShiftError::StartNotBeforeEnd { shift: 0 }
    );
    assert_eq!(
        error(&[shift("2026-10-05T10:00", "2026-10-05T09:00")]),
        ShiftError::StartNotBeforeEnd { shift: 0 }
    );
    // 日時が入力の範囲（2000-01-01T00:00〜2099-12-31T23:59）の外。型は1900〜2200年を持てるので、
    // ライブラリを直接使うときも、ここで拒否する。2100-01-01T00:00 に終わる勤務も不可。
    let late = kintai_core::Instant::INPUT_MAX;
    let shifts_out_of_range = [
        ShiftInput {
            start: late.checked_add_minutes(-60).unwrap(),
            end: late.checked_add_minutes(1).unwrap(),
            breaks: Vec::new(),
        },
        ShiftInput {
            start: kintai_core::Instant::INPUT_MIN
                .checked_add_minutes(-60)
                .unwrap(),
            end: kintai_core::Instant::INPUT_MIN
                .checked_add_minutes(60)
                .unwrap(),
            breaks: Vec::new(),
        },
    ];
    for s in shifts_out_of_range {
        assert_eq!(error(&[s]), ShiftError::OutOfRange { shift: 0 });
    }
    assert!(
        ShiftSet::new(&[ShiftInput {
            start: late.checked_add_minutes(-60).unwrap(),
            end: late,
            breaks: Vec::new(),
        }])
        .is_ok()
    );
    // 勤務が1440分以上（1439分はよい）
    assert_eq!(
        error(&[shift("2026-10-05T09:00", "2026-10-06T09:00")]),
        ShiftError::TooLong { shift: 0 }
    );
    assert!(ShiftSet::new(&[shift("2026-10-05T09:00", "2026-10-06T08:59")]).is_ok());
    // 勤務どうしが重なる（1分でも）
    assert_eq!(
        error(&[
            shift("2026-10-05T13:00", "2026-10-05T18:00"),
            shift("2026-10-05T09:00", "2026-10-05T13:01"),
        ]),
        ShiftError::ShiftsOverlap {
            first: 1,
            second: 0
        }
    );
    // 休憩の開始が終了以上
    assert_eq!(
        error(&[shift_with_breaks(
            "2026-10-05T09:00",
            "2026-10-05T18:00",
            &[("2026-10-05T12:00", "2026-10-05T12:00")]
        )]),
        ShiftError::BreakStartNotBeforeEnd { shift: 0, rest: 0 }
    );
    // 休憩が勤務の内側にない（両端に接するのも不可）
    for (rest_start, rest_end) in [
        ("2026-10-05T09:00", "2026-10-05T10:00"),
        ("2026-10-05T17:00", "2026-10-05T18:00"),
        ("2026-10-05T08:00", "2026-10-05T10:00"),
        ("2026-10-05T17:00", "2026-10-05T19:00"),
        ("2026-10-05T19:00", "2026-10-05T20:00"),
    ] {
        assert_eq!(
            error(&[shift_with_breaks(
                "2026-10-05T09:00",
                "2026-10-05T18:00",
                &[
                    ("2026-10-05T12:00", "2026-10-05T12:30"),
                    (rest_start, rest_end)
                ]
            )]),
            ShiftError::BreakOutsideShift { shift: 0, rest: 1 },
            "{rest_start}〜{rest_end}"
        );
    }
    // 休憩どうしが重なる
    assert_eq!(
        error(&[shift_with_breaks(
            "2026-10-05T09:00",
            "2026-10-05T18:00",
            &[
                ("2026-10-05T12:00", "2026-10-05T13:00"),
                ("2026-10-05T12:59", "2026-10-05T13:30")
            ]
        )]),
        ShiftError::BreaksOverlap { shift: 0 }
    );
}

/// AC-13.5 接する勤務・接する休憩はよい。並びは始業の順になる。
#[test]
fn ac13_5_touching_is_allowed() {
    let set = ShiftSet::new(&[
        shift("2026-10-05T13:00", "2026-10-05T18:00"),
        shift_with_breaks(
            "2026-10-05T09:00",
            "2026-10-05T13:00",
            &[
                ("2026-10-05T11:30", "2026-10-05T12:00"),
                ("2026-10-05T11:00", "2026-10-05T11:30"),
            ],
        ),
    ])
    .unwrap();
    assert_eq!(set.len(), 2);
    assert!(!set.is_empty());
    let first = &set.shifts()[0];
    assert_eq!(first.span().start(), at("2026-10-05T09:00"));
    assert_eq!(first.labor_day(), date("2026-10-05"));
    assert_eq!(
        first.breaks(),
        &[
            Interval::new(at("2026-10-05T11:00"), at("2026-10-05T11:30")).unwrap(),
            Interval::new(at("2026-10-05T11:30"), at("2026-10-05T12:00")).unwrap(),
        ]
    );
    assert_eq!(
        first.work_intervals(),
        vec![
            Interval::new(at("2026-10-05T09:00"), at("2026-10-05T11:00")).unwrap(),
            Interval::new(at("2026-10-05T12:00"), at("2026-10-05T13:00")).unwrap(),
        ]
    );
    assert_eq!(first.work_minutes(), 180);
    assert!(ShiftSet::new(&[]).unwrap().is_empty());
}

/// 勤務の誤りの説明文は、入力の中身（日時）を含まず、番号だけを示す。
#[test]
fn shift_error_messages_have_only_indices() {
    let errors = [
        ShiftError::StartNotBeforeEnd { shift: 3 },
        ShiftError::TooLong { shift: 3 },
        ShiftError::OutOfRange { shift: 3 },
        ShiftError::BreakStartNotBeforeEnd { shift: 3, rest: 1 },
        ShiftError::BreakOutsideShift { shift: 3, rest: 1 },
        ShiftError::BreaksOverlap { shift: 3 },
        ShiftError::ShiftsOverlap {
            first: 3,
            second: 4,
        },
    ];
    for e in errors {
        let text = e.to_string();
        assert!(text.contains("shifts[3]"), "{text}");
        assert!(!text.contains("2026"), "{text}");
    }
}
