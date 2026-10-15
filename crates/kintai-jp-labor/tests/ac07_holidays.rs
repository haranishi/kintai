//! 国民の祝日（`docs/spec/holidays.md`）。
//!
//! - AC-07.1: 埋め込んだ表が、公式CSV（`docs/legal/sources/syukujitsu-2026-10-15.csv`）の2020〜2027年の全行と一致する。
//! - AC-07.2: 規則から生成した2020〜2027年の祝日・振替休日・国民の休日が、公式CSVと全件一致する（143件）。
//! - AC-07.3: 近似式の春分日・秋分日が、公式の16件と、国立天文台の予想表の62件と一致する。
//! - AC-07.4: 2028年以降は `Estimated`、2020年より前は `OutOfRange`。
//! - AC-07.5: 振替休日と国民の休日の種別が取れ、3条の休日が祝日の再帰に混ざらない。
//! - AC-07.6: 祝日であることが、法定休日の判定に影響しない。

use std::collections::BTreeSet;

use kintai_core::{Date, Weekday};
use kintai_jp_labor::holidays::{
    EQUINOX_VERIFIED_LAST_YEAR, ESTIMATED_LAST_YEAR, HOLIDAYS_ACT_2, HOLIDAYS_ACT_3_2,
    HOLIDAYS_ACT_3_3, Holiday, HolidayKind, HolidayNotice, HolidayStatus, NATIONAL_HOLIDAYS_ACT,
    OFFICIAL_FIRST_YEAR, OFFICIAL_HOLIDAYS, OFFICIAL_LAST_YEAR, SOURCE_NOTE, article3_holidays,
    autumnal_equinox, generate, holiday_on, holidays_of_year, notices, status, statutory_holidays,
    vernal_equinox,
};
use kintai_jp_labor::provenance::CONFIRMED_ON;
use kintai_jp_labor::{ShiftInput, ShiftSet, WorkPolicy, WorkSystem, classify};

fn source(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../docs/legal/sources/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

/// 公式CSVの、2020年以降の行（日付、名称）。
fn official_csv_rows() -> Vec<(String, String)> {
    let text = source("syukujitsu-2026-10-15.csv");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("日付,名称"));
    lines
        .filter(|line| line.as_bytes() >= b"2020".as_slice())
        .map(|line| {
            let (day, name) = line.split_once(',').unwrap();
            (day.to_owned(), name.to_owned())
        })
        .collect()
}

fn rows_of(holidays: &[Holiday]) -> Vec<(String, String)> {
    holidays
        .iter()
        .map(|h| (h.date.to_string(), h.name.to_owned()))
        .collect()
}

/// AC-07.1 埋め込んだ表（143行）が、公式CSVの2020年以降の全行と、日付・名称・順序まで一致する。
#[test]
fn ac07_1_embedded_table_equals_the_official_csv() {
    let csv = official_csv_rows();
    assert_eq!(csv.len(), 143);
    assert_eq!(csv.last().unwrap().0, "2027-11-23", "公式CSVは2027年まで");
    let embedded: Vec<(String, String)> = OFFICIAL_HOLIDAYS
        .iter()
        .map(|row| {
            let day = Date::from_ymd(row.year, row.month, row.day).unwrap();
            (day.to_string(), row.name.to_owned())
        })
        .collect();
    assert_eq!(embedded, csv);
    // 年ごとの件数（調査の検算の表と同じ）。
    let per_year: Vec<usize> = (2020..=2027)
        .map(|y| OFFICIAL_HOLIDAYS.iter().filter(|r| r.year == y).count())
        .collect();
    assert_eq!(per_year, [18, 17, 16, 17, 21, 19, 18, 17]);
    // 種別は「休日」の行だけに付き、3条3項（前日と翌日が祝日）は2026-09-22 だけ。
    for row in &OFFICIAL_HOLIDAYS {
        assert_eq!(
            row.kind == HolidayKind::Statutory,
            row.name != "休日",
            "{row:?}"
        );
    }
    let citizens: Vec<String> = OFFICIAL_HOLIDAYS
        .iter()
        .filter(|r| r.kind == HolidayKind::Citizens)
        .map(|r| format!("{}-{:02}-{:02}", r.year, r.month, r.day))
        .collect();
    assert_eq!(citizens, ["2026-09-22"]);
    assert_eq!(
        OFFICIAL_HOLIDAYS
            .iter()
            .filter(|r| r.kind == HolidayKind::Substitute)
            .count(),
        14
    );
}

