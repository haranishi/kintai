//! ケースの JSON（`docs/spec/case-format.md`）の読み書き。
//!
//! 入力は、いったん `serde_json::Value` に読み（同じ名前の項目は [`crate::strict_json`] で拒否済み）、
//! 項目ごとに型を確かめる（`docs/spec/validation.md` の手順1）。
//! `serde` の derive で直接読むと、構造体が JSON の配列も受け付けてしまい、知らない項目と `null` の扱いも
//! 細かく決められないため。値の読み替え（列挙の値）は手順2で、[`RawPolicy::to_work_policy`] が行う。
//! 出力は、仕様の例と同じ項目の並びの構造体を `serde_json` で書く。

use kintai_core::Weekday;
use kintai_jp_labor::{
    AmbiguityMode, Classification, HolidaySystem, NightWindow, WorkPolicy, WorkSystem, WorkerKind,
};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::failure::Failure;

/// 区分のケースの入力の `schema`。
pub const CASE_SCHEMA: &str = "kintai.case.v1";
/// 区分の結果の `schema`。
pub const CLASSIFY_SCHEMA: &str = "kintai.classify.v1";

const CASE_KEYS: [&str; 4] = ["schema", "data_from", "policy", "shifts"];
const POLICY_KEYS: [&str; 12] = [
    "work_system",
    "scheduled_minutes_per_day",
    "week_start",
    "weekly_limit_minutes",
    "special_44h_confirmed",
    "legal_holiday_weekday",
    "month_start_day",
    "ambiguity",
    "worker_kind",
    "holiday_system",
    "holiday_swaps",
    "night_window",
];
const SHIFT_KEYS: [&str; 3] = ["start", "end", "breaks"];

/// 型を確かめた方針（列挙の値は、まだ文字列のまま）。
#[derive(Debug, PartialEq, Eq)]
pub struct RawPolicy<'a> {
    work_system: &'a str,
    scheduled_minutes_per_day: i64,
    week_start: Option<&'a str>,
    weekly_limit_minutes: Option<i64>,
    special_44h_confirmed: Option<bool>,
    legal_holiday_weekday: Option<&'a str>,
    month_start_day: i64,
    ambiguity: Option<&'a str>,
    worker_kind: Option<&'a str>,
    holiday_system: Option<&'a str>,
    holiday_swaps: usize,
    night_window: Option<&'a str>,
}

/// 型を確かめた勤務（日時は、まだ文字列のまま。読むのは手順4）。
#[derive(Debug, PartialEq, Eq)]
pub struct RawShift<'a> {
    /// 始業。
    pub start: &'a str,
    /// 終業。
    pub end: &'a str,
    /// 休憩の `(開始, 終了)`。
    pub breaks: Vec<(&'a str, &'a str)>,
}

/// 型を確かめた区分のケース。
#[derive(Debug, PartialEq, Eq)]
pub struct RawCase<'a> {
    /// 打刻がそろっている最初の日（文字列のまま。読むのは手順4）。
    pub data_from: Option<&'a str>,
    /// 方針。
    pub policy: RawPolicy<'a>,
    /// 勤務。
    pub shifts: Vec<RawShift<'a>>,
}

fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, Failure> {
    value
        .as_object()
        .ok_or_else(|| Failure::invalid(format!("{path} はオブジェクトではありません")))
}

fn reject_unknown(map: &Map<String, Value>, known: &[&str], path: &str) -> Result<(), Failure> {
    if map.keys().any(|key| !known.contains(&key.as_str())) {
        return Err(Failure::invalid(format!("{path} に知らない項目があります")));
    }
    Ok(())
}

/// 必須の項目。ないときは不正な入力。`null` は型の誤りとして、読む側で不正な入力になる。
fn required<'a>(map: &'a Map<String, Value>, key: &str, path: &str) -> Result<&'a Value, Failure> {
    map.get(key)
        .ok_or_else(|| Failure::invalid(format!("{path}.{key} がありません")))
}

/// 任意の項目。ないとき、または `null` のときは `None`（既定を使う）。
fn optional<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    map.get(key).filter(|value| !value.is_null())
}

fn string<'a>(value: &'a Value, path: &str) -> Result<&'a str, Failure> {
    value
        .as_str()
        .ok_or_else(|| Failure::invalid(format!("{path} は文字列ではありません")))
}

fn integer(value: &Value, path: &str) -> Result<i64, Failure> {
    if let Some(number) = value.as_i64() {
        return Ok(number);
    }
    if value.is_u64() {
        return Err(Failure::invalid(format!("{path} の値が範囲外です")));
    }
    Err(Failure::invalid(format!("{path} は整数ではありません")))
}

