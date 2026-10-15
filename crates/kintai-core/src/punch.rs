//! 打刻の列を勤務と休憩に対にする（`docs/spec/punch.md`。AC-01.1〜01.5）。
//!
//! 打刻（[`Punch`]: 従業員ID・秒までの時点・種別）を従業員ごとに時刻順に並べ、
//! 「勤務外」「勤務中」「休憩中」の3つの状態の遷移で、勤務（始業から終業）と休憩の区間に対にする。
//!
//! | 今の状態 | 打刻 | 次の状態 |
//! |---|---|---|
//! | 勤務外 | `in` | 勤務中 |
//! | 勤務中 | `break_start` | 休憩中 |
//! | 休憩中 | `break_end` | 勤務中 |
//! | 勤務中 | `out` | 勤務外（勤務が1つ完成） |
//!
//! それ以外の遷移、最後に勤務中（休憩中）のまま終わること、24時間以上の勤務、
//! 順序の決まらない同時刻の打刻は、異常（[`AnomalyKind`]）として一覧する。
//! 異常のある従業員の勤務は返さない（[`PairingOutcome::Refused`]。確定集計を拒否する）。
//!
//! 秒は、方針（[`SecondsPolicy`]）で分にする。既定の `worker_favorable` は、
//! 開始側（`in` と `break_end`）を切り捨て、終了側（`out` と `break_start`）を切り上げる。
//! `reject` は、秒が0でない打刻を不正な入力にする。1日ごとの切り捨てや、15分単位などへの丸めは持たない。
//!
//! ```
//! use kintai_core::punch::{EmployeeId, PairingOutcome, Punch, PunchKind, SecondsPolicy, pair};
//!
//! let employee = EmployeeId::new("E001").unwrap();
//! let punch = |time: &str, kind| Punch { employee: employee.clone(), time: time.parse().unwrap(), kind };
//! let punches = [
//!     punch("2026-10-07 18:00:01", PunchKind::Out),
//!     punch("2026-10-07 09:00:30", PunchKind::In),
//!     punch("2026-10-07 12:00:10", PunchKind::BreakStart),
//!     punch("2026-10-07 12:59:50", PunchKind::BreakEnd),
//! ];
//! let result = pair(&punches, SecondsPolicy::WorkerFavorable).unwrap();
//! let PairingOutcome::Paired(shifts) = &result[0].outcome else { panic!("異常がある") };
//! assert_eq!(shifts[0].span().start().to_string(), "2026-10-07T09:00");
//! assert_eq!(shifts[0].span().end().to_string(), "2026-10-07T18:01");
//! assert_eq!(shifts[0].breaks()[0].start().to_string(), "2026-10-07T12:01");
//! assert_eq!(shifts[0].breaks()[0].end().to_string(), "2026-10-07T12:59");
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use crate::date::number;
use crate::error::CoreError;
use crate::instant::Instant;
use crate::interval::Interval;

/// 従業員IDの長さの上限（文字数。使える文字は ASCII だけなので、バイト数と同じ）。
pub const EMPLOYEE_ID_MAX_LEN: usize = 64;
/// 1人が1つの暦の月に始める勤務（`in` の打刻）の数の上限（`docs/spec/nfr.md` の入力の上限）。
pub const MAX_SHIFTS_PER_MONTH: usize = 1000;
/// この長さ（分）以上の勤務は [`AnomalyKind::LongShift`]（24時間）。
pub const LONG_SHIFT_MINUTES: i64 = 1440;

/// 従業員ID。`[A-Za-z0-9][A-Za-z0-9._-]{0,63}` の形だけを受け付ける（AC-08.7）。
///
/// 先頭が英数字なので、CSV に書き出しても、表計算ソフトが式（`=` `+` `-` `@` で始まるもの）として実行しない。
/// 並べ替えはバイトの順（使える文字は ASCII だけ）。複製は中身を共有するので安い。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EmployeeId(Arc<str>);

impl EmployeeId {
    /// 形を確かめて作る。
    ///
    /// # Errors
    ///
    /// 形が `[A-Za-z0-9][A-Za-z0-9._-]{0,63}` でなければ [`PunchError::InvalidEmployeeId`]。
    pub fn new(text: &str) -> Result<EmployeeId, PunchError> {
        if is_employee_id(text) {
            Ok(EmployeeId(Arc::from(text)))
        } else {
            Err(PunchError::InvalidEmployeeId)
        }
    }

