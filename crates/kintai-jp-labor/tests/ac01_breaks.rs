//! 休憩の不足の警告（`docs/spec/punch.md`）。AC-01.6（360・361・480・481分の境界）、AC-01.7（自動で足し引きしない）。
//! 根拠の定数（労働基準法第34条第1項）が、取得した条文と合うことも見る。

mod common;

use common::{at, policy, run, shift, shift_with_breaks};
use kintai_core::Weekday;
use kintai_core::punch::{EmployeeId, PairingOutcome, Punch, PunchKind, SecondsPolicy, pair};
use kintai_jp_labor::breaks::{
    BREAK_45_OVER_MINUTES, BREAK_60_OVER_MINUTES, BreakShortage, LSA_34_1, break_shortages,
    required_break_minutes,
};
use kintai_jp_labor::{ShiftInput, ShiftSet};
use proptest::prelude::*;

fn shortages(inputs: &[ShiftInput]) -> Vec<BreakShortage> {
    break_shortages(&ShiftSet::new(inputs).unwrap()).unwrap()
}

/// （労働日, 実労働, 休憩, 必要, 不足）の組。
fn summary(found: &[BreakShortage]) -> Vec<(String, i64, i64, i64, i64)> {
    found
        .iter()
        .map(|s| {
            (
                s.date.to_string(),
                s.work_minutes,
                s.break_minutes,
                s.required_minutes,
                s.shortage_minutes,
            )
        })
        .collect()
}

/// AC-01.6: 必要な休憩は、360分以下で0、361〜480で45、481以上で60（60に45を足さない）。
#[test]
fn ac01_6_required_minutes_at_the_boundaries() {
    assert_eq!((BREAK_45_OVER_MINUTES, BREAK_60_OVER_MINUTES), (360, 480));
    for (work, required) in [
        (0, 0),
        (1, 0),
        (360, 0),
        (361, 45),
        (480, 45),
        (481, 60),
        (1439, 60),
    ] {
        assert_eq!(required_break_minutes(work), required, "{work}");
    }
}

/// AC-01.6: 休憩なしの勤務で、360分は警告なし、361分は45分の不足、480分は45分、481分は60分。
#[test]
fn ac01_6_shortage_at_360_361_480_481() {
    for (end, expected) in [
        ("2026-10-07T15:00", None),
        ("2026-10-07T15:01", Some((361, 45))),
        ("2026-10-07T17:00", Some((480, 45))),
        ("2026-10-07T17:01", Some((481, 60))),
    ] {
        let found = shortages(&[shift("2026-10-07T09:00", end)]);
        let got = found.first().map(|s| (s.work_minutes, s.shortage_minutes));
        assert_eq!(got, expected, "{end}");
        if let Some(s) = found.first() {
            assert_eq!(s.break_minutes, 0);
            assert_eq!(s.required_minutes, s.shortage_minutes);
        }
    }
}

/// AC-01.6: 実労働8時間ちょうどに45分の休憩は足りる。8時間1分に45分は、60分に15分足りない（105分ではない）。
#[test]
fn ac01_6_eight_hours_and_one_minute() {
    let exact = shift_with_breaks(
        "2026-10-07T09:00",
        "2026-10-07T17:45",
        &[("2026-10-07T12:00", "2026-10-07T12:45")],
    );
    assert_eq!(shortages(&[exact]), Vec::new());
    let one_more = shift_with_breaks(
        "2026-10-07T09:00",
        "2026-10-07T17:46",
        &[("2026-10-07T12:00", "2026-10-07T12:45")],
    );
    assert_eq!(
        summary(&shortages(&[one_more])),
        [("2026-10-07".to_string(), 481, 45, 60, 15)]
    );
    // 6時間1分に休憩44分は1分の不足、45分なら足りる。
    let short = shift_with_breaks(
        "2026-10-07T09:00",
        "2026-10-07T15:45",
        &[("2026-10-07T12:00", "2026-10-07T12:44")],
    );
    assert_eq!(
        summary(&shortages(&[short])),
        [("2026-10-07".to_string(), 361, 44, 45, 1)]
    );
}

