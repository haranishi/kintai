//! 打刻CSV・休暇CSV・36協定の履歴CSVの取込（UTF-8・`Shift_JIS`）。
//!
//! - [`read_punches`]: 打刻CSV `employee_id,timestamp,kind`（`docs/spec/punch.md`）。
//! - [`read_leaves`]: 休暇CSV `employee_id,date,unit`（`docs/spec/cli.md`）。
//! - [`read_history`]: 36協定の履歴CSV `employee_id,month_start,total_minutes`（`docs/spec/cli.md`）。
//!
//! 読み方の決まり:
//!
//! - 文字コードは UTF-8（先頭の BOM は取り除く）か `Shift_JIS`（Windows の CP932 を含む）。
//!   BOM がなく UTF-8 として正しければ UTF-8、そうでなければ `Shift_JIS` として読む。
//! - 改行は CRLF か LF。改行（LF）の前にない CR は不正。
//! - 1行目は見出し（列名）で、必須。列の順序は見出しで決める。列名が足りない・知らない・重なる見出しは拒否する
//!   （見出しのない CSV は、1行目の値が知らない列名になるので拒否される）。余分な列も拒否する。
//! - 値は `"` で囲んでもよい（囲みの中の `""` は `"` が1つ）。囲みの中で改行はできない。
//!   値の前後の空白は取り除かない（空白のある値は、その列の形に合わないので拒否される）。
//! - 空の行は、ファイルの末尾にだけあってよい。
//! - 入力の上限（`docs/spec/nfr.md`）: ファイル 256MiB（[`MAX_FILE_BYTES`]）、1行 4096 バイト
//!   （改行を除く。[`MAX_LINE_BYTES`]）、従業員10万人（[`MAX_EMPLOYEES`]）、日付は 2000-01-01〜2099-12-31。
//! - エラーの文には、入力の行の中身を含めない（AC-10.7）。行番号と、この版が決めた列名
//!   （見出しの誤りは列の位置）だけを示す。
//! - 誤りは、ファイルの大きさ → 行の長さ → 文字コード → 見出し → データの行（上から順。
//!   1つの行の中では、列の並びによらず `employee_id` → 2列目 → 3列目の順）の順に見て、最初に見つけたものを返す。
//!
//! ```
//! use kintai_import::read_punches;
//!
//! let csv = "kind,employee_id,timestamp\r\nin,E001,2026-10-07 09:00:30\r\nout,E001,2026-10-07T18:00\r\n";
//! let imported = read_punches(csv.as_bytes()).unwrap();
//! assert_eq!(imported.punches.len(), 2);
//! assert_eq!(imported.punches[0].time.to_string(), "2026-10-07T09:00:30");
//! assert_eq!(imported.line(1), Some(3));
//!
//! let error = read_punches(b"E001,2026-10-07 09:00,in\n").unwrap_err();
//! assert_eq!(error.to_string(), "1行目: 見出しの1列目が、知らない列名です（1行目は見出しにしてください: employee_id,timestamp,kind）");
//! ```

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;

use encoding_rs::{DecoderResult, SHIFT_JIS};
use kintai_core::punch::{EmployeeId, Punch, PunchKind, PunchTime};
use kintai_core::{CoreError, Date};

/// ファイルの大きさの上限（バイト。256MiB）。
pub const MAX_FILE_BYTES: usize = 256 * 1024 * 1024;
/// 1行の長さの上限（バイト。改行の CR・LF を除く）。
pub const MAX_LINE_BYTES: usize = 4096;
/// 1つのファイルに現れてよい従業員（異なる従業員ID）の数の上限。
pub const MAX_EMPLOYEES: usize = 100_000;
/// 履歴CSVの `total_minutes` の上限（31日×1440分。どの月の期間も31日以下なので、Tはこれを超えない）。
pub const MAX_MONTH_MINUTES: i64 = 31 * 1440;

