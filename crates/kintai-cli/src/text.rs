//! 出力の共通の部品: コマンドの出力の型、人が読む表、どの出力にも付ける注意・表の確認日・確認の状態。
//!
//! `docs/spec/report.md` の「警告と記録」のとおり、`report` `check` `leave` `explain` `retention` の出力には、
//! 計算に使わなかった事情の注意（`notices`）、表の最終確認日（`table_checked_on`）、専門家の確認前である
//! こと（`review_status: "unreviewed"`）を必ず付ける。人が読む出力は、末尾の1行で参考値であることを示す。

use kintai_jp_labor::report::{Notice, ReviewStatus, TABLE_CHECKED_ON};

/// 成功の終了コード。
pub const EXIT_OK: u8 = 0;
/// `check --fail-on-violation` で違反または警告が見つかったときの終了コード。
pub const EXIT_VIOLATION: u8 = 1;

/// 確認の状態（この版はすべて確認前）。
pub const REVIEW_STATUS: ReviewStatus = ReviewStatus::Unreviewed;

/// 人が読む出力の最後の1行。
pub const REFERENCE_LINE: &str =
    "この出力は参考値です（専門家の確認前。review_status: unreviewed）。";

/// コマンドの出力（標準出力に出す文字列と、終了コード）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    /// 標準出力に出す文字列（末尾の改行を含む）。
    pub stdout: String,
    /// 終了コード。
    pub exit: u8,
}

/// 出力の `notices` に出す、計算に使わなかった事情のコード（決まった順）。
#[must_use]
pub fn notice_codes() -> Vec<&'static str> {
    Notice::ALL.iter().map(|notice| notice.code()).collect()
}

/// 表示の幅（ASCII は1、それ以外は2として数える。端末での揃えの目安）。
#[must_use]
pub fn width(text: &str) -> usize {
    text.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum()
}

/// 列の揃え。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// 左揃え（文字）。
    Left,
    /// 右揃え（数）。
    Right,
}

fn spaces(out: &mut String, count: usize) {
    out.extend(std::iter::repeat_n(' ', count));
}

fn push_row<'a>(
    out: &mut String,
    cells: impl Iterator<Item = &'a str>,
    widths: &[usize],
    aligns: &[Align],
) {
    let mut line = String::from("  ");
    for (index, ((cell, column_width), align)) in cells.zip(widths).zip(aligns).enumerate() {
        if index > 0 {
            line.push_str("  ");
        }
        let pad = column_width.saturating_sub(width(cell));
        match align {
            Align::Left => {
                line.push_str(cell);
                spaces(&mut line, pad);
            }
            Align::Right => {
                spaces(&mut line, pad);
                line.push_str(cell);
            }
        }
    }
    out.push_str(line.trim_end());
    out.push('\n');
}

/// 見出しつきの表（各行の前に2文字の字下げ、列の間に2文字）。行の末尾の空白は付けない。
#[must_use]
pub fn render_table(headers: &[&str], aligns: &[Align], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| width(header)).collect();
    for row in rows {
        for (column_width, cell) in widths.iter_mut().zip(row) {
            *column_width = (*column_width).max(width(cell));
        }
    }
    let mut out = String::new();
    push_row(&mut out, headers.iter().copied(), &widths, aligns);
    for row in rows {
        push_row(&mut out, row.iter().map(String::as_str), &widths, aligns);
    }
    out
}

/// 半日の数を日にする（`17` → `8.5日`）。人が読む出力だけで使う（浮動小数点は使わない）。
#[must_use]
pub fn days(halves: i64) -> String {
    let sign = if halves < 0 { "-" } else { "" };
    let magnitude = halves.unsigned_abs();
    let whole = magnitude / 2;
    if magnitude.is_multiple_of(2) {
        format!("{sign}{whole}日")
    } else {
        format!("{sign}{whole}.5日")
    }
}

/// 1行を足す（末尾に改行）。
pub fn line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}

/// 見出しの下の行（字下げ済みの文字列）。なければ「なし」。
pub fn list(out: &mut String, items: &[String]) {
    if items.is_empty() {
        line(out, "  なし");
    }
    for item in items {
        line(out, item);
    }
}

/// 人が読む出力の末尾: 計算に使わなかった事情の注意（`extra` はコマンドに固有の注意）、表の確認日、
/// 確認の状態、参考値であることの1行。
#[must_use]
pub fn footer(extra: &[(&str, &str)]) -> String {
    let mut out = String::new();
    line(&mut out, "注意（計算に使わなかった事情。notices）:");
    for notice in Notice::ALL {
        line(
            &mut out,
            &format!("  - {}: {}", notice.code(), notice.message()),
        );
    }
    for (code, message) in extra {
        line(&mut out, &format!("  - {code}: {message}"));
    }
    line(
        &mut out,
        &format!("表の確認日（table_checked_on）: {TABLE_CHECKED_ON}"),
    );
    line(
        &mut out,
        &format!("確認の状態（review_status）: {}", REVIEW_STATUS.code()),
    );
    line(&mut out, REFERENCE_LINE);
    out
}

#[cfg(test)]
mod tests {
    use super::{Align, REFERENCE_LINE, days, footer, notice_codes, render_table, width};

    #[test]
    fn half_days_are_shown_without_floats() {
        assert_eq!(days(0), "0日");
        assert_eq!(days(1), "0.5日");
        assert_eq!(days(17), "8.5日");
        assert_eq!(days(20), "10日");
        assert_eq!(days(-3), "-1.5日");
    }

    #[test]
    fn width_counts_wide_characters_as_two() {
        assert_eq!(width("E001"), 4);
        assert_eq!(width("延長"), 4);
        assert_eq!(width(""), 0);
    }

    #[test]
    fn tables_align_columns_and_trim_trailing_spaces() {
        let rows = vec![
            vec!["E001".to_owned(), "5".to_owned(), "ab".to_owned()],
            vec!["E10".to_owned(), "120".to_owned(), String::new()],
        ];
        let text = render_table(
            &["ID", "分", "注"],
            &[Align::Left, Align::Right, Align::Left],
            &rows,
        );
        assert_eq!(text, "  ID     分  注\n  E001    5  ab\n  E10   120\n");
    }

    /// AC-08.6・AC-12.10 人が読む出力の末尾に、注意・表の確認日・確認の状態・参考値の1行が必ずある。
    #[test]
    fn ac08_6_footer_has_the_meta_and_ends_with_the_reference_line() {
        let text = footer(&[("OwnEmployerOnly", "自社の時間だけ")]);
        for code in notice_codes() {
            assert!(text.contains(code), "{code}");
        }
        assert!(text.contains("OwnEmployerOnly"));
        assert!(text.contains("table_checked_on）: 2026-10-15"));
        assert!(text.contains("review_status）: unreviewed"));
        assert!(text.ends_with(&format!("{REFERENCE_LINE}\n")));
        assert_eq!(notice_codes().len(), 4);
    }
}
