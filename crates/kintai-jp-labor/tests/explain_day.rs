//! 区間ごとの区分と割増の下限の説明（`explain`）の試験。
//!
//! 正解は、公的な数値例（`vectors/official-examples.json`）の出典に書かれた率と区間:
//! 厚生労働省「時間外・休日労働と割増賃金」例1（17〜18時は1.00、18〜22時は1.25、22〜翌5時は1.50）と
//! 例2（法定休日の実働12時間は1.35、22〜24時は1.60）、「改正労働基準法」p.3（27日の時間外の3時間目から60時間超）。

mod common;

use common::{date, policy, policy_from_json, shift, shift_with_breaks, shifts_from_json, vectors};
use kintai_core::{Date, Weekday};
use kintai_jp_labor::Provenance;
use kintai_jp_labor::explain::{
    Category, DayExplanation, ExplainError, ExplainNote, LSA_37_1_PROVISO, REG_20_1, REG_20_2,
    Segment, explain_day, over60_rate_applies, premium_floor,
};
use kintai_jp_labor::report::{Notice, ReviewStatus};
use kintai_jp_labor::{
    AmbiguityCode, AmbiguityMode, Classification, ShiftInput, ShiftSet, ValidPolicy, classify,
};

/// 区間を（開始, 終了, 区分, 深夜, 率）の組にする。
fn rows(explanation: &DayExplanation) -> Vec<(String, String, Category, bool, Option<u32>)> {
    explanation
        .segments
        .iter()
        .map(|s| {
            (
                s.interval.start().to_string(),
                s.interval.end().to_string(),
                s.category,
                s.night,
                s.premium.percent,
            )
        })
        .collect()
}

fn row(
    start: &str,
    end: &str,
    category: Category,
    night: bool,
    percent: Option<u32>,
) -> (String, String, Category, bool, Option<u32>) {
    (start.to_string(), end.to_string(), category, night, percent)
}

struct Case {
    policy: ValidPolicy,
    shifts: ShiftSet,
    result: Classification,
}

fn case(policy: ValidPolicy, inputs: &[ShiftInput], data_from: Option<Date>) -> Case {
    let shifts = ShiftSet::new(inputs).unwrap();
    let result = classify(&policy, &shifts, data_from).unwrap();
    Case {
        policy,
        shifts,
        result,
    }
}

