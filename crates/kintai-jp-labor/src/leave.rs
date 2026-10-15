//! 年次有給休暇の付与・失効・残数・5日取得義務・管理簿（`docs/spec/leave.md`。入出力は
//! `docs/spec/case-format-36-leave.md`）。
//!
//! 労働基準法第39条と、労働基準法施行規則第24条の3〜第24条の7に沿う。日数はすべて「半日」を1とする
//! 整数（`*_halves`）で数える。付与日・失効日・5日の期間の終わりは、民法143条2項の読みの
//! [`period_last_day`]（`期間の終わり(S, n)`）と [`next_period_first_day`]（`次の初日(S, n)`）で決める。
//!
//! 検査の順序は `docs/spec/validation.md` の流儀で、[`compute`] が、範囲（手順2）→ 対応外
//! （手順3。[`check_unsupported`]）→ 計算（手順5。残数が足りない取得は不正な入力）の順に見る。
//! 日付の文字列を読む前に対応外を見たいとき（コマンドライン）は、[`WorkingTerms::validate`] と
//! [`check_unsupported`] を先に呼ぶ（読めなかった日付は `None` で渡す）。
//!
//! ```
//! use kintai_jp_labor::leave::{
//!     Attendance, FiveDayStatus, LeaveInput, LeaveUnit, TakenInput, WorkingTerms, compute,
//! };
//!
//! let input = LeaveInput {
//!     as_of: "2026-10-01".parse().unwrap(),
//!     hire_date: "2024-04-01".parse().unwrap(),
//!     terms: WorkingTerms {
//!         weekly_scheduled_days: 5,
//!         weekly_scheduled_minutes: 2400,
//!         annual_scheduled_days: None,
//!     },
//!     attendance: vec![Attendance::Meets, Attendance::Meets],
//!     grants: None,
//!     taken: vec![TakenInput { date: "2025-06-02".parse().unwrap(), unit: LeaveUnit::Full }],
//! };
//! let result = compute(&input).unwrap();
//! // 2024-10-01 に10日（20半日）。失効日の当日（2026-10-01）から失効済み。
//! let lot = &result.lots[0];
//! assert_eq!(lot.grant_date.to_string(), "2024-10-01");
//! assert_eq!((lot.granted_halves, lot.used_halves, lot.remaining_halves, lot.expired_halves), (20, 2, 0, 18));
//! assert_eq!(result.five_day[0].status, FiveDayStatus::NotMet);
//! assert_eq!(result.five_day[0].needed_halves, 8);
//! ```

use std::fmt;
use std::ops::RangeInclusive;

use kintai_core::{CoreError, Date, days_in_month};

use crate::provenance::{
    CONFIRMED_ON, LABOR_STANDARDS_ACT, LABOR_STANDARDS_REGULATION, Provenance,
};

const fn act(article: &'static str, summary: &'static str) -> Provenance {
    Provenance {
        law: LABOR_STANDARDS_ACT,
        article,
        summary,
        confirmed_on: CONFIRMED_ON,
    }
}

const fn regulation(article: &'static str, summary: &'static str) -> Provenance {
    Provenance {
        law: LABOR_STANDARDS_REGULATION,
        article,
        summary,
        confirmed_on: CONFIRMED_ON,
    }
}

/// 労働基準法第39条第1項: 6か月継続勤務し、全労働日の8割以上出勤した労働者に10労働日。
pub const LSA_39_1: Provenance = act(
    "第39条第1項",
    "雇入れの日から6か月継続勤務し、全労働日の8割以上出勤した労働者に、10労働日の有給休暇",
);
/// 労働基準法第39条第2項: 六箇月経過日から1年ごとの加算（6年以上は10労働日を加算）。8割未満の翌年は付与しない。
pub const LSA_39_2: Provenance = act(
    "第39条第2項",
    "六箇月経過日から起算した継続勤務年数1年ごとに加算（1・2・4・6・8・10労働日）。出勤が8割未満の期間の翌1年は与えなくてよい",
);
/// 労働基準法第39条第3項: 所定労働日数が少ない労働者の比例付与。
pub const LSA_39_3: Provenance = act(
    "第39条第3項",
    "週の所定労働時間が短く、所定労働日数が少ない労働者の日数は、厚生労働省令で定める（比例付与）",
);
/// 労働基準法第39条第4項: 時間を単位とする有給休暇。
pub const LSA_39_4: Provenance = act(
    "第39条第4項",
    "労使協定により、5日以内に限り時間を単位として有給休暇を与えることができる",
);
/// 労働基準法第39条第7項: 10労働日以上の労働者に、基準日から1年以内に5日を時季を定めて与える。
pub const LSA_39_7: Provenance = act(
    "第39条第7項",
    "10労働日以上の有給休暇の労働者に、基準日から1年以内の期間に、5日について時季を定めて与える",
);
/// 労働基準法第115条: 賃金以外の請求権は2年で時効（年休の請求権の2年）。
pub const LSA_115: Provenance = act(
    "第115条",
    "この法律の規定による災害補償その他の請求権（賃金の請求権を除く）は、2年間行わないと時効によって消滅する",
);
/// 労働基準法施行規則第24条の3: 比例付与の日数の表。週30時間以上は比例付与の対象外。
pub const REG_24NO3: Provenance = regulation(
    "第24条の3",
    "比例付与の日数の表（週の所定労働日数と1年間の所定労働日数の区分）。週30時間、週4日、年216日",
);
/// 労働基準法施行規則第24条の5: 基準日より前に与えるときの第一基準日・第二基準日（分割付与の扱い）。
pub const REG_24NO5: Provenance = regulation(
    "第24条の5",
    "基準日より前に10労働日以上を与えるときの第一基準日、第二基準日、10労働日未満を分けて与えるときの扱い",
);
/// 労働基準法施行規則第24条の7: 年次有給休暇管理簿（時季、日数、基準日）。
pub const REG_24NO7: Provenance = regulation(
    "第24条の7",
    "時季、日数及び基準日を労働者ごとに明らかにした年次有給休暇管理簿を作成し、保存する",
);