/// 打刻CSVの列。
pub const PUNCH_COLUMNS: [&str; 3] = ["employee_id", "timestamp", "kind"];
/// 休暇CSVの列。
pub const LEAVE_COLUMNS: [&str; 3] = ["employee_id", "date", "unit"];
/// 36協定の履歴CSVの列。
pub const HISTORY_COLUMNS: [&str; 3] = ["employee_id", "month_start", "total_minutes"];

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// 読み込んだ打刻。`punches[i]` は、ファイルの `lines[i]` 行目。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportedPunches {
    /// 打刻（ファイルの行の順）。
    pub punches: Vec<Punch>,
    /// 各打刻の行番号（1から。見出しが1行目）。
    pub lines: Vec<u64>,
}

impl ImportedPunches {
    /// `index` 番目の打刻の行番号（打刻の対の誤りや異常の番号を、行番号に直すのに使う）。
    #[must_use]
    pub fn line(&self, index: usize) -> Option<u64> {
        self.lines.get(index).copied()
    }
}

/// 休暇の単位（休暇CSVの `unit`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LeaveUnit {
    /// 1日（`full`。2半日）。
    Full,
    /// 半日（`half`。1半日）。
    Half,
    /// 時間単位（`hours`）。この版は対応外（`LeaveHourly`）で、拒否するのは使う側。
    Hours,
}

impl LeaveUnit {
    /// CSV の値。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            LeaveUnit::Full => "full",
            LeaveUnit::Half => "half",
            LeaveUnit::Hours => "hours",
        }
    }

    /// CSV の値から読む。
    #[must_use]
    pub fn from_code(text: &str) -> Option<LeaveUnit> {
        [LeaveUnit::Full, LeaveUnit::Half, LeaveUnit::Hours]
            .into_iter()
            .find(|unit| unit.code() == text)
    }
}

/// 休暇CSVの1行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaveRow {
    /// 行番号（1から）。
    pub line: u64,
    /// 従業員ID。
    pub employee: EmployeeId,
    /// 取得した日。
    pub date: Date,
    /// 単位。
    pub unit: LeaveUnit,
}

/// 36協定の履歴CSVの1行（`months` より前の協定月の T＝O＋H）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRow {
    /// 行番号（1から）。
    pub line: u64,
    /// 従業員ID。
    pub employee: EmployeeId,
    /// 協定月の初日（協定月の初日であるかは、使う側が方針で確かめる）。
    pub month_start: Date,
    /// T（分。0〜[`MAX_MONTH_MINUTES`]）。
    pub total_minutes: i64,
}

/// 取込の誤りの種類。どれも不正な入力（終了コード2）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImportErrorKind {
    /// ファイルを読めない（入出力の誤り）。
    Io,
    /// ファイルが [`MAX_FILE_BYTES`] を超える。
    FileTooLarge,
    /// 行が [`MAX_LINE_BYTES`] バイトを超える（改行を除く）。
    LineTooLong,
    /// UTF-8 としても `Shift_JIS` としても読めない。
    Encoding,
    /// 改行（LF）の前にない CR がある。
    StrayCarriageReturn,
    /// 空の行がある（ファイルの末尾の空の行は許す）。
    BlankLine,
    /// 引用符（`"`）の使い方が正しくない。
    Quote,
    /// 見出しの行がない（空のファイル、または1行目が空）。
    MissingHeader,
    /// 見出しに知らない列名がある。見出しのない CSV もこれになる。
    UnknownColumn {
        /// 列の位置（1から）。
        position: usize,
    },
    /// 見出しに、前の列と同じ列名がある。
    DuplicateColumn {
        /// 列の位置（1から）。
        position: usize,
    },
    /// 見出しに必要な列がない（列名は [`ImportError::column`]）。
    MissingColumn,
    /// 行の値の数が、見出しの列の数と違う。
    FieldCount {
        /// 見出しの列の数。
        expected: usize,
    },
    /// 従業員IDの形が `[A-Za-z0-9][A-Za-z0-9._-]{0,63}` でない（AC-08.7）。
    InvalidEmployeeId,
    /// 従業員が [`MAX_EMPLOYEES`] 人を超える。
    TooManyEmployees,
    /// 日時の書式が違う（`YYYY-MM-DD HH:MM[:SS]`、区切りは空白か `T`）。
    InvalidTimestamp,
    /// 日付の書式が違う（`YYYY-MM-DD`）。
    InvalidDate,
    /// 日付が 2000-01-01〜2099-12-31 の外。
    OutOfRange,
    /// 打刻の種別が `in` `out` `break_start` `break_end` のどれでもない。
    InvalidKind,
    /// 休暇の単位が `full` `half` `hours` のどれでもない。
    InvalidUnit,
    /// 分の数が、0〜[`MAX_MONTH_MINUTES`] の整数（数字だけ）でない。
    InvalidMinutes,
}