fn official(id: &str) -> Case {
    let vector = vectors()["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap()
        .clone();
    let input = &vector["case"];
    case(
        policy_from_json(&input["policy"]).validate().unwrap(),
        &shifts_from_json(&input["shifts"]),
        input["data_from"].as_str().map(date),
    )
}

fn explain(case: &Case, day: &str) -> DayExplanation {
    explain_day(&case.policy, &case.shifts, &case.result, date(day)).unwrap()
}

/// 例1: 所定9〜17時、翌5時まで、休憩1時間（法定内480分のうち所定外法定内60分、時間外660分、深夜420分）。
#[test]
fn official_example_1_overtime_into_night() {
    let explanation = explain(&official("mhlw-check-roudou-ex1"), "2026-10-07");
    assert_eq!(
        rows(&explanation),
        [
            row(
                "2026-10-07T09:00",
                "2026-10-07T12:00",
                Category::Within,
                false,
                Some(0)
            ),
            row(
                "2026-10-07T13:00",
                "2026-10-07T17:00",
                Category::Within,
                false,
                Some(0)
            ),
            row(
                "2026-10-07T17:00",
                "2026-10-07T18:00",
                Category::ExtraWithin,
                false,
                Some(0)
            ),
            row(
                "2026-10-07T18:00",
                "2026-10-07T22:00",
                Category::Overtime,
                false,
                Some(25)
            ),
            row(
                "2026-10-07T22:00",
                "2026-10-08T05:00",
                Category::Overtime,
                true,
                Some(50)
            ),
        ]
    );
    assert_eq!(explanation.notes, [ExplainNote::ExtraWithinIsContractual]);
    let night = &explanation.segments[4];
    assert_eq!(night.minutes(), 420);
    let citations: Vec<String> = night
        .premium
        .basis
        .iter()
        .map(Provenance::citation)
        .collect();
    assert_eq!(
        citations,
        [
            "労働基準法第37条第1項",
            "労働基準法第三十七条第一項の時間外及び休日の割増賃金に係る率の最低限度を定める政令（本則）",
            "労働基準法第37条第4項",
            "労働基準法施行規則第20条第1項",
        ]
    );
}

/// 例2: 法定休日の9〜24時、休憩1時間。実働12時間は35%、22〜24時は60%。
#[test]
fn official_example_2_legal_holiday() {
    let explanation = explain(&official("mhlw-check-roudou-ex2"), "2026-10-04");
    assert_eq!(
        rows(&explanation),
        [
            row(
                "2026-10-04T09:00",
                "2026-10-04T12:00",
                Category::Holiday,
                false,
                Some(35)
            ),
            row(
                "2026-10-04T13:00",
                "2026-10-04T22:00",
                Category::Holiday,
                false,
                Some(35)
            ),
            row(
                "2026-10-04T22:00",
                "2026-10-05T00:00",
                Category::Holiday,
                true,
                Some(60)
            ),
        ]
    );
    assert_eq!(explanation.notes, Vec::new());
    let total: i64 = explanation.segments.iter().map(Segment::minutes).sum();
    assert_eq!(total, 840);
}

/// 月60時間超の例: 27日の時間外の3時間目（20時）から60時間超で、2023-04-01 以降なので50%。
#[test]
fn official_sixty_hour_example_on_the_27th() {
    let case = official("mhlw-60h-leaflet-31day-month");
    let explanation = explain(&case, "2026-07-27");
    assert_eq!(
        rows(&explanation),
        [
            row(
                "2026-07-27T09:00",
                "2026-07-27T12:00",
                Category::Within,
                false,
                Some(0)
            ),
            row(
                "2026-07-27T13:00",
                "2026-07-27T18:00",
                Category::Within,
                false,
                Some(0)
            ),
            row(
                "2026-07-27T18:00",
                "2026-07-27T20:00",
                Category::Overtime,
                false,
                Some(25)
            ),
            row(
                "2026-07-27T20:00",
                "2026-07-27T21:00",
                Category::Over60,
                false,
                Some(50)
            ),
        ]
    );
    // 前の日（24日）は、まだ60時間以下。
    let before = explain(&case, "2026-07-24");
    assert!(
        before
            .segments
            .iter()
            .all(|s| s.category != Category::Over60)
    );
}

/// 平日を `count` 日（月曜〜金曜）、8:00〜20:00（休憩なし、時間外240分）で働く。最後の日だけ 14:00〜翌2:00。
fn heavy_weekdays(first: &str, count: usize) -> Vec<ShiftInput> {
    let mut day = date(first);
    let mut inputs = Vec::new();
    while inputs.len() < count {
        if !matches!(day.weekday(), Weekday::Saturday | Weekday::Sunday) {
            let next = day.checked_add_days(1).unwrap();
            if inputs.len() + 1 == count {
                inputs.push(shift(&format!("{day}T14:00"), &format!("{next}T02:00")));
            } else {
                inputs.push(shift(&format!("{day}T08:00"), &format!("{day}T20:00")));
            }
        }
        day = day.checked_add_days(1).unwrap();
    }
    inputs
}

/// 2023-04-01 より前の日付では、60時間超の率を出さず注意を付ける。以後は50%（深夜は75%）を出す。
#[test]
fn over60_rate_is_shown_from_april_2023() {
    // 2023年3月: 16日目の平日（3/22）の時間外は、すべて60時間超（15日×240分＝3600分の後）。
    let march = case(
        policy(Weekday::Sunday).validate().unwrap(),
        &heavy_weekdays("2023-03-01", 16),
        None,
    );
    let explanation = explain(&march, "2023-03-22");
    assert_eq!(
        rows(&explanation),
        [
            row(
                "2023-03-22T14:00",
                "2023-03-22T22:00",
                Category::Within,
                false,
                Some(0)
            ),
            row(
                "2023-03-22T22:00",
                "2023-03-23T02:00",
                Category::Over60,
                true,
                None
            ),
        ]
    );
    assert_eq!(explanation.notes, [ExplainNote::Over60RateNotShown]);
    assert_eq!(ExplainNote::Over60RateNotShown.code(), "Over60RateNotShown");

    // 2023年4月: 16日目の平日（4/24）。
    let april = case(
        policy(Weekday::Sunday).validate().unwrap(),
        &heavy_weekdays("2023-04-01", 16),
        None,
    );
    let explanation = explain(&april, "2023-04-24");
    assert_eq!(
        rows(&explanation),
        [
            row(
                "2023-04-24T14:00",
                "2023-04-24T22:00",
                Category::Within,
                false,
                Some(0)
            ),
            row(
                "2023-04-24T22:00",
                "2023-04-25T02:00",
                Category::Over60,
                true,
                Some(75)
            ),
        ]
    );
    assert_eq!(explanation.notes, Vec::new());
    // 境界の日付。
    assert!(!over60_rate_applies(date("2023-03-31")));
    assert!(over60_rate_applies(date("2023-04-01")));
}

/// 法定内の深夜は25%（日をまたいでも、続いていれば1つの区間）。
#[test]
fn night_within_is_25_percent() {
    let night = case(
        policy(Weekday::Sunday).validate().unwrap(),
        &[shift("2026-10-07T21:00", "2026-10-08T05:00")],
        None,
    );
    assert_eq!(
        rows(&explain(&night, "2026-10-07")),
        [
            row(
                "2026-10-07T21:00",
                "2026-10-07T22:00",
                Category::Within,
                false,
                Some(0)
            ),
            row(
                "2026-10-07T22:00",
                "2026-10-08T05:00",
                Category::Within,
                true,
                Some(25)
            ),
        ]
    );
}

/// 週の上限を超えた土曜（法定外休日）の労働は、すべて時間外。
#[test]
fn weekly_excess_on_saturday_is_overtime() {
    let mut inputs: Vec<ShiftInput> = ["05", "06", "07", "08", "09"]
        .iter()
        .map(|d| {
            shift_with_breaks(
                &format!("2026-10-{d}T09:00"),
                &format!("2026-10-{d}T18:00"),
                &[(&format!("2026-10-{d}T12:00"), &format!("2026-10-{d}T13:00"))],
            )
        })
        .collect();
    inputs.push(shift("2026-10-10T09:00", "2026-10-10T13:00"));
    let week = case(policy(Weekday::Sunday).validate().unwrap(), &inputs, None);
    assert_eq!(
        rows(&explain(&week, "2026-10-10")),
        [row(
            "2026-10-10T09:00",
            "2026-10-10T13:00",
            Category::Overtime,
            false,
            Some(25)
        )]
    );
}

/// 法定休日をまたぐ勤務（`use_default`）: 土曜の分は平日型、日曜の分は法定休日。曖昧さの注意が付く。
#[test]
fn crossing_into_the_legal_holiday_with_use_default() {
    let mut base = policy(Weekday::Sunday);
    base.ambiguity = Some(AmbiguityMode::UseDefault);
    let crossing = case(
        base.validate().unwrap(),
        &[shift("2026-10-10T20:00", "2026-10-11T02:00")],
        None,
    );
    let explanation = explain(&crossing, "2026-10-10");
    assert_eq!(
        rows(&explanation),
        [
            row(
                "2026-10-10T20:00",
                "2026-10-10T22:00",
                Category::Within,
                false,
                Some(0)
            ),
            row(
                "2026-10-10T22:00",
                "2026-10-11T00:00",
                Category::Within,
                true,
                Some(25)
            ),
            row(
                "2026-10-11T00:00",
                "2026-10-11T02:00",
                Category::Holiday,
                true,
                Some(60)
            ),
        ]
    );
    assert_eq!(
        explanation.notes,
        [
            ExplainNote::Ambiguity(AmbiguityCode::HolidayBoundary),
            ExplainNote::Ambiguity(AmbiguityCode::WeekBoundary),
        ]
    );
    assert_eq!(explanation.notes[0].code(), "HolidayBoundary");
    assert_ne!(explanation.notes[0].message(), "");
}

/// 勤務のない日は区間が空。表の確認日・確認の状態・注意は、いつも付く。
#[test]
fn empty_day_still_has_meta() {
    let quiet = case(
        policy(Weekday::Sunday).validate().unwrap(),
        &[shift("2026-10-07T09:00", "2026-10-07T17:00")],
        None,
    );
    let explanation = explain(&quiet, "2026-10-08");
    assert_eq!(explanation.date, date("2026-10-08"));
    assert_eq!(explanation.segments, Vec::new());
    assert_eq!(explanation.notes, Vec::new());
    assert_eq!(explanation.notices, Notice::ALL);
    assert_eq!(explanation.table_checked_on, "2026-10-15");
    assert_eq!(explanation.review_status, ReviewStatus::Unreviewed);
}

/// 拒否された区分の結果と、勤務と合わない区分の結果からは、説明を作らない。
#[test]
fn refused_and_mismatched_results() {
    let policy = policy(Weekday::Sunday).validate().unwrap();
    let crossing = case(
        policy,
        &[shift("2026-10-10T20:00", "2026-10-11T02:00")],
        None,
    );
    assert!(crossing.result.refused.is_some());
    assert_eq!(
        explain_day(
            &policy,
            &crossing.shifts,
            &crossing.result,
            date("2026-10-10")
        ),
        Err(ExplainError::Refused)
    );

    let a = case(
        policy,
        &[shift("2026-10-07T09:00", "2026-10-07T17:00")],
        None,
    );
    let b = case(
        policy,
        &[shift("2026-10-07T09:00", "2026-10-07T19:00")],
        None,
    );
    let empty = case(policy, &[], None);
    for (shifts, result) in [
        (&b.shifts, &a.result),
        (&a.shifts, &b.result),
        (&empty.shifts, &a.result),
        (&a.shifts, &empty.result),
    ] {
        assert_eq!(
            explain_day(&policy, shifts, result, date("2026-10-07")),
            Err(ExplainError::Mismatch)
        );
    }
    assert_ne!(ExplainError::Mismatch.to_string(), "");
}

/// 割増の下限の表（指示書の表）。率と根拠の条文。
#[test]
fn premium_floor_table() {
    let after = date("2026-10-07");
    let cases = [
        (
            Category::Within,
            false,
            Some(0),
            vec!["労働基準法第32条第1項", "労働基準法第32条第2項"],
        ),
        (
            Category::Within,
            true,
            Some(25),
            vec![
                "労働基準法第32条第1項",
                "労働基準法第32条第2項",
                "労働基準法第37条第4項",
            ],
        ),
        (
            Category::ExtraWithin,
            false,
            Some(0),
            vec!["労働基準法第32条第1項", "労働基準法第32条第2項"],
        ),
        (
            Category::ExtraWithin,
            true,
            Some(25),
            vec![
                "労働基準法第32条第1項",
                "労働基準法第32条第2項",
                "労働基準法第37条第4項",
            ],
        ),
        (
            Category::Overtime,
            false,
            Some(25),
            vec![
                "労働基準法第37条第1項",
                "労働基準法第三十七条第一項の時間外及び休日の割増賃金に係る率の最低限度を定める政令（本則）",
            ],
        ),
        (
            Category::Overtime,
            true,
            Some(50),
            vec![
                "労働基準法第37条第1項",
                "労働基準法第三十七条第一項の時間外及び休日の割増賃金に係る率の最低限度を定める政令（本則）",
                "労働基準法第37条第4項",
                "労働基準法施行規則第20条第1項",
            ],
        ),
        (
            Category::Over60,
            false,
            Some(50),
            vec!["労働基準法第37条第1項ただし書"],
        ),
        (
            Category::Over60,
            true,
            Some(75),
            vec![
                "労働基準法第37条第1項ただし書",
                "労働基準法第37条第4項",
                "労働基準法施行規則第20条第1項",
            ],
        ),
        (
            Category::Holiday,
            false,
            Some(35),
            vec![
                "労働基準法第37条第1項",
                "労働基準法第三十七条第一項の時間外及び休日の割増賃金に係る率の最低限度を定める政令（本則）",
            ],
        ),
        (
            Category::Holiday,
            true,
            Some(60),
            vec![
                "労働基準法第37条第1項",
                "労働基準法第三十七条第一項の時間外及び休日の割増賃金に係る率の最低限度を定める政令（本則）",
                "労働基準法第37条第4項",
                "労働基準法施行規則第20条第2項",
            ],
        ),
    ];
    for (category, night, percent, citations) in cases {
        let premium = premium_floor(category, night, after);
        assert_eq!(premium.percent, percent, "{category:?} {night}");
        let got: Vec<String> = premium.basis.iter().map(Provenance::citation).collect();
        assert_eq!(got, citations, "{category:?} {night}");
    }
}

/// 2023-04-01 より前は、60時間超の率だけを出さない（ほかの率は出す）。区分の値と名前。
#[test]
fn premium_floor_before_april_2023_and_codes() {
    let before = date("2023-03-31");
    for category in Category::ALL {
        for night in [false, true] {
            let shown = premium_floor(category, night, before).percent.is_some();
            assert_eq!(shown, category != Category::Over60, "{category:?} {night}");
        }
    }
    assert_eq!(
        Category::ALL.map(Category::code),
        ["within", "extra_within", "overtime", "over60", "holiday"]
    );
    assert!(Category::ALL.iter().all(|c| !c.label().is_empty()));
}

/// 説明の根拠の定数（37条1項ただし書、労基則20条1項・2項）と、表の率の文言が、取得した条文の記録に合う。
#[test]
fn provenance_matches_the_retrieved_statutes() {
    let statutes = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/legal/sources/statutes-2026-10-15.txt"
    ))
    .unwrap();
    for (provenance, phrase) in [
        (
            LSA_37_1_PROVISO,
            "ただし、当該延長して労働させた時間が一箇月について六十時間を超えた場合においては、その超えた時間の労働については、通常の労働時間の賃金の計算額の五割以上の率で計算した割増賃金を支払わなければならない。",
        ),
        (
            REG_20_1,
            "の五割以上（その時間の労働のうち、一箇月について六十時間を超える労働時間の延長に係るものについては、七割五分以上）の率で計算した割増賃金を支払わなければならない。",
        ),
        (
            REG_20_2,
            "[2項] 法第三十三条又は法第三十六条第一項の規定による休日の労働時間が午後十時から午前五時",
        ),
    ] {
        assert_eq!(provenance.confirmed_on, "2026-10-15");
        assert!(statutes.contains(phrase), "{}", provenance.citation());
    }
    assert!(statutes.contains(
        "金額にその労働時間数を乗じた金額の六割以上の率で計算した割増賃金を支払わなければならない。"
    ));
    // 深夜の25%（37条4項）と、割増賃金令の25%・35%。
    assert!(statutes.contains(
        "通常の労働時間の賃金の計算額の二割五分以上の率で計算した割増賃金を支払わなければならない。"
    ));
    assert!(statutes.contains("延長した労働時間の労働については二割五分とし、これらの規定により労働させた休日の労働については三割五分とする。"));
    assert_eq!(REG_20_1.citation(), "労働基準法施行規則第20条第1項");
    assert_eq!(LSA_37_1_PROVISO.citation(), "労働基準法第37条第1項ただし書");
}