/// 最初の付与日は、雇入れ日から6か月の期間の次の日（六箇月経過日）。
pub const FIRST_GRANT_MONTHS: u32 = 6;
/// 付与の間隔（12か月）。k 回目の付与日は六箇月経過日から 12k か月の期間の次の日。
pub const GRANT_INTERVAL_MONTHS: u32 = 12;
/// 失効までの期間（24か月）。失効日の当日は使えない。
pub const EXPIRY_MONTHS: u32 = 24;
/// 5日の期間の長さ（12か月）。付与日から `期間の終わり(付与日, 12)` まで。
pub const FIVE_DAY_WINDOW_MONTHS: u32 = 12;
/// 5日取得義務の対象になる付与の最小（10日＝20半日）。
pub const FIVE_DAY_GRANT_MIN_HALVES: i64 = 20;
/// 5日（10半日）。期間の取得がこれ以上なら達成。
pub const FIVE_DAY_TAKEN_HALVES: i64 = 10;
/// 通常付与になる週の所定労働日数の最小（5日。労基則第24条の3第4項の「4日」を超える）。
pub const NORMAL_WEEKLY_DAYS_MIN: u32 = 5;
/// 通常付与になる週の所定労働時間の最小（30時間＝1800分。労基則第24条の3第1項）。
pub const NORMAL_WEEKLY_MINUTES_MIN: u32 = 1800;
/// 通常付与になる年間の所定労働日数の最小（217日。労基則第24条の3第5項の「216日」を超える）。
pub const NORMAL_ANNUAL_DAYS_MIN: u32 = 217;
/// 明示の付与の日数（半日）の範囲。
pub const GRANT_HALVES_RANGE: RangeInclusive<i64> = 1..=200;

/// 通常付与の日数（6か月、1年6か月、…、6年6か月以上の順）。労働基準法第39条第1項・第2項。
pub const NORMAL_GRANT_DAYS: [u32; 7] = [10, 11, 12, 14, 16, 18, 20];
/// 比例付与の日数（週4日、年169〜216日）。労働基準法施行規則第24条の3の表。
pub const FOUR_DAYS_GRANT_DAYS: [u32; 7] = [7, 8, 9, 10, 12, 13, 15];
/// 比例付与の日数（週3日、年121〜168日）。
pub const THREE_DAYS_GRANT_DAYS: [u32; 7] = [5, 6, 6, 8, 9, 10, 11];
/// 比例付与の日数（週2日、年73〜120日）。
pub const TWO_DAYS_GRANT_DAYS: [u32; 7] = [3, 4, 4, 5, 6, 6, 7];
/// 比例付与の日数（週1日、年48〜72日）。
pub const ONE_DAY_GRANT_DAYS: [u32; 7] = [1, 2, 2, 2, 3, 3, 3];

/// 範囲を問わない暦日（`YYYY-MM-DD`。年は0001〜9999）。並べ替えは暦の順。
///
/// 対応外の判定（手順3）は、日付の検査（手順4）より前に行う。そこでは、書式が正しければ、入力の
/// 範囲（2000〜2099年）の外の日付も比べる（範囲の誤りは手順4で不正な入力になる）。
/// [`period_last_day`] などの日付の規則も、この型の上で計算する。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDate {
    year: i32,
    month: u32,
    day: u32,
}

impl CalendarDate {
    /// `YYYY-MM-DD` を読む（前後の空白や桁の違う表記、暦にない日付、0000年は `None`）。
    /// 入力の範囲は見ない。
    #[must_use]
    pub fn parse(text: &str) -> Option<CalendarDate> {
        let bytes: [u8; 10] = text.as_bytes().try_into().ok()?;
        let [y1, y2, y3, y4, b'-', m1, m2, b'-', d1, d2] = bytes else {
            return None;
        };
        let year = i32::try_from(digits(&[y1, y2, y3, y4])?).ok()?;
        let date = CalendarDate {
            year,
            month: digits(&[m1, m2])?,
            day: digits(&[d1, d2])?,
        };
        let last = days_in_month(date.year, date.month)?;
        (year >= 1 && (1..=last).contains(&date.day)).then_some(date)
    }

    /// 日付の型から。
    #[must_use]
    pub fn from_date(date: Date) -> CalendarDate {
        CalendarDate {
            year: date.year(),
            month: date.month(),
            day: date.day(),
        }
    }

    /// 日付の型へ。
    ///
    /// # Errors
    ///
    /// 日付の型の範囲（1900-01-01〜2200-12-31）の外なら [`CoreError::OutOfRange`]。
    pub fn to_date(self) -> Result<Date, CoreError> {
        Date::from_ymd(self.year, self.month, self.day)
    }

    /// 西暦の年。
    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    /// 月（1〜12）。
    #[must_use]
    pub const fn month(self) -> u32 {
        self.month
    }

    /// 日（1〜31）。
    #[must_use]
    pub const fn day(self) -> u32 {
        self.day
    }

    /// 前日。
    fn previous(self) -> Option<CalendarDate> {
        if self.day > 1 {
            return Some(CalendarDate {
                day: self.day - 1,
                ..self
            });
        }
        let (year, month) = if self.month == 1 {
            (self.year.checked_sub(1)?, 12)
        } else {
            (self.year, self.month - 1)
        };
        Some(CalendarDate {
            year,
            month,
            day: days_in_month(year, month)?,
        })
    }

