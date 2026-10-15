//! 付与日数の表と、付与日数の区分の優先順位（`docs/spec/leave.md` の規則2）。
//!
//! - AC-06.1: 付与表（通常7行、比例28セル）が、条文の写し（`docs/legal/sources/statutes-2026-10-15.txt`
//!   の労働基準法第39条と労働基準法施行規則第24条の3）と全セル一致する。試験が条文の漢数字を読んで比べる。
//! - AC-06.12: 付与日数の優先順位（週5日以上か週1800分以上 → 年間の日数 → 週の日数）が、定めた順になる。

use kintai_jp_labor::leave::{
    FOUR_DAYS_GRANT_DAYS, GrantRow, LeaveError, LeaveUnsupportedCode, NORMAL_ANNUAL_DAYS_MIN,
    NORMAL_GRANT_DAYS, NORMAL_WEEKLY_DAYS_MIN, NORMAL_WEEKLY_MINUTES_MIN, ONE_DAY_GRANT_DAYS,
    THREE_DAYS_GRANT_DAYS, TWO_DAYS_GRANT_DAYS, WorkingTerms,
};

fn statutes() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/legal/sources/statutes-2026-10-15.txt"
    ))
    .unwrap()
}

/// 条文の一部（見出し `from` から、次の見出し `to` の前まで）。
fn section<'a>(text: &'a str, from: &str, to: &str) -> &'a str {
    let start = text.find(from).unwrap_or_else(|| panic!("{from} がない"));
    let rest = &text[start..];
    let end = rest.find(to).unwrap_or_else(|| panic!("{to} がない"));
    &rest[..end]
}

/// 漢数字（一〜九、十、百）を数にする。数字以外の文字（日・労働日など）は無視する。
fn kanji_number(text: &str) -> u32 {
    let mut total = 0;
    let mut current = 0;
    for c in text.chars() {
        let digit = "一二三四五六七八九"
            .chars()
            .position(|d| d == c)
            .map(|p| u32::try_from(p).unwrap() + 1);
        match (digit, c) {
            (Some(d), _) => current = d,
            (None, '十') => {
                total += current.max(1) * 10;
                current = 0;
            }
            (None, '百') => {
                total += current.max(1) * 100;
                current = 0;
            }
            _ => {}
        }
    }
    total + current
}

/// 表の行（`| A | B | … |`）のセル。
fn cells(line: &str) -> Vec<&str> {
    line.split('|')
        .map(str::trim)
        .filter(|cell| !cell.is_empty())
        .collect()
}

#[test]
fn kanji_numbers_are_read() {
    assert_eq!(kanji_number("十五日"), 15);
    assert_eq!(kanji_number("百六十九日"), 169);
    assert_eq!(kanji_number("二百十六日"), 216);
    assert_eq!(kanji_number("四十八日"), 48);
    assert_eq!(kanji_number("十労働日"), 10);
    assert_eq!(kanji_number("三十時間"), 30);
}

/// AC-06.1 通常付与の7行が、第39条第1項の10労働日と、第2項の加算の表（1・2・4・6・8・10労働日）から作れる。
#[test]
fn ac06_1_normal_grant_table_matches_article_39() {
    let text = statutes();
    let article = section(&text, "第三十九条 （年次有給休暇）", "[3項]");
    assert!(article.contains("継続し、又は分割した十労働日の有給休暇"));
    let base = kanji_number("十");
    let additions: Vec<u32> = article
        .lines()
        .map(cells)
        .filter(|row| {
            // 「一年」〜「六年以上」の行（見出しの行は除く）。
            let years = row[0].trim_end_matches("以上").trim_end_matches('年');
            row.len() == 2
                && row[0] != years
                && !years.is_empty()
                && years.chars().all(|c| "一二三四五六七八九十".contains(c))
                && row[1].ends_with("労働日")
        })
        .map(|row| kanji_number(row[1]))
        .collect();
    assert_eq!(additions, [1, 2, 4, 6, 8, 10], "加算の表の6行");
    let from_statute: Vec<u32> = std::iter::once(base)
        .chain(additions.iter().map(|a| base + a))
        .collect();
    assert_eq!(from_statute, NORMAL_GRANT_DAYS);
    assert_eq!(GrantRow::Normal.days_by_step(), NORMAL_GRANT_DAYS);
}