impl fmt::Display for ImportErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportErrorKind::Io => f.write_str("ファイルを読めませんでした"),
            ImportErrorKind::FileTooLarge => {
                write!(
                    f,
                    "ファイルが大きすぎます（上限は {MAX_FILE_BYTES} バイト）"
                )
            }
            ImportErrorKind::LineTooLong => write!(
                f,
                "行が長すぎます（上限は改行を除いて {MAX_LINE_BYTES} バイト）"
            ),
            ImportErrorKind::Encoding => {
                f.write_str("文字コードを読めません（UTF-8 か Shift_JIS にしてください）")
            }
            ImportErrorKind::StrayCarriageReturn => {
                f.write_str("改行でない CR があります（改行は CRLF か LF にしてください）")
            }
            ImportErrorKind::BlankLine => f.write_str("空の行があります"),
            ImportErrorKind::Quote => f.write_str("引用符（\"）の使い方が正しくありません"),
            ImportErrorKind::MissingHeader => {
                f.write_str("見出しの行がありません（1行目に列名が要ります）")
            }
            ImportErrorKind::UnknownColumn { position } => {
                write!(f, "見出しの{position}列目が、知らない列名です")
            }
            ImportErrorKind::DuplicateColumn { position } => {
                write!(f, "見出しの{position}列目が、前の列と同じ列名です")
            }
            ImportErrorKind::MissingColumn => f.write_str("見出しにこの列がありません"),
            ImportErrorKind::FieldCount { expected } => {
                write!(f, "値の数が見出しの列の数（{expected}）と違います")
            }
            ImportErrorKind::InvalidEmployeeId => f.write_str(
                "従業員IDの形が違います（英数字で始まり、英数字と . _ - だけの64文字まで）",
            ),
            ImportErrorKind::TooManyEmployees => {
                write!(f, "従業員が多すぎます（上限は {MAX_EMPLOYEES} 人）")
            }
            ImportErrorKind::InvalidTimestamp => f.write_str(
                "日時の書式が違います（YYYY-MM-DD HH:MM か YYYY-MM-DD HH:MM:SS。区切りは空白か T）",
            ),
            ImportErrorKind::InvalidDate => f.write_str("日付の書式が違います（YYYY-MM-DD）"),
            ImportErrorKind::OutOfRange => {
                f.write_str("日付が範囲の外です（2000-01-01〜2099-12-31）")
            }
            ImportErrorKind::InvalidKind => {
                f.write_str("打刻の種別が違います（in・out・break_start・break_end）")
            }
            ImportErrorKind::InvalidUnit => {
                f.write_str("休暇の単位が違います（full・half・hours）")
            }
            ImportErrorKind::InvalidMinutes => write!(
                f,
                "分の数が違います（0〜{MAX_MONTH_MINUTES} の整数を数字だけで）"
            ),
        }
    }
}

/// 取込の誤り。行番号と列名だけを持ち、入力の中身は持たない（AC-10.7）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImportError {
    /// 種類。
    pub kind: ImportErrorKind,
    /// 行番号（1から）。ファイル全体の誤りは `None`。
    pub line: Option<u64>,
    /// 列名（この版が決めた列名。入力の文字列ではない）。
    pub column: Option<&'static str>,
    /// 見出しの列名の並び（見出しの誤りのときだけ）。文に `employee_id,timestamp,kind` のように出す。
    pub header: Option<&'static [&'static str]>,
}

impl ImportError {
    const fn whole(kind: ImportErrorKind) -> ImportError {
        ImportError {
            kind,
            line: None,
            column: None,
            header: None,
        }
    }

    const fn line(kind: ImportErrorKind, line: u64) -> ImportError {
        ImportError {
            kind,
            line: Some(line),
            column: None,
            header: None,
        }
    }