    /// 翌日。
    fn next(self) -> Option<CalendarDate> {
        if self.day < days_in_month(self.year, self.month)? {
            return Some(CalendarDate {
                day: self.day + 1,
                ..self
            });
        }
        let (year, month) = if self.month == 12 {
            (self.year.checked_add(1)?, 1)
        } else {
            (self.year, self.month + 1)
        };
        Some(CalendarDate {
            year,
            month,
            day: 1,
        })
    }

    /// `期間の終わり(S, n)`（[`period_last_day`] と同じ規則）。年が `i32` を超えるなら `None`。
    #[must_use]
    pub fn period_last_day(self, months: u32) -> Option<CalendarDate> {
        let index = i64::from(self.year) * 12 + i64::from(self.month) - 1 + i64::from(months);
        let year = i32::try_from(index.div_euclid(12)).ok()?;
        let month = u32::try_from(index.rem_euclid(12) + 1).ok()?;
        let last = days_in_month(year, month)?;
        if self.day <= last {
            CalendarDate {
                year,
                month,
                day: self.day,
            }
            .previous()
        } else {
            Some(CalendarDate {
                year,
                month,
                day: last,
            })
        }
    }

    /// `次の初日(S, n)`（[`next_period_first_day`] と同じ規則）。年が `i32` を超えるなら `None`。
    #[must_use]
    pub fn next_period_first_day(self, months: u32) -> Option<CalendarDate> {
        self.period_last_day(months)?.next()
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// ASCII の数字の並びを読む。数字でない文字があれば `None`。
fn digits(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0_u32, |acc, &b| {
        b.is_ascii_digit().then(|| acc * 10 + u32::from(b - b'0'))
    })
}

/// `期間の終わり(S, n)`: 日付 `start` から `months` か月の期間の最後の日（民法143条2項）。
///
/// `start` の `months` か月後に同じ日付があれば、その前日。なければ、その月の末日。
///
/// # Errors
///
/// 結果が日付の型の範囲（1900-01-01〜2200-12-31）の外なら [`CoreError::OutOfRange`]。
pub fn period_last_day(start: Date, months: u32) -> Result<Date, CoreError> {
    CalendarDate::from_date(start)
        .period_last_day(months)
        .ok_or(CoreError::OutOfRange)?
        .to_date()
}

/// `次の初日(S, n)`: [`period_last_day`] の翌日（次の期間の初日）。
///
/// # Errors
///
/// 結果が日付の型の範囲の外なら [`CoreError::OutOfRange`]。
pub fn next_period_first_day(start: Date, months: u32) -> Result<Date, CoreError> {
    CalendarDate::from_date(start)
        .next_period_first_day(months)
        .ok_or(CoreError::OutOfRange)?
        .to_date()
}

/// 最初の付与日（六箇月経過日）g0 ＝ `次の初日(雇入れ日, 6)`。
///
/// # Errors
///
/// 結果が日付の型の範囲の外なら [`CoreError::OutOfRange`]。
pub fn first_grant_date(hire_date: Date) -> Result<Date, CoreError> {
    next_period_first_day(hire_date, FIRST_GRANT_MONTHS)
}

/// k 回目（0から）の法定の付与日 `g_k` ＝ `次の初日(g0, 12k)`。前回の付与日からではなく、`g0` から数える。
///
/// # Errors
///
/// 結果が日付の型の範囲の外か、月数が `u32` を超えるなら [`CoreError::OutOfRange`]。
pub fn grant_date(hire_date: Date, k: u32) -> Result<Date, CoreError> {
    let months = k
        .checked_mul(GRANT_INTERVAL_MONTHS)
        .ok_or(CoreError::OutOfRange)?;
    next_period_first_day(first_grant_date(hire_date)?, months)
}

/// 失効日 ＝ `次の初日(付与日, 24)`。この日の当日は使えない（前日まで使える）。
///
/// # Errors
///
/// 結果が日付の型の範囲の外なら [`CoreError::OutOfRange`]。
pub fn expiry_date(grant_date: Date) -> Result<Date, CoreError> {
    next_period_first_day(grant_date, EXPIRY_MONTHS)
}

/// 5日の期間の最後の日（含む）＝ `期間の終わり(付与日, 12)`。
///
/// # Errors
///
/// 結果が日付の型の範囲の外なら [`CoreError::OutOfRange`]。
pub fn five_day_window_end(grant_date: Date) -> Result<Date, CoreError> {
    period_last_day(grant_date, FIVE_DAY_WINDOW_MONTHS)
}

/// 付与日数の表の行（労働基準法第39条、労働基準法施行規則第24条の3）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GrantRow {
    /// 通常付与（週5日以上、週30時間以上、または年217日以上）。
    Normal,
    /// 週4日（年169〜216日）の比例付与。
    FourDays,
    /// 週3日（年121〜168日）の比例付与。
    ThreeDays,
    /// 週2日（年73〜120日）の比例付与。
    TwoDays,
    /// 週1日（年48〜72日）の比例付与。
    OneDay,
}

impl GrantRow {
    /// 比例付与の行（週4日、3日、2日、1日の順）。
    pub const PROPORTIONAL: [GrantRow; 4] = [
        GrantRow::FourDays,
        GrantRow::ThreeDays,
        GrantRow::TwoDays,
        GrantRow::OneDay,
    ];

    /// 継続勤務の区分（6か月、1年6か月、…、6年6か月以上）ごとの日数。
    #[must_use]
    pub const fn days_by_step(self) -> [u32; 7] {
        match self {
            GrantRow::Normal => NORMAL_GRANT_DAYS,
            GrantRow::FourDays => FOUR_DAYS_GRANT_DAYS,
            GrantRow::ThreeDays => THREE_DAYS_GRANT_DAYS,
            GrantRow::TwoDays => TWO_DAYS_GRANT_DAYS,
            GrantRow::OneDay => ONE_DAY_GRANT_DAYS,
        }
    }