fn boolean(value: &Value, path: &str) -> Result<bool, Failure> {
    value
        .as_bool()
        .ok_or_else(|| Failure::invalid(format!("{path} は真偽値ではありません")))
}

fn optional_string<'a>(
    map: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<&'a str>, Failure> {
    optional(map, key)
        .map(|value| string(value, &format!("{path}.{key}")))
        .transpose()
}

fn read_policy(value: &Value) -> Result<RawPolicy<'_>, Failure> {
    let path = "policy";
    let map = object(value, path)?;
    reject_unknown(map, &POLICY_KEYS, path)?;
    Ok(RawPolicy {
        work_system: string(required(map, "work_system", path)?, "policy.work_system")?,
        scheduled_minutes_per_day: integer(
            required(map, "scheduled_minutes_per_day", path)?,
            "policy.scheduled_minutes_per_day",
        )?,
        week_start: optional_string(map, "week_start", path)?,
        weekly_limit_minutes: optional(map, "weekly_limit_minutes")
            .map(|v| integer(v, "policy.weekly_limit_minutes"))
            .transpose()?,
        special_44h_confirmed: optional(map, "special_44h_confirmed")
            .map(|v| boolean(v, "policy.special_44h_confirmed"))
            .transpose()?,
        legal_holiday_weekday: optional_string(map, "legal_holiday_weekday", path)?,
        month_start_day: integer(
            required(map, "month_start_day", path)?,
            "policy.month_start_day",
        )?,
        ambiguity: optional_string(map, "ambiguity", path)?,
        worker_kind: optional_string(map, "worker_kind", path)?,
        holiday_system: optional_string(map, "holiday_system", path)?,
        holiday_swaps: optional(map, "holiday_swaps")
            .map(|value| {
                value
                    .as_array()
                    .map(Vec::len)
                    .ok_or_else(|| Failure::invalid("policy.holiday_swaps は配列ではありません"))
            })
            .transpose()?
            .unwrap_or(0),
        night_window: optional_string(map, "night_window", path)?,
    })
}

fn read_pair<'a>(value: &'a Value, path: &str) -> Result<(&'a str, &'a str), Failure> {
    match value.as_array().map(Vec::as_slice) {
        Some([start, end]) => Ok((
            string(start, &format!("{path}[0]"))?,
            string(end, &format!("{path}[1]"))?,
        )),
        _ => Err(Failure::invalid(format!(
            "{path} は [開始, 終了] の2つの組ではありません"
        ))),
    }
}

fn read_shift(value: &Value, index: usize) -> Result<RawShift<'_>, Failure> {
    let path = format!("shifts[{index}]");
    let map = object(value, &path)?;
    reject_unknown(map, &SHIFT_KEYS, &path)?;
    let start = string(required(map, "start", &path)?, &format!("{path}.start"))?;
    let end = string(required(map, "end", &path)?, &format!("{path}.end"))?;
    let breaks = match optional(map, "breaks") {
        None => Vec::new(),
        Some(list) => list
            .as_array()
            .ok_or_else(|| Failure::invalid(format!("{path}.breaks は配列ではありません")))?
            .iter()
            .enumerate()
            .map(|(i, pair)| read_pair(pair, &format!("{path}.breaks[{i}]")))
            .collect::<Result<_, _>>()?,
    };
    Ok(RawShift { start, end, breaks })
}

/// 手順1: JSON の形・型・知らない項目・`schema` を確かめて読む。
///
/// # Errors
///
/// どれかに当たれば不正な入力。
pub fn read_case(document: &Value) -> Result<RawCase<'_>, Failure> {
    let map = object(document, "入力")?;
    reject_unknown(map, &CASE_KEYS, "入力")?;
    let schema = string(required(map, "schema", "入力")?, "schema")?;
    if schema != CASE_SCHEMA {
        return Err(Failure::invalid("schema が kintai.case.v1 ではありません"));
    }
    let data_from = optional_string(map, "data_from", "入力")?;
    let policy = read_policy(required(map, "policy", "入力")?)?;
    let shifts = required(map, "shifts", "入力")?
        .as_array()
        .ok_or_else(|| Failure::invalid("shifts は配列ではありません"))?
        .iter()
        .enumerate()
        .map(|(index, shift)| read_shift(shift, index))
        .collect::<Result<_, _>>()?;
    Ok(RawCase {
        data_from,
        policy,
        shifts,
    })
}

// 列挙の値の読み替え。`*_from` は定義にない値で `None` を返し、方針ファイル（TOML）の読み込みと共用する。