/// AC-06.1 比例付与の28セル（4行×7列）と年間の日数の区分が、施行規則第24条の3の表と一致する。
/// 週30時間・週4日・年216日の境界も、同条第1項・第4項・第5項と一致する。
#[test]
fn ac06_1_proportional_grant_table_matches_regulation_24_3() {
    let text = statutes();
    let article = section(&text, "第二十四条の三", "第二十四条の四");
    let rows: Vec<Vec<&str>> = article
        .lines()
        .map(cells)
        .filter(|row| row.len() == 9 && row[1].contains("から"))
        .collect();
    assert_eq!(rows.len(), 4, "週4日・3日・2日・1日の4行");
    let ours = [
        (GrantRow::FourDays, FOUR_DAYS_GRANT_DAYS),
        (GrantRow::ThreeDays, THREE_DAYS_GRANT_DAYS),
        (GrantRow::TwoDays, TWO_DAYS_GRANT_DAYS),
        (GrantRow::OneDay, ONE_DAY_GRANT_DAYS),
    ];
    let mut cells_checked = 0;
    for (row, (grant_row, days)) in rows.iter().zip(ours) {
        assert_eq!(Some(kanji_number(row[0])), grant_row.weekly_days());
        let (low, high) = row[1].split_once("から").unwrap();
        assert_eq!(
            Some((kanji_number(low), kanji_number(high))),
            grant_row.annual_days()
        );
        let from_statute: Vec<u32> = row[2..].iter().map(|cell| kanji_number(cell)).collect();
        assert_eq!(from_statute, days, "{grant_row:?}");
        assert_eq!(grant_row.days_by_step(), days);
        cells_checked += from_statute.len();
    }
    assert_eq!(cells_checked, 28);
    assert_eq!(
        GrantRow::PROPORTIONAL.map(GrantRow::days_by_step),
        ours.map(|(_, d)| d)
    );
    // 第1項: 30時間。第4項: 4日（以下が比例付与）。第5項: 216日（以下が比例付与）。
    assert!(
        article.contains("[1項] 法第三十九条第三項の厚生労働省令で定める時間は、三十時間とする。")
    );
    assert_eq!(kanji_number("三十") * 60, NORMAL_WEEKLY_MINUTES_MIN);
    assert!(
        article
            .contains("[4項] 法第三十九条第三項第一号の厚生労働省令で定める日数は、四日とする。")
    );
    assert_eq!(kanji_number("四") + 1, NORMAL_WEEKLY_DAYS_MIN);
    assert!(article.contains(
        "[5項] 法第三十九条第三項第二号の厚生労働省令で定める日数は、二百十六日とする。"
    ));
    assert_eq!(kanji_number("二百十六") + 1, NORMAL_ANNUAL_DAYS_MIN);
    assert_eq!(GrantRow::Normal.weekly_days(), None);
    assert_eq!(GrantRow::Normal.annual_days(), None);
}

/// AC-06.1 k 回目の付与の日数（6年6か月以上は最後の列）と、半日の数。
#[test]
fn ac06_1_days_by_service_length() {
    let normal: Vec<u32> = (0..10).map(|k| GrantRow::Normal.days(k)).collect();
    assert_eq!(normal, [10, 11, 12, 14, 16, 18, 20, 20, 20, 20]);
    assert_eq!(GrantRow::FourDays.days(3), 10);
    assert_eq!(GrantRow::ThreeDays.days(5), 10);
    assert_eq!(GrantRow::ThreeDays.days(4), 9);
    assert_eq!(GrantRow::OneDay.days(u32::MAX), 3);
    assert_eq!(GrantRow::Normal.halves(0), 20);
    assert_eq!(GrantRow::TwoDays.halves(6), 14);
}

fn row(days: i64, minutes: i64, annual: Option<i64>) -> Result<GrantRow, LeaveUnsupportedCode> {
    WorkingTerms {
        weekly_scheduled_days: days,
        weekly_scheduled_minutes: minutes,
        annual_scheduled_days: annual,
    }
    .validate()
    .unwrap()
    .grant_row()
}

