//! 区間ごとの区分と割増の下限の説明（`docs/spec/cli.md` の `explain`）。文章の整形はコマンドライン側。
//!
//! ある従業員のある労働日について、実労働を「同じ区分・同じ深夜の属性が続く区間」に分け、区間ごとに
//! 区分（法定内・所定外法定内・時間外・時間外60時間超・法定休日）、深夜か、割増の下限（%）、根拠の条文を返す。
//! 区間の区分は、区分の計算（[`classify()`](crate::classify())）と同じ規則で時刻順に割り当てる:
//! 平日型の分は、先頭から所定内（法定内のうち所定労働時間まで）→ 所定外法定内 → 時間外（60時間以下）→
//! 60時間超の順。法定休日の暦日の分は法定休日。分の数は区分の結果と一致することを確かめる。
//!
//! 割増の下限の表（%）:
//!
//! | 区分 | 深夜でない | 深夜 |
//! |---|---|---|
//! | 法定内・所定外法定内 | 0（労基法32条） | 25（37条4項） |
//! | 時間外 | 25（37条1項、割増賃金令） | 50（労基則20条1項） |
//! | 時間外60時間超 | 50（37条1項ただし書） | 75（労基則20条1項） |
//! | 法定休日 | 35（37条1項、割増賃金令） | 60（労基則20条2項） |
//!
//! 2023-04-01 より前の日付では、時間外60時間超の率を出さず、注意を付ける
//! （中小企業への5割の適用が2023-04-01からで、それより前は企業規模で変わるため）。
//!
//! ```
//! use kintai_core::Weekday;
//! use kintai_jp_labor::explain::{Category, explain_day};
//! use kintai_jp_labor::{ShiftInput, ShiftSet, WorkPolicy, WorkSystem, classify};
//!
//! let policy = WorkPolicy {
//!     work_system: WorkSystem::Standard,
//!     scheduled_minutes_per_day: 420,
//!     week_start: None,
//!     weekly_limit_minutes: None,
//!     special_44h_confirmed: None,
//!     legal_holiday_weekday: Some(Weekday::Sunday),
//!     month_start_day: 1,
//!     ambiguity: None,
//!     worker_kind: None,
//!     holiday_system: None,
//!     holiday_swaps: 0,
//!     night_window: None,
//! }
//! .validate()
//! .unwrap();
//! // 厚生労働省「時間外・休日労働と割増賃金」例1: 所定9〜17時、翌5時まで、休憩1時間。
//! let shift = ShiftInput {
//!     start: "2026-10-07T09:00".parse().unwrap(),
//!     end: "2026-10-08T05:00".parse().unwrap(),
//!     breaks: vec![("2026-10-07T12:00".parse().unwrap(), "2026-10-07T13:00".parse().unwrap())],
//! };
//! let shifts = ShiftSet::new(&[shift]).unwrap();
//! let result = classify(&policy, &shifts, None).unwrap();
//! let day = explain_day(&policy, &shifts, &result, "2026-10-07".parse().unwrap()).unwrap();
//! let last = day.segments.last().unwrap();
//! assert_eq!((last.category, last.night, last.premium.percent), (Category::Overtime, true, Some(50)));
//! assert_eq!(last.minutes(), 420);
//! ```

use std::fmt;

use kintai_core::{Date, Instant, Interval, MINUTES_PER_DAY};

use crate::classify::{AmbiguityCode, Classification, DayResult, RecordDate};
use crate::policy::ValidPolicy;
use crate::provenance::{
    CONFIRMED_ON, LABOR_STANDARDS_ACT, LABOR_STANDARDS_REGULATION, LSA_32_1, LSA_32_2, LSA_37_1,
    LSA_37_4, PREMIUM_ORDER, Provenance,
};
use crate::report::{Notice, ReviewStatus, TABLE_CHECKED_ON};
use crate::shift::{Shift, ShiftSet};

/// 深夜の終わり（その日の何分目。5:00 は含まない）。
const NIGHT_END_MINUTE: i64 = 5 * 60;
/// 深夜の始まり（その日の何分目。22:00 を含む）。
const NIGHT_START_MINUTE: i64 = 22 * 60;

/// 労働基準法第37条第1項ただし書: 1か月60時間を超えた時間外は5割以上。
pub const LSA_37_1_PROVISO: Provenance = Provenance {
    law: LABOR_STANDARDS_ACT,
    article: "第37条第1項ただし書",
    summary: "延長して労働させた時間が1か月60時間を超えたときは、その超えた時間は5割以上",
    confirmed_on: CONFIRMED_ON,
};
/// 労働基準法施行規則第20条第1項: 時間外が深夜に及ぶときは5割以上（60時間超は7割5分以上）。
pub const REG_20_1: Provenance = Provenance {
    law: LABOR_STANDARDS_REGULATION,
    article: "第20条第1項",
    summary: "延長した労働時間が深夜に及ぶときは5割以上（1か月60時間を超える延長は7割5分以上）",
    confirmed_on: CONFIRMED_ON,
};
/// 労働基準法施行規則第20条第2項: 休日の労働が深夜に及ぶときは6割以上。
pub const REG_20_2: Provenance = Provenance {
    law: LABOR_STANDARDS_REGULATION,
    article: "第20条第2項",
    summary: "休日の労働時間が深夜に及ぶときは6割以上",
    confirmed_on: CONFIRMED_ON,
};