/// AC-07.2 規則から生成した2020〜2027年が、公式CSVと全件一致する（143件）。種別も埋め込んだ表と一致する。
#[test]
fn ac07_2_rules_reproduce_all_143_official_rows() {
    let mut generated = Vec::new();
    for year in 2020..=2027 {
        let holidays = generate(year).unwrap();
        let official = holidays_of_year(year);
        assert_eq!(official.status, HolidayStatus::Official);
        let kinds: Vec<(Date, HolidayKind)> = holidays.iter().map(|h| (h.date, h.kind)).collect();
        let official_kinds: Vec<(Date, HolidayKind)> =
            official.holidays.iter().map(|h| (h.date, h.kind)).collect();
        assert_eq!(kinds, official_kinds, "{year}");
        generated.extend(holidays);
    }
    assert_eq!(rows_of(&generated), official_csv_rows());
    assert_eq!(generated.len(), 143);
}

/// AC-07.2 2020・2021年は、特別措置法による海の日・スポーツの日・山の日の移動（公式CSVの日付）。
#[test]
fn ac07_2_olympic_years() {
    let names = |year| -> Vec<(String, &'static str)> {
        statutory_holidays(year)
            .unwrap()
            .iter()
            .filter(|h| ["海の日", "スポーツの日", "山の日"].contains(&h.name))
            .map(|h| (h.date.to_string(), h.name))
            .collect()
    };
    assert_eq!(
        names(2020),
        [
            ("2020-07-23".to_owned(), "海の日"),
            ("2020-07-24".to_owned(), "スポーツの日"),
            ("2020-08-10".to_owned(), "山の日"),
        ]
    );
    assert_eq!(
        names(2021),
        [
            ("2021-07-22".to_owned(), "海の日"),
            ("2021-07-23".to_owned(), "スポーツの日"),
            ("2021-08-08".to_owned(), "山の日"),
        ]
    );
    // 2021-08-08 は日曜なので、翌日が振替休日。
    assert_eq!(
        holiday_on(date("2021-08-09")).holiday.unwrap().kind,
        HolidayKind::Substitute
    );
    assert_eq!(
        names(2022),
        [
            ("2022-07-18".to_owned(), "海の日"),
            ("2022-08-11".to_owned(), "山の日"),
            ("2022-10-10".to_owned(), "スポーツの日"),
        ]
    );
}

/// AC-07.3 近似式の春分日・秋分日が、公式の日付（暦要項、2020〜2027年の16件）と一致する。
#[test]
fn ac07_3_equinoxes_match_the_official_16() {
    let text = source("equinox-official-2026-10-15.csv");
    let mut lines = text.lines();
    assert!(lines.next().unwrap().starts_with("year,name,official"));
    let mut checked = 0;
    for line in lines {
        let fields: Vec<&str> = line.split(',').collect();
        let year: i32 = fields[0].parse().unwrap();
        let official = date(fields[2]);
        let ours = match fields[1] {
            "春分の日" => vernal_equinox(year),
            "秋分の日" => autumnal_equinox(year),
            other => panic!("{other}"),
        };
        assert_eq!(ours, Some(official), "{line}");
        checked += 1;
    }
    assert_eq!(checked, 16);
}

/// AC-07.3 2020〜2050年の国立天文台の予想表（62件）と一致する。
#[test]
fn ac07_3_equinoxes_match_the_naoj_forecast_62() {
    let text = source("equinox-naoj-forecast-2020-2050.csv");
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("year,spring_equinox_day,autumn_equinox_day")
    );
    let mut checked = 0;
    for line in lines {
        let fields: Vec<&str> = line.split(',').collect();
        let year: i32 = fields[0].parse().unwrap();
        assert_eq!(vernal_equinox(year), Some(date(fields[1])), "{line}");
        assert_eq!(autumnal_equinox(year), Some(date(fields[2])), "{line}");
        checked += 2;
    }
    assert_eq!(checked, 62);
    // 近似式は2020〜2099年だけ。
    assert_eq!(vernal_equinox(2019), None);
    assert_eq!(autumnal_equinox(2100), None);
    assert!(vernal_equinox(2099).is_some());
}