    /// 比例付与の行の、週の所定労働日数（通常付与は `None`）。
    #[must_use]
    pub const fn weekly_days(self) -> Option<u32> {
        match self {
            GrantRow::Normal => None,
            GrantRow::FourDays => Some(4),
            GrantRow::ThreeDays => Some(3),
            GrantRow::TwoDays => Some(2),
            GrantRow::OneDay => Some(1),
        }
    }

    /// 比例付与の行の、1年間の所定労働日数の区分（両端を含む。通常付与は `None`）。
    #[must_use]
    pub const fn annual_days(self) -> Option<(u32, u32)> {
        match self {
            GrantRow::Normal => None,
            GrantRow::FourDays => Some((169, 216)),
            GrantRow::ThreeDays => Some((121, 168)),
            GrantRow::TwoDays => Some((73, 120)),
            GrantRow::OneDay => Some((48, 72)),
        }
    }

    /// k 回目（0から。0が6か月、6以上が6年6か月以上）の付与の日数。
    #[must_use]
    pub fn days(self, k: u32) -> u32 {
        let table = self.days_by_step();
        let step = usize::try_from(k).map_or(6, |k| k.min(6));
        table.get(step).copied().unwrap_or(0)
    }

    /// k 回目の付与の半日の数。
    #[must_use]
    pub fn halves(self, k: u32) -> i64 {
        i64::from(self.days(k)) * 2
    }
}

/// 年休の対応外のコード。終了コードは3。並びは判定の順（`docs/spec/leave.md` の「範囲」）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeaveUnsupportedCode {
    /// 時間単位の年休（取得の `unit` が `hours`）。
    LeaveHourly,
    /// 分割付与（10日未満の付与の次の付与が、その付与日から12か月の期間の中にある）。
    LeaveSplitGrant,
    /// 付与日数の表に当たらない（年間の所定労働日数が48日未満、または週0日）。
    LeaveOutOfTable,
}

impl LeaveUnsupportedCode {
    /// すべてのコード（判定の順）。
    pub const ALL: [LeaveUnsupportedCode; 3] = [
        LeaveUnsupportedCode::LeaveHourly,
        LeaveUnsupportedCode::LeaveSplitGrant,
        LeaveUnsupportedCode::LeaveOutOfTable,
    ];

    /// エラーの `code` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            LeaveUnsupportedCode::LeaveHourly => "LeaveHourly",
            LeaveUnsupportedCode::LeaveSplitGrant => "LeaveSplitGrant",
            LeaveUnsupportedCode::LeaveOutOfTable => "LeaveOutOfTable",
        }
    }

    /// 対応外の扱いの名前。
    #[must_use]
    pub const fn work_style(self) -> &'static str {
        match self {
            LeaveUnsupportedCode::LeaveHourly => "時間単位の年次有給休暇",
            LeaveUnsupportedCode::LeaveSplitGrant => "年次有給休暇の分割付与",
            LeaveUnsupportedCode::LeaveOutOfTable => "付与日数の表に当たらない所定労働日数",
        }
    }

    /// 通常の運用として計算すると、どこが違ってしまうか。
    #[must_use]
    pub const fn difference(self) -> &'static str {
        match self {
            LeaveUnsupportedCode::LeaveHourly => {
                "時間で取った分は、残数を時間で数え、5日の取得には数えない"
            }
            LeaveUnsupportedCode::LeaveSplitGrant => {
                "10日に達した日が第一基準日になり、5日の期間と、その前の取得の扱いが変わる"
            }
            LeaveUnsupportedCode::LeaveOutOfTable => "法令の付与日数の表に、当てはまる行がない",
        }
    }

    /// 根拠の条文。
    #[must_use]
    pub const fn provenances(self) -> &'static [Provenance] {
        match self {
            LeaveUnsupportedCode::LeaveHourly => &[LSA_39_4],
            LeaveUnsupportedCode::LeaveSplitGrant => &[LSA_39_7, REG_24NO5],
            LeaveUnsupportedCode::LeaveOutOfTable => &[LSA_39_3, REG_24NO3],
        }
    }

    /// 根拠の条文をつないだ表記（例: `労働基準法第39条第3項、労働基準法施行規則第24条の3`）。
    #[must_use]
    pub fn citation(self) -> String {
        self.provenances()
            .iter()
            .map(Provenance::citation)
            .collect::<Vec<_>>()
            .join("、")
    }
}

impl fmt::Display for LeaveUnsupportedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "対応外の年次有給休暇です: {}（根拠: {}）。{}ため、計算しません",
            self.work_style(),
            self.citation(),
            self.difference()
        )
    }
}

/// 年休の計算の誤り。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LeaveError {
    /// 不正な入力（終了コード2）。所定労働の項目が範囲外。
    OutOfRange {
        /// 項目の名前（例: `weekly_scheduled_days`）。
        field: &'static str,
        /// 受け付ける値の説明。
        expected: &'static str,
    },
    /// 不正な入力。明示の付与の日数（`halves`）が1〜200の外。
    GrantHalvesOutOfRange {
        /// `grants` の何番目か（0から）。
        index: usize,
    },
    /// 対応外（終了コード3）。
    Unsupported(LeaveUnsupportedCode),
    /// 不正な入力。その日に使える残数が足りない取得の記録。
    InsufficientBalance {
        /// `taken` の何番目か（0から）。
        index: usize,
    },
    /// 不正な入力。計算した日付が、日付の型の範囲（1900〜2200年）の外。
    DateOutOfRange,
    /// 不正な入力。半日の合計が `i64` を超える。
    Overflow,
}