/// 区間の区分。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// 法定内のうち、所定労働時間まで（`within`）。
    Within,
    /// 所定外法定内（法定内のうち、所定労働時間を超える分。`extra_within`）。
    ExtraWithin,
    /// 時間外（月60時間以下。`overtime`）。
    Overtime,
    /// 時間外のうち、月60時間超（`over60`）。
    Over60,
    /// 法定休日（`holiday`）。
    Holiday,
}

impl Category {
    /// すべての区分（時刻順に割り当てる順。法定休日は最後）。
    pub const ALL: [Category; 5] = [
        Category::Within,
        Category::ExtraWithin,
        Category::Overtime,
        Category::Over60,
        Category::Holiday,
    ];

    /// 出力の `category` に出す文字列（区分の結果の項目名に合わせる）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Category::Within => "within",
            Category::ExtraWithin => "extra_within",
            Category::Overtime => "overtime",
            Category::Over60 => "over60",
            Category::Holiday => "holiday",
        }
    }

    /// 人が読む名前。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Category::Within => "法定内",
            Category::ExtraWithin => "所定外法定内",
            Category::Overtime => "時間外",
            Category::Over60 => "時間外（月60時間超）",
            Category::Holiday => "法定休日",
        }
    }
}

/// 割増の下限と、その根拠。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Premium {
    /// 割増の下限（%。賃金の総額でなく、上乗せの率）。`None` は率を出さないとき（2023-04-01 より前の60時間超）。
    pub percent: Option<u32>,
    /// 根拠の条文。
    pub basis: &'static [Provenance],
}

/// 区分・深夜の属性・日付から、割増の下限を求める（モジュールの説明の表）。
///
/// `date` は労働日。2023-04-01 より前の [`Category::Over60`] は、率を `None` にする。
#[must_use]
pub fn premium_floor(category: Category, night: bool, date: Date) -> Premium {
    let (percent, basis): (u32, &'static [Provenance]) = match (category, night) {
        (Category::Within | Category::ExtraWithin, false) => (0, &[LSA_32_1, LSA_32_2]),
        (Category::Within | Category::ExtraWithin, true) => (25, &[LSA_32_1, LSA_32_2, LSA_37_4]),
        (Category::Overtime, false) => (25, &[LSA_37_1, PREMIUM_ORDER]),
        (Category::Overtime, true) => (50, &[LSA_37_1, PREMIUM_ORDER, LSA_37_4, REG_20_1]),
        (Category::Over60, false) => (50, &[LSA_37_1_PROVISO]),
        (Category::Over60, true) => (75, &[LSA_37_1_PROVISO, LSA_37_4, REG_20_1]),
        (Category::Holiday, false) => (35, &[LSA_37_1, PREMIUM_ORDER]),
        (Category::Holiday, true) => (60, &[LSA_37_1, PREMIUM_ORDER, LSA_37_4, REG_20_2]),
    };
    let shown = category != Category::Over60 || over60_rate_applies(date);
    Premium {
        percent: shown.then_some(percent),
        basis,
    }
}

/// 月60時間超の5割が、企業規模によらず適用される日付か（2023-04-01 以降）。
#[must_use]
pub fn over60_rate_applies(date: Date) -> bool {
    Date::from_ymd(2023, 4, 1).is_ok_and(|from| date >= from)
}

/// 区間（同じ区分・同じ深夜の属性が続く実労働）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Segment {
    /// 時刻の区間 `[開始, 終了)`。
    pub interval: Interval,
    /// 区分。
    pub category: Category,
    /// 深夜（22:00〜5:00）か。
    pub night: bool,
    /// 割増の下限と根拠。
    pub premium: Premium,
}

impl Segment {
    /// 長さ（分）。
    #[must_use]
    pub const fn minutes(&self) -> i64 {
        self.interval.len_minutes()
    }
}

/// 説明に付ける注意。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExplainNote {
    /// 2023-04-01 より前の日付で、時間外60時間超の率を出していない。
    Over60RateNotShown,
    /// 所定外法定内には法定の割増がなく、割増を払うかは契約（就業規則など）による。
    ExtraWithinIsContractual,
    /// その労働日に、曖昧さの記録がある（`use_default` で計算した）。
    Ambiguity(AmbiguityCode),
}