/// 小数で書いた係数（例 `20.8431`）を、10^7 倍の整数にする（試験の中だけの読み方）。
fn scaled(decimal: &str) -> i128 {
    let (whole, fraction) = decimal.split_once('.').unwrap();
    let digits = format!("{fraction:0<7}");
    whole.parse::<i128>().unwrap() * 10_000_000 + digits.parse::<i128>().unwrap()
}

/// 指示書の近似式 `floor(c + 0.242194*(y-1980) - floor((y-1980)/4))` を、係数の文字列から
/// 10^7 倍の整数で計算する（実装とは別の倍率と別の書き方）。
fn formula_day(constant: &str, year: i32) -> u32 {
    let years = i128::from(year) - 1980;
    let value = scaled(constant) + scaled("0.242194") * years - 10_000_000 * years.div_euclid(4);
    u32::try_from(value.div_euclid(10_000_000)).unwrap()
}

/// AC-07.3 AC-07.4 近似式が、2020〜2099年の全年で仕様の式どおり（浮動小数点を使わず正確に）計算される。
/// 2088年の春分は、式の値が 20.000052 で整数の境に最も近い。誤差のある計算では3月19日になりうる。
#[test]
fn ac07_3_equinox_formula_is_exact_for_every_year() {
    for year in 2020..=2099 {
        assert_eq!(
            vernal_equinox(year).unwrap(),
            Date::from_ymd(year, 3, formula_day("20.8431", year)).unwrap(),
            "{year}"
        );
        assert_eq!(
            autumnal_equinox(year).unwrap(),
            Date::from_ymd(year, 9, formula_day("23.2488", year)).unwrap(),
            "{year}"
        );
    }
    assert_eq!(vernal_equinox(2088), Some(date("2088-03-20")));
    assert_eq!(
        holiday_on(date("2088-03-20")).holiday.unwrap().name,
        "春分の日"
    );
}

/// AC-07.4 2020〜2027年は `Official`、2028〜2099年は `Estimated`（2051年以降は注意つき）、ほかは `OutOfRange`。
#[test]
fn ac07_4_status_by_year() {
    assert_eq!(
        (OFFICIAL_FIRST_YEAR, OFFICIAL_LAST_YEAR, ESTIMATED_LAST_YEAR),
        (2020, 2027, 2099)
    );
    assert_eq!(EQUINOX_VERIFIED_LAST_YEAR, 2050);
    for (year, expected) in [
        (1999, HolidayStatus::OutOfRange),
        (2019, HolidayStatus::OutOfRange),
        (2020, HolidayStatus::Official),
        (2027, HolidayStatus::Official),
        (2028, HolidayStatus::Estimated),
        (2099, HolidayStatus::Estimated),
        (2100, HolidayStatus::OutOfRange),
    ] {
        assert_eq!(status(year), expected, "{year}");
        let lookup = holiday_on(Date::from_ymd(year, 1, 1).unwrap());
        assert_eq!(lookup.status, expected, "{year}");
        assert_eq!(
            lookup.holiday.is_some(),
            expected != HolidayStatus::OutOfRange
        );
    }
    assert_eq!(holidays_of_year(2019).holidays, []);
    assert_eq!(holidays_of_year(2100).holidays, []);
    assert_eq!(generate(2019), None);
    assert_eq!(generate(2100), None);
    // 2051年以降は、近似式が未検証の注意を付ける。
    assert_eq!(notices(2050), []);
    assert_eq!(notices(2051), [HolidayNotice::EquinoxUnverified]);
    assert_eq!(notices(2099), [HolidayNotice::EquinoxUnverified]);
    assert_eq!(notices(2027), []);
    assert_eq!(notices(2100), []);
    assert_eq!(
        holiday_on(date("2051-03-21")).notices,
        [HolidayNotice::EquinoxUnverified]
    );
    assert_eq!(HolidayNotice::EquinoxUnverified.code(), "EquinoxUnverified");
    // 推定の年も、日付順に16の祝日と3条の休日を持つ。
    let estimated = holidays_of_year(2028);
    assert_eq!(estimated.status, HolidayStatus::Estimated);
    assert_eq!(estimated.year, 2028);
    let statutory = estimated
        .holidays
        .iter()
        .filter(|h| h.kind == HolidayKind::Statutory)
        .count();
    assert_eq!(statutory, 16);
    assert!(
        estimated
            .holidays
            .windows(2)
            .all(|pair| pair[0].date < pair[1].date)
    );
    // 2028-01-10 は1月の第2月曜（成人の日）。
    assert_eq!(
        holiday_on(date("2028-01-10")).holiday.unwrap().name,
        "成人の日"
    );
    assert_eq!(
        [
            HolidayStatus::Official.code(),
            HolidayStatus::Estimated.code(),
            HolidayStatus::OutOfRange.code()
        ],
        ["Official", "Estimated", "OutOfRange"]
    );
}