    /// 文字列。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_employee_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((first, rest)) = bytes.split_first() else {
        return false;
    };
    bytes.len() <= EMPLOYEE_ID_MAX_LEN
        && first.is_ascii_alphanumeric()
        && rest
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

impl FromStr for EmployeeId {
    type Err = PunchError;

    fn from_str(s: &str) -> Result<EmployeeId, PunchError> {
        EmployeeId::new(s)
    }
}

impl fmt::Display for EmployeeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for EmployeeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmployeeId({})", self.0)
    }
}

/// 打刻の種別。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PunchKind {
    /// 始業（`in`）。
    In,
    /// 終業（`out`）。
    Out,
    /// 休憩の開始（`break_start`）。
    BreakStart,
    /// 休憩の終了（`break_end`）。
    BreakEnd,
}

impl PunchKind {
    /// 4つの種別。
    pub const ALL: [PunchKind; 4] = [
        PunchKind::In,
        PunchKind::Out,
        PunchKind::BreakStart,
        PunchKind::BreakEnd,
    ];

    /// CSV の `kind` の値。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            PunchKind::In => "in",
            PunchKind::Out => "out",
            PunchKind::BreakStart => "break_start",
            PunchKind::BreakEnd => "break_end",
        }
    }

    /// CSV の `kind` の値から読む。大文字や前後の空白は受け付けない。
    #[must_use]
    pub fn from_code(text: &str) -> Option<PunchKind> {
        PunchKind::ALL.into_iter().find(|kind| kind.code() == text)
    }

    /// 同じ時刻の打刻を並べる順（終わるものを先に）。並びを決めるためだけに使う。
    /// 同じ時刻の打刻があれば、その従業員は [`AnomalyKind::SameTimestamp`] になる。
    const fn order_at_same_time(self) -> u8 {
        match self {
            PunchKind::BreakEnd => 0,
            PunchKind::Out => 1,
            PunchKind::In => 2,
            PunchKind::BreakStart => 3,
        }
    }
}

/// 秒まで持つ時点（日本時間の壁時計）。打刻の時刻に使う。
///
/// 入力として受け付ける 2000-01-01T00:00:00〜2099-12-31T23:59:59 だけを表す。
/// `YYYY-MM-DD HH:MM[:SS]` と `YYYY-MM-DDTHH:MM[:SS]` を読み、`YYYY-MM-DDTHH:MM:SS` で書く。
/// 並べ替えは時刻の順。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PunchTime {
    minute: Instant,
    second: u8,
}

impl PunchTime {
    /// 分（[`Instant`]）と秒から作る。
    ///
    /// # Errors
    ///
    /// 秒が 0〜59 の外なら [`CoreError::InvalidTime`]、`minute` が入力の範囲
    /// （2000-01-01T00:00〜2099-12-31T23:59）の外なら [`CoreError::OutOfRange`]。
    pub fn new(minute: Instant, second: u32) -> Result<PunchTime, CoreError> {
        let second = u8::try_from(second)
            .ok()
            .filter(|second| *second < 60)
            .ok_or(CoreError::InvalidTime)?;
        if !minute.is_input_range() {
            return Err(CoreError::OutOfRange);
        }
        Ok(PunchTime { minute, second })
    }

    /// 入力の文字列を読む（[`FromStr`] と同じ）。
    ///
    /// # Errors
    ///
    /// 形の誤りは [`CoreError::InvalidFormat`]、暦にない日付は [`CoreError::InvalidDate`]、
    /// 24:00 は [`CoreError::Hour24`]、25時・60分・60秒などは [`CoreError::InvalidTime`]、
    /// 日付が 2000-01-01〜2099-12-31 の外は [`CoreError::OutOfRange`]。
    pub fn parse(text: &str) -> Result<PunchTime, CoreError> {
        text.parse()
    }

    /// 秒を切り捨てた分（開始側の丸め）。
    #[must_use]
    pub const fn floor(self) -> Instant {
        self.minute
    }

    /// 秒を切り上げた分（終了側の丸め）。秒が0なら、そのまま。
    ///
    /// # Errors
    ///
    /// 結果が時点の型の範囲の外なら [`CoreError::OutOfRange`]（入力の範囲の時点では起きない）。
    pub fn ceil(self) -> Result<Instant, CoreError> {
        if self.second == 0 {
            Ok(self.minute)
        } else {
            self.minute.checked_add_minutes(1)
        }
    }

    /// 秒（0〜59）。
    #[must_use]
    pub fn second(self) -> u32 {
        u32::from(self.second)
    }
}

