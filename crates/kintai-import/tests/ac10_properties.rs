//! 取込の性質の試験（proptest）。AC-10.5（任意のバイト列・壊れたCSVでパニックしない）、
//! AC-10.7（誤りの文に行の中身を含めない）、AC-01.8（UTF-8・BOM・CRLF・列の並び・引用符によらず同じ値に読む）、
//! AC-10.4（同じ入力は同じ結果）。

use kintai_import::{ImportError, read_history, read_leaves, read_punches};
use proptest::prelude::*;

/// 誤りの行番号が、ファイルの行の数を超えない。
fn line_is_within(error: &ImportError, bytes: &[u8]) {
    let lines = bytes.split(|byte| *byte == b'\n').count();
    if let Some(line) = error.line {
        assert!(line >= 1, "{error:?}");
        assert!(usize::try_from(line).unwrap() <= lines, "{error:?}");
    }
}

/// 3つの読み取りをすべて試す（パニックしないことと、行番号の範囲）。
fn read_all(bytes: &[u8]) {
    if let Err(error) = read_punches(bytes) {
        line_is_within(&error, bytes);
    }
    if let Err(error) = read_leaves(bytes) {
        line_is_within(&error, bytes);
    }
    if let Err(error) = read_history(bytes) {
        line_is_within(&error, bytes);
    }
}

/// 正しい打刻の行（従業員ID, 日時, 種別）。
fn valid_row() -> impl Strategy<Value = (String, String, &'static str)> {
    (
        "[A-Za-z0-9][A-Za-z0-9._-]{0,9}",
        2000_i32..=2099,
        1_u32..=12,
        1_u32..=28,
        0_u32..24,
        0_u32..60,
        prop::option::of(0_u32..60),
        prop::bool::ANY,
        prop::sample::select(vec!["in", "out", "break_start", "break_end"]),
    )
        .prop_map(|(id, year, month, day, hour, minute, second, t, kind)| {
            let separator = if t { 'T' } else { ' ' };
            let time = match second {
                Some(second) => {
                    format!(
                        "{year:04}-{month:02}-{day:02}{separator}{hour:02}:{minute:02}:{second:02}"
                    )
                }
                None => format!("{year:04}-{month:02}-{day:02}{separator}{hour:02}:{minute:02}"),
            };
            (id, time, kind)
        })
}

/// 書き方の選び方（列の並び、CRLF、BOM、引用符、末尾の改行）。
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] // 試験の入力の組み合わせを、そのまま表す。
struct Style {
    order: [usize; 3],
    crlf: bool,
    bom: bool,
    quote: bool,
    trailing_newline: bool,
}

fn style() -> impl Strategy<Value = Style> {
    (
        prop::sample::select(vec![
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ]),
        prop::bool::ANY,
        prop::bool::ANY,
        prop::bool::ANY,
        prop::bool::ANY,
    )
        .prop_map(|(order, crlf, bom, quote, trailing_newline)| Style {
            order,
            crlf,
            bom,
            quote,
            trailing_newline,
        })
}