/// AC-07.5 振替休日（3条2項）と国民の休日（3条3項）の区別が取れる。
#[test]
fn ac07_5_kinds_are_distinguished() {
    let substitute = holiday_on(date("2026-05-06")).holiday.unwrap();
    assert_eq!(
        (substitute.name, substitute.kind),
        ("休日", HolidayKind::Substitute)
    );
    let citizens = holiday_on(date("2026-09-22")).holiday.unwrap();
    assert_eq!(
        (citizens.name, citizens.kind),
        ("休日", HolidayKind::Citizens)
    );
    let statutory = holiday_on(date("2026-09-21")).holiday.unwrap();
    assert_eq!(
        (statutory.name, statutory.kind),
        ("敬老の日", HolidayKind::Statutory)
    );
    assert_eq!(holiday_on(date("2026-09-24")).holiday, None);
    assert_eq!(HolidayKind::Substitute.provenance(), HOLIDAYS_ACT_3_2);
    assert_eq!(HolidayKind::Citizens.provenance(), HOLIDAYS_ACT_3_3);
    assert_eq!(HolidayKind::Statutory.provenance(), HOLIDAYS_ACT_2);
    assert_eq!(
        [
            HolidayKind::Statutory.code(),
            HolidayKind::Substitute.code(),
            HolidayKind::Citizens.code()
        ],
        ["Statutory", "Substitute", "Citizens"]
    );
}

fn derived(statutory: &[&str]) -> Vec<(String, HolidayKind)> {
    let dates: Vec<Date> = statutory.iter().map(|d| date(d)).collect();
    article3_holidays(&dates)
        .iter()
        .map(|h| (h.date.to_string(), h.kind))
        .collect()
}

/// AC-07.5 3条の休日は2条の祝日として扱わず、再帰しない。土曜の祝日に振替はない。
#[test]
fn ac07_5_article_3_does_not_recurse() {
    // 2023-01-01（日）と 2023-01-04（水）: 1/2 は振替休日。1/3 は振替休日と祝日に挟まれるが、国民の休日にならない。
    assert_eq!(
        derived(&["2023-01-01", "2023-01-04"]),
        [("2023-01-02".to_owned(), HolidayKind::Substitute)]
    );
    // 日曜から3日続く祝日の振替は、続く祝日の後の最初の日。
    assert_eq!(
        derived(&["2026-05-03", "2026-05-04", "2026-05-05"]),
        [("2026-05-06".to_owned(), HolidayKind::Substitute)]
    );
    // 土曜の祝日には振替がない。
    assert_eq!(derived(&["2026-10-03"]), []);
    // 祝日に挟まれた平日は国民の休日。3日空くと付かない。
    assert_eq!(
        derived(&["2026-09-21", "2026-09-23"]),
        [("2026-09-22".to_owned(), HolidayKind::Citizens)]
    );
    assert_eq!(derived(&["2026-09-21", "2026-09-24"]), []);
    // 振替休日と国民の休日の両方に当たる日（日曜と火曜の祝日に挟まれた月曜）は、振替休日として1日。
    assert_eq!(
        derived(&["2026-10-04", "2026-10-06"]),
        [("2026-10-05".to_owned(), HolidayKind::Substitute)]
    );
    assert_eq!(derived(&[]), []);
}

