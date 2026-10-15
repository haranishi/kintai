//! 入力の上限（`docs/spec/nfr.md`）を超えると不正な入力になる。AC-10.6。
//! ファイル 256MiB、1行 4096 バイト（改行を除く）、従業員10万人、日付は 2000-01-01〜2099-12-31。

use std::fmt::Write as _;
use std::io::Read;

use kintai_import::{
    ImportError, ImportErrorKind, MAX_EMPLOYEES, MAX_FILE_BYTES, MAX_LINE_BYTES, read_history,
    read_leaves, read_limited, read_punches,
};

fn parts(error: ImportError) -> (ImportErrorKind, Option<u64>, Option<&'static str>) {
    (error.kind, error.line, error.column)
}

/// AC-10.6: ファイルは 256MiB（268,435,456 バイト）まで。ちょうどは読め、1バイト超えたら不正な入力。
#[test]
fn ac10_6_file_size_limit() {
    assert_eq!(MAX_FILE_BYTES, 268_435_456);
    let limit = u64::try_from(MAX_FILE_BYTES).unwrap();
    let exact = read_limited(std::io::repeat(b'\n').take(limit)).unwrap();
    assert_eq!(exact.len(), MAX_FILE_BYTES);
    drop(exact);
    let over = read_limited(std::io::repeat(b'\n').take(limit + 1)).unwrap_err();
    assert_eq!(parts(over), (ImportErrorKind::FileTooLarge, None, None));
    // 終わりのない入力でも、上限の次の1バイトで止まる。
    let endless = read_limited(std::io::repeat(b'a')).unwrap_err();
    assert_eq!(endless.kind, ImportErrorKind::FileTooLarge);

    // バイト列を直接渡しても、まず大きさを見る（中身を読む前に拒否する）。
    let big = vec![b'\n'; MAX_FILE_BYTES + 1];
    for error in [
        read_punches(&big).unwrap_err(),
        read_leaves(&big).unwrap_err(),
        read_history(&big).unwrap_err(),
    ] {
        assert_eq!(parts(error), (ImportErrorKind::FileTooLarge, None, None));
    }
}

/// 読めない入力は入出力の誤りにする（中身は出さない）。
#[test]
fn io_errors_are_reported_without_details() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("秘密のパス /tmp/secret"))
        }
    }
    let error = read_limited(Broken).unwrap_err();
    assert_eq!(parts(error), (ImportErrorKind::Io, None, None));
    assert!(!error.to_string().contains("secret"));
}

/// 4096 バイトちょうどになる行（`E001,` ＋ 埋め ＋ `,in`）。
fn row_of(length: usize) -> String {
    let (prefix, suffix) = ("E001,", ",in");
    format!(
        "{prefix}{}{suffix}",
        "x".repeat(length - prefix.len() - suffix.len())
    )
}

/// AC-10.6: 1行は改行を除いて 4096 バイトまで。4097 バイトは、その行の不正な入力。
#[test]
fn ac10_6_line_length_limit() {
    assert_eq!(MAX_LINE_BYTES, 4096);
    let header = "employee_id,timestamp,kind\n";
    // ちょうど 4096 バイト（CRLF の CR は数えない）は長さの誤りにならず、値の誤りになる。
    let exact = format!("{header}{}\r\n", row_of(4096));
    assert_eq!(
        parts(read_punches(exact.as_bytes()).unwrap_err()),
        (
            ImportErrorKind::InvalidTimestamp,
            Some(2),
            Some("timestamp")
        )
    );
    let over = format!("{header}{}\n", row_of(4097));
    assert_eq!(
        parts(read_punches(over.as_bytes()).unwrap_err()),
        (ImportErrorKind::LineTooLong, Some(2), None)
    );
    // 最後の行（改行なし）も同じ。
    let last = format!("{header}{}", row_of(4097));
    assert_eq!(
        parts(read_punches(last.as_bytes()).unwrap_err()),
        (ImportErrorKind::LineTooLong, Some(2), None)
    );
    // 長さは、値の読み取りより先に、ファイル全体で見る。
    let order = format!("{header}E001,bad,in\n{}\n", row_of(4097));
    assert_eq!(
        parts(read_punches(order.as_bytes()).unwrap_err()),
        (ImportErrorKind::LineTooLong, Some(3), None)
    );
    // 見出しの行も、休暇CSV・履歴CSV も同じ。
    let long_header = format!("{}\n", "a".repeat(4097));
    for error in [
        read_punches(long_header.as_bytes()).unwrap_err(),
        read_leaves(long_header.as_bytes()).unwrap_err(),
        read_history(long_header.as_bytes()).unwrap_err(),
    ] {
        assert_eq!(parts(error), (ImportErrorKind::LineTooLong, Some(1), None));
    }
}

