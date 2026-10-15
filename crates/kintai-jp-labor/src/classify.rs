//! 時間の区分（通常の労働時間制）。仕様は `docs/spec/classify.md` と `docs/spec/over60.md`。
//!
//! 実労働を「法定内・時間外・法定休日」に分け、深夜を別に重ねて数える。
//! 計算は区間の演算で行い、1分ずつは数えない（1分ずつ数える別実装と突き合わせるため、
//! 手順は仕様の文の順に対応させてある）。

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;

use kintai_core::{Date, Interval, MINUTES_PER_DAY, MonthPeriod, Week};

use crate::policy::{AmbiguityMode, DAILY_LIMIT_MINUTES, OVER60_THRESHOLD_MINUTES, ValidPolicy};
use crate::shift::{Shift, ShiftSet};

/// 深夜の終わり（その日の何分目。5:00 は含まない）。
const NIGHT_END_MINUTE: i64 = 5 * 60;
/// 深夜の始まり（その日の何分目。22:00 を含む）。
const NIGHT_START_MINUTE: i64 = 22 * 60;

/// 曖昧さのコード。並びは `docs/spec/validation.md` の表の順。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AmbiguityCode {
    /// 勤務の実労働の暦日に、法定休日の曜日がある（またぐ勤務）。
    HolidayBoundary,
    /// 勤務の実労働の暦日が、2つ以上の週にまたがる。
    WeekBoundary,
    /// 勤務の実労働の暦日が、2つ以上の月の期間にまたがる。
    MonthBoundary,
    /// `data_from` より前に起算日がある週に、実労働のある労働日がある（打刻がそろっていない週）。
    IncompleteWeek,
}

impl AmbiguityCode {
    /// 出力の `code` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            AmbiguityCode::HolidayBoundary => "HolidayBoundary",
            AmbiguityCode::WeekBoundary => "WeekBoundary",
            AmbiguityCode::MonthBoundary => "MonthBoundary",
            AmbiguityCode::IncompleteWeek => "IncompleteWeek",
        }
    }
}

/// 曖昧さの記録の日付。`HolidayBoundary` `WeekBoundary` `MonthBoundary` は労働日、
/// `IncompleteWeek` は週の起算日（2000-01-01 より前になることがあるので [`Week`] で持つ）。
///
/// 並べ替えと、`refused.dates` の重複の判定は、暦の日（1970-01-01 からの日数）で行う。
/// 表示は `YYYY-MM-DD`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecordDate {
    /// 労働日（勤務の始業の暦日）。
    LaborDay(Date),
    /// 週（日付はその起算日）。
    WeekStart(Week),
}

impl RecordDate {
    /// 1970-01-01 を 0 とする日数。
    #[must_use]
    pub const fn day_number(self) -> i64 {
        match self {
            RecordDate::LaborDay(date) => date.day_number(),
            RecordDate::WeekStart(week) => week.first_day_number(),
        }
    }

    const fn kind(self) -> u8 {
        match self {
            RecordDate::LaborDay(_) => 0,
            RecordDate::WeekStart(_) => 1,
        }
    }
}

impl Ord for RecordDate {
    fn cmp(&self, other: &RecordDate) -> Ordering {
        (self.day_number(), self.kind(), week_start_key(*self)).cmp(&(
            other.day_number(),
            other.kind(),
            week_start_key(*other),
        ))
    }
}

impl PartialOrd for RecordDate {
    fn partial_cmp(&self, other: &RecordDate) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 同じ日に起算する週どうしを区別する鍵（起算の曜日）。
fn week_start_key(date: RecordDate) -> u32 {
    match date {
        RecordDate::LaborDay(_) => 0,
        RecordDate::WeekStart(week) => week.start_weekday().index_from_monday(),
    }
}

impl fmt::Display for RecordDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordDate::LaborDay(date) => write!(f, "{date}"),
            RecordDate::WeekStart(week) => write!(f, "{week}"),
        }
    }
}

/// 曖昧さの記録（日付とコードの組）。並びは日付の昇順、同じ日付ではコードの順。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ambiguity {
    /// 日付（労働日、または週の起算日）。
    pub date: RecordDate,
    /// コード。
    pub code: AmbiguityCode,
}

impl Ord for Ambiguity {
    fn cmp(&self, other: &Ambiguity) -> Ordering {
        (self.date.day_number(), self.code, self.date).cmp(&(
            other.date.day_number(),
            other.code,
            other.date,
        ))
    }
}

impl PartialOrd for Ambiguity {
    fn partial_cmp(&self, other: &Ambiguity) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 確定集計の拒否のコード。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefusalCode {
    /// 曖昧さ（休日・週・月の境界をまたぐ勤務、打刻のそろっていない週）があり、方針が `reject`。
    AmbiguousBoundary,
}

impl RefusalCode {
    /// 出力の `code` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            RefusalCode::AmbiguousBoundary => "AmbiguousBoundary",
        }
    }
}