/// 曜日（`sun`〜`sat`）。
pub(crate) fn weekday_from(text: &str) -> Option<Weekday> {
    match text {
        "mon" => Some(Weekday::Monday),
        "tue" => Some(Weekday::Tuesday),
        "wed" => Some(Weekday::Wednesday),
        "thu" => Some(Weekday::Thursday),
        "fri" => Some(Weekday::Friday),
        "sat" => Some(Weekday::Saturday),
        "sun" => Some(Weekday::Sunday),
        _ => None,
    }
}

fn weekday(text: &str, path: &str) -> Result<Weekday, Failure> {
    weekday_from(text).ok_or_else(|| not_defined(path))
}

fn not_defined(path: &str) -> Failure {
    Failure::invalid(format!("{path} の値が定義にありません"))
}

/// 労働時間制（`work_system`）。
pub(crate) fn work_system_from(text: &str) -> Option<WorkSystem> {
    match text {
        "standard" => Some(WorkSystem::Standard),
        "variable_monthly" => Some(WorkSystem::VariableMonthly),
        "variable_yearly" => Some(WorkSystem::VariableYearly),
        "variable_weekly_irregular" => Some(WorkSystem::VariableWeeklyIrregular),
        "flex" => Some(WorkSystem::Flex),
        "discretionary" => Some(WorkSystem::Discretionary),
        "offsite_deemed" => Some(WorkSystem::OffsiteDeemed),
        "managerial" => Some(WorkSystem::Managerial),
        "agriculture_exempt" => Some(WorkSystem::AgricultureExempt),
        "surveillance_intermittent" => Some(WorkSystem::SurveillanceIntermittent),
        "night_duty_permit" => Some(WorkSystem::NightDutyPermit),
        "highly_professional" => Some(WorkSystem::HighlyProfessional),
        _ => None,
    }
}

fn work_system(text: &str) -> Result<WorkSystem, Failure> {
    work_system_from(text).ok_or_else(|| not_defined("policy.work_system"))
}

/// 従業員の区分（`worker_kind`）。
pub(crate) fn worker_kind_from(text: &str) -> Option<WorkerKind> {
    match text {
        "standard" => Some(WorkerKind::Standard),
        "minor" => Some(WorkerKind::Minor),
        "pregnant_or_postpartum" => Some(WorkerKind::PregnantOrPostpartum),
        "research_development" => Some(WorkerKind::ResearchDevelopment),
        "driver" => Some(WorkerKind::Driver),
        "physician" => Some(WorkerKind::Physician),
        "disaster_recovery_construction" => Some(WorkerKind::DisasterRecoveryConstruction),
        "hazardous_work" => Some(WorkerKind::HazardousWork),
        _ => None,
    }
}

fn worker_kind(text: &str) -> Result<WorkerKind, Failure> {
    worker_kind_from(text).ok_or_else(|| not_defined("policy.worker_kind"))
}

/// 休日の制度（`holiday_system`）。
pub(crate) fn holiday_system_from(text: &str) -> Option<HolidaySystem> {
    match text {
        "weekly" => Some(HolidaySystem::Weekly),
        "four_weeks" => Some(HolidaySystem::FourWeeks),
        "continuous_rotation" => Some(HolidaySystem::ContinuousRotation),
        _ => None,
    }
}

fn holiday_system(text: &str) -> Result<HolidaySystem, Failure> {
    holiday_system_from(text).ok_or_else(|| not_defined("policy.holiday_system"))
}

/// 深夜の時間帯（`night_window`）。
pub(crate) fn night_window_from(text: &str) -> Option<NightWindow> {
    match text {
        "22-05" => Some(NightWindow::From22To05),
        "23-06" => Some(NightWindow::From23To06),
        _ => None,
    }
}

fn night_window(text: &str) -> Result<NightWindow, Failure> {
    night_window_from(text).ok_or_else(|| not_defined("policy.night_window"))
}

/// 曖昧さの扱い（`ambiguity`）。
pub(crate) fn ambiguity_from(text: &str) -> Option<AmbiguityMode> {
    match text {
        "reject" => Some(AmbiguityMode::Reject),
        "use_default" => Some(AmbiguityMode::UseDefault),
        _ => None,
    }
}

fn ambiguity(text: &str) -> Result<AmbiguityMode, Failure> {
    ambiguity_from(text).ok_or_else(|| not_defined("policy.ambiguity"))
}