impl ExplainNote {
    /// 出力の `code` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            ExplainNote::Over60RateNotShown => "Over60RateNotShown",
            ExplainNote::ExtraWithinIsContractual => "ExtraWithinIsContractual",
            ExplainNote::Ambiguity(code) => code.code(),
        }
    }

    /// 人が読む文。
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            ExplainNote::Over60RateNotShown => {
                "2023-04-01 より前の日付は、月60時間超の割増率が企業規模で異なる（中小企業は2023-04-01から5割）ため、率を示していません"
            }
            ExplainNote::ExtraWithinIsContractual => {
                "所定外法定内には法定の割増がありません。割増を払うかは、就業規則などの契約によります"
            }
            ExplainNote::Ambiguity(_) => {
                "この日の勤務には、帰属を直接定めた公的資料のない境界（休日・週・月）があり、製品の既定の読みで計算しています（専門家の確認が要ります）"
            }
        }
    }
}

/// ある労働日の説明（`kintai explain` の中身）。表の確認日・確認の状態・注意が、必ず付く。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayExplanation {
    /// 労働日（始業の暦日）。
    pub date: Date,
    /// 区間（時刻順）。実労働がなければ空。
    pub segments: Vec<Segment>,
    /// 注意（決まった順、重複なし）。
    pub notes: Vec<ExplainNote>,
    /// 計算に使わなかった事情の注意（4つ、決まった順）。
    pub notices: [Notice; 4],
    /// 表の最終確認日（`table_checked_on`）。
    pub table_checked_on: &'static str,
    /// 専門家の確認の状態（`review_status`）。
    pub review_status: ReviewStatus,
}

/// 説明を作れない理由。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExplainError {
    /// 区分の結果が、確定集計の拒否（`refused`）。
    Refused,
    /// 区分の結果が、渡した勤務から計算したものと合わない（呼び出し側の誤り）。
    Mismatch,
}

impl fmt::Display for ExplainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ExplainError::Refused => {
                "曖昧さのため確定集計を拒否した区分の結果からは、説明を作りません"
            }
            ExplainError::Mismatch => "区分の結果が、勤務から計算したものと合いません",
        })
    }
}

impl std::error::Error for ExplainError {}

/// 労働日 `date` の区間ごとの説明を作る。
///
/// `classification` は、同じ方針で `shifts` 全体（週と月の累計のため、その日だけでなく）を区分した結果。
///
/// # Errors
///
/// 区分の結果が拒否なら [`ExplainError::Refused`]。区分の結果と勤務の分が合わなければ [`ExplainError::Mismatch`]。
pub fn explain_day(
    policy: &ValidPolicy,
    shifts: &ShiftSet,
    classification: &Classification,
    date: Date,
) -> Result<DayExplanation, ExplainError> {
    if classification.refused.is_some() {
        return Err(ExplainError::Refused);
    }
    let of_day: Vec<&Shift> = shifts
        .shifts()
        .iter()
        .filter(|shift| shift.labor_day() == date)
        .collect();
    let day = classification.days.iter().find(|day| day.date == date);
    let segments = match day {
        Some(day) => build_segments(policy, &of_day, day, date)?,
        None if of_day.is_empty() => Vec::new(),
        None => return Err(ExplainError::Mismatch),
    };
    let mut notes = Vec::new();
    if !over60_rate_applies(date)
        && segments
            .iter()
            .any(|segment| segment.category == Category::Over60)
    {
        notes.push(ExplainNote::Over60RateNotShown);
    }
    if segments
        .iter()
        .any(|segment| segment.category == Category::ExtraWithin)
    {
        notes.push(ExplainNote::ExtraWithinIsContractual);
    }
    for record in &classification.ambiguities {
        let applies = match record.date {
            RecordDate::LaborDay(labor_day) => labor_day == date,
            RecordDate::WeekStart(week) => week.contains(date),
        };
        let note = ExplainNote::Ambiguity(record.code);
        if applies && !notes.contains(&note) {
            notes.push(note);
        }
    }
    Ok(DayExplanation {
        date,
        segments,
        notes,
        notices: Notice::ALL,
        table_checked_on: TABLE_CHECKED_ON,
        review_status: ReviewStatus::Unreviewed,
    })
}

/// 実労働の小片（分）。1つの暦日に収まる。
#[derive(Clone, Copy)]
struct Piece {
    start: i64,
    end: i64,
    day_start: i64,
    holiday: bool,
}

/// 区分と深夜の属性を付けた区間（つなぐ前）。
#[derive(Clone, Copy, PartialEq, Eq)]
struct Part {
    start: i64,
    end: i64,
    category: Category,
    night: bool,
}