impl fmt::Display for PunchTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{:02}", self.minute, self.second)
    }
}

impl fmt::Debug for PunchTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PunchTime({self})")
    }
}

impl FromStr for PunchTime {
    type Err = CoreError;

    /// `YYYY-MM-DD HH:MM` または `YYYY-MM-DD HH:MM:SS`（区切りは空白か `T`）を読む。
    /// 前後の空白や、桁の違う表記は受け付けない。
    fn from_str(s: &str) -> Result<PunchTime, CoreError> {
        match s.len() {
            16 => PunchTime::new(s.parse()?, 0),
            19 => {
                let (head, tail) = s.split_at_checked(16).ok_or(CoreError::InvalidFormat)?;
                let minute: Instant = head.parse()?;
                let tail: [u8; 3] = tail
                    .as_bytes()
                    .try_into()
                    .map_err(|_| CoreError::InvalidFormat)?;
                let [b':', s1, s2] = tail else {
                    return Err(CoreError::InvalidFormat);
                };
                let second = number(&[s1, s2]).ok_or(CoreError::InvalidFormat)?;
                PunchTime::new(minute, second)
            }
            _ => Err(CoreError::InvalidFormat),
        }
    }
}

/// 秒を分にする方針（方針ファイルの `seconds`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SecondsPolicy {
    /// 開始側（`in`・`break_end`）は切り捨て、終了側（`out`・`break_start`）は切り上げる（`worker_favorable`。既定）。
    /// 実労働が実際の時間を下回らない。
    #[default]
    WorkerFavorable,
    /// 秒が0でない打刻を、不正な入力にする（`reject`）。
    Reject,
}

impl SecondsPolicy {
    /// 方針ファイルの値。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            SecondsPolicy::WorkerFavorable => "worker_favorable",
            SecondsPolicy::Reject => "reject",
        }
    }

    /// 方針ファイルの値から読む。
    #[must_use]
    pub fn from_code(text: &str) -> Option<SecondsPolicy> {
        [SecondsPolicy::WorkerFavorable, SecondsPolicy::Reject]
            .into_iter()
            .find(|policy| policy.code() == text)
    }
}

/// 打刻（1件）。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Punch {
    /// 従業員ID。
    pub employee: EmployeeId,
    /// 時点（秒まで）。
    pub time: PunchTime,
    /// 種別。
    pub kind: PunchKind,
}

/// 打刻の異常の種類（`docs/spec/punch.md`）。並びは仕様の列挙の順。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnomalyKind {
    /// 勤務中か休憩中に、また `in` がある。
    DuplicateIn,
    /// 勤務外で `out` がある（対になる `in` がない）。
    OutWithoutIn,
    /// 勤務中でないとき（勤務外か休憩中）に `break_start` がある。
    BreakStartWithoutIn,
    /// 休憩中でないときに `break_end` がある（対になる `break_start` がない）。
    BreakEndWithoutStart,
    /// 休憩中に `out` がある（休憩が閉じていない）。
    OutDuringBreak,
    /// 打刻の最後で、勤務中か休憩中のまま終わっている。
    OpenShift,
    /// 同じ時刻（秒まで同じ）の打刻が2件以上あり、順序が決まらない。
    SameTimestamp,
    /// 始業から終業まで24時間以上（秒を分にしたあとの長さで見る）。
    LongShift,
}

impl AnomalyKind {
    /// すべての種類（仕様の列挙の順）。
    pub const ALL: [AnomalyKind; 8] = [
        AnomalyKind::DuplicateIn,
        AnomalyKind::OutWithoutIn,
        AnomalyKind::BreakStartWithoutIn,
        AnomalyKind::BreakEndWithoutStart,
        AnomalyKind::OutDuringBreak,
        AnomalyKind::OpenShift,
        AnomalyKind::SameTimestamp,
        AnomalyKind::LongShift,
    ];

    /// 出力の `code` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            AnomalyKind::DuplicateIn => "DuplicateIn",
            AnomalyKind::OutWithoutIn => "OutWithoutIn",
            AnomalyKind::BreakStartWithoutIn => "BreakStartWithoutIn",
            AnomalyKind::BreakEndWithoutStart => "BreakEndWithoutStart",
            AnomalyKind::OutDuringBreak => "OutDuringBreak",
            AnomalyKind::OpenShift => "OpenShift",
            AnomalyKind::SameTimestamp => "SameTimestamp",
            AnomalyKind::LongShift => "LongShift",
        }
    }
}