/// 確定集計の拒否（出力の `refused`）。終了コードは4。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// コード。
    pub code: RefusalCode,
    /// 曖昧さの記録のある日付（暦の日として重複なし、昇順）。
    pub dates: Vec<RecordDate>,
}

/// 労働日ごとの分（出力の `days` の1件）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayResult {
    /// 労働日（始業の暦日）。
    pub date: Date,
    /// 実労働（`within + overtime + holiday`）。
    pub total: i64,
    /// 法定内。
    pub within: i64,
    /// 所定外法定内（`within` の内数。所定労働時間を超える分）。
    pub extra_within: i64,
    /// 時間外。
    pub overtime: i64,
    /// 月の期間の60時間超（`overtime` の内数）。
    pub over60: i64,
    /// 法定休日。
    pub holiday: i64,
    /// 深夜（`night_within + night_overtime + night_holiday`）。
    pub night: i64,
    /// 法定内の深夜。
    pub night_within: i64,
    /// 時間外の深夜。
    pub night_overtime: i64,
    /// 60時間超の深夜（`night_overtime` の内数）。
    pub night_over60: i64,
    /// 法定休日の深夜。
    pub night_holiday: i64,
}

/// 月の期間ごとの合計（出力の `periods` の1件）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeriodResult {
    /// 月の期間（表示は初日の `YYYY-MM-DD`）。
    pub start: MonthPeriod,
    /// 実労働。
    pub total: i64,
    /// 法定内。
    pub within: i64,
    /// 時間外。
    pub overtime: i64,
    /// 60時間超（`overtime` の内数）。
    pub over60: i64,
    /// 法定休日。
    pub holiday: i64,
    /// 深夜。
    pub night: i64,
}

/// 区分の結果（出力 `kintai.classify.v1` に1対1で対応する）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Classification {
    /// 労働日ごと（日付順。実労働が0の日は出さない）。拒否のときは空。
    pub days: Vec<DayResult>,
    /// 月の期間ごと（日付順）。拒否のときは空。
    pub periods: Vec<PeriodResult>,
    /// 曖昧さの記録（`use_default` のときだけ）。
    pub ambiguities: Vec<Ambiguity>,
    /// 確定集計の拒否（`reject` で曖昧さがあったとき）。
    pub refused: Option<Refusal>,
}

/// 計算の誤り。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClassifyError {
    /// 分の合計が `i64` を超える（検査済みの勤務では起きない）。
    Overflow,
}

impl fmt::Display for ClassifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClassifyError::Overflow => f.write_str("分の合計が大きすぎます"),
        }
    }
}

impl std::error::Error for ClassifyError {}

/// 区分を計算する（`docs/spec/classify.md` の手順1〜7と、`docs/spec/validation.md` の手順5）。
///
/// `data_from` は、打刻がそろっている最初の日（入力の `data_from`）。`None` なら、勤務がすべてだとみなす。
/// 曖昧さがあり、方針が [`AmbiguityMode::Reject`] なら、`days` と `periods` を空にして
/// `refused` を返す。[`AmbiguityMode::UseDefault`] なら、手順をそのまま当てて `ambiguities` を付ける。
///
/// # Errors
///
/// 分の合計の桁あふれは [`ClassifyError::Overflow`]（検査済みの勤務では起きない）。
pub fn classify(
    policy: &ValidPolicy,
    shifts: &ShiftSet,
    data_from: Option<Date>,
) -> Result<Classification, ClassifyError> {
    let ambiguities = find_ambiguities(policy, shifts, data_from);
    match policy.ambiguity() {
        AmbiguityMode::Reject => {
            if !ambiguities.is_empty() {
                let mut dates: Vec<RecordDate> =
                    ambiguities.iter().map(|record| record.date).collect();
                dates.sort();
                dates.dedup_by_key(|date| date.day_number());
                return Ok(Classification {
                    days: Vec::new(),
                    periods: Vec::new(),
                    ambiguities: Vec::new(),
                    refused: Some(Refusal {
                        code: RefusalCode::AmbiguousBoundary,
                        dates,
                    }),
                });
            }
            let days = classify_days(policy, shifts)?;
            let periods = sum_periods(policy, &days)?;
            Ok(Classification {
                days,
                periods,
                ambiguities: Vec::new(),
                refused: None,
            })
        }
        AmbiguityMode::UseDefault => {
            let days = classify_days(policy, shifts)?;
            let periods = sum_periods(policy, &days)?;
            Ok(Classification {
                days,
                periods,
                ambiguities,
                refused: None,
            })
        }
    }
}