impl fmt::Display for LeaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeaveError::OutOfRange { field, expected } => {
                write!(f, "employee.{field} の値が範囲外です（{expected}）")
            }
            LeaveError::GrantHalvesOutOfRange { index } => {
                write!(f, "employee.grants[{index}].halves は1〜200の整数です")
            }
            LeaveError::Unsupported(code) => write!(f, "{code}"),
            LeaveError::InsufficientBalance { index } => write!(
                f,
                "taken[{index}] の日に使える年次有給休暇の残数が足りません"
            ),
            LeaveError::DateOutOfRange => {
                f.write_str("計算した日付が、日付の範囲（1900〜2200年）の外です")
            }
            LeaveError::Overflow => f.write_str("日数の合計が大きすぎます"),
        }
    }
}

impl std::error::Error for LeaveError {}

impl From<CoreError> for LeaveError {
    fn from(_: CoreError) -> LeaveError {
        LeaveError::DateOutOfRange
    }
}

/// 所定労働の条件（検査の前の値）。範囲外の数値も持てる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkingTerms {
    /// 週の所定労働日数（0〜7）。
    pub weekly_scheduled_days: i64,
    /// 週の所定労働時間（0〜10080分）。
    pub weekly_scheduled_minutes: i64,
    /// 1年間の所定労働日数（0〜366。`None` は指定なし）。
    pub annual_scheduled_days: Option<i64>,
}

/// 検査済みの所定労働の条件。[`WorkingTerms::validate`] だけが作る。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidWorkingTerms {
    weekly_days: u32,
    weekly_minutes: u32,
    annual_days: Option<u32>,
}

fn in_range(
    value: i64,
    max: u32,
    field: &'static str,
    expected: &'static str,
) -> Result<u32, LeaveError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value <= max)
        .ok_or(LeaveError::OutOfRange { field, expected })
}

impl WorkingTerms {
    /// 範囲を検査する（手順2）。週の日数は0〜7、週の分は0〜10080、年間の日数は0〜366。
    ///
    /// # Errors
    ///
    /// 範囲外は [`LeaveError::OutOfRange`]。
    pub fn validate(&self) -> Result<ValidWorkingTerms, LeaveError> {
        Ok(ValidWorkingTerms {
            weekly_days: in_range(
                self.weekly_scheduled_days,
                7,
                "weekly_scheduled_days",
                "0〜7",
            )?,
            weekly_minutes: in_range(
                self.weekly_scheduled_minutes,
                10_080,
                "weekly_scheduled_minutes",
                "0〜10080",
            )?,
            annual_days: self
                .annual_scheduled_days
                .map(|days| in_range(days, 366, "annual_scheduled_days", "0〜366"))
                .transpose()?,
        })
    }
}

impl ValidWorkingTerms {
    /// 週の所定労働日数。
    #[must_use]
    pub const fn weekly_days(&self) -> u32 {
        self.weekly_days
    }

    /// 週の所定労働時間（分）。
    #[must_use]
    pub const fn weekly_minutes(&self) -> u32 {
        self.weekly_minutes
    }

    /// 1年間の所定労働日数（指定なしは `None`）。
    #[must_use]
    pub const fn annual_days(&self) -> Option<u32> {
        self.annual_days
    }

    /// 付与日数の行を決める（`docs/spec/leave.md` の規則2。手順3の [`LeaveUnsupportedCode::LeaveOutOfTable`] を含む）。
    ///
    /// ① 週5日以上か週1800分以上なら通常付与。② そうでなく年間の日数があれば、週の日数より優先して
    /// その区分（217日以上は通常、48日未満は表の外）。③ それ以外は週の日数の行（週0日は表の外）。
    ///
    /// # Errors
    ///
    /// 表に当たらなければ [`LeaveUnsupportedCode::LeaveOutOfTable`]。
    pub fn grant_row(&self) -> Result<GrantRow, LeaveUnsupportedCode> {
        if self.weekly_days >= NORMAL_WEEKLY_DAYS_MIN
            || self.weekly_minutes >= NORMAL_WEEKLY_MINUTES_MIN
        {
            return Ok(GrantRow::Normal);
        }
        if let Some(annual) = self.annual_days {
            if annual >= NORMAL_ANNUAL_DAYS_MIN {
                return Ok(GrantRow::Normal);
            }
            return GrantRow::PROPORTIONAL
                .into_iter()
                .find(|row| {
                    row.annual_days()
                        .is_some_and(|(low, high)| (low..=high).contains(&annual))
                })
                .ok_or(LeaveUnsupportedCode::LeaveOutOfTable);
        }
        GrantRow::PROPORTIONAL
            .into_iter()
            .find(|row| row.weekly_days() == Some(self.weekly_days))
            .ok_or(LeaveUnsupportedCode::LeaveOutOfTable)
    }
}

/// 出勤率（付与ごと）。この版は出勤率そのものを計算しない。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Attendance {
    /// 8割以上（`meets`）。付与する。
    Meets,
    /// 8割未満（`below`）。その年は付与しない（勤続年数は進む）。
    Below,
    /// 未確認（`unknown`）。付与し、[`LeaveNotice::AttendanceUnconfirmed`] を付ける。
    Unknown,
}

/// 取得の単位。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LeaveUnit {
    /// 1日（`full`）＝2半日。
    Full,
    /// 半日（`half`）＝1半日。
    Half,
    /// 時間単位（`hours`）。対応外（[`LeaveUnsupportedCode::LeaveHourly`]）。
    Hours,
}

impl LeaveUnit {
    /// 半日の数（時間単位は `None`）。
    #[must_use]
    pub const fn halves(self) -> Option<i64> {
        match self {
            LeaveUnit::Full => Some(2),
            LeaveUnit::Half => Some(1),
            LeaveUnit::Hours => None,
        }
    }