fn build_segments(
    policy: &ValidPolicy,
    shifts: &[&Shift],
    day: &DayResult,
    date: Date,
) -> Result<Vec<Segment>, ExplainError> {
    let pieces = collect_pieces(policy, shifts);
    // 平日型の分に、時刻順に割り当てる区分と分の数。
    let mut quotas = [
        (Category::Within, day.within - day.extra_within),
        (Category::ExtraWithin, day.extra_within),
        (Category::Overtime, day.overtime - day.over60),
        (Category::Over60, day.over60),
    ];
    let mut parts: Vec<Part> = Vec::new();
    let mut holiday_minutes = 0_i64;
    for piece in pieces {
        if piece.holiday {
            // 分の数は勤務の長さで限られるので桁あふれしないが、念のため飽和させる（合わなければ Mismatch）。
            holiday_minutes = holiday_minutes.saturating_add(piece.end - piece.start);
            push_night_split(&mut parts, piece, Category::Holiday);
            continue;
        }
        let mut cursor = piece.start;
        for (category, remaining) in &mut quotas {
            if cursor >= piece.end {
                break;
            }
            let take = (*remaining).min(piece.end - cursor);
            if take > 0 {
                let part = Piece {
                    start: cursor,
                    end: cursor + take,
                    ..piece
                };
                push_night_split(&mut parts, part, *category);
                *remaining -= take;
                cursor += take;
            }
        }
        if cursor < piece.end {
            return Err(ExplainError::Mismatch);
        }
    }
    if quotas.iter().any(|(_, remaining)| *remaining != 0) || holiday_minutes != day.holiday {
        return Err(ExplainError::Mismatch);
    }
    check_night(&parts, day)?;
    merge(parts, date)
}

fn collect_pieces(policy: &ValidPolicy, shifts: &[&Shift]) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for shift in shifts {
        for work in shift.work_intervals() {
            for piece in work.split_at_midnight() {
                let calendar_day = piece.start().date();
                pieces.push(Piece {
                    start: piece.start().minutes(),
                    end: piece.end().minutes(),
                    day_start: calendar_day.day_number() * MINUTES_PER_DAY,
                    holiday: calendar_day.weekday() == policy.legal_holiday_weekday(),
                });
            }
        }
    }
    pieces
}

/// 小片を、暦日の 5:00 と 22:00 で深夜とそれ以外に分けて足す（空の部分は足さない）。
fn push_night_split(parts: &mut Vec<Part>, piece: Piece, category: Category) {
    let bounds = [
        (piece.day_start, piece.day_start + NIGHT_END_MINUTE, true),
        (
            piece.day_start + NIGHT_END_MINUTE,
            piece.day_start + NIGHT_START_MINUTE,
            false,
        ),
        (
            piece.day_start + NIGHT_START_MINUTE,
            piece.day_start + MINUTES_PER_DAY,
            true,
        ),
    ];
    for (from, to, night) in bounds {
        let start = piece.start.max(from);
        let end = piece.end.min(to);
        if start < end {
            parts.push(Part {
                start,
                end,
                category,
                night,
            });
        }
    }
}

/// 深夜の分が、区分の結果の深夜の内訳と一致するかを確かめる。
fn check_night(parts: &[Part], day: &DayResult) -> Result<(), ExplainError> {
    let night = |categories: &[Category]| -> i64 {
        parts
            .iter()
            .filter(|part| part.night && categories.contains(&part.category))
            .map(|part| part.end - part.start)
            .sum()
    };
    let matches = night(&[Category::Within, Category::ExtraWithin]) == day.night_within
        && night(&[Category::Overtime, Category::Over60]) == day.night_overtime
        && night(&[Category::Over60]) == day.night_over60
        && night(&[Category::Holiday]) == day.night_holiday;
    if matches {
        Ok(())
    } else {
        Err(ExplainError::Mismatch)
    }
}

/// 続いていて（前の終わり＝次の始まり）、区分と深夜の属性が同じ区間をつなぎ、区間にする。
fn merge(parts: Vec<Part>, date: Date) -> Result<Vec<Segment>, ExplainError> {
    let mut merged: Vec<Part> = Vec::new();
    for part in parts {
        match merged.last_mut() {
            Some(last)
                if last.end == part.start
                    && last.category == part.category
                    && last.night == part.night =>
            {
                last.end = part.end;
            }
            _ => merged.push(part),
        }
    }
    merged
        .into_iter()
        .map(|part| {
            let start = Instant::from_minutes(part.start).map_err(|_| ExplainError::Mismatch)?;
            let end = Instant::from_minutes(part.end).map_err(|_| ExplainError::Mismatch)?;
            Ok(Segment {
                interval: Interval::new(start, end).map_err(|_| ExplainError::Mismatch)?,
                category: part.category,
                night: part.night,
                premium: premium_floor(part.category, part.night, date),
            })
        })
        .collect()
}