/// 曖昧さの記録を求める（`docs/spec/validation.md` の「曖昧さの厳密な定義」）。
///
/// 勤務の実労働の分が属する暦日の集合 D が2つ以上の暦日を含む勤務（またぐ勤務）について、
/// D に法定休日の曜日がある、D が2つ以上の週にまたがる、D が2つ以上の月の期間にまたがる、を見る
/// （日付は労働日）。`data_from` があれば、実労働のある労働日を含む週のうち、起算日が `data_from`
/// より前の週を `IncompleteWeek` にする（日付は週の起算日）。
/// 記録は（日付、コード）の組ごとに1件。並びは日付の昇順、同じ日付ではコードの順。
#[must_use]
pub fn find_ambiguities(
    policy: &ValidPolicy,
    shifts: &ShiftSet,
    data_from: Option<Date>,
) -> Vec<Ambiguity> {
    let mut found = BTreeSet::new();
    for shift in shifts.shifts() {
        let labor_day = shift.labor_day();
        if let Some(first_complete_day) = data_from {
            let week = labor_day.week(policy.week_start());
            if week.first_day_number() < first_complete_day.day_number() {
                found.insert(Ambiguity {
                    date: RecordDate::WeekStart(week),
                    code: AmbiguityCode::IncompleteWeek,
                });
            }
        }
        let days: BTreeSet<Date> = shift
            .work_intervals()
            .into_iter()
            .flat_map(Interval::split_at_midnight)
            .map(|piece| piece.start().date())
            .collect();
        if days.len() < 2 {
            continue;
        }
        let date = RecordDate::LaborDay(labor_day);
        if days
            .iter()
            .any(|day| day.weekday() == policy.legal_holiday_weekday())
        {
            found.insert(Ambiguity {
                date,
                code: AmbiguityCode::HolidayBoundary,
            });
        }
        let weeks: BTreeSet<Week> = days
            .iter()
            .map(|day| day.week(policy.week_start()))
            .collect();
        if weeks.len() >= 2 {
            found.insert(Ambiguity {
                date,
                code: AmbiguityCode::WeekBoundary,
            });
        }
        let periods: BTreeSet<MonthPeriod> = days
            .iter()
            .map(|day| day.month_period(policy.month_start_day()))
            .collect();
        if periods.len() >= 2 {
            found.insert(Ambiguity {
                date,
                code: AmbiguityCode::MonthBoundary,
            });
        }
    }
    found.into_iter().collect()
}

/// 実労働の小片。1つの暦日に収まる `[start, end)`（1970-01-01 00:00 からの分）。
#[derive(Clone, Copy, Debug)]
struct Piece {
    start: i64,
    end: i64,
    /// 小片の暦日の 0:00（分）。
    day_start: i64,
}

impl Piece {
    const fn len(self) -> i64 {
        self.end - self.start
    }

    /// 深夜（暦日の 0:00〜5:00 と 22:00〜24:00）に重なる分。
    fn night(self) -> i64 {
        overlap(
            self.start,
            self.end,
            self.day_start,
            self.day_start + NIGHT_END_MINUTE,
        ) + overlap(
            self.start,
            self.end,
            self.day_start + NIGHT_START_MINUTE,
            self.day_start + MINUTES_PER_DAY,
        )
    }
}

fn overlap(a_start: i64, a_end: i64, b_start: i64, b_end: i64) -> i64 {
    (a_end.min(b_end) - a_start.max(b_start)).max(0)
}

/// 小片の列の先頭 `head_minutes` 分と、残りに分ける（時刻順を保つ）。
fn split_head(pieces: &[Piece], head_minutes: i64) -> (Vec<Piece>, Vec<Piece>) {
    let mut head = Vec::new();
    let mut tail = Vec::new();
    let mut remaining = head_minutes;
    for &piece in pieces {
        if remaining >= piece.len() {
            remaining -= piece.len();
            head.push(piece);
        } else if remaining > 0 {
            let cut = piece.start + remaining;
            head.push(Piece { end: cut, ..piece });
            tail.push(Piece {
                start: cut,
                ..piece
            });
            remaining = 0;
        } else {
            tail.push(piece);
        }
    }
    (head, tail)
}

fn add(a: i64, b: i64) -> Result<i64, ClassifyError> {
    a.checked_add(b).ok_or(ClassifyError::Overflow)
}

fn sum_by(pieces: &[Piece], measure: fn(Piece) -> i64) -> Result<i64, ClassifyError> {
    pieces
        .iter()
        .try_fold(0_i64, |acc, &piece| add(acc, measure(piece)))
}

/// 1つの労働日の小片（手順1・2）。平日型と法定休日に分け、それぞれ時刻順。
#[derive(Default)]
struct LaborDayPieces {
    weekday: Vec<Piece>,
    holiday: Vec<Piece>,
}