fn write_csv(rows: &[(String, String, &str)], style: &Style) -> Vec<u8> {
    let names = ["employee_id", "timestamp", "kind"];
    let newline = if style.crlf { "\r\n" } else { "\n" };
    let cell = |text: &str| {
        if style.quote {
            format!("\"{text}\"")
        } else {
            text.to_string()
        }
    };
    let mut lines = vec![
        style
            .order
            .iter()
            .map(|&i| cell(names[i]))
            .collect::<Vec<_>>()
            .join(","),
    ];
    for (id, time, kind) in rows {
        let values = [id.as_str(), time.as_str(), kind];
        lines.push(
            style
                .order
                .iter()
                .map(|&i| cell(values[i]))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    let mut text = lines.join(newline);
    if style.trailing_newline {
        text.push_str(newline);
    }
    let mut bytes = if style.bom {
        b"\xef\xbb\xbf".to_vec()
    } else {
        Vec::new()
    };
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

/// 壊し方: 位置と、入れるバイト（または消す）。
fn mutation() -> impl Strategy<Value = (prop::sample::Index, Option<u8>)> {
    (
        any::<prop::sample::Index>(),
        prop::option::of(prop::sample::select(vec![
            b'"', b',', b'\r', b'\n', b' ', b'\t', 0x00, 0x81, 0xb1, 0xef, 0xbb, 0xbf, 0xff, b'=',
            b'-',
        ])),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// AC-10.5: 任意のバイト列で、パニックせず、誤りの行番号はファイルの中を指す。
    #[test]
    fn ac10_5_arbitrary_bytes_do_not_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        read_all(&bytes);
    }

    /// AC-10.5: 見出しつきで、ASCII と改行・引用符・Shift_JIS の先頭バイトが混ざった入力でパニックしない。
    #[test]
    fn ac10_5_csv_like_bytes_do_not_panic(
        body in prop::collection::vec(
            prop::sample::select(b"E01,:-T \"\r\n2in_outbreak\x81\xb1\xef".to_vec()),
            0..300,
        ),
        header in prop::sample::select(vec![
            &b"employee_id,timestamp,kind\n"[..],
            b"employee_id,date,unit\r\n",
            b"employee_id,month_start,total_minutes\n",
        ]),
    ) {
        let mut bytes = header.to_vec();
        bytes.extend_from_slice(&body);
        read_all(&bytes);
    }

    /// AC-01.8・AC-10.4: 正しいCSVは、列の並び・CRLF・BOM・引用符・末尾の改行によらず、同じ値と行番号に読める。
    #[test]
    fn ac01_8_valid_csv_round_trips(
        rows in prop::collection::vec(valid_row(), 0..30),
        style in style(),
    ) {
        let bytes = write_csv(&rows, &style);
        let imported = read_punches(&bytes).unwrap();
        prop_assert_eq!(imported.punches.len(), rows.len());
        for (index, (punch, (id, time, kind))) in imported.punches.iter().zip(&rows).enumerate() {
            prop_assert_eq!(punch.employee.as_str(), id.as_str());
            let expected = if time.len() == 16 { format!("{time}:00") } else { time.clone() };
            prop_assert_eq!(punch.time.to_string(), expected.replace(' ', "T"));
            prop_assert_eq!(punch.kind.code(), *kind);
            prop_assert_eq!(imported.line(index), Some(u64::try_from(index).unwrap() + 2));
        }
        // 同じ入力は同じ結果。
        prop_assert_eq!(read_punches(&bytes).unwrap(), imported);
    }

    /// AC-10.5: 正しいCSVを1バイト壊しても（足す・消す）パニックせず、読めたなら値は正しい形のまま。
    #[test]
    fn ac10_5_broken_csv_does_not_panic(
        rows in prop::collection::vec(valid_row(), 1..10),
        style in style(),
        mutations in prop::collection::vec(mutation(), 1..4),
    ) {
        let mut bytes = write_csv(&rows, &style);
        for (at, insert) in mutations {
            let position = at.index(bytes.len() + 1);
            match insert {
                Some(byte) => bytes.insert(position, byte),
                None if position < bytes.len() => {
                    bytes.remove(position);
                }
                None => bytes.truncate(position / 2),
            }
        }
        match read_punches(&bytes) {
            Ok(imported) => {
                prop_assert_eq!(imported.punches.len(), imported.lines.len());
                for punch in &imported.punches {
                    prop_assert!(punch.employee.as_str().len() <= 64);
                }
            }
            Err(error) => line_is_within(&error, &bytes),
        }
    }

    /// AC-10.7: 行の値に混ぜた目印の文字列が、誤りの文に出ない（どの列が壊れていても）。
    #[test]
    fn ac10_7_error_message_has_no_row_content(
        rows in prop::collection::vec(valid_row(), 1..6),
        style in style(),
        token in "Zq9[0-9]{6}",
        target in (any::<prop::sample::Index>(), 0_usize..3),
        breaker in prop::sample::select(vec!["!", " ", "*", "\u{3042}"]),
    ) {
        let mut rows = rows;
        let (row, column) = target;
        let row = row.index(rows.len());
        let broken = format!("{token}{breaker}");
        match column {
            0 => rows[row].0 = format!("{breaker}{token}"),
            1 => rows[row].1 = broken,
            _ => rows[row].2 = "zzz",
        }
        let bytes = write_csv(&rows, &style);
        let mut texts = Vec::new();
        if let Err(error) = read_punches(&bytes) {
            texts.push(error.to_string());
        }
        if let Err(error) = read_leaves(&bytes) {
            texts.push(error.to_string());
        }
        if let Err(error) = read_history(&bytes) {
            texts.push(error.to_string());
        }
        prop_assert!(!texts.is_empty());
        for text in texts {
            // 目印の先頭の `Zq9` は、どの誤りの文にも無い並び。
            prop_assert!(!text.contains("Zq9"), "{}", text);
        }
    }
}