    const fn cell(kind: ImportErrorKind, line: u64, column: &'static str) -> ImportError {
        ImportError {
            kind,
            line: Some(line),
            column: Some(column),
            header: None,
        }
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(f, "{line}行目の列 {column}: ")?,
            (Some(line), None) => write!(f, "{line}行目: ")?,
            (None, Some(column)) => write!(f, "列 {column}: ")?,
            (None, None) => {}
        }
        write!(f, "{}", self.kind)?;
        if let Some(header) = self.header {
            write!(f, "（1行目は見出しにしてください: {}）", header.join(","))?;
        }
        Ok(())
    }
}

impl std::error::Error for ImportError {}

/// `reader` を、[`MAX_FILE_BYTES`] を超えない範囲で読み切る。
///
/// 上限まで読んだあと、1バイトだけ続きを読んで、超えていないかを確かめる（超えた分は持たない）。
///
/// # Errors
///
/// 読めなければ [`ImportErrorKind::Io`]（入出力の誤りの中身は出さない）、
/// [`MAX_FILE_BYTES`] を超えれば [`ImportErrorKind::FileTooLarge`]。
pub fn read_limited<R: Read>(mut reader: R) -> Result<Vec<u8>, ImportError> {
    let io_error = |_| ImportError::whole(ImportErrorKind::Io);
    let limit = u64::try_from(MAX_FILE_BYTES).unwrap_or(u64::MAX);
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    let mut probe = [0_u8; 1];
    loop {
        match reader.read(&mut probe) {
            Ok(0) => return Ok(bytes),
            Ok(_) => return Err(ImportError::whole(ImportErrorKind::FileTooLarge)),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(io_error(error)),
        }
    }
}

/// 打刻CSV（`employee_id,timestamp,kind`）を読む。
///
/// `timestamp` は `YYYY-MM-DD HH:MM[:SS]`（区切りは空白か `T`、日本時間）。秒はそのまま持ち、
/// 分にするのは打刻の対（`kintai_core::punch::pair`）の方針による。
///
/// # Errors
///
/// 上の「読み方の決まり」に外れれば [`ImportError`]（行番号と列名つき）。
pub fn read_punches(bytes: &[u8]) -> Result<ImportedPunches, ImportError> {
    let text = decode(bytes)?;
    let mut ids = EmployeeIds::default();
    let mut imported = ImportedPunches::default();
    for_each_row(
        &text,
        &PUNCH_COLUMNS,
        |line, [employee, timestamp, kind]| {
            let employee = ids.get(employee, line)?;
            let time = PunchTime::parse(timestamp).map_err(|error| {
                ImportError::cell(
                    time_error(error, ImportErrorKind::InvalidTimestamp),
                    line,
                    "timestamp",
                )
            })?;
            let kind = PunchKind::from_code(kind).ok_or(ImportError::cell(
                ImportErrorKind::InvalidKind,
                line,
                "kind",
            ))?;
            imported.punches.push(Punch {
                employee,
                time,
                kind,
            });
            imported.lines.push(line);
            Ok(())
        },
    )?;
    Ok(imported)
}

/// 休暇CSV（`employee_id,date,unit`）を読む。`unit` が `hours` の行も読む（拒否するのは使う側）。
///
/// # Errors
///
/// 上の「読み方の決まり」に外れれば [`ImportError`]（行番号と列名つき）。
pub fn read_leaves(bytes: &[u8]) -> Result<Vec<LeaveRow>, ImportError> {
    let text = decode(bytes)?;
    let mut ids = EmployeeIds::default();
    let mut rows = Vec::new();
    for_each_row(&text, &LEAVE_COLUMNS, |line, [employee, date, unit]| {
        let employee = ids.get(employee, line)?;
        let date = parse_date(date, line, "date")?;
        let unit = LeaveUnit::from_code(unit).ok_or(ImportError::cell(
            ImportErrorKind::InvalidUnit,
            line,
            "unit",
        ))?;
        rows.push(LeaveRow {
            line,
            employee,
            date,
            unit,
        });
        Ok(())
    })?;
    Ok(rows)
}

