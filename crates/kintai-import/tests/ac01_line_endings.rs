//! 改行と空の行の読み方（`docs/spec/punch.md` の「CSVの読み方」。AC-01.8 の取込の決まりの一部）。
//! 査読 L2 の L2-09（LF で終わらない最後の行の CR）と L2-10（空の行と CR の誤りの順）。

use kintai_import::{ImportErrorKind, read_history, read_leaves, read_punches};

fn kind_and_line(bytes: &[u8]) -> (ImportErrorKind, Option<u64>) {
    let error = read_punches(bytes).unwrap_err();
    (error.kind, error.line)
}

/// AC-01.8・L2-09 LF で終わらない最後の行の CR は、改行でない単独の CR なので不正（見出しだけの行も）。
#[test]
fn l2_09_final_lone_cr_is_rejected() {
    assert_eq!(
        kind_and_line(b"employee_id,timestamp,kind\nE001,2026-10-04 17:00,out\r"),
        (ImportErrorKind::StrayCarriageReturn, Some(2))
    );
    assert_eq!(
        kind_and_line(b"employee_id,timestamp,kind\r"),
        (ImportErrorKind::StrayCarriageReturn, Some(1))
    );
    assert_eq!(
        kind_and_line(b"employee_id,timestamp,kind\nE001,2026-10-04 17:00,out\n\r"),
        (ImportErrorKind::StrayCarriageReturn, Some(3))
    );
    let leaves = read_leaves(b"employee_id,date,unit\nE001,2026-10-04,full\r").unwrap_err();
    assert_eq!(leaves.kind, ImportErrorKind::StrayCarriageReturn);
    let history =
        read_history(b"employee_id,month_start,total_minutes\nE001,2026-09-01,0\r").unwrap_err();
    assert_eq!(history.kind, ImportErrorKind::StrayCarriageReturn);
}

/// CRLF で終わる行、末尾の空の行（CRLF も）、LF で終わらない最後の行（CR なし）は、これまでどおり読める。
#[test]
fn crlf_and_unterminated_lines_still_read() {
    let count = |bytes: &[u8]| read_punches(bytes).unwrap().punches.len();
    assert_eq!(
        count(b"employee_id,timestamp,kind\r\nE001,2026-10-04 17:00,out\r\n"),
        1
    );
    assert_eq!(count(b"employee_id,timestamp,kind\r\n"), 0);
    assert_eq!(
        count(b"employee_id,timestamp,kind\r\nE001,2026-10-04 17:00,out\r\n\r\n"),
        1
    );
    assert_eq!(
        count(b"employee_id,timestamp,kind\nE001,2026-10-04 17:00,out"),
        1
    );
    assert_eq!(count(b"employee_id,timestamp,kind"), 0);
}

/// L2-10 途中の空の行のあとに CR の誤りのある行が来たら、上の空の行の誤りを先に返す（上から最初の1つ）。
#[test]
fn l2_10_blank_line_error_comes_before_a_stray_cr_below() {
    assert_eq!(
        kind_and_line(
            b"employee_id,timestamp,kind\nE001,2026-10-04 09:00,in\n\nE001,2026-10-04 17:00,o\rut\n"
        ),
        (ImportErrorKind::BlankLine, Some(3))
    );
    assert_eq!(
        kind_and_line(
            b"employee_id,timestamp,kind\r\nE001,2026-10-04 09:00,in\r\n\r\nE001,2026-10-04 17:00,o\rut\r\n"
        ),
        (ImportErrorKind::BlankLine, Some(3))
    );
    // 空の行がなければ、CR の誤りの行を返す。
    assert_eq!(
        kind_and_line(b"employee_id,timestamp,kind\nE001,2026-10-04 17:00,o\rut\n"),
        (ImportErrorKind::StrayCarriageReturn, Some(2))
    );
}