/// 打刻の異常（1件）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Anomaly {
    /// 種類。
    pub kind: AnomalyKind,
    /// 時点。遷移の異常はその打刻、`OpenShift` と `LongShift` はその勤務の `in`、
    /// `SameTimestamp` は重なった時刻。
    pub time: PunchTime,
    /// 打刻の番号（[`pair`] に渡した並びの0から）。`SameTimestamp` は、重なった打刻のうち最も小さい番号。
    pub index: usize,
}

/// 打刻から対にした勤務（1つの継続勤務）。時点は分に丸めてある。
///
/// 休憩は始業の後・終業の前（両端に接しない）にあり、時刻順で、互いに重ならず接しない。
/// 長さは1440分未満。同じ従業員の勤務どうしは重ならない（接することはある）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedShift {
    span: Interval,
    breaks: Vec<Interval>,
}

impl PairedShift {
    /// 始業から終業までの区間（休憩を含む）。
    #[must_use]
    pub const fn span(&self) -> Interval {
        self.span
    }

    /// 休憩（時刻順）。
    #[must_use]
    pub fn breaks(&self) -> &[Interval] {
        &self.breaks
    }
}

/// 1人の従業員の、対にした結果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingOutcome {
    /// 異常がなく、すべての打刻を勤務に対にできた（始業の順）。
    Paired(Vec<PairedShift>),
    /// 異常がある。勤務は返さない（その従業員の確定集計を拒否する。AC-01.3）。異常は時刻の順。
    Refused(Vec<Anomaly>),
}

/// 従業員ごとの結果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmployeePairing {
    /// 従業員ID。
    pub employee: EmployeeId,
    /// 勤務か、異常の一覧。
    pub outcome: PairingOutcome,
}

/// 打刻の誤り（不正な入力。終了コード2）。番号は [`pair`] に渡した並びの0から。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PunchError {
    /// 従業員IDの形が `[A-Za-z0-9][A-Za-z0-9._-]{0,63}` でない。
    InvalidEmployeeId,
    /// 秒の方針が `reject` で、秒が0でない打刻がある。
    NonZeroSeconds {
        /// 打刻の番号。
        index: usize,
    },
    /// 1人が1つの暦の月に始める勤務（`in` の打刻）が [`MAX_SHIFTS_PER_MONTH`] を超える。
    TooManyShifts {
        /// 上限を超えた `in` の打刻の番号。
        index: usize,
    },
    /// 秒を分にした結果が、時点の型の範囲の外（入力の範囲の打刻では起きない）。
    OutOfRange {
        /// 打刻の番号。
        index: usize,
    },
}

impl fmt::Display for PunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PunchError::InvalidEmployeeId => f.write_str(
                "従業員IDの形が違います（英数字で始まり、英数字と . _ - だけの64文字まで）",
            ),
            PunchError::NonZeroSeconds { index } => write!(
                f,
                "punches[{index}] の秒が0ではありません（秒の方針が reject です）"
            ),
            PunchError::TooManyShifts { index } => write!(
                f,
                "punches[{index}]: 1人が1か月に始める勤務が{MAX_SHIFTS_PER_MONTH}件を超えています"
            ),
            PunchError::OutOfRange { index } => {
                write!(f, "punches[{index}] の時点を分にすると範囲の外になります")
            }
        }
    }
}

impl std::error::Error for PunchError {}

