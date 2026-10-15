//! 区分の規則の小さな手書きの例（`docs/spec/classify.md`）。
//! 2026-10-04 は日曜。特に書かなければ、法定休日は日曜、週は日曜起算、所定480分、月の起算日1。

mod common;

use common::{day, labor, period, policy, run, shift, shift_with_breaks};
use kintai_core::Weekday;
use kintai_jp_labor::{
    Ambiguity, AmbiguityCode, AmbiguityMode, RefusalCode, ShiftInput, ShiftSet, classify,
};

fn full_week_mon_to_fri() -> Vec<ShiftInput> {
    ["05", "06", "07", "08", "09"]
        .iter()
        .map(|d| shift(&format!("2026-10-{d}T09:00"), &format!("2026-10-{d}T17:00")))
        .collect()
}

/// AC-03.1 休憩を除いた分だけを実労働として数える。
#[test]
fn ac03_1_breaks_are_not_work() {
    let p = policy(Weekday::Sunday);
    let one = run(
        &p,
        &[shift_with_breaks(
            "2026-10-05T09:00",
            "2026-10-05T18:00",
            &[("2026-10-05T12:00", "2026-10-05T13:00")],
        )],
    );
    let d = day(&one, "2026-10-05");
    assert_eq!((d.total, d.within, d.overtime), (480, 480, 0));

    let two = run(
        &p,
        &[shift_with_breaks(
            "2026-10-05T09:00",
            "2026-10-05T19:00",
            &[
                ("2026-10-05T15:00", "2026-10-05T15:15"),
                ("2026-10-05T12:00", "2026-10-05T12:45"),
            ],
        )],
    );
    let d = day(&two, "2026-10-05");
    assert_eq!((d.total, d.within, d.overtime), (540, 480, 60));

    // 接する休憩（12:00〜12:30 と 12:30〜13:00）は、合わせて60分の休憩。
    let touching = run(
        &p,
        &[shift_with_breaks(
            "2026-10-05T09:00",
            "2026-10-05T18:00",
            &[
                ("2026-10-05T12:00", "2026-10-05T12:30"),
                ("2026-10-05T12:30", "2026-10-05T13:00"),
            ],
        )],
    );
    assert_eq!(day(&touching, "2026-10-05").total, 480);
}

/// AC-03.2 1日の平日型の分のうち、先頭480分を超えた分（後ろの分）が時間外になる。480分ちょうど・481分。
#[test]
fn ac03_2_daily_480_boundary() {
    let p = policy(Weekday::Sunday);
    let exact = run(&p, &[shift("2026-10-05T14:00", "2026-10-05T22:00")]);
    let d = day(&exact, "2026-10-05");
    assert_eq!((d.total, d.within, d.overtime, d.night), (480, 480, 0, 0));

    // 481分目（22:00〜22:01）が時間外で、それは深夜。
    let one_more = run(&p, &[shift("2026-10-05T14:00", "2026-10-05T22:01")]);
    let d = day(&one_more, "2026-10-05");
    assert_eq!((d.total, d.within, d.overtime), (481, 480, 1));
    assert_eq!((d.night, d.night_within, d.night_overtime), (1, 0, 1));

    // 暦日をまたいでも、先頭480分（21:00〜5:00）が法定内。481分目は 5:00 で深夜でない。
    let overnight = run(&p, &[shift("2026-10-05T21:00", "2026-10-06T05:01")]);
    let d = day(&overnight, "2026-10-05");
    assert_eq!((d.total, d.within, d.overtime), (481, 480, 1));
    assert_eq!((d.night, d.night_within, d.night_overtime), (420, 420, 0));
}