fn collect_pieces(policy: &ValidPolicy, shifts: &[Shift]) -> LaborDayPieces {
    let mut pieces = LaborDayPieces::default();
    for shift in shifts {
        for work in shift.work_intervals() {
            for piece in work.split_at_midnight() {
                let calendar_day = piece.start().date();
                let item = Piece {
                    start: piece.start().minutes(),
                    end: piece.end().minutes(),
                    day_start: calendar_day.day_number() * MINUTES_PER_DAY,
                };
                if calendar_day.weekday() == policy.legal_holiday_weekday() {
                    pieces.holiday.push(item);
                } else {
                    pieces.weekday.push(item);
                }
            }
        }
    }
    pieces
}

/// 労働日ごとの分を求める（手順3〜7）。
fn classify_days(policy: &ValidPolicy, shifts: &ShiftSet) -> Result<Vec<DayResult>, ClassifyError> {
    let mut days = Vec::new();
    // 週の法定内の累計と、月の期間の時間外の累計。労働日は昇順に来るので、鍵が替われば0に戻す。
    let mut week_within: Option<(Week, i64)> = None;
    let mut period_overtime: Option<(MonthPeriod, i64)> = None;

    for group in shifts
        .shifts()
        .chunk_by(|a, b| a.labor_day() == b.labor_day())
    {
        let Some(first) = group.first() else {
            continue;
        };
        let labor_day = first.labor_day();
        let pieces = collect_pieces(policy, group);

        // 手順4: 週の残り枠 r と、日の先頭 min(n, 480, r) 分を法定内に。
        let week = labor_day.week(policy.week_start());
        let within_before = match week_within {
            Some((current, minutes)) if current == week => minutes,
            _ => 0,
        };
        let remaining = (policy.weekly_limit_minutes() - within_before).max(0);
        let weekday_minutes = sum_by(&pieces.weekday, Piece::len)?;
        let within = weekday_minutes.min(DAILY_LIMIT_MINUTES).min(remaining);
        let (within_pieces, overtime_pieces) = split_head(&pieces.weekday, within);
        week_within = Some((week, add(within_before, within)?));

        // 手順5: 月の期間ごとに、時間外を時刻順に数え、3600分を超えた分が60時間超。
        let period = labor_day.month_period(policy.month_start_day());
        let overtime_before = match period_overtime {
            Some((current, minutes)) if current == period => minutes,
            _ => 0,
        };
        let overtime = weekday_minutes - within;
        // 60時間以下として数える残り（0以上、この日の時間外以下）。clamp は使わない（最小＞最大でパニックするため）。
        let up_to_60 = (OVER60_THRESHOLD_MINUTES - overtime_before)
            .max(0)
            .min(overtime);
        let (_, over60_pieces) = split_head(&overtime_pieces, up_to_60);
        period_overtime = Some((period, add(overtime_before, overtime)?));

        // 手順6・7: 所定外法定内と深夜。
        let holiday = sum_by(&pieces.holiday, Piece::len)?;
        let night_within = sum_by(&within_pieces, Piece::night)?;
        let night_overtime = sum_by(&overtime_pieces, Piece::night)?;
        let night_holiday = sum_by(&pieces.holiday, Piece::night)?;
        let day = DayResult {
            date: labor_day,
            total: add(add(within, overtime)?, holiday)?,
            within,
            extra_within: (within - policy.scheduled_minutes_per_day()).max(0),
            overtime,
            over60: overtime - up_to_60,
            holiday,
            night: add(add(night_within, night_overtime)?, night_holiday)?,
            night_within,
            night_overtime,
            night_over60: sum_by(&over60_pieces, Piece::night)?,
            night_holiday,
        };
        if day.total > 0 {
            days.push(day);
        }
    }
    Ok(days)
}

/// 月の期間ごとの合計（労働日の属する期間で数える）。
fn sum_periods(
    policy: &ValidPolicy,
    days: &[DayResult],
) -> Result<Vec<PeriodResult>, ClassifyError> {
    let mut periods: Vec<PeriodResult> = Vec::new();
    for day in days {
        let start = day.date.month_period(policy.month_start_day());
        let needs_new = periods.last().is_none_or(|last| last.start != start);
        if needs_new {
            periods.push(PeriodResult {
                start,
                total: 0,
                within: 0,
                overtime: 0,
                over60: 0,
                holiday: 0,
                night: 0,
            });
        }
        if let Some(sum) = periods.last_mut() {
            sum.total = add(sum.total, day.total)?;
            sum.within = add(sum.within, day.within)?;
            sum.overtime = add(sum.overtime, day.overtime)?;
            sum.over60 = add(sum.over60, day.over60)?;
            sum.holiday = add(sum.holiday, day.holiday)?;
            sum.night = add(sum.night, day.night)?;
        }
    }
    Ok(periods)
}