/// AC-07.5 2020〜2099年の規則の祝日で、振替休日が同時に国民の休日に当たる日はない（種別の取り決めが効かない）。
/// 祝日が日曜なら、その年の休日に振替がある。
#[test]
fn ac07_5_generated_years_are_consistent() {
    for year in 2020..=2099 {
        let statutory = statutory_holidays(year).unwrap();
        let set: BTreeSet<Date> = statutory.iter().map(|h| h.date).collect();
        let all = generate(year).unwrap();
        for holiday in &all {
            if holiday.kind == HolidayKind::Substitute {
                let before = holiday.date.checked_add_days(-1).unwrap();
                let after = holiday.date.checked_add_days(1).unwrap();
                assert!(
                    !(set.contains(&before) && set.contains(&after)),
                    "{holiday:?}"
                );
            }
        }
        let sundays = statutory
            .iter()
            .filter(|h| h.date.weekday() == Weekday::Sunday)
            .count();
        let substitutes = all
            .iter()
            .filter(|h| h.kind == HolidayKind::Substitute)
            .count();
        assert_eq!(sundays, substitutes, "{year}");
        assert_eq!(statutory.len(), 16);
    }
}

fn shift(start: &str, end: &str) -> ShiftInput {
    ShiftInput {
        start: start.parse().unwrap(),
        end: end.parse().unwrap(),
        breaks: Vec::new(),
    }
}

fn holiday_minutes(legal: Weekday, start: &str, end: &str) -> (i64, i64, i64) {
    let policy = WorkPolicy {
        work_system: WorkSystem::Standard,
        scheduled_minutes_per_day: 480,
        week_start: None,
        weekly_limit_minutes: None,
        special_44h_confirmed: None,
        legal_holiday_weekday: Some(legal),
        month_start_day: 1,
        ambiguity: None,
        worker_kind: None,
        holiday_system: None,
        holiday_swaps: 0,
        night_window: None,
    }
    .validate()
    .unwrap();
    let result = classify(&policy, &ShiftSet::new(&[shift(start, end)]).unwrap(), None).unwrap();
    let day = &result.days[0];
    (day.within, day.overtime, day.holiday)
}

/// AC-07.6 祝日の労働は、法定休日の曜日でなければ平日と同じに数える。法定休日は方針の曜日だけで決まる。
#[test]
fn ac07_6_national_holidays_are_not_legal_holidays() {
    // 2026-11-03（火、文化の日）と、祝日でない 2026-11-10（火）は同じ。
    assert_eq!(
        holiday_on(date("2026-11-03")).holiday.unwrap().name,
        "文化の日"
    );
    let culture_day = holiday_minutes(Weekday::Sunday, "2026-11-03T09:00", "2026-11-03T19:00");
    let ordinary = holiday_minutes(Weekday::Sunday, "2026-11-10T09:00", "2026-11-10T19:00");
    assert_eq!(culture_day, (480, 120, 0));
    assert_eq!(culture_day, ordinary);
    // 振替休日（2026-05-06、水）も平日と同じ。
    assert_eq!(
        holiday_minutes(Weekday::Sunday, "2026-05-06T09:00", "2026-05-06T17:00"),
        (480, 0, 0)
    );
    // 法定休日の曜日（日曜）なら、祝日（2026-05-03、憲法記念日）かどうかによらず法定休日。
    assert_eq!(
        holiday_minutes(Weekday::Sunday, "2026-05-03T09:00", "2026-05-03T17:00"),
        (0, 0, 480)
    );
    assert_eq!(
        holiday_minutes(Weekday::Sunday, "2026-05-10T09:00", "2026-05-10T17:00"),
        (0, 0, 480)
    );
    // 法定休日を火曜にすると、祝日でない火曜も法定休日。
    assert_eq!(
        holiday_minutes(Weekday::Tuesday, "2026-11-10T09:00", "2026-11-10T17:00"),
        (0, 0, 480)
    );
}