/// AC-03.3 週の上限を超えた分は時間外になる。週の上限ちょうど。日の時間外と二重にしない。
#[test]
fn ac03_3_weekly_limit() {
    let p = policy(Weekday::Sunday);
    // 月〜金 480分ずつで、週の上限2400分ちょうど。時間外なし。
    let exact = run(&p, &full_week_mon_to_fri());
    assert_eq!(period(&exact, "2026-10-01").within, 2400);
    assert_eq!(period(&exact, "2026-10-01").overtime, 0);

    // 土曜の1時間は、週の上限を超えるので全部時間外。
    let mut shifts = full_week_mon_to_fri();
    shifts.push(shift("2026-10-10T09:00", "2026-10-10T10:00"));
    let over = run(&p, &shifts);
    let sat = day(&over, "2026-10-10");
    assert_eq!((sat.within, sat.overtime), (0, 60));

    // 金曜が420分なら、土曜は60分まで法定内（残り枠ちょうど）、61分目から時間外。
    let mut shifts = full_week_mon_to_fri();
    shifts[4] = shift("2026-10-09T09:00", "2026-10-09T16:00");
    shifts.push(shift("2026-10-10T09:00", "2026-10-10T10:01"));
    let sat_result = run(&p, &shifts);
    let sat = day(&sat_result, "2026-10-10");
    assert_eq!((sat.within, sat.overtime), (60, 1));
}

/// AC-03.3 日の時間外として数えた分は、週の法定内の累計に入れない（二重にしない）。
#[test]
fn ac03_3_daily_overtime_is_not_counted_twice() {
    let p = policy(Weekday::Sunday);
    let mut shifts = full_week_mon_to_fri();
    shifts[0] = shift("2026-10-05T09:00", "2026-10-05T19:00"); // 月 600分 = 480 + 時間外120
    shifts.push(shift("2026-10-10T09:00", "2026-10-10T10:00")); // 土 60分
    let result = run(&p, &shifts);
    assert_eq!(day(&result, "2026-10-05").overtime, 120);
    assert_eq!(day(&result, "2026-10-09").within, 480);
    assert_eq!(day(&result, "2026-10-10").overtime, 60);
    let october = period(&result, "2026-10-01");
    assert_eq!((october.within, october.overtime), (2400, 180));

    // 東京労働局の例と同じ形: 月 600、火〜木 480、金 300、土 300。
    let mut shifts = full_week_mon_to_fri();
    shifts[0] = shift("2026-10-05T09:00", "2026-10-05T19:00");
    shifts[4] = shift("2026-10-09T09:00", "2026-10-09T14:00");
    shifts.push(shift("2026-10-10T09:00", "2026-10-10T14:00"));
    let result = run(&p, &shifts);
    let sat = day(&result, "2026-10-10");
    assert_eq!((sat.within, sat.overtime), (180, 120));
    assert_eq!(period(&result, "2026-10-01").overtime, 240);
}

/// AC-03.4 週44時間は確認があるときだけ使え、1日の480分は変わらない。
#[test]
fn ac03_4_weekly_44_hours() {
    let mut p = policy(Weekday::Sunday);
    p.weekly_limit_minutes = Some(2640);
    p.special_44h_confirmed = Some(true);
    let mut shifts = full_week_mon_to_fri();
    shifts[0] = shift("2026-10-05T09:00", "2026-10-05T18:00"); // 月 540分
    shifts.push(shift("2026-10-10T09:00", "2026-10-10T14:00")); // 土 300分
    let result = run(&p, &shifts);
    let mon = day(&result, "2026-10-05");
    assert_eq!(
        (mon.within, mon.overtime),
        (480, 60),
        "日の480分は変わらない"
    );
    let sat = day(&result, "2026-10-10");
    assert_eq!(
        (sat.within, sat.overtime),
        (240, 60),
        "週の残り枠は2640−2400"
    );

    // 確認がなければ使えない（対応外）。
    p.special_44h_confirmed = Some(false);
    assert!(p.validate().is_err());
    p.special_44h_confirmed = None;
    assert!(p.validate().is_err());
}