    /// 入出力の文字列（`full` `half` `hours`）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            LeaveUnit::Full => "full",
            LeaveUnit::Half => "half",
            LeaveUnit::Hours => "hours",
        }
    }
}

/// 前倒しなどで明示した付与の1件。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantInput {
    /// 付与日。
    pub date: Date,
    /// 日数（半日。1〜200）。
    pub halves: i64,
}

/// 取得の記録の1件。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TakenInput {
    /// 取得した日。
    pub date: Date,
    /// 単位。
    pub unit: LeaveUnit,
}

/// 対応外の判定に使う、明示の付与の1件。日付は書式が正しいときだけ `Some`（範囲は問わない）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantCheck {
    /// 付与日（書式の誤りなら `None`）。
    pub date: Option<CalendarDate>,
    /// 日数（半日）。
    pub halves: i64,
}

/// 対応外の判定に使う、取得の記録の1件。日付は書式が正しいときだけ `Some`（範囲は問わない）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TakenCheck {
    /// 取得した日（書式の誤りなら `None`）。
    pub date: Option<CalendarDate>,
    /// 単位。
    pub unit: LeaveUnit,
}

/// 分割付与か。付与日の昇順に安定に並べ（同じ日は入力の順）、10日（20半日）未満の付与の次の付与が
/// `次の初日(その付与日, 12)` より前にあれば真。書式の誤りのある付与日があれば、並べられないので
/// 判定しない（偽。その誤りは、日付の検査で不正な入力になる）。
fn is_split_grant(grants: &[&GrantCheck]) -> bool {
    let mut dated = Vec::with_capacity(grants.len());
    for grant in grants {
        let Some(date) = grant.date else {
            return false;
        };
        dated.push((date, grant.halves));
    }
    dated.sort_by_key(|(date, _)| *date);
    dated.windows(2).any(|pair| match pair {
        [(date, halves), (next, _)] => {
            *halves < FIVE_DAY_GRANT_MIN_HALVES
                && date
                    .next_period_first_day(GRANT_INTERVAL_MONTHS)
                    .is_some_and(|limit| *next < limit)
        }
        _ => false,
    })
}

/// 対応外を判定する（手順3）。順は時間単位 → 分割付与 → 表の外。表の外でなければ付与日数の行を返す。
///
/// `as_of` と記録の日付が両方とも正しい書式で、記録の日付が `as_of` より後なら、その取得と付与は
/// 入れない（範囲は問わずに比べる）。どちらかが書式の誤り（`None`）の記録は入れたまま判定する
/// （その誤りは日付の検査で不正な入力になる）。ただし書式の誤りのある付与日があれば、分割付与の
/// 判定はしない。`grants` は明示の付与（`None` は法定の付与で、分割付与にならない）。
///
/// # Errors
///
/// 最初に当たった [`LeaveUnsupportedCode`]。
pub fn check_unsupported(
    as_of: Option<CalendarDate>,
    terms: &ValidWorkingTerms,
    grants: Option<&[GrantCheck]>,
    taken: &[TakenCheck],
) -> Result<GrantRow, LeaveUnsupportedCode> {
    let in_scope = |date: Option<CalendarDate>| match (as_of, date) {
        (Some(as_of), Some(date)) => date <= as_of,
        _ => true,
    };
    if taken
        .iter()
        .any(|record| record.unit == LeaveUnit::Hours && in_scope(record.date))
    {
        return Err(LeaveUnsupportedCode::LeaveHourly);
    }
    if let Some(grants) = grants {
        let counted: Vec<&GrantCheck> = grants.iter().filter(|g| in_scope(g.date)).collect();
        if is_split_grant(&counted) {
            return Err(LeaveUnsupportedCode::LeaveSplitGrant);
        }
    }
    terms.grant_row()
}

/// 年休の計算の入力（日付は読んだ後）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaveInput {
    /// 基準の日付（この日より後の付与と取得は出さず、数えない）。
    pub as_of: Date,
    /// 雇入れ日。
    pub hire_date: Date,
    /// 所定労働の条件。
    pub terms: WorkingTerms,
    /// 付与の順（6か月、1年6か月、…）の出勤率。足りない付与は未確認。余りは使わない。
    pub attendance: Vec<Attendance>,
    /// 明示の付与（`None` なら法定の付与日。明示したときは出勤率を使わない）。
    pub grants: Option<Vec<GrantInput>>,
    /// 取得の記録（並びは問わない。同じ日の記録は入力の順）。
    pub taken: Vec<TakenInput>,
}

/// 付与のロット（出力の `lots` の1件）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lot {
    /// 付与日（基準日）。
    pub grant_date: Date,
    /// 付与の半日の数。
    pub granted_halves: i64,
    /// 失効日（この日の当日から使えない）。
    pub expires: Date,
    /// `as_of` までに、このロットから消化した半日の数。
    pub used_halves: i64,
    /// `as_of` の時点の未使用分（失効済みなら0）。
    pub remaining_halves: i64,
    /// 失効した未使用分（失効していなければ0）。
    pub expired_halves: i64,
}

/// 5日取得義務の状態。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FiveDayStatus {
    /// 期間の取得が5日（10半日）以上。
    Met,
    /// 期間の中で、まだ5日に満たない。
    InProgress,
    /// 期間が過ぎて、5日に満たない（法違反とは言わない。休業などで取得が不可能だった場合は違反でない）。
    NotMet,
    /// 付与が10日未満で、義務の対象外。
    NotApplicable,
}