/// 打刻を従業員ごとに時刻順に並べ、勤務と休憩に対にする（AC-01.1〜01.5）。
///
/// 結果は従業員ID順。打刻の並びは問わない（並びを変えても、勤務と異常の種類・時点は変わらない）。
/// 異常のある従業員は [`PairingOutcome::Refused`] で、勤務を返さない。
///
/// 秒を分にするときは、開始側（`in`・`break_end`）を切り捨て、終了側（`out`・`break_start`）を切り上げる
/// （`reject` は秒が0の打刻しか受け付けないので、何も変わらない）。丸めで長さが0以下になった休憩は除く。
/// 同じ1分の中で、ある勤務が終わり次の勤務が始まると、丸めた2つの勤務が重なる。そのときは前の勤務の
/// 終わりを次の始業に合わせる（その1分は次の勤務の分になる。働いた時間は数から漏れない）。
///
/// # Errors
///
/// `seconds` が [`SecondsPolicy::Reject`] で秒が0でない打刻があれば [`PunchError::NonZeroSeconds`]
/// （最も小さい番号）、1人が1つの暦の月に始める勤務が [`MAX_SHIFTS_PER_MONTH`] を超えれば
/// [`PunchError::TooManyShifts`]（従業員ID順で最初の人）。
pub fn pair(punches: &[Punch], seconds: SecondsPolicy) -> Result<Vec<EmployeePairing>, PunchError> {
    if seconds == SecondsPolicy::Reject
        && let Some(index) = punches.iter().position(|punch| punch.time.second() != 0)
    {
        return Err(PunchError::NonZeroSeconds { index });
    }
    let mut by_employee: BTreeMap<&EmployeeId, Vec<Item>> = BTreeMap::new();
    for (index, punch) in punches.iter().enumerate() {
        by_employee.entry(&punch.employee).or_default().push(Item {
            time: punch.time,
            kind: punch.kind,
            index,
        });
    }
    let mut result = Vec::with_capacity(by_employee.len());
    for (employee, mut items) in by_employee {
        items.sort_by_key(|item| (item.time, item.kind.order_at_same_time(), item.index));
        check_shift_count(&items)?;
        result.push(EmployeePairing {
            employee: employee.clone(),
            outcome: pair_sorted(&items)?,
        });
    }
    Ok(result)
}

/// 打刻（並べ替えのための写し）。
#[derive(Clone, Copy)]
struct Item {
    time: PunchTime,
    kind: PunchKind,
    index: usize,
}

/// 1人の `in` の数を、暦の月ごとに数える（`items` は時刻順）。
fn check_shift_count(items: &[Item]) -> Result<(), PunchError> {
    let mut month: Option<(i32, u32)> = None;
    let mut count = 0_usize;
    for item in items.iter().filter(|item| item.kind == PunchKind::In) {
        let date = item.time.floor().date();
        let this_month = (date.year(), date.month());
        if month != Some(this_month) {
            month = Some(this_month);
            count = 0;
        }
        count += 1;
        if count > MAX_SHIFTS_PER_MONTH {
            return Err(PunchError::TooManyShifts { index: item.index });
        }
    }
    Ok(())
}

/// 開いている勤務（勤務中か休憩中）。
struct Open {
    start: Item,
    breaks: Vec<(Item, Item)>,
    break_start: Option<Item>,
}

/// 完成した勤務。終業だけ分に丸めてある（24時間以上かを見るため）。
struct Closed {
    start: Item,
    end: Instant,
    breaks: Vec<(Item, Item)>,
}

/// 状態遷移（1人分）。
#[derive(Default)]
struct Machine {
    open: Option<Open>,
    closed: Vec<Closed>,
    anomalies: Vec<Anomaly>,
}

impl Machine {
    fn anomaly(&mut self, kind: AnomalyKind, item: Item) {
        self.anomalies.push(Anomaly {
            kind,
            time: item.time,
            index: item.index,
        });
    }

    fn start(&mut self, item: Item) {
        self.open = Some(Open {
            start: item,
            breaks: Vec::new(),
            break_start: None,
        });
    }

    /// 打刻1件の遷移。遷移の外の打刻は異常にして、次のように立て直す（その人の勤務は返さないので、
    /// 立て直しは異常をなるべく続けて出さないためだけのもの）: 勤務外の `out` `break_start` `break_end` と、
    /// 休憩中の `break_start`、勤務中の `break_end` は無視する。重なった `in` は前の勤務を捨てて始め直す。
    /// 休憩中の `out` は、その勤務を捨てて勤務外に戻る。
    fn step(&mut self, item: Item) -> Result<(), PunchError> {
        match (self.open.take(), item.kind) {
            (None, PunchKind::In) => self.start(item),
            (None, PunchKind::Out) => self.anomaly(AnomalyKind::OutWithoutIn, item),
            (None, PunchKind::BreakStart) => self.anomaly(AnomalyKind::BreakStartWithoutIn, item),
            (None, PunchKind::BreakEnd) => self.anomaly(AnomalyKind::BreakEndWithoutStart, item),
            (Some(_), PunchKind::In) => {
                self.anomaly(AnomalyKind::DuplicateIn, item);
                self.start(item);
            }
            (Some(open), PunchKind::Out) => {
                if open.break_start.is_some() {
                    self.anomaly(AnomalyKind::OutDuringBreak, item);
                } else {
                    self.close(open, item)?;
                }
            }
            (Some(mut open), PunchKind::BreakStart) => {
                if open.break_start.is_some() {
                    self.anomaly(AnomalyKind::BreakStartWithoutIn, item);
                } else {
                    open.break_start = Some(item);
                }
                self.open = Some(open);
            }
            (Some(mut open), PunchKind::BreakEnd) => {
                match open.break_start.take() {
                    Some(start) => open.breaks.push((start, item)),
                    None => self.anomaly(AnomalyKind::BreakEndWithoutStart, item),
                }
                self.open = Some(open);
            }
        }
        Ok(())
    }