/// 36協定の履歴CSV（`employee_id,month_start,total_minutes`）を読む。
///
/// `total_minutes` は数字だけの整数で、0〜[`MAX_MONTH_MINUTES`]。符号・小数点・空白は不正。
///
/// # Errors
///
/// 上の「読み方の決まり」に外れれば [`ImportError`]（行番号と列名つき）。
pub fn read_history(bytes: &[u8]) -> Result<Vec<HistoryRow>, ImportError> {
    let text = decode(bytes)?;
    let mut ids = EmployeeIds::default();
    let mut rows = Vec::new();
    for_each_row(
        &text,
        &HISTORY_COLUMNS,
        |line, [employee, month_start, total]| {
            let employee = ids.get(employee, line)?;
            let month_start = parse_date(month_start, line, "month_start")?;
            let total_minutes = parse_minutes(total).ok_or(ImportError::cell(
                ImportErrorKind::InvalidMinutes,
                line,
                "total_minutes",
            ))?;
            rows.push(HistoryRow {
                line,
                employee,
                month_start,
                total_minutes,
            });
            Ok(())
        },
    )?;
    Ok(rows)
}

/// ファイルの大きさと行の長さを確かめ、文字列に直す（UTF-8 か `Shift_JIS`）。
fn decode(bytes: &[u8]) -> Result<Cow<'_, str>, ImportError> {
    check_size(bytes.len())?;
    let (body, has_bom) = match bytes.strip_prefix(UTF8_BOM) {
        Some(rest) => (rest, true),
        None => (bytes, false),
    };
    check_line_lengths(body)?;
    match std::str::from_utf8(body) {
        Ok(text) => Ok(Cow::Borrowed(text)),
        Err(error) if has_bom => Err(encoding_error(body, error.valid_up_to())),
        Err(_) => SHIFT_JIS
            .decode_without_bom_handling_and_without_replacement(body)
            .ok_or_else(|| encoding_error(body, shift_jis_error_offset(body))),
    }
}

/// ファイルの大きさの上限（[`MAX_FILE_BYTES`] ちょうどまでは読む）。
fn check_size(len: usize) -> Result<(), ImportError> {
    if len > MAX_FILE_BYTES {
        Err(ImportError::whole(ImportErrorKind::FileTooLarge))
    } else {
        Ok(())
    }
}

fn check_line_lengths(bytes: &[u8]) -> Result<(), ImportError> {
    for (number, line) in (1_u64..).zip(bytes.split(|byte| *byte == b'\n')) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > MAX_LINE_BYTES {
            return Err(ImportError::line(ImportErrorKind::LineTooLong, number));
        }
    }
    Ok(())
}

/// 読めない文字の位置（バイト）から、文字コードの誤りを作る。行番号は、その位置までの LF の数＋1
/// （LF は、`Shift_JIS` の2バイト文字の中にも現れない）。
fn encoding_error(bytes: &[u8], offset: usize) -> ImportError {
    let before = bytes.get(..offset).unwrap_or(bytes);
    // LF で分けた個数が、その位置の行番号（LF の数＋1）。
    let line = u64::try_from(before.split(|byte| *byte == b'\n').count()).unwrap_or(u64::MAX);
    ImportError::line(ImportErrorKind::Encoding, line)
}

/// `Shift_JIS` として読めない最初のバイトの位置。
fn shift_jis_error_offset(bytes: &[u8]) -> usize {
    let mut decoder = SHIFT_JIS.new_decoder_without_bom_handling();
    let mut buffer = [0_u8; 4096];
    let mut consumed = 0_usize;
    loop {
        let rest = bytes.get(consumed..).unwrap_or_default();
        let (result, read, _) = decoder.decode_to_utf8_without_replacement(rest, &mut buffer, true);
        consumed = consumed.saturating_add(read);
        match result {
            DecoderResult::OutputFull if read > 0 => {}
            DecoderResult::Malformed(length, after) => {
                return consumed
                    .saturating_sub(usize::from(length))
                    .saturating_sub(usize::from(after));
            }
            DecoderResult::InputEmpty | DecoderResult::OutputFull => return consumed,
        }
    }
}

/// 日時・日付の読み取りの誤りを、取込の誤りの種類にする（範囲外は [`ImportErrorKind::OutOfRange`]）。
fn time_error(error: CoreError, otherwise: ImportErrorKind) -> ImportErrorKind {
    if error == CoreError::OutOfRange {
        ImportErrorKind::OutOfRange
    } else {
        otherwise
    }
}