/// 漢数字（一〜九、十）を数にする。
fn kanji_number(text: &str) -> u32 {
    let mut total = 0;
    let mut current = 0;
    for c in text.chars() {
        match "一二三四五六七八九".chars().position(|d| d == c) {
            Some(p) => current = u32::try_from(p).unwrap() + 1,
            None if c == '十' => {
                total += current.max(1) * 10;
                current = 0;
            }
            None => {}
        }
    }
    total + current
}

/// 祝日法2条の表の16の祝日の名前と日の決め方が、規則の生成と一致する（2030年で確かめる）。
/// 法令の版ID・URL は、取得の記録と一致する。
#[test]
fn ac07_holidays_follow_article_2_and_the_law_record() {
    let statutes = source("statutes-2026-10-15.txt");
    let start = statutes.find("国民の祝日に関する法律  法令ID=").unwrap();
    let law = &statutes[start..];
    let article2 = &law[law.find("第二条").unwrap()..law.find("第三条").unwrap()];
    let rows: Vec<Vec<&str>> = article2
        .lines()
        .map(|line| {
            line.split('|')
                .map(str::trim)
                .filter(|cell| !cell.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|cells| cells.len() == 3)
        .collect();
    assert_eq!(rows.len(), 16);
    let generated = statutory_holidays(2030).unwrap();
    for (row, holiday) in rows.iter().zip(&generated) {
        assert_eq!(row[0], holiday.name);
        let rule = row[1];
        if let Some((month, rest)) = rule.split_once('月') {
            assert_eq!(kanji_number(month), holiday.date.month(), "{rule}");
            if let Some(nth) = rest
                .strip_prefix("の第")
                .and_then(|r| r.strip_suffix("月曜日"))
            {
                assert_eq!(holiday.date.weekday(), Weekday::Monday, "{rule}");
                assert_eq!(
                    (holiday.date.day() - 1) / 7 + 1,
                    kanji_number(nth),
                    "{rule}"
                );
            } else {
                assert_eq!(kanji_number(rest), holiday.date.day(), "{rule}");
            }
        } else {
            // 政令で定める日（建国記念の日、2月11日）、春分日、秋分日。
            let expected = match rule {
                "政令で定める日" => date("2030-02-11"),
                "春分日" => vernal_equinox(2030).unwrap(),
                "秋分日" => autumnal_equinox(2030).unwrap(),
                other => panic!("{other}"),
            };
            assert_eq!(holiday.date, expected, "{rule}");
        }
    }
    assert!(law.contains("「国民の祝日」が日曜日に当たるときは、その日後においてその日に最も近い「国民の祝日」でない日を休日とする。"));
    assert!(law.contains(
        "その前日及び翌日が「国民の祝日」である日（「国民の祝日」でない日に限る。）は、休日とする。"
    ));
    // 法令の記録。
    assert!(law.starts_with(&format!(
        "国民の祝日に関する法律  法令ID={}",
        NATIONAL_HOLIDAYS_ACT.law_id
    )));
    assert!(law.contains(&format!("版ID={}", NATIONAL_HOLIDAYS_ACT.revision_id)));
    assert!(law.contains(&format!("施行日={}", NATIONAL_HOLIDAYS_ACT.enforced_on)));
    assert!(law.contains(&format!("出典: {}", NATIONAL_HOLIDAYS_ACT.url)));
    assert!(source("revisions-2026-10-15.txt").contains(NATIONAL_HOLIDAYS_ACT.revision_id));
    for provenance in [HOLIDAYS_ACT_2, HOLIDAYS_ACT_3_2, HOLIDAYS_ACT_3_3] {
        assert_eq!(provenance.law, NATIONAL_HOLIDAYS_ACT);
        assert_eq!(provenance.confirmed_on, CONFIRMED_ON);
    }
    assert_eq!(
        HOLIDAYS_ACT_3_3.citation(),
        "国民の祝日に関する法律第3条第3項"
    );
    assert!(SOURCE_NOTE.contains("内閣府") && SOURCE_NOTE.contains("2026-10-15"));
}
