//! 打刻CSV・休暇CSV・履歴CSVの取込の試験。AC-01.8（`Shift_JIS` と UTF-8、見出しの必須）、
//! AC-08.7（従業員IDの形）、AC-10.7（エラーの文に行の中身を含めない）の例。
//! 試験データは合成（`tests/fixtures/synthetic/`）。

use kintai_core::punch::{EmployeeId, PunchKind, PunchTime};
use kintai_import::{
    HistoryRow, ImportError, ImportErrorKind, LeaveRow, LeaveUnit, MAX_MONTH_MINUTES, read_history,
    read_leaves, read_punches,
};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/synthetic/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn time(text: &str) -> PunchTime {
    text.parse().unwrap()
}

fn id(text: &str) -> EmployeeId {
    EmployeeId::new(text).unwrap()
}

fn punch_error(bytes: &[u8]) -> ImportError {
    read_punches(bytes).unwrap_err()
}

/// 誤りの種類・行番号・列名の組（比べやすくするため）。
type Parts = (ImportErrorKind, Option<u64>, Option<&'static str>);

fn at(kind: ImportErrorKind, line: u64, column: Option<&'static str>) -> Parts {
    (kind, Some(line), column)
}

fn parts(error: ImportError) -> Parts {
    (error.kind, error.line, error.column)
}

// ---- AC-01.8 文字コード ----

/// AC-01.8: UTF-8 の BOM つき・CRLF のファイルを読む。列の順序は見出しで決まり、`"` で囲んだ値も読む。
#[test]
fn ac01_8_utf8_with_bom_and_crlf() {
    let imported = read_punches(&fixture("punches-utf8-bom-crlf.csv")).unwrap();
    let got: Vec<(String, String, &str)> = imported
        .punches
        .iter()
        .map(|p| (p.employee.to_string(), p.time.to_string(), p.kind.code()))
        .collect();
    assert_eq!(
        got,
        [
            ("E001", "2026-10-05T09:00:30", "in"),
            ("E001", "2026-10-05T12:00:10", "break_start"),
            ("E001", "2026-10-05T12:59:50", "break_end"),
            ("E001", "2026-10-05T18:00:01", "out"),
            ("E002", "2026-10-05T22:00:00", "in"),
            ("E002", "2026-10-06T05:00:00", "out"),
            ("E001", "2026-10-06T09:00:00", "in"),
            ("E001", "2026-10-06T17:30:00", "out"),
        ]
        .map(|(e, t, k)| (e.to_string(), t.to_string(), k))
    );
    assert_eq!(imported.lines, [2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(imported.line(0), Some(2));
    assert_eq!(imported.line(8), None);
}

/// AC-01.8: `Shift_JIS`・CRLF のファイル（値は ASCII）を読む。
#[test]
fn ac01_8_shift_jis_with_crlf() {
    let imported = read_punches(&fixture("punches-sjis-crlf.csv")).unwrap();
    assert_eq!(imported.punches.len(), 2);
    assert_eq!(imported.punches[1].employee, id("E001"));
    assert_eq!(imported.punches[1].time, time("2026-10-05 18:00"));
    assert_eq!(imported.punches[1].kind, PunchKind::Out);
}

/// AC-01.8・AC-10.7: `Shift_JIS` の日本語の値は、文字コードの誤りでなく、その行と列の形の誤りになる
/// （`Shift_JIS` として読めている）。エラーの文に、値（日本語）を含めない。
#[test]
fn ac01_8_shift_jis_japanese_value_is_read_then_rejected() {
    let error = punch_error(&fixture("punches-sjis-japanese-id.csv"));
    assert_eq!(
        parts(error),
        at(ImportErrorKind::InvalidEmployeeId, 4, Some("employee_id"))
    );
    let message = error.to_string();
    assert!(
        message.starts_with("4行目の列 employee_id: 従業員IDの形が違います"),
        "{message}"
    );
    assert!(!message.contains("社員"), "{message}");

    // 半角カナ（Shift_JIS の1バイト文字。UTF-8 としては読めない）も同じ。
    let mut bytes = b"employee_id,timestamp,kind\nE001,2026-10-05 09:00,in\n".to_vec();
    bytes.extend_from_slice(b"E\xb1,2026-10-05 18:00,out\n");
    assert_eq!(
        parts(punch_error(&bytes)),
        at(ImportErrorKind::InvalidEmployeeId, 3, Some("employee_id"))
    );
}

/// AC-01.8: UTF-8 としても `Shift_JIS` としても読めないバイトは、その行の文字コードの誤り。
/// BOM つきなら UTF-8 だけで読む（`Shift_JIS` に切り替えない）。
#[test]
fn ac01_8_unreadable_bytes() {
    let header = b"employee_id,timestamp,kind\nE001,2026-10-05 09:00,in\n".to_vec();
    for bad in [
        &b"E\x81 ,2026-10-05 18:00,out\n"[..],
        b"E\xff,2026-10-05 18:00,out\n",
        b"E\x81",
    ] {
        let mut bytes = header.clone();
        bytes.extend_from_slice(bad);
        assert_eq!(
            parts(punch_error(&bytes)),
            at(ImportErrorKind::Encoding, 3, None),
            "{bad:?}"
        );
    }
    let mut with_bom = b"\xef\xbb\xbf".to_vec();
    with_bom.extend_from_slice(b"employee_id,timestamp,kind\n");
    with_bom.extend_from_slice(b"E\xb1,2026-10-05 18:00,out\n");
    assert_eq!(
        parts(punch_error(&with_bom)),
        at(ImportErrorKind::Encoding, 2, None)
    );
    // BOM を取り除かなければ見出しの1列目が知らない列名になる。取り除くので読める。
    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend_from_slice(b"employee_id,timestamp,kind\nE001,2026-10-05 09:00,in\n");
    assert_eq!(read_punches(&bom).unwrap().punches.len(), 1);
}

// ---- AC-01.8 見出し ----

/// AC-01.8: 見出しのない CSV は拒否する（1行目の値が知らない列名になる）。文に、見出しの列名を示す。
#[test]
fn ac01_8_rejects_csv_without_header() {
    let error = punch_error(&fixture("punches-no-header.csv"));
    assert_eq!(
        parts(error),
        at(ImportErrorKind::UnknownColumn { position: 1 }, 1, None)
    );
    let message = error.to_string();
    assert_eq!(
        message,
        "1行目: 見出しの1列目が、知らない列名です（1行目は見出しにしてください: employee_id,timestamp,kind）"
    );
    assert!(!message.contains("E001"), "{message}");
}

/// AC-01.8: 見出しの誤り（空のファイル、空の1行目、足りない列、重なる列、余分な列、大文字）。
#[test]
fn ac01_8_header_errors() {
    assert_eq!(
        parts(punch_error(b"")),
        at(ImportErrorKind::MissingHeader, 1, None)
    );
    assert_eq!(
        parts(punch_error(b"\xef\xbb\xbf")),
        at(ImportErrorKind::MissingHeader, 1, None)
    );
    assert_eq!(
        parts(punch_error(b"\nemployee_id,timestamp,kind\n")),
        at(ImportErrorKind::MissingHeader, 1, None)
    );
    assert_eq!(
        parts(punch_error(
            b"employee_id,timestamp\nE001,2026-10-05 09:00\n"
        )),
        at(ImportErrorKind::MissingColumn, 1, Some("kind"))
    );
    assert_eq!(
        parts(punch_error(b"employee_id,kind,kind\n")),
        at(ImportErrorKind::DuplicateColumn { position: 3 }, 1, None)
    );
    assert_eq!(
        parts(punch_error(b"employee_id,timestamp,kind,kind\n")),
        at(ImportErrorKind::DuplicateColumn { position: 4 }, 1, None)
    );
    assert_eq!(
        parts(punch_error(b"employee_id,timestamp,kind,memo\n")),
        at(ImportErrorKind::UnknownColumn { position: 4 }, 1, None)
    );
    assert_eq!(
        parts(punch_error(b"Employee_ID,timestamp,kind\n")),
        at(ImportErrorKind::UnknownColumn { position: 1 }, 1, None)
    );
    assert_eq!(
        parts(punch_error(b"employee_id, timestamp,kind\n")),
        at(ImportErrorKind::UnknownColumn { position: 2 }, 1, None)
    );
    // 見出しだけのファイルは、打刻0件。
    assert_eq!(
        read_punches(b"employee_id,timestamp,kind\r\n")
            .unwrap()
            .punches,
        Vec::new()
    );
    // 見出しを引用符で囲んでもよい。
    assert_eq!(
        read_punches(b"\"employee_id\",\"timestamp\",\"kind\"\nE001,2026-10-05 09:00,in\n")
            .unwrap()
            .punches
            .len(),
        1
    );
}

// ---- 行の形 ----

/// 値の数の違い、引用符の誤り、改行でない CR、空の行（末尾だけ許す）。
#[test]
fn malformed_rows() {
    let header = "employee_id,timestamp,kind\n";
    let cases: [(&str, Parts); 9] = [
        (
            "E001,2026-10-05 09:00\n",
            at(ImportErrorKind::FieldCount { expected: 3 }, 2, None),
        ),
        (
            "E001,2026-10-05 09:00,in,\n",
            at(ImportErrorKind::FieldCount { expected: 3 }, 2, None),
        ),
        (
            "\"E001,2026-10-05 09:00,in\n",
            at(ImportErrorKind::Quote, 2, None),
        ),
        (
            "\"E001\"x,2026-10-05 09:00,in\n",
            at(ImportErrorKind::Quote, 2, None),
        ),
        (
            "E0\"01,2026-10-05 09:00,in\n",
            at(ImportErrorKind::Quote, 2, None),
        ),
        (
            "E001,2026-10-05 09:00,in\rE001,2026-10-05 18:00,out\n",
            at(ImportErrorKind::StrayCarriageReturn, 2, None),
        ),
        (
            "E001,2026-10-05 09:00,in\n\nE001,2026-10-05 18:00,out\n",
            at(ImportErrorKind::BlankLine, 3, None),
        ),
        (
            "E001,2026-10-05 09:00,in\n\r\nE001,2026-10-05 18:00,out\n",
            at(ImportErrorKind::BlankLine, 3, None),
        ),
        (
            "\"E0\"\"01\",2026-10-05 09:00,in\n",
            at(ImportErrorKind::InvalidEmployeeId, 2, Some("employee_id")),
        ),
    ];
    for (rows, expected) in cases {
        let csv = format!("{header}{rows}");
        assert_eq!(parts(punch_error(csv.as_bytes())), expected, "{rows:?}");
    }
    // 末尾の空の行は許す。最後の行に改行がなくてもよい。
    for tail in ["\n\n\n", "\r\n\r\n", ""] {
        let csv = format!("{header}E001,2026-10-05 09:00,in{tail}");
        assert_eq!(
            read_punches(csv.as_bytes()).unwrap().punches.len(),
            1,
            "{tail:?}"
        );
    }
    // `"` で囲んだ値の中の `,` は区切りでない（その値は日時の形に合わないので拒否される）。
    let comma = format!("{header}E001,\"2026-10-05,09:00\",in\n");
    assert_eq!(
        parts(punch_error(comma.as_bytes())),
        at(ImportErrorKind::InvalidTimestamp, 2, Some("timestamp"))
    );
}

/// 値の誤りは、行番号と列名で示す。1つの行の中では、列の並びによらず `employee_id` → 2列目 → 3列目の順。
#[test]
fn value_errors_point_at_line_and_column() {
    let cases: [(&str, Parts); 8] = [
        (
            "E 001,2026-10-05 09:00,in",
            at(ImportErrorKind::InvalidEmployeeId, 3, Some("employee_id")),
        ),
        (
            "=cmd,2026-10-05 09:00,in",
            at(ImportErrorKind::InvalidEmployeeId, 3, Some("employee_id")),
        ),
        (
            "E001,2026-10-05 9:00,in",
            at(ImportErrorKind::InvalidTimestamp, 3, Some("timestamp")),
        ),
        (
            "E001,2026-02-30 09:00,in",
            at(ImportErrorKind::InvalidTimestamp, 3, Some("timestamp")),
        ),
        (
            "E001,2026-10-05 24:00,in",
            at(ImportErrorKind::InvalidTimestamp, 3, Some("timestamp")),
        ),
        (
            "E001,2026-10-05 09:00,IN",
            at(ImportErrorKind::InvalidKind, 3, Some("kind")),
        ),
        (
            "E001,2026-10-05 09:00, in",
            at(ImportErrorKind::InvalidKind, 3, Some("kind")),
        ),
        (
            "-E1,bad,bad",
            at(ImportErrorKind::InvalidEmployeeId, 3, Some("employee_id")),
        ),
    ];
    for (row, expected) in cases {
        let csv = format!("employee_id,timestamp,kind\nE001,2026-10-05 08:00,in\n{row}\n");
        assert_eq!(parts(punch_error(csv.as_bytes())), expected, "{row}");
    }
    // 列の並びが違っても、employee_id を先に見る。
    let csv = "kind,timestamp,employee_id\nbad,bad,-E1\n";
    assert_eq!(
        parts(punch_error(csv.as_bytes())),
        at(ImportErrorKind::InvalidEmployeeId, 2, Some("employee_id"))
    );
    // 上の行の誤りが先に返る。
    let csv = "employee_id,timestamp,kind\nE001,bad,in\n-E1,2026-10-05 09:00,in\n";
    assert_eq!(
        parts(punch_error(csv.as_bytes())),
        at(ImportErrorKind::InvalidTimestamp, 2, Some("timestamp"))
    );
}

// ---- AC-08.7 従業員ID ----

/// AC-08.7: 従業員IDは `[A-Za-z0-9][A-Za-z0-9._-]{0,63}` だけを受け付ける（式として実行される先頭の文字を拒否する）。
#[test]
fn ac08_7_employee_id_in_csv() {
    let longest = format!("A{}", "b".repeat(63));
    for ok in ["E001", "a", "e.g_1-2", longest.as_str()] {
        let csv = format!("employee_id,timestamp,kind\n{ok},2026-10-05 09:00,in\n");
        assert_eq!(
            read_punches(csv.as_bytes()).unwrap().punches[0]
                .employee
                .as_str(),
            ok
        );
    }
    let too_long = format!("A{}", "b".repeat(64));
    for bad in [
        "=SUM(A1)",
        "+81",
        "-1",
        "@SUM",
        "\"=1\"",
        "_E",
        ".E",
        "E=1",
        "E+1",
        "E@1",
        "",
        too_long.as_str(),
    ] {
        let csv = format!("employee_id,timestamp,kind\n{bad},2026-10-05 09:00,in\n");
        assert_eq!(
            parts(punch_error(csv.as_bytes())),
            at(ImportErrorKind::InvalidEmployeeId, 2, Some("employee_id")),
            "{bad}"
        );
    }
}

// ---- 休暇CSV・履歴CSV ----

/// 休暇CSV を読む。`hours` も読み（拒否は使う側）、ほかの単位は不正な入力。
#[test]
fn leaves_csv() {
    let rows = read_leaves(&fixture("leaves.csv")).unwrap();
    assert_eq!(
        rows,
        [
            LeaveRow {
                line: 2,
                employee: id("E001"),
                date: "2026-10-07".parse().unwrap(),
                unit: LeaveUnit::Full,
            },
            LeaveRow {
                line: 3,
                employee: id("E001"),
                date: "2026-10-08".parse().unwrap(),
                unit: LeaveUnit::Half,
            },
            LeaveRow {
                line: 4,
                employee: id("E002"),
                date: "2026-10-09".parse().unwrap(),
                unit: LeaveUnit::Hours,
            },
        ]
    );
    for unit in [LeaveUnit::Full, LeaveUnit::Half, LeaveUnit::Hours] {
        assert_eq!(LeaveUnit::from_code(unit.code()), Some(unit));
    }
    for (row, expected) in [
        (
            "E001,2026-10-07,day",
            at(ImportErrorKind::InvalidUnit, 2, Some("unit")),
        ),
        (
            "E001,2026-10-07,Full",
            at(ImportErrorKind::InvalidUnit, 2, Some("unit")),
        ),
        (
            "E001,2026-10-07 09:00,full",
            at(ImportErrorKind::InvalidDate, 2, Some("date")),
        ),
        (
            "E001,2026-13-01,full",
            at(ImportErrorKind::InvalidDate, 2, Some("date")),
        ),
        (
            "E001,1999-12-31,full",
            at(ImportErrorKind::OutOfRange, 2, Some("date")),
        ),
    ] {
        let csv = format!("employee_id,date,unit\n{row}\n");
        assert_eq!(
            parts(read_leaves(csv.as_bytes()).unwrap_err()),
            expected,
            "{row}"
        );
    }
    let error = read_leaves(b"employee_id,timestamp,kind\n").unwrap_err();
    assert_eq!(
        error.to_string(),
        "1行目: 見出しの2列目が、知らない列名です（1行目は見出しにしてください: employee_id,date,unit）"
    );
}

/// 履歴CSV を読む。`total_minutes` は数字だけの 0〜44640。
#[test]
fn history_csv() {
    let rows = read_history(&fixture("history.csv")).unwrap();
    assert_eq!(
        rows,
        [
            HistoryRow {
                line: 2,
                employee: id("E001"),
                month_start: "2026-08-01".parse().unwrap(),
                total_minutes: 600,
            },
            HistoryRow {
                line: 3,
                employee: id("E001"),
                month_start: "2026-09-01".parse().unwrap(),
                total_minutes: 0,
            },
            HistoryRow {
                line: 4,
                employee: id("E002"),
                month_start: "2026-09-01".parse().unwrap(),
                total_minutes: MAX_MONTH_MINUTES,
            },
        ]
    );
    assert_eq!(MAX_MONTH_MINUTES, 44_640);
    for bad in [
        "44641",
        "-1",
        "+5",
        "1.0",
        "1e3",
        " 5",
        "5 ",
        "",
        "99999999999999999999",
        "６",
    ] {
        let csv = format!("employee_id,month_start,total_minutes\nE001,2026-09-01,{bad}\n");
        assert_eq!(
            parts(read_history(csv.as_bytes()).unwrap_err()),
            at(ImportErrorKind::InvalidMinutes, 2, Some("total_minutes")),
            "{bad}"
        );
    }
    let csv = "employee_id,month_start,total_minutes\nE001,2100-01-01,5\n";
    assert_eq!(
        parts(read_history(csv.as_bytes()).unwrap_err()),
        at(ImportErrorKind::OutOfRange, 2, Some("month_start"))
    );
    // 先頭の0は数字なので読める。
    let csv = "employee_id,month_start,total_minutes\nE001,2026-09-01,0600\n";
    assert_eq!(read_history(csv.as_bytes()).unwrap()[0].total_minutes, 600);
}

/// AC-10.7: どの誤りの文にも、入力の行の中身が入らない（固定の文、行番号、この版の列名だけ）。
#[test]
fn ac10_7_messages_have_no_row_content() {
    let secret = "Zq9Secret";
    let cases = [
        format!("employee_id,timestamp,kind\n{secret}!,2026-10-05 09:00,in\n"),
        format!("employee_id,timestamp,kind\nE001,{secret},in\n"),
        format!("employee_id,timestamp,kind\nE001,2026-10-05 09:00,{secret}\n"),
        format!("employee_id,timestamp,{secret}\n"),
        format!("{secret},timestamp,kind\n"),
        format!("employee_id,timestamp,kind\n\"{secret},2026-10-05 09:00,in\n"),
        format!("employee_id,timestamp,kind\n{secret},2026-10-05 09:00\n"),
        format!("employee_id,timestamp,kind\nE001,2026-10-05 09:00,in\r{secret}\n"),
        format!("employee_id,timestamp,kind\nE001,2026-10-05 09:00,in\n\n{secret}\n"),
    ];
    for csv in &cases {
        let message = punch_error(csv.as_bytes()).to_string();
        assert!(!message.contains(secret), "{message}");
        assert!(!message.contains("Zq9"), "{message}");
    }
    let leave = format!("employee_id,date,unit\nE001,{secret},full\n");
    assert!(
        !read_leaves(leave.as_bytes())
            .unwrap_err()
            .to_string()
            .contains(secret)
    );
    let history = format!("employee_id,month_start,total_minutes\nE001,2026-09-01,{secret}\n");
    assert!(
        !read_history(history.as_bytes())
            .unwrap_err()
            .to_string()
            .contains(secret)
    );
}

/// すべての誤りの種類の文が、決まった形で出る（行の中身を持たない型であることも、ここで見る）。
#[test]
fn every_error_kind_has_a_message() {
    let kinds = [
        ImportErrorKind::Io,
        ImportErrorKind::FileTooLarge,
        ImportErrorKind::LineTooLong,
        ImportErrorKind::Encoding,
        ImportErrorKind::StrayCarriageReturn,
        ImportErrorKind::BlankLine,
        ImportErrorKind::Quote,
        ImportErrorKind::MissingHeader,
        ImportErrorKind::UnknownColumn { position: 2 },
        ImportErrorKind::DuplicateColumn { position: 3 },
        ImportErrorKind::MissingColumn,
        ImportErrorKind::FieldCount { expected: 3 },
        ImportErrorKind::InvalidEmployeeId,
        ImportErrorKind::TooManyEmployees,
        ImportErrorKind::InvalidTimestamp,
        ImportErrorKind::InvalidDate,
        ImportErrorKind::OutOfRange,
        ImportErrorKind::InvalidKind,
        ImportErrorKind::InvalidUnit,
        ImportErrorKind::InvalidMinutes,
    ];
    let mut seen = std::collections::BTreeSet::new();
    for kind in kinds {
        let message = kind.to_string();
        assert!(seen.insert(message), "{kind:?}");
    }
    let whole = ImportError {
        kind: ImportErrorKind::FileTooLarge,
        line: None,
        column: None,
        header: None,
    };
    assert_eq!(
        whole.to_string(),
        "ファイルが大きすぎます（上限は 268435456 バイト）"
    );
}