/// AC-01.6: 同じ労働日の勤務と勤務の間の空きは、休憩に数える。別の労働日との空きは数えない。
#[test]
fn ac01_6_gap_between_shifts_of_the_same_labor_day() {
    let enough = [
        shift("2026-10-07T09:00", "2026-10-07T13:00"),
        shift("2026-10-07T14:00", "2026-10-07T18:30"),
    ];
    assert_eq!(shortages(&enough), Vec::new());
    let short = [
        shift("2026-10-07T09:00", "2026-10-07T13:00"),
        shift("2026-10-07T13:30", "2026-10-07T18:00"),
    ];
    assert_eq!(
        summary(&shortages(&short)),
        [("2026-10-07".to_string(), 510, 30, 60, 30)]
    );
    // 接する勤務は空きが0分。
    let touching = [
        shift("2026-10-07T09:00", "2026-10-07T13:00"),
        shift("2026-10-07T13:00", "2026-10-07T17:00"),
    ];
    assert_eq!(
        summary(&shortages(&touching)),
        [("2026-10-07".to_string(), 480, 0, 45, 45)]
    );
    // 日をまたぐ勤務は始業の日の労働日。翌日に始まる勤務との空きは数えない。
    let across = [
        shift("2026-10-07T20:00", "2026-10-07T23:59"),
        shift("2026-10-08T00:30", "2026-10-08T08:00"),
    ];
    assert_eq!(
        summary(&shortages(&across)),
        [("2026-10-08".to_string(), 450, 0, 45, 45)]
    );
}

/// AC-01.6: 夜勤の休憩の不足は、始業の日（労働日）の警告になる。日付順に並ぶ。
#[test]
fn ac01_6_night_shift_belongs_to_the_start_day() {
    let found = shortages(&[
        shift("2026-10-09T09:00", "2026-10-09T18:00"),
        shift("2026-10-07T22:00", "2026-10-08T07:00"),
    ]);
    assert_eq!(
        summary(&found),
        [
            ("2026-10-07".to_string(), 540, 0, 60, 60),
            ("2026-10-09".to_string(), 540, 0, 60, 60),
        ]
    );
}

/// AC-01.7: 休憩を自動で足し引きしない。休憩のない9時間の勤務は、実労働540分のまま区分され、警告だけが出る。
#[test]
fn ac01_7_no_automatic_deduction() {
    let policy = policy(Weekday::Sunday);
    let inputs = [shift("2026-10-07T09:00", "2026-10-07T18:00")];
    let before = run(&policy, &inputs);
    let shifts = ShiftSet::new(&inputs).unwrap();
    let found = break_shortages(&shifts).unwrap();
    assert_eq!(
        summary(&found),
        [("2026-10-07".to_string(), 540, 0, 60, 60)]
    );
    // 勤務も区分の結果も変わらない（実労働は540分で、60分を引いた480分にしない）。
    assert_eq!(shifts, ShiftSet::new(&inputs).unwrap());
    let after = run(&policy, &inputs);
    assert_eq!(before, after);
    assert_eq!(
        (
            after.days[0].total,
            after.days[0].within,
            after.days[0].overtime
        ),
        (540, 480, 60)
    );
}

/// AC-01.7: 警告のコードと文。文は根拠の条文を示す（入力の中身は日付と分だけ）。
#[test]
fn ac01_7_warning_code_and_message() {
    assert_eq!(BreakShortage::CODE, "BreakShortage");
    let found = shortages(&[shift("2026-10-07T09:00", "2026-10-07T18:00")]);
    assert_eq!(
        found[0].to_string(),
        "2026-10-07: 実労働540分に対して休憩が0分です（必要60分、60分不足。根拠: 労働基準法第34条第1項）"
    );
}

/// AC-01.6: 打刻から対にした勤務でも同じ（取込 → 対 → 勤務 → 休憩の不足）。
#[test]
fn ac01_6_from_punches() {
    let employee = EmployeeId::new("E001").unwrap();
    let punch = |time: &str, kind| Punch {
        employee: employee.clone(),
        time: time.parse().unwrap(),
        kind,
    };
    let punches = [
        punch("2026-10-07 09:00:40", PunchKind::In),
        punch("2026-10-07 12:00:20", PunchKind::BreakStart),
        punch("2026-10-07 12:44:50", PunchKind::BreakEnd),
        punch("2026-10-07 17:45:01", PunchKind::Out),
    ];
    let result = pair(&punches, SecondsPolicy::WorkerFavorable).unwrap();
    let PairingOutcome::Paired(paired) = &result[0].outcome else {
        panic!("異常がある: {result:?}");
    };
    let inputs: Vec<ShiftInput> = paired
        .iter()
        .map(|p| ShiftInput {
            start: p.span().start(),
            end: p.span().end(),
            breaks: p.breaks().iter().map(|b| (b.start(), b.end())).collect(),
        })
        .collect();
    // 丸め: 始業 9:00、休憩 12:01〜12:44（43分）、終業 17:46。実労働 526−43＝483分 → 60分の休憩が要る。
    assert_eq!(
        summary(&shortages(&inputs)),
        [("2026-10-07".to_string(), 483, 43, 60, 17)]
    );
}