impl FiveDayStatus {
    /// 出力の `status` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            FiveDayStatus::Met => "Met",
            FiveDayStatus::InProgress => "InProgress",
            FiveDayStatus::NotMet => "NotMet",
            FiveDayStatus::NotApplicable => "NotApplicable",
        }
    }
}

/// 5日取得義務（出力の `five_day` の1件）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FiveDay {
    /// 基準日（付与日）。
    pub base_date: Date,
    /// 5日の期間の最後の日（含む）。
    pub window_end: Date,
    /// 期間の取得（半日。`as_of` まで。どのロットから消化したかによらない）。
    pub taken_halves: i64,
    /// 状態。
    pub status: FiveDayStatus,
    /// あと何半日（`Met` と `NotApplicable` は0）。
    pub needed_halves: i64,
}

/// 取得の記録（管理簿の行の中）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TakenRecord {
    /// 取得した日。
    pub date: Date,
    /// 単位（`full` か `half`）。
    pub unit: LeaveUnit,
}

/// 年次有給休暇管理簿の行（出力の `register` の1件）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterEntry {
    /// 基準日（付与日）。
    pub base_date: Date,
    /// 付与の半日の数。
    pub granted_halves: i64,
    /// 5日の期間に入る取得（日付の昇順、同じ日は入力の順。そのロットから消化した分とは限らない）。
    pub taken: Vec<TakenRecord>,
    /// 取得の半日の計。
    pub taken_halves: i64,
}

/// 年休の注意。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeaveNotice {
    /// 出勤率が未確認の付与がある（明示の付与では付かない）。
    AttendanceUnconfirmed,
}

impl LeaveNotice {
    /// 出力の `notices` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            LeaveNotice::AttendanceUnconfirmed => "AttendanceUnconfirmed",
        }
    }
}

/// 年休の計算の結果（出力 `kintai.leave.v1` に1対1で対応する）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaveResult {
    /// ロット（付与日の昇順）。
    pub lots: Vec<Lot>,
    /// 残数（`remaining_halves` の合計）。
    pub balance_halves: i64,
    /// 5日取得義務（付与日の昇順）。
    pub five_day: Vec<FiveDay>,
    /// 管理簿（付与日の昇順）。
    pub register: Vec<RegisterEntry>,
    /// 注意。
    pub notices: Vec<LeaveNotice>,
}

/// ロットの下書き（付与日と日数と、決まった日付）。
#[derive(Clone, Copy, Debug)]
struct Draft {
    grant_date: Date,
    granted_halves: i64,
    expires: Date,
    window_end: Date,
}

impl Draft {
    fn new(grant_date: Date, granted_halves: i64) -> Result<Draft, LeaveError> {
        Ok(Draft {
            grant_date,
            granted_halves,
            expires: expiry_date(grant_date)?,
            window_end: five_day_window_end(grant_date)?,
        })
    }

    /// その日に使えるか（付与日 ≤ 日付 < 失効日。失効日の当日は使えない）。消化の判定はこの1か所で決める。
    fn usable_on(&self, date: Date) -> bool {
        self.grant_date <= date && date < self.expires
    }
}

/// 法定の付与（`g_k`）のロット。2つ目は、出勤率が未確認の付与があったか。
fn statutory_drafts(input: &LeaveInput, row: GrantRow) -> Result<(Vec<Draft>, bool), LeaveError> {
    let mut drafts = Vec::new();
    let mut unconfirmed = false;
    let mut k: u32 = 0;
    // 型の範囲を超える付与日は、as_of（型の範囲の中）より後なので、そこで終わる。
    while let Ok(date) = grant_date(input.hire_date, k) {
        if date > input.as_of {
            break;
        }
        let attendance = usize::try_from(k)
            .ok()
            .and_then(|index| input.attendance.get(index))
            .copied()
            .unwrap_or(Attendance::Unknown);
        match attendance {
            Attendance::Below => {}
            Attendance::Meets | Attendance::Unknown => {
                unconfirmed |= attendance == Attendance::Unknown;
                drafts.push(Draft::new(date, row.halves(k))?);
            }
        }
        k = k.checked_add(1).ok_or(LeaveError::Overflow)?;
    }
    Ok((drafts, unconfirmed))
}

/// 明示の付与のロット（`as_of` より後は出さない。付与日の昇順、同じ日は入力の順）。
fn explicit_drafts(grants: &[GrantInput], as_of: Date) -> Result<Vec<Draft>, LeaveError> {
    let mut drafts = grants
        .iter()
        .filter(|grant| grant.date <= as_of)
        .map(|grant| Draft::new(grant.date, grant.halves))
        .collect::<Result<Vec<Draft>, LeaveError>>()?;
    drafts.sort_by_key(|draft| draft.grant_date);
    Ok(drafts)
}

fn add(a: i64, b: i64) -> Result<i64, LeaveError> {
    a.checked_add(b).ok_or(LeaveError::Overflow)
}

/// 取得を日付順に、その日に使えるロット（付与日 ≤ 日付 < 失効日）の古いものから消化する。
/// `counted` は `as_of` までの取得（入力の番号つき、日付の昇順、同じ日は入力の順）。
fn consume(drafts: &[Draft], counted: &[(usize, TakenInput)]) -> Result<Vec<i64>, LeaveError> {
    let mut used = vec![0_i64; drafts.len()];
    // 失効日は付与日について単調なので、付与済みで使えない（失効した）ロットは先頭に並ぶ。
    // 取得は日付の昇順なので、一度失効したロットは後の取得でも使えず、飛ばしてよい。
    let mut first_active = 0;
    for (index, record) in counted {
        let mut need = record
            .unit
            .halves()
            .ok_or(LeaveError::Unsupported(LeaveUnsupportedCode::LeaveHourly))?;
        while drafts
            .get(first_active)
            .is_some_and(|draft| draft.grant_date <= record.date && !draft.usable_on(record.date))
        {
            first_active += 1;
        }
        for (draft, used) in drafts.iter().zip(used.iter_mut()).skip(first_active) {
            if need == 0 || draft.grant_date > record.date {
                break;
            }
            if draft.usable_on(record.date) {
                let take = (draft.granted_halves - *used).min(need);
                *used += take;
                need -= take;
            }
        }
        if need > 0 {
            return Err(LeaveError::InsufficientBalance { index: *index });
        }
    }
    Ok(used)
}