/// AC-03.5 法定休日の労働は、8時間を超えても法定休日だけに数え、時間外に重ねない。週の枠も使わない。
#[test]
fn ac03_5_legal_holiday_is_only_holiday() {
    let p = policy(Weekday::Sunday);
    let mut shifts = vec![shift("2026-10-04T08:00", "2026-10-04T20:00")]; // 日 720分
    shifts.extend(full_week_mon_to_fri());
    let result = run(&p, &shifts);
    let sun = day(&result, "2026-10-04");
    assert_eq!(
        (sun.total, sun.within, sun.overtime, sun.over60, sun.holiday),
        (720, 0, 0, 0, 720)
    );
    assert_eq!(
        day(&result, "2026-10-09").within,
        480,
        "法定休日の分は週の枠を使わない"
    );
    assert_eq!(period(&result, "2026-10-01").overtime, 0);
}

/// AC-03.6 法定外休日の労働は、平日と同じ規則（日8時間、週の上限）で数える。
#[test]
fn ac03_6_non_legal_holiday_follows_weekday_rules() {
    let p = policy(Weekday::Sunday);
    let result = run(&p, &[shift("2026-10-10T09:00", "2026-10-10T19:00")]); // 土 600分
    let sat = day(&result, "2026-10-10");
    assert_eq!((sat.within, sat.overtime, sat.holiday), (480, 120, 0));

    // 法定休日を土曜にすると、同じ勤務が法定休日になる。
    let p = policy(Weekday::Saturday);
    let result = run(&p, &[shift("2026-10-10T09:00", "2026-10-10T19:00")]);
    let sat = day(&result, "2026-10-10");
    assert_eq!((sat.within, sat.overtime, sat.holiday), (0, 0, 600));
}

/// AC-03.7 深夜は 22:00 以上 5:00 未満で、区分と別に数える。22:00 ちょうどは入り、5:00 ちょうどは入らない。
#[test]
fn ac03_7_night_boundaries() {
    let p = policy(Weekday::Sunday);
    let cases = [
        ("2026-10-05T21:00", "2026-10-05T22:00", 0),
        ("2026-10-05T21:00", "2026-10-05T22:01", 1),
        ("2026-10-05T22:00", "2026-10-05T22:01", 1),
        ("2026-10-05T05:00", "2026-10-05T06:00", 0),
        ("2026-10-05T04:59", "2026-10-05T06:00", 1),
        ("2026-10-05T00:00", "2026-10-05T05:00", 300),
        ("2026-10-05T23:00", "2026-10-06T00:00", 60),
    ];
    for (start, end, night) in cases {
        let result = run(&p, &[shift(start, end)]);
        assert_eq!(result.days[0].night, night, "{start}〜{end}");
    }
    // 区分ごとの内数: 13:00〜23:00 は法定内 13〜21時、時間外 21〜23時。深夜は時間外の 22〜23時。
    let result = run(&p, &[shift("2026-10-05T13:00", "2026-10-05T23:00")]);
    let d = day(&result, "2026-10-05");
    assert_eq!(
        (d.night, d.night_within, d.night_overtime, d.night_holiday),
        (60, 0, 60, 0)
    );
    // 法定休日の深夜。
    let result = run(&p, &[shift("2026-10-04T20:00", "2026-10-05T00:00")]);
    let d = day(&result, "2026-10-04");
    assert_eq!((d.holiday, d.night, d.night_holiday), (240, 120, 120));
    // 深夜は合計に足さない。
    assert_eq!(d.total, 240);
}

/// AC-03.8 所定外法定内は、法定内から所定労働時間を引いた分（0未満にならない）。
#[test]
fn ac03_8_extra_within() {
    let mut p = policy(Weekday::Sunday);
    p.scheduled_minutes_per_day = 420;
    let result = run(
        &p,
        &[
            shift("2026-10-05T09:00", "2026-10-05T17:00"), // 480
            shift("2026-10-06T09:00", "2026-10-06T15:00"), // 360
            shift("2026-10-07T09:00", "2026-10-07T19:00"), // 600
        ],
    );
    assert_eq!(day(&result, "2026-10-05").extra_within, 60);
    assert_eq!(day(&result, "2026-10-06").extra_within, 0);
    assert_eq!(day(&result, "2026-10-07").extra_within, 60);
    p.scheduled_minutes_per_day = 0;
    let result = run(&p, &[shift("2026-10-05T09:00", "2026-10-05T17:00")]);
    assert_eq!(day(&result, "2026-10-05").extra_within, 480);
    p.scheduled_minutes_per_day = 480;
    let result = run(&p, &[shift("2026-10-05T09:00", "2026-10-05T19:00")]);
    assert_eq!(day(&result, "2026-10-05").extra_within, 0);
}