/// 根拠の定数（労働基準法第34条第1項）が、取得した条文の記録に合う。
#[test]
fn provenance_of_article_34() {
    let statutes = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/legal/sources/statutes-2026-10-15.txt"
    ))
    .unwrap();
    assert_eq!(LSA_34_1.citation(), "労働基準法第34条第1項");
    assert_eq!(LSA_34_1.confirmed_on, "2026-10-15");
    assert!(statutes.contains("第三十四条 （休憩）"));
    assert!(statutes.contains(
        "[1項] 使用者は、労働時間が六時間を超える場合においては少くとも四十五分、八時間を超える場合においては少くとも一時間の休憩時間を労働時間の途中に与えなければならない。"
    ));
}

/// 勤務の作り方（単位は分）: 前の勤務からの空き、（実労働, 休憩）の並び、最後の実労働。
fn arbitrary_shifts() -> impl Strategy<Value = Vec<ShiftInput>> {
    prop::collection::vec(
        (
            0_i64..900,
            prop::collection::vec((1_i64..300, 1_i64..90), 0..=2),
            1_i64..300,
        ),
        1..8,
    )
    .prop_map(|plans| {
        let mut cursor = at("2026-10-05T00:00").minutes();
        let mut shifts = Vec::new();
        for (gap, segments, last) in plans {
            cursor += gap;
            let start = cursor;
            let mut breaks = Vec::new();
            for (work, rest) in segments {
                cursor += work;
                breaks.push((cursor, cursor + rest));
                cursor += rest;
            }
            cursor += last;
            let instant = |m: i64| kintai_core::Instant::from_minutes(m).unwrap();
            shifts.push(ShiftInput {
                start: instant(start),
                end: instant(cursor),
                breaks: breaks
                    .into_iter()
                    .map(|(s, e)| (instant(s), instant(e)))
                    .collect(),
            });
        }
        shifts
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// AC-01.6・01.7: 労働日ごとに数え直した実労働と休憩（中の休憩＋同じ労働日の勤務の間の空き）から、
    /// 必要な休憩を引いた不足が、警告と一致する（不足のない日は警告がない）。
    #[test]
    fn ac01_6_shortage_matches_a_recount(inputs in arbitrary_shifts()) {
        let found = shortages(&inputs);
        let mut days = std::collections::BTreeMap::new();
        let mut sorted: Vec<&ShiftInput> = inputs.iter().collect();
        sorted.sort_by_key(|s| s.start);
        let mut previous: Option<&ShiftInput> = None;
        for input in sorted {
            let labor_day = input.start.date();
            let rest: i64 = input.breaks.iter().map(|(s, e)| e.minutes() - s.minutes()).sum();
            let span = input.end.minutes() - input.start.minutes();
            let entry = days.entry(labor_day).or_insert((0_i64, 0_i64));
            entry.0 += span - rest;
            entry.1 += rest;
            if let Some(before) = previous && before.start.date() == labor_day {
                entry.1 += input.start.minutes() - before.end.minutes();
            }
            previous = Some(input);
        }
        let expected: Vec<(String, i64, i64, i64, i64)> = days
            .into_iter()
            .filter_map(|(day, (work, rest))| {
                let required = if work > 480 { 60 } else if work > 360 { 45 } else { 0 };
                (rest < required).then(|| (day.to_string(), work, rest, required, required - rest))
            })
            .collect();
        prop_assert_eq!(summary(&found), expected);
        prop_assert!(found.iter().all(|s| s.shortage_minutes > 0 && s.work_minutes > 360));
        // 日付順。
        let dates: Vec<kintai_core::Date> = found.iter().map(|s| s.date).collect();
        let mut in_order = dates.clone();
        in_order.sort();
        prop_assert_eq!(dates, in_order);
    }
}