fn parse_date(text: &str, line: u64, column: &'static str) -> Result<Date, ImportError> {
    Date::parse(text).map_err(|error| {
        ImportError::cell(
            time_error(error, ImportErrorKind::InvalidDate),
            line,
            column,
        )
    })
}

/// 数字だけの整数で、0〜[`MAX_MONTH_MINUTES`]。
fn parse_minutes(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value: i64 = text.parse().ok()?;
    (0..=MAX_MONTH_MINUTES).contains(&value).then_some(value)
}

/// 従業員IDの形を確かめ、同じIDの値を共有する（数も数える）。
#[derive(Default)]
struct EmployeeIds<'a> {
    known: BTreeMap<&'a str, EmployeeId>,
}

impl<'a> EmployeeIds<'a> {
    fn get(&mut self, text: &'a str, line: u64) -> Result<EmployeeId, ImportError> {
        if let Some(id) = self.known.get(text) {
            return Ok(id.clone());
        }
        let id = EmployeeId::new(text).map_err(|_| {
            ImportError::cell(ImportErrorKind::InvalidEmployeeId, line, "employee_id")
        })?;
        if self.known.len() >= MAX_EMPLOYEES {
            return Err(ImportError::cell(
                ImportErrorKind::TooManyEmployees,
                line,
                "employee_id",
            ));
        }
        self.known.insert(text, id.clone());
        Ok(id)
    }
}

/// 見出しを読み、データの各行を、列名の並び `columns` の順の値にして `on_row` に渡す。
fn for_each_row<'a, const N: usize>(
    text: &'a str,
    columns: &'static [&'static str; N],
    mut on_row: impl FnMut(u64, [&'a str; N]) -> Result<(), ImportError>,
) -> Result<(), ImportError> {
    // 最後の区切り（LF のあとの残り）だけは LF で終わらない。その行の CR は改行ではない（L2-09）。
    let mut lines = (1_u64..).zip(text.split('\n')).peekable();
    let mut fields: Vec<&'a str> = Vec::with_capacity(N);
    let header = lines.next();
    let order = read_header(header, lines.peek().is_some(), columns, &mut fields)?;
    let mut first_blank: Option<u64> = None;
    while let Some((number, raw)) = lines.next() {
        let terminated = lines.peek().is_some();
        // 空の行（LF で終わる CRLF の空の行を含む）は、空でない行が後に来たときに誤りにする。
        // 空でない行の CR の誤りより先に、上の空の行の誤りを返す（上から最初の1つ。L2-10）。
        if raw.is_empty() || (terminated && raw == "\r") {
            first_blank.get_or_insert(number);
            continue;
        }
        if let Some(blank) = first_blank {
            return Err(ImportError::line(ImportErrorKind::BlankLine, blank));
        }
        let line = strip_carriage_return(raw, number, terminated)?;
        split_fields(line, &mut fields).map_err(|kind| ImportError::line(kind, number))?;
        if fields.len() != N {
            return Err(ImportError::line(
                ImportErrorKind::FieldCount { expected: N },
                number,
            ));
        }
        let mut row = [""; N];
        for (field, &column) in fields.iter().zip(order.iter()) {
            if let Some(slot) = row.get_mut(column) {
                *slot = field;
            }
        }
        on_row(number, row)?;
    }
    Ok(())
}

/// 見出しを読み、各位置の値が `columns` の何番目かを返す。
///
/// 位置の順に、知らない列名 → 前と同じ列名を見て、最後に足りない列名を見る。
fn read_header<'a, const N: usize>(
    first: Option<(u64, &'a str)>,
    terminated: bool,
    columns: &'static [&'static str; N],
    fields: &mut Vec<&'a str>,
) -> Result<[usize; N], ImportError> {
    let with_header = |mut error: ImportError| {
        error.header = Some(columns.as_slice());
        error
    };
    // 空のファイルも、空の1行目として扱う（`str::split` は少なくとも1つ返す）。
    let (number, raw) = first.unwrap_or((1, ""));
    let line = strip_carriage_return(raw, number, terminated)?;
    if line.is_empty() {
        return Err(with_header(ImportError::line(
            ImportErrorKind::MissingHeader,
            number,
        )));
    }
    split_fields(line, fields).map_err(|kind| ImportError::line(kind, number))?;
    let mut order = [0_usize; N];
    let mut seen = [false; N];
    for (index, field) in fields.iter().enumerate() {
        let position = index.saturating_add(1);
        let Some(column) = columns.iter().position(|name| name == field) else {
            return Err(with_header(ImportError::line(
                ImportErrorKind::UnknownColumn { position },
                number,
            )));
        };
        match seen.get_mut(column) {
            Some(true) => {
                return Err(with_header(ImportError::line(
                    ImportErrorKind::DuplicateColumn { position },
                    number,
                )));
            }
            Some(flag) => *flag = true,
            None => {}
        }
        // 知らない列名と重なる列名を除くと、値の数は N 以下なので、位置はいつも N 未満。
        if let Some(slot) = order.get_mut(index) {
            *slot = column;
        }
    }
    for (name, found) in columns.iter().zip(seen) {
        if !found {
            return Err(with_header(ImportError::cell(
                ImportErrorKind::MissingColumn,
                number,
                name,
            )));
        }
    }
    Ok(order)
}