/// AC-10.6: 従業員は10万人まで。10万1人目が現れた行の不正な入力。同じ人の行は数を増やさない。
#[test]
fn ac10_6_at_most_100000_employees() {
    assert_eq!(MAX_EMPLOYEES, 100_000);
    let mut punches = String::from("employee_id,timestamp,kind\n");
    let mut leaves = String::from("employee_id,date,unit\n");
    for n in 0..MAX_EMPLOYEES {
        writeln!(punches, "E{n},2026-10-05 09:00,in").unwrap();
        writeln!(leaves, "E{n},2026-10-05,full").unwrap();
    }
    punches.push_str("E0,2026-10-05 18:00,out\n");
    assert_eq!(
        read_punches(punches.as_bytes()).unwrap().punches.len(),
        MAX_EMPLOYEES + 1
    );
    assert_eq!(read_leaves(leaves.as_bytes()).unwrap().len(), MAX_EMPLOYEES);

    punches.push_str("E100000,2026-10-05 09:00,in\n");
    assert_eq!(
        parts(read_punches(punches.as_bytes()).unwrap_err()),
        (
            ImportErrorKind::TooManyEmployees,
            Some(100_003),
            Some("employee_id")
        )
    );
    leaves.push_str("E100000,2026-10-05,full\n");
    assert_eq!(
        parts(read_leaves(leaves.as_bytes()).unwrap_err()),
        (
            ImportErrorKind::TooManyEmployees,
            Some(100_002),
            Some("employee_id")
        )
    );
}

/// AC-10.6: 日付は 2000-01-01〜2099-12-31。外は、その行と列の範囲外の誤り。
#[test]
fn ac10_6_date_range() {
    for (timestamp, inside) in [
        ("1999-12-31 23:59:59", false),
        ("2000-01-01 00:00:00", true),
        ("2000-01-01T00:00", true),
        ("2099-12-31 23:59:59", true),
        ("2100-01-01 00:00:00", false),
        ("1900-01-01 00:00", false),
    ] {
        let csv = format!("employee_id,timestamp,kind\nE001,{timestamp},in\n");
        let result = read_punches(csv.as_bytes());
        if inside {
            assert!(result.is_ok(), "{timestamp}");
        } else {
            assert_eq!(
                parts(result.unwrap_err()),
                (ImportErrorKind::OutOfRange, Some(2), Some("timestamp")),
                "{timestamp}"
            );
        }
    }
    for (date, inside) in [
        ("1999-12-31", false),
        ("2000-01-01", true),
        ("2099-12-31", true),
        ("2100-01-01", false),
    ] {
        let leave = format!("employee_id,date,unit\nE001,{date},half\n");
        let history = format!("employee_id,month_start,total_minutes\nE001,{date},60\n");
        let (leave, history) = (
            read_leaves(leave.as_bytes()),
            read_history(history.as_bytes()),
        );
        if inside {
            assert!(leave.is_ok() && history.is_ok(), "{date}");
        } else {
            assert_eq!(
                parts(leave.unwrap_err()),
                (ImportErrorKind::OutOfRange, Some(2), Some("date"))
            );
            assert_eq!(
                parts(history.unwrap_err()),
                (ImportErrorKind::OutOfRange, Some(2), Some("month_start"))
            );
        }
    }
}
