//! 勤務（継続勤務）と休憩、その検査（`docs/spec/validation.md` の手順4）。

use std::fmt;

use kintai_core::{Date, Instant, Interval};

/// 1つの勤務の長さの上限（分）。これ以上は不正な入力。
pub const MAX_SHIFT_MINUTES: i64 = 1439;

/// 勤務の入力（検査の前）。休憩は `(開始, 終了)` の組。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShiftInput {
    /// 始業。
    pub start: Instant,
    /// 終業（含まない）。
    pub end: Instant,
    /// 休憩の `(開始, 終了)`。
    pub breaks: Vec<(Instant, Instant)>,
}

/// 検査済みの勤務（継続勤務1つ）。休憩は勤務の内側（両端に接しない）にあり、互いに重ならない。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shift {
    span: Interval,
    breaks: Vec<Interval>,
}

impl Shift {
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

    /// 労働日（始業の暦日）。
    #[must_use]
    pub const fn labor_day(&self) -> Date {
        self.span.start().date()
    }

    /// 実労働の区間（休憩を除く。時刻順）。
    #[must_use]
    pub fn work_intervals(&self) -> Vec<Interval> {
        let mut work = Vec::with_capacity(self.breaks.len() + 1);
        let mut cursor = self.span.start();
        for rest in &self.breaks {
            if let Ok(piece) = Interval::new(cursor, rest.start()) {
                work.push(piece);
            }
            cursor = rest.end();
        }
        if let Ok(piece) = Interval::new(cursor, self.span.end()) {
            work.push(piece);
        }
        work
    }

    /// 実労働の分（休憩を除く）。
    #[must_use]
    pub fn work_minutes(&self) -> i64 {
        self.work_intervals()
            .iter()
            .map(|piece| piece.len_minutes())
            .sum()
    }
}

/// 検査済みの勤務の集まり。始業の順に並び、互いに重ならない（接するのはよい）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShiftSet {
    shifts: Vec<Shift>,
}

/// 勤務の検査で見つかった誤り（不正な入力。終了コード2）。番号は入力の並びの0から。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShiftError {
    /// 開始が終了以上。
    StartNotBeforeEnd {
        /// 勤務の番号。
        shift: usize,
    },
    /// 勤務が1440分以上。
    TooLong {
        /// 勤務の番号。
        shift: usize,
    },
    /// 日時（始業・終業・休憩）が入力の範囲（2000-01-01T00:00〜2099-12-31T23:59）の外。
    OutOfRange {
        /// 勤務の番号。
        shift: usize,
    },
    /// 休憩の開始が終了以上。
    BreakStartNotBeforeEnd {
        /// 勤務の番号。
        shift: usize,
        /// 休憩の番号。
        rest: usize,
    },
    /// 休憩が勤務の内側にない（両端に接するのも不可）。
    BreakOutsideShift {
        /// 勤務の番号。
        shift: usize,
        /// 休憩の番号。
        rest: usize,
    },
    /// 休憩どうしが重なる。
    BreaksOverlap {
        /// 勤務の番号。
        shift: usize,
    },
    /// 勤務どうしが重なる。
    ShiftsOverlap {
        /// 先に始まる勤務の番号。
        first: usize,
        /// 後に始まる勤務の番号。
        second: usize,
    },
}

impl fmt::Display for ShiftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShiftError::StartNotBeforeEnd { shift } => {
                write!(f, "shifts[{shift}] の開始が終了以上です")
            }
            ShiftError::TooLong { shift } => {
                write!(f, "shifts[{shift}] が1440分以上です")
            }
            ShiftError::OutOfRange { shift } => write!(
                f,
                "shifts[{shift}] の日時が 2000-01-01T00:00〜2099-12-31T23:59 の外です"
            ),
            ShiftError::BreakStartNotBeforeEnd { shift, rest } => {
                write!(f, "shifts[{shift}].breaks[{rest}] の開始が終了以上です")
            }
            ShiftError::BreakOutsideShift { shift, rest } => write!(
                f,
                "shifts[{shift}].breaks[{rest}] が勤務の内側にありません（両端に接するのも不可）"
            ),
            ShiftError::BreaksOverlap { shift } => {
                write!(f, "shifts[{shift}] の休憩どうしが重なっています")
            }
            ShiftError::ShiftsOverlap { first, second } => {
                write!(f, "shifts[{first}] と shifts[{second}] が重なっています")
            }
        }
    }
}

impl std::error::Error for ShiftError {}

impl ShiftSet {
    /// 勤務を検査して集める。並びは入力の順によらず、始業の順になる。
    ///
    /// # Errors
    ///
    /// 開始が終了以上、1440分以上、日時が入力の範囲（2000-01-01T00:00〜2099-12-31T23:59）の外、
    /// 休憩の開始が終了以上、休憩が勤務の内側にない、休憩どうしが重なる、勤務どうしが重なる、
    /// のいずれかなら [`ShiftError`]。接する勤務・接する休憩はよい（つながず別のものとして扱う）。
    pub fn new(inputs: &[ShiftInput]) -> Result<ShiftSet, ShiftError> {
        let mut checked: Vec<(usize, Shift)> = Vec::with_capacity(inputs.len());
        for (index, input) in inputs.iter().enumerate() {
            checked.push((index, check_shift(index, input)?));
        }
        checked.sort_by_key(|(index, shift)| (shift.span.start(), *index));
        // 始業の順に並べたので、隣どうしだけを見れば、重なる組があるかが分かる。
        for ((first, a), (second, b)) in checked.iter().zip(checked.iter().skip(1)) {
            if a.span.overlaps(b.span) {
                return Err(ShiftError::ShiftsOverlap {
                    first: *first,
                    second: *second,
                });
            }
        }
        Ok(ShiftSet {
            shifts: checked.into_iter().map(|(_, shift)| shift).collect(),
        })
    }

    /// 勤務（始業の順）。
    #[must_use]
    pub fn shifts(&self) -> &[Shift] {
        &self.shifts
    }

    /// 勤務の数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.shifts.len()
    }

    /// 勤務がないか。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shifts.is_empty()
    }
}

fn check_shift(index: usize, input: &ShiftInput) -> Result<Shift, ShiftError> {
    let instants = [input.start, input.end]
        .into_iter()
        .chain(input.breaks.iter().flat_map(|&(start, end)| [start, end]));
    if instants
        .into_iter()
        .any(|instant| !instant.is_input_range())
    {
        return Err(ShiftError::OutOfRange { shift: index });
    }
    let span = Interval::new(input.start, input.end)
        .map_err(|_| ShiftError::StartNotBeforeEnd { shift: index })?;
    if span.len_minutes() > MAX_SHIFT_MINUTES {
        return Err(ShiftError::TooLong { shift: index });
    }
    let mut breaks = Vec::with_capacity(input.breaks.len());
    for (rest, &(start, end)) in input.breaks.iter().enumerate() {
        let interval = Interval::new(start, end)
            .map_err(|_| ShiftError::BreakStartNotBeforeEnd { shift: index, rest })?;
        if interval.start() <= span.start() || interval.end() >= span.end() {
            return Err(ShiftError::BreakOutsideShift { shift: index, rest });
        }
        breaks.push(interval);
    }
    breaks.sort();
    if breaks.windows(2).any(|pair| match pair {
        [a, b] => a.overlaps(*b),
        _ => false,
    }) {
        return Err(ShiftError::BreaksOverlap { shift: index });
    }
    Ok(Shift { span, breaks })
}