/// AC-03.9 暦日をまたぐ継続勤務は、日の枠を始業日で数える。翌日の別の勤務は別の労働日。
#[test]
fn ac03_9_overnight_shift_counts_on_start_day() {
    let p = policy(Weekday::Sunday);
    let result = run(
        &p,
        &[
            shift("2026-10-07T20:00", "2026-10-08T06:00"), // 水 20:00〜木 6:00 = 600分
            shift("2026-10-08T09:00", "2026-10-08T17:00"), // 木 480分
        ],
    );
    assert_eq!(result.days.len(), 2);
    let wed = day(&result, "2026-10-07");
    assert_eq!((wed.total, wed.within, wed.overtime), (600, 480, 120));
    assert_eq!(
        (wed.night, wed.night_within, wed.night_overtime),
        (420, 360, 60)
    );
    let thu = day(&result, "2026-10-08");
    assert_eq!((thu.total, thu.within, thu.overtime), (480, 480, 0));
}

/// AC-03.11 週は月をまたいで連続して数える（2026-09-27 日曜から 10-03 土曜）。
#[test]
fn ac03_11_week_continues_across_months() {
    let p = policy(Weekday::Sunday);
    let mut shifts: Vec<ShiftInput> = [
        "2026-09-28",
        "2026-09-29",
        "2026-09-30",
        "2026-10-01",
        "2026-10-02",
    ]
    .iter()
    .map(|d| shift(&format!("{d}T09:00"), &format!("{d}T17:00")))
    .collect();
    shifts.push(shift("2026-10-03T09:00", "2026-10-03T10:00"));
    let result = run(&p, &shifts);
    let sat = day(&result, "2026-10-03");
    assert_eq!((sat.within, sat.overtime), (0, 60));
    let september = period(&result, "2026-09-01");
    let october = period(&result, "2026-10-01");
    assert_eq!((september.within, september.overtime), (1440, 0));
    assert_eq!((october.within, october.overtime), (960, 60));
}

/// AC-03.12 休日の境界をまたぐ勤務: 既定で拒否し、`use_default` では記録を付けて計算する。
#[test]
fn ac03_12_holiday_boundary() {
    let mut p = policy(Weekday::Sunday);
    p.week_start = Some(Weekday::Monday); // 土と日を同じ週にして、休日の境界だけにする
    let night_shift = [shift("2026-10-10T22:00", "2026-10-11T02:00")];
    let refused = run(&p, &night_shift);
    assert_eq!(refused.days, common::NO_DAYS);
    assert_eq!(refused.periods, common::NO_PERIODS);
    assert_eq!(refused.ambiguities, common::NO_AMBIGUITIES);
    let refusal = refused.refused.unwrap();
    assert_eq!(refusal.code, RefusalCode::AmbiguousBoundary);
    assert_eq!(refusal.dates, vec![labor("2026-10-10")]);

    p.ambiguity = Some(AmbiguityMode::UseDefault);
    let result = run(&p, &night_shift);
    assert_eq!(
        result.ambiguities,
        vec![Ambiguity {
            date: labor("2026-10-10"),
            code: AmbiguityCode::HolidayBoundary
        }]
    );
    let sat = day(&result, "2026-10-10");
    assert_eq!((sat.total, sat.within, sat.holiday), (240, 120, 120));
    assert_eq!((sat.night_within, sat.night_holiday), (120, 120));
}