impl RawPolicy<'_> {
    /// 手順2のうち、列挙の値を読み替える。定義にない値は不正な入力。数値の範囲は
    /// [`WorkPolicy::validate`] が見る。
    ///
    /// # Errors
    ///
    /// 列挙の値が定義にないとき。
    pub fn to_work_policy(&self) -> Result<WorkPolicy, Failure> {
        Ok(WorkPolicy {
            work_system: work_system(self.work_system)?,
            scheduled_minutes_per_day: self.scheduled_minutes_per_day,
            week_start: self
                .week_start
                .map(|t| weekday(t, "policy.week_start"))
                .transpose()?,
            weekly_limit_minutes: self.weekly_limit_minutes,
            special_44h_confirmed: self.special_44h_confirmed,
            legal_holiday_weekday: self
                .legal_holiday_weekday
                .map(|t| weekday(t, "policy.legal_holiday_weekday"))
                .transpose()?,
            month_start_day: self.month_start_day,
            ambiguity: self.ambiguity.map(ambiguity).transpose()?,
            worker_kind: self.worker_kind.map(worker_kind).transpose()?,
            holiday_system: self.holiday_system.map(holiday_system).transpose()?,
            holiday_swaps: self.holiday_swaps,
            night_window: self.night_window.map(night_window).transpose()?,
        })
    }
}

/// 出力 `kintai.classify.v1`。項目の並びは仕様の例のとおり。
#[derive(Serialize)]
pub struct ClassifyOut {
    schema: &'static str,
    days: Vec<DayOut>,
    periods: Vec<PeriodOut>,
    ambiguities: Vec<AmbiguityOut>,
    refused: Option<RefusedOut>,
}

#[derive(Serialize)]
struct DayOut {
    date: String,
    total: i64,
    within: i64,
    extra_within: i64,
    overtime: i64,
    over60: i64,
    holiday: i64,
    night: i64,
    night_within: i64,
    night_overtime: i64,
    night_over60: i64,
    night_holiday: i64,
}

#[derive(Serialize)]
struct PeriodOut {
    start: String,
    total: i64,
    within: i64,
    overtime: i64,
    over60: i64,
    holiday: i64,
    night: i64,
}

#[derive(Serialize)]
struct AmbiguityOut {
    date: String,
    code: &'static str,
}

#[derive(Serialize)]
struct RefusedOut {
    code: &'static str,
    dates: Vec<String>,
}

impl From<&Classification> for ClassifyOut {
    fn from(result: &Classification) -> ClassifyOut {
        ClassifyOut {
            schema: CLASSIFY_SCHEMA,
            days: result
                .days
                .iter()
                .map(|d| DayOut {
                    date: d.date.to_string(),
                    total: d.total,
                    within: d.within,
                    extra_within: d.extra_within,
                    overtime: d.overtime,
                    over60: d.over60,
                    holiday: d.holiday,
                    night: d.night,
                    night_within: d.night_within,
                    night_overtime: d.night_overtime,
                    night_over60: d.night_over60,
                    night_holiday: d.night_holiday,
                })
                .collect(),
            periods: result
                .periods
                .iter()
                .map(|p| PeriodOut {
                    start: p.start.to_string(),
                    total: p.total,
                    within: p.within,
                    overtime: p.overtime,
                    over60: p.over60,
                    holiday: p.holiday,
                    night: p.night,
                })
                .collect(),
            ambiguities: result
                .ambiguities
                .iter()
                .map(|a| AmbiguityOut {
                    date: a.date.to_string(),
                    code: a.code.code(),
                })
                .collect(),
            refused: result.refused.as_ref().map(|r| RefusedOut {
                code: r.code.code(),
                dates: r.dates.iter().map(ToString::to_string).collect(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{read_case, weekday};
    use serde_json::json;

    #[test]
    fn arrays_are_not_objects() {
        let as_array = json!(["kintai.case.v1", {}, []]);
        assert!(read_case(&as_array).is_err());
    }

    #[test]
    fn null_optional_items_use_defaults() {
        let case = json!({
            "schema": "kintai.case.v1",
            "policy": {
                "work_system": "standard", "scheduled_minutes_per_day": 480,
                "legal_holiday_weekday": "sun", "month_start_day": 1,
                "week_start": null, "weekly_limit_minutes": null, "special_44h_confirmed": null,
                "ambiguity": null, "worker_kind": null, "holiday_system": null, "night_window": null
            },
            "shifts": [{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00", "breaks": null}]
        });
        let raw = read_case(&case).unwrap();
        let policy = raw.policy.to_work_policy().unwrap();
        assert_eq!(policy.week_start, None);
        assert_eq!(raw.shifts[0].breaks, vec![]);
        assert!(weekday("Sun", "x").is_err());
    }
}
