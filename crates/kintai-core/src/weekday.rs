//! 曜日。

/// 曜日（月曜〜日曜）。並びは月曜が最小。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
    /// 月曜日
    Monday,
    /// 火曜日
    Tuesday,
    /// 水曜日
    Wednesday,
    /// 木曜日
    Thursday,
    /// 金曜日
    Friday,
    /// 土曜日
    Saturday,
    /// 日曜日
    Sunday,
}

impl Weekday {
    /// 月曜から日曜の順の7つ。
    pub const ALL: [Weekday; 7] = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];

    /// 月曜を0、日曜を6とする番号。
    #[must_use]
    pub const fn index_from_monday(self) -> u32 {
        match self {
            Weekday::Monday => 0,
            Weekday::Tuesday => 1,
            Weekday::Wednesday => 2,
            Weekday::Thursday => 3,
            Weekday::Friday => 4,
            Weekday::Saturday => 5,
            Weekday::Sunday => 6,
        }
    }

    /// 週の起算日 `start` から数えて何日目か。起算日そのものが0、最後の日が6。
    #[must_use]
    pub const fn days_since(self, start: Weekday) -> u32 {
        (self.index_from_monday() + 7 - start.index_from_monday()) % 7
    }

    /// 翌日の曜日。
    #[must_use]
    pub const fn succ(self) -> Weekday {
        match self {
            Weekday::Monday => Weekday::Tuesday,
            Weekday::Tuesday => Weekday::Wednesday,
            Weekday::Wednesday => Weekday::Thursday,
            Weekday::Thursday => Weekday::Friday,
            Weekday::Friday => Weekday::Saturday,
            Weekday::Saturday => Weekday::Sunday,
            Weekday::Sunday => Weekday::Monday,
        }
    }

    /// 1970-01-01（木曜）を 0 とする日数から、その日の曜日を求める。
    pub(crate) const fn from_day_number(days: i64) -> Weekday {
        match days.rem_euclid(7) {
            0 => Weekday::Thursday,
            1 => Weekday::Friday,
            2 => Weekday::Saturday,
            3 => Weekday::Sunday,
            4 => Weekday::Monday,
            5 => Weekday::Tuesday,
            _ => Weekday::Wednesday,
        }
    }
}