/// 年休を計算する（`docs/spec/leave.md`）。
///
/// 検査は、範囲（手順2: 所定労働の条件、明示の付与の日数）→ 対応外（手順3）→ 計算（手順5）の順。
/// `as_of` より後の付与と取得は出さず、数えない（残数の検査もしない）。
///
/// # Errors
///
/// 範囲外は [`LeaveError::OutOfRange`]・[`LeaveError::GrantHalvesOutOfRange`]、対応外は
/// [`LeaveError::Unsupported`]、残数が足りない取得は [`LeaveError::InsufficientBalance`]。
pub fn compute(input: &LeaveInput) -> Result<LeaveResult, LeaveError> {
    // 手順2
    let terms = input.terms.validate()?;
    if let Some(index) = input.grants.as_ref().and_then(|grants| {
        grants
            .iter()
            .position(|grant| !GRANT_HALVES_RANGE.contains(&grant.halves))
    }) {
        return Err(LeaveError::GrantHalvesOutOfRange { index });
    }
    // 手順3
    let grant_checks: Option<Vec<GrantCheck>> = input.grants.as_ref().map(|grants| {
        grants
            .iter()
            .map(|grant| GrantCheck {
                date: Some(CalendarDate::from_date(grant.date)),
                halves: grant.halves,
            })
            .collect()
    });
    let taken_checks: Vec<TakenCheck> = input
        .taken
        .iter()
        .map(|record| TakenCheck {
            date: Some(CalendarDate::from_date(record.date)),
            unit: record.unit,
        })
        .collect();
    let row = check_unsupported(
        Some(CalendarDate::from_date(input.as_of)),
        &terms,
        grant_checks.as_deref(),
        &taken_checks,
    )
    .map_err(LeaveError::Unsupported)?;
    // 手順5
    let (drafts, unconfirmed) = match &input.grants {
        Some(grants) => (explicit_drafts(grants, input.as_of)?, false),
        None => statutory_drafts(input, row)?,
    };
    let mut counted: Vec<(usize, TakenInput)> = input
        .taken
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, record)| record.date <= input.as_of)
        .collect();
    counted.sort_by_key(|(_, record)| record.date);
    let used = consume(&drafts, &counted)?;
    finish(input.as_of, &drafts, &used, &counted, unconfirmed)
}

/// ロット・5日取得義務・管理簿を組み立てる。
fn finish(
    as_of: Date,
    drafts: &[Draft],
    used: &[i64],
    counted: &[(usize, TakenInput)],
    unconfirmed: bool,
) -> Result<LeaveResult, LeaveError> {
    let mut lots = Vec::with_capacity(drafts.len());
    let mut five_day = Vec::with_capacity(drafts.len());
    let mut register = Vec::with_capacity(drafts.len());
    let mut balance_halves = 0_i64;
    for (draft, &used_halves) in drafts.iter().zip(used) {
        let unused = draft.granted_halves - used_halves;
        let expired = as_of >= draft.expires;
        let remaining_halves = if expired { 0 } else { unused };
        balance_halves = add(balance_halves, remaining_halves)?;
        lots.push(Lot {
            grant_date: draft.grant_date,
            granted_halves: draft.granted_halves,
            expires: draft.expires,
            used_halves,
            remaining_halves,
            expired_halves: if expired { unused } else { 0 },
        });
        // 5日の期間 [付与日, 期間の終わり] に入る取得（counted は日付の昇順）。
        let from = counted.partition_point(|(_, record)| record.date < draft.grant_date);
        let to = counted.partition_point(|(_, record)| record.date <= draft.window_end);
        let in_window = counted.get(from..to).unwrap_or(&[]);
        let mut taken_halves = 0_i64;
        let mut taken = Vec::with_capacity(in_window.len());
        for (_, record) in in_window {
            taken_halves = add(taken_halves, record.unit.halves().unwrap_or(0))?;
            taken.push(TakenRecord {
                date: record.date,
                unit: record.unit,
            });
        }
        let status = if draft.granted_halves < FIVE_DAY_GRANT_MIN_HALVES {
            FiveDayStatus::NotApplicable
        } else if taken_halves >= FIVE_DAY_TAKEN_HALVES {
            FiveDayStatus::Met
        } else if as_of > draft.window_end {
            FiveDayStatus::NotMet
        } else {
            FiveDayStatus::InProgress
        };
        let needed_halves = match status {
            FiveDayStatus::Met | FiveDayStatus::NotApplicable => 0,
            FiveDayStatus::InProgress | FiveDayStatus::NotMet => {
                (FIVE_DAY_TAKEN_HALVES - taken_halves).max(0)
            }
        };
        five_day.push(FiveDay {
            base_date: draft.grant_date,
            window_end: draft.window_end,
            taken_halves,
            status,
            needed_halves,
        });
        register.push(RegisterEntry {
            base_date: draft.grant_date,
            granted_halves: draft.granted_halves,
            taken,
            taken_halves,
        });
    }
    Ok(LeaveResult {
        lots,
        balance_halves,
        five_day,
        register,
        notices: if unconfirmed {
            vec![LeaveNotice::AttendanceUnconfirmed]
        } else {
            Vec::new()
        },
    })
}