/// AC-06.12 ① 週5日以上か週1800分以上なら、年間の日数によらず通常付与。
#[test]
fn ac06_12_weekly_five_days_or_1800_minutes_come_first() {
    assert_eq!(row(5, 0, None), Ok(GrantRow::Normal));
    assert_eq!(row(7, 0, Some(10)), Ok(GrantRow::Normal));
    assert_eq!(row(4, 1800, None), Ok(GrantRow::Normal));
    assert_eq!(row(0, 1800, Some(0)), Ok(GrantRow::Normal));
    assert_eq!(row(4, 1799, None), Ok(GrantRow::FourDays));
}

/// AC-06.12 ② そうでなく年間の日数があれば、週の日数より優先してその区分。各区分の両端。
#[test]
fn ac06_12_annual_days_take_priority_over_weekly_days() {
    let cases = [
        (366, Ok(GrantRow::Normal)),
        (217, Ok(GrantRow::Normal)),
        (216, Ok(GrantRow::FourDays)),
        (169, Ok(GrantRow::FourDays)),
        (168, Ok(GrantRow::ThreeDays)),
        (121, Ok(GrantRow::ThreeDays)),
        (120, Ok(GrantRow::TwoDays)),
        (73, Ok(GrantRow::TwoDays)),
        (72, Ok(GrantRow::OneDay)),
        (48, Ok(GrantRow::OneDay)),
        (47, Err(LeaveUnsupportedCode::LeaveOutOfTable)),
        (0, Err(LeaveUnsupportedCode::LeaveOutOfTable)),
    ];
    for (annual, expected) in cases {
        // 週の日数（1日・4日・0日）は、年間の日数があれば使わない。
        for weekly in [0, 1, 4] {
            assert_eq!(
                row(weekly, 600, Some(annual)),
                expected,
                "年{annual}日 週{weekly}日"
            );
        }
    }
}

/// AC-06.12 ③ それ以外は週の日数の行。週0日は表の外。
#[test]
fn ac06_12_weekly_days_last() {
    assert_eq!(row(4, 0, None), Ok(GrantRow::FourDays));
    assert_eq!(row(3, 1000, None), Ok(GrantRow::ThreeDays));
    assert_eq!(row(2, 0, None), Ok(GrantRow::TwoDays));
    assert_eq!(row(1, 0, None), Ok(GrantRow::OneDay));
    assert_eq!(row(0, 0, None), Err(LeaveUnsupportedCode::LeaveOutOfTable));
    assert_eq!(
        row(0, 1799, None),
        Err(LeaveUnsupportedCode::LeaveOutOfTable)
    );
}

/// AC-06.12 所定労働の条件の範囲（週0〜7日、0〜10080分、年0〜366日）。外は不正な入力。
#[test]
fn ac06_12_working_terms_ranges() {
    let terms = |days, minutes, annual| WorkingTerms {
        weekly_scheduled_days: days,
        weekly_scheduled_minutes: minutes,
        annual_scheduled_days: annual,
    };
    let valid = terms(7, 10_080, Some(366)).validate().unwrap();
    assert_eq!(
        (
            valid.weekly_days(),
            valid.weekly_minutes(),
            valid.annual_days()
        ),
        (7, 10_080, Some(366))
    );
    assert!(terms(0, 0, Some(0)).validate().is_ok());
    for (wrong, field) in [
        (terms(8, 0, None), "weekly_scheduled_days"),
        (terms(-1, 0, None), "weekly_scheduled_days"),
        (terms(5, 10_081, None), "weekly_scheduled_minutes"),
        (terms(5, -1, None), "weekly_scheduled_minutes"),
        (terms(5, 0, Some(367)), "annual_scheduled_days"),
        (terms(5, 0, Some(-1)), "annual_scheduled_days"),
        (terms(i64::MAX, 0, None), "weekly_scheduled_days"),
    ] {
        match wrong.validate() {
            Err(LeaveError::OutOfRange { field: got, .. }) => assert_eq!(got, field),
            other => panic!("{field}: {other:?}"),
        }
    }
}
