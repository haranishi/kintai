//! 区間（半開区間 `[開始, 終了)`）。

use crate::error::CoreError;
use crate::instant::{Instant, MINUTES_PER_DAY};

/// 開始 < 終了の半開区間 `[開始, 終了)`。長さは分。
///
/// 端が接するだけの2つの区間（`[9:00, 12:00)` と `[12:00, 13:00)`）は重ならない。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Interval {
    start: Instant,
    end: Instant,
}

impl Interval {
    /// 開始と終了から作る。
    ///
    /// # Errors
    ///
    /// 開始が終了以上なら [`CoreError::EmptyInterval`]。
    pub fn new(start: Instant, end: Instant) -> Result<Interval, CoreError> {
        if start < end {
            Ok(Interval { start, end })
        } else {
            Err(CoreError::EmptyInterval)
        }
    }

    /// 開始（含む）。
    #[must_use]
    pub const fn start(self) -> Instant {
        self.start
    }

    /// 終了（含まない）。
    #[must_use]
    pub const fn end(self) -> Instant {
        self.end
    }

    /// 長さ（分）。常に1以上。
    #[must_use]
    pub const fn len_minutes(self) -> i64 {
        self.end.minutes_since(self.start)
    }

    /// 時点 `instant` を含むか（開始を含み、終了を含まない）。
    #[must_use]
    pub fn contains(self, instant: Instant) -> bool {
        self.start <= instant && instant < self.end
    }

    /// 1分以上重なるか。端が接するだけなら重ならない。
    #[must_use]
    pub fn overlaps(self, other: Interval) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// 重なる部分。重ならなければ `None`。
    #[must_use]
    pub fn intersection(self, other: Interval) -> Option<Interval> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        (start < end).then_some(Interval { start, end })
    }

    /// 暦日の 0:00 で分ける。各小片は1つの暦日に収まり、長さの合計は元と同じ。並びは時刻順。
    #[must_use]
    pub fn split_at_midnight(self) -> Vec<Interval> {
        let mut pieces = Vec::new();
        let end = self.end.minutes();
        let mut cursor = self.start.minutes();
        while cursor < end {
            let next_midnight = (cursor.div_euclid(MINUTES_PER_DAY) + 1) * MINUTES_PER_DAY;
            let piece_end = next_midnight.min(end);
            // 小片の端は、元の区間の端か、その内側の 0:00 なので、範囲の中にある。
            pieces.push(Interval {
                start: Instant::from_minutes_unchecked(cursor),
                end: Instant::from_minutes_unchecked(piece_end),
            });
            cursor = piece_end;
        }
        pieces
    }
}