    fn close(&mut self, open: Open, out: Item) -> Result<(), PunchError> {
        let end = out
            .time
            .ceil()
            .map_err(|_| PunchError::OutOfRange { index: out.index })?;
        if end.minutes_since(open.start.time.floor()) >= LONG_SHIFT_MINUTES {
            self.anomaly(AnomalyKind::LongShift, open.start);
        }
        self.closed.push(Closed {
            start: open.start,
            end,
            breaks: open.breaks,
        });
        Ok(())
    }

    fn finish(mut self) -> Result<PairingOutcome, PunchError> {
        if let Some(open) = self.open.take() {
            self.anomaly(AnomalyKind::OpenShift, open.start);
        }
        if self.anomalies.is_empty() {
            return Ok(PairingOutcome::Paired(round_shifts(&self.closed)?));
        }
        self.anomalies
            .sort_by_key(|anomaly| (anomaly.time, anomaly.kind, anomaly.index));
        Ok(PairingOutcome::Refused(self.anomalies))
    }
}

/// 1人の打刻（時刻順）を対にする。
fn pair_sorted(items: &[Item]) -> Result<PairingOutcome, PunchError> {
    let mut machine = Machine::default();
    for group in items.chunk_by(|a, b| a.time == b.time) {
        if let [first, _, ..] = group {
            let index = group
                .iter()
                .map(|item| item.index)
                .min()
                .unwrap_or(first.index);
            machine.anomaly(AnomalyKind::SameTimestamp, Item { index, ..*first });
        }
        for &item in group {
            machine.step(item)?;
        }
    }
    machine.finish()
}

/// 分に丸めた勤務（作りかけ）。
struct Rounded {
    start: Instant,
    end: Instant,
    breaks: Vec<Interval>,
}

impl Rounded {
    /// 終わりを `limit` 以前に切る。`limit` に届く休憩があれば、その休憩の始まりで勤務を終える
    /// （休憩が勤務の終わりに接しないように）。
    fn clip_end(&mut self, limit: Instant) {
        let mut end = self.end.min(limit);
        while let Some(last) = self.breaks.last() {
            if last.end() < end {
                break;
            }
            end = end.min(last.start());
            self.breaks.pop();
        }
        self.end = end;
    }

    fn into_paired(self) -> Option<PairedShift> {
        Interval::new(self.start, self.end)
            .ok()
            .map(|span| PairedShift {
                span,
                breaks: self.breaks,
            })
    }
}

/// 完成した勤務（始業の順）を分に丸め、同じ1分の中で重なる前後の勤務を切り分ける。
fn round_shifts(closed: &[Closed]) -> Result<Vec<PairedShift>, PunchError> {
    let mut rounded: Vec<Rounded> = Vec::with_capacity(closed.len());
    for shift in closed {
        let next = round_one(shift)?;
        while let Some(previous) = rounded.last_mut() {
            if previous.end <= next.start {
                break;
            }
            previous.clip_end(next.start);
            if previous.start < previous.end {
                break;
            }
            rounded.pop();
        }
        rounded.push(next);
    }
    Ok(rounded
        .into_iter()
        .filter_map(Rounded::into_paired)
        .collect())
}

/// 勤務1つを分に丸める（開始側は切り捨て、終了側は切り上げ）。
fn round_one(shift: &Closed) -> Result<Rounded, PunchError> {
    let mut breaks = Vec::with_capacity(shift.breaks.len());
    for &(start, end) in &shift.breaks {
        let from = start
            .time
            .ceil()
            .map_err(|_| PunchError::OutOfRange { index: start.index })?;
        // 丸めで長さが0以下になった休憩は、無かったものとする（働いた時間を減らさない側）。
        if let Ok(rest) = Interval::new(from, end.time.floor()) {
            breaks.push(rest);
        }
    }
    Ok(Rounded {
        start: shift.start.time.floor(),
        end: shift.end,
        breaks,
    })
}