/// LF で終わる行（`terminated`）なら、行末の CR を1つ取り除く（CRLF）。ほかの位置の CR は不正。
/// LF で終わらないファイルの最後の行の CR は、改行でない単独の CR なので取り除かない（L2-09）。
fn strip_carriage_return(raw: &str, number: u64, terminated: bool) -> Result<&str, ImportError> {
    let line = if terminated {
        raw.strip_suffix('\r').unwrap_or(raw)
    } else {
        raw
    };
    if line.contains('\r') {
        Err(ImportError::line(
            ImportErrorKind::StrayCarriageReturn,
            number,
        ))
    } else {
        Ok(line)
    }
}

/// 1行を `,` で値に分ける。`"` で囲んだ値は囲みを外す（中の `""` はそのまま残すが、
/// どの列の形にも `"` は入らないので、その値は列の検査で拒否される）。
fn split_fields<'a>(line: &'a str, fields: &mut Vec<&'a str>) -> Result<(), ImportErrorKind> {
    fields.clear();
    let bytes = line.as_bytes();
    let mut position = 0_usize;
    loop {
        if bytes.get(position) == Some(&b'"') {
            let content = position.saturating_add(1);
            let mut cursor = content;
            loop {
                match bytes.get(cursor) {
                    None => return Err(ImportErrorKind::Quote),
                    Some(b'"') if bytes.get(cursor.saturating_add(1)) == Some(&b'"') => {
                        cursor = cursor.saturating_add(2);
                    }
                    Some(b'"') => break,
                    Some(_) => cursor = cursor.saturating_add(1),
                }
            }
            fields.push(line.get(content..cursor).ok_or(ImportErrorKind::Quote)?);
            position = cursor.saturating_add(1);
            match bytes.get(position) {
                None => return Ok(()),
                Some(b',') => position = position.saturating_add(1),
                Some(_) => return Err(ImportErrorKind::Quote),
            }
        } else {
            let rest = bytes.get(position..).unwrap_or_default();
            let end = rest
                .iter()
                .position(|byte| *byte == b',')
                .map_or(bytes.len(), |offset| position.saturating_add(offset));
            let field = line.get(position..end).ok_or(ImportErrorKind::Quote)?;
            if field.contains('"') {
                return Err(ImportErrorKind::Quote);
            }
            fields.push(field);
            if end >= bytes.len() {
                return Ok(());
            }
            position = end.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ImportErrorKind, MAX_FILE_BYTES, check_size};

    /// AC-10.6: ファイルの大きさは 256MiB ちょうどまで読み、1バイト超えたら拒否する
    /// （大きな入力を作らずに、境界だけを確かめる。読み取り全体の試験は `tests/ac10_limits.rs`）。
    #[test]
    fn ac10_6_size_boundary() {
        assert_eq!(check_size(0), Ok(()));
        assert_eq!(check_size(MAX_FILE_BYTES), Ok(()));
        assert_eq!(
            check_size(MAX_FILE_BYTES + 1).map_err(|error| error.kind),
            Err(ImportErrorKind::FileTooLarge)
        );
    }
}