/// AC-03.12 週の境界・月の境界をまたぐ勤務。3つ全部に当たる勤務。
#[test]
fn ac03_12_week_and_month_boundaries() {
    // 週の境界だけ（法定休日を水曜にする）。
    let mut p = policy(Weekday::Wednesday);
    p.ambiguity = Some(AmbiguityMode::UseDefault);
    let result = run(&p, &[shift("2026-10-10T22:00", "2026-10-11T02:00")]);
    assert_eq!(
        result.ambiguities,
        vec![Ambiguity {
            date: labor("2026-10-10"),
            code: AmbiguityCode::WeekBoundary
        }]
    );
    assert_eq!(day(&result, "2026-10-10").within, 240);

    // 月の境界だけ（9/30 水曜 → 10/1 木曜）。期間は始業日の9月で数える。
    let result = run(&p.clone(), &[shift("2026-09-30T22:00", "2026-10-01T02:00")]);
    let mut q = policy(Weekday::Sunday);
    q.ambiguity = Some(AmbiguityMode::UseDefault);
    let result_sun = run(&q, &[shift("2026-09-30T22:00", "2026-10-01T02:00")]);
    assert_eq!(
        result_sun.ambiguities,
        vec![Ambiguity {
            date: labor("2026-09-30"),
            code: AmbiguityCode::MonthBoundary
        }]
    );
    assert_eq!(period(&result_sun, "2026-09-01").total, 240);
    assert_eq!(result_sun.periods.len(), 1);
    // 法定休日が水曜なら、9/30（水）を含むので休日の境界も付く。
    assert_eq!(
        result
            .ambiguities
            .iter()
            .map(|a| a.code)
            .collect::<Vec<_>>(),
        vec![AmbiguityCode::HolidayBoundary, AmbiguityCode::MonthBoundary]
    );

    // 10/31（土）→ 11/1（日）は、休日・週・月の3つ。並びは表の順。
    let result = run(&q, &[shift("2026-10-31T22:00", "2026-11-01T02:00")]);
    assert_eq!(
        result.ambiguities,
        vec![
            Ambiguity {
                date: labor("2026-10-31"),
                code: AmbiguityCode::HolidayBoundary
            },
            Ambiguity {
                date: labor("2026-10-31"),
                code: AmbiguityCode::WeekBoundary
            },
            Ambiguity {
                date: labor("2026-10-31"),
                code: AmbiguityCode::MonthBoundary
            },
        ]
    );
}

/// AC-03.12 24:00 ちょうど（翌日の 0:00）に終わる勤務はまたがない。
#[test]
fn ac03_12_ending_at_midnight_does_not_cross() {
    let p = policy(Weekday::Sunday); // reject
    let result = run(&p, &[shift("2026-10-10T20:00", "2026-10-11T00:00")]);
    assert_eq!(result.refused, None);
    assert_eq!(result.ambiguities, common::NO_AMBIGUITIES);
    let sat = day(&result, "2026-10-10");
    assert_eq!((sat.total, sat.holiday, sat.night), (240, 0, 120));
    // 1分でも 0:00 を過ぎれば、またぐ。
    let result = run(&p, &[shift("2026-10-10T20:00", "2026-10-11T00:01")]);
    assert_eq!(result.refused.unwrap().dates, vec![labor("2026-10-10")]);
}

/// AC-03.12 拒否のときは、曖昧でない勤務の分も出さない（黙って一部だけを出さない）。
#[test]
fn ac03_12_refusal_hides_all_days() {
    let p = policy(Weekday::Sunday);
    let shifts = [
        shift("2026-10-05T09:00", "2026-10-05T17:00"),
        shift("2026-10-10T22:00", "2026-10-11T02:00"),
        shift("2026-10-31T22:00", "2026-11-01T02:00"),
    ];
    let valid = p.validate().unwrap();
    let result = classify(&valid, &ShiftSet::new(&shifts).unwrap(), None).unwrap();
    assert_eq!(result.days, common::NO_DAYS);
    assert_eq!(result.periods, common::NO_PERIODS);
    assert_eq!(
        result.refused.unwrap().dates,
        vec![labor("2026-10-10"), labor("2026-10-31")]
    );
}
