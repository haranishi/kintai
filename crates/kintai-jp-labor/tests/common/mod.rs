//! 試験の共通の部品: 方針と勤務の組み立て、公的な数値例のJSONの読み込み、
//! 1分ずつ数える素朴な実装（区間の演算の実装と突き合わせるための、試験だけの実装）。
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use kintai_core::{Date, Instant, MonthPeriod, Week, Weekday};
use kintai_jp_labor::{
    Ambiguity, AmbiguityCode, AmbiguityMode, Classification, DayResult, HolidaySystem, NightWindow,
    PeriodResult, RecordDate, Refusal, RefusalCode, ShiftInput, ShiftSet, ValidPolicy, WorkPolicy,
    WorkSystem, WorkerKind, classify,
};
use serde_json::Value;

pub fn at(text: &str) -> Instant {
    text.parse().unwrap()
}

pub fn date(text: &str) -> Date {
    text.parse().unwrap()
}

/// 通常制、所定480分、日曜起算、週2400分、月の起算日1、曖昧さは reject の方針。
pub fn policy(legal_holiday: Weekday) -> WorkPolicy {
    WorkPolicy {
        work_system: WorkSystem::Standard,
        scheduled_minutes_per_day: 480,
        week_start: None,
        weekly_limit_minutes: None,
        special_44h_confirmed: None,
        legal_holiday_weekday: Some(legal_holiday),
        month_start_day: 1,
        ambiguity: None,
        worker_kind: None,
        holiday_system: None,
        holiday_swaps: 0,
        night_window: None,
    }
}

pub fn shift(start: &str, end: &str) -> ShiftInput {
    ShiftInput {
        start: at(start),
        end: at(end),
        breaks: Vec::new(),
    }
}

pub fn shift_with_breaks(start: &str, end: &str, breaks: &[(&str, &str)]) -> ShiftInput {
    ShiftInput {
        start: at(start),
        end: at(end),
        breaks: breaks.iter().map(|(s, e)| (at(s), at(e))).collect(),
    }
}

pub fn run(policy: &WorkPolicy, shifts: &[ShiftInput]) -> Classification {
    run_from(policy, shifts, None)
}

/// `data_from`（打刻がそろっている最初の日）つきで計算する。
pub fn run_from(
    policy: &WorkPolicy,
    shifts: &[ShiftInput],
    data_from: Option<&str>,
) -> Classification {
    let valid = policy.validate().unwrap();
    classify(&valid, &ShiftSet::new(shifts).unwrap(), data_from.map(date)).unwrap()
}

/// 労働日の記録の日付。
pub fn labor(text: &str) -> RecordDate {
    RecordDate::LaborDay(date(text))
}

pub fn day<'a>(result: &'a Classification, text: &str) -> &'a DayResult {
    let wanted = date(text);
    result
        .days
        .iter()
        .find(|d| d.date == wanted)
        .unwrap_or_else(|| panic!("{text} が days にない: {result:?}"))
}

pub fn period<'a>(result: &'a Classification, text: &str) -> &'a PeriodResult {
    result
        .periods
        .iter()
        .find(|p| p.start.to_string() == text)
        .unwrap_or_else(|| panic!("{text} が periods にない: {result:?}"))
}

// ---- 公的な数値例のJSON ----

pub fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/spec/vectors/official-examples.json"
    );
    let text = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&text).unwrap()
}

pub fn weekday_from_name(name: &str) -> Weekday {
    match name {
        "mon" => Weekday::Monday,
        "tue" => Weekday::Tuesday,
        "wed" => Weekday::Wednesday,
        "thu" => Weekday::Thursday,
        "fri" => Weekday::Friday,
        "sat" => Weekday::Saturday,
        "sun" => Weekday::Sunday,
        other => panic!("曜日でない: {other}"),
    }
}

/// 数値例の `policy`（すべて通常制）を方針にする。
pub fn policy_from_json(value: &Value) -> WorkPolicy {
    assert_eq!(value["work_system"], "standard");
    WorkPolicy {
        work_system: WorkSystem::Standard,
        scheduled_minutes_per_day: value["scheduled_minutes_per_day"].as_i64().unwrap(),
        week_start: value["week_start"].as_str().map(weekday_from_name),
        weekly_limit_minutes: value["weekly_limit_minutes"].as_i64(),
        special_44h_confirmed: value["special_44h_confirmed"].as_bool(),
        legal_holiday_weekday: value["legal_holiday_weekday"]
            .as_str()
            .map(weekday_from_name),
        month_start_day: value["month_start_day"].as_i64().unwrap(),
        ambiguity: value["ambiguity"].as_str().map(|text| match text {
            "reject" => AmbiguityMode::Reject,
            "use_default" => AmbiguityMode::UseDefault,
            other => panic!("ambiguity でない: {other}"),
        }),
        worker_kind: None,
        holiday_system: None,
        holiday_swaps: 0,
        night_window: None,
    }
}

pub fn shifts_from_json(value: &Value) -> Vec<ShiftInput> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| ShiftInput {
            start: at(item["start"].as_str().unwrap()),
            end: at(item["end"].as_str().unwrap()),
            breaks: item["breaks"].as_array().map_or_else(Vec::new, |list| {
                list.iter()
                    .map(|pair| (at(pair[0].as_str().unwrap()), at(pair[1].as_str().unwrap())))
                    .collect()
            }),
        })
        .collect()
}

pub const DAY_FIELDS: [&str; 11] = [
    "total",
    "within",
    "extra_within",
    "overtime",
    "over60",
    "holiday",
    "night",
    "night_within",
    "night_overtime",
    "night_over60",
    "night_holiday",
];

pub const PERIOD_FIELDS: [&str; 6] = ["total", "within", "overtime", "over60", "holiday", "night"];

pub fn day_field(day: &DayResult, field: &str) -> i64 {
    match field {
        "total" => day.total,
        "within" => day.within,
        "extra_within" => day.extra_within,
        "overtime" => day.overtime,
        "over60" => day.over60,
        "holiday" => day.holiday,
        "night" => day.night,
        "night_within" => day.night_within,
        "night_overtime" => day.night_overtime,
        "night_over60" => day.night_over60,
        "night_holiday" => day.night_holiday,
        other => panic!("項目でない: {other}"),
    }
}

pub fn period_field(period: &PeriodResult, field: &str) -> i64 {
    match field {
        "total" => period.total,
        "within" => period.within,
        "overtime" => period.overtime,
        "over60" => period.over60,
        "holiday" => period.holiday,
        "night" => period.night,
        other => panic!("項目でない: {other}"),
    }
}

// ---- 1分ずつ数える素朴な実装（試験だけで使う）----

const DAILY: i64 = 480;
const OVER60: i64 = 3600;

fn instant(minutes: i64) -> Instant {
    Instant::from_minutes(minutes).unwrap()
}

fn is_break(shift: &ShiftInput, minute: i64) -> bool {
    shift
        .breaks
        .iter()
        .any(|(s, e)| s.minutes() <= minute && minute < e.minutes())
}

fn work_minutes(shift: &ShiftInput) -> impl Iterator<Item = i64> + '_ {
    (shift.start.minutes()..shift.end.minutes()).filter(move |m| !is_break(shift, *m))
}

fn is_night(minute: i64) -> bool {
    !(300..1320).contains(&minute.rem_euclid(1440))
}

/// 曖昧さの記録を1分ずつ求める（D は実労働の分の暦日の集合）。
fn naive_ambiguities(
    policy: &ValidPolicy,
    shifts: &[&ShiftInput],
    data_from: Option<Date>,
) -> Vec<Ambiguity> {
    // （日数, コード, 記録の日付）で集めると、日付の昇順・同じ日付ではコードの順に並ぶ。
    let mut found: BTreeSet<(i64, AmbiguityCode, RecordDate)> = BTreeSet::new();
    let mut add = |date: RecordDate, code| {
        found.insert((date.day_number(), code, date));
    };
    for shift in shifts {
        let labor_day = shift.start.date();
        let minutes: Vec<i64> = work_minutes(shift).collect();
        if let Some(first) = data_from {
            let week = labor_day.week(policy.week_start());
            if !minutes.is_empty() && week.first_day_number() < first.day_number() {
                add(RecordDate::WeekStart(week), AmbiguityCode::IncompleteWeek);
            }
        }
        let days: BTreeSet<Date> = minutes.iter().map(|m| instant(*m).date()).collect();
        if days.len() < 2 {
            continue;
        }
        let labor = RecordDate::LaborDay(labor_day);
        if days
            .iter()
            .any(|d| d.weekday() == policy.legal_holiday_weekday())
        {
            add(labor, AmbiguityCode::HolidayBoundary);
        }
        let weeks: BTreeSet<Week> = days.iter().map(|d| d.week(policy.week_start())).collect();
        if weeks.len() > 1 {
            add(labor, AmbiguityCode::WeekBoundary);
        }
        let periods: BTreeSet<MonthPeriod> = days
            .iter()
            .map(|d| d.month_period(policy.month_start_day()))
            .collect();
        if periods.len() > 1 {
            add(labor, AmbiguityCode::MonthBoundary);
        }
    }
    found
        .into_iter()
        .map(|(_, code, date)| Ambiguity { date, code })
        .collect()
}

fn empty_day(date: Date) -> DayResult {
    DayResult {
        date,
        total: 0,
        within: 0,
        extra_within: 0,
        overtime: 0,
        over60: 0,
        holiday: 0,
        night: 0,
        night_within: 0,
        night_overtime: 0,
        night_over60: 0,
        night_holiday: 0,
    }
}

/// 1分ずつ、労働日の順・時刻の順に区分する（所定外法定内は後で求める）。
fn naive_days(policy: &ValidPolicy, shifts: &[&ShiftInput]) -> BTreeMap<Date, DayResult> {
    let mut days: BTreeMap<Date, DayResult> = BTreeMap::new();
    let mut day_within: BTreeMap<Date, i64> = BTreeMap::new();
    let mut week_within: BTreeMap<Week, i64> = BTreeMap::new();
    let mut period_overtime: BTreeMap<MonthPeriod, i64> = BTreeMap::new();
    for shift in shifts {
        let labor_day = shift.start.date();
        for minute in work_minutes(shift) {
            let calendar_day = instant(minute).date();
            let night = i64::from(is_night(minute));
            let entry = days
                .entry(labor_day)
                .or_insert_with(|| empty_day(labor_day));
            entry.total += 1;
            entry.night += night;
            if calendar_day.weekday() == policy.legal_holiday_weekday() {
                entry.holiday += 1;
                entry.night_holiday += night;
                continue;
            }
            let week = labor_day.week(policy.week_start());
            let today = day_within.entry(labor_day).or_insert(0);
            let this_week = week_within.entry(week).or_insert(0);
            if *today < DAILY && *this_week < policy.weekly_limit_minutes() {
                *today += 1;
                *this_week += 1;
                entry.within += 1;
                entry.night_within += night;
            } else {
                entry.overtime += 1;
                entry.night_overtime += night;
                let counted = period_overtime
                    .entry(labor_day.month_period(policy.month_start_day()))
                    .or_insert(0);
                if *counted >= OVER60 {
                    entry.over60 += 1;
                    entry.night_over60 += night;
                }
                *counted += 1;
            }
        }
    }
    days
}

/// 仕様の文を1分ずつそのまま数える。勤務は検査済み（重ならない、休憩は内側）とする。
pub fn naive(policy: &ValidPolicy, inputs: &[ShiftInput]) -> Classification {
    naive_from(policy, inputs, None)
}

/// `data_from` つきの [`naive`]。
pub fn naive_from(
    policy: &ValidPolicy,
    inputs: &[ShiftInput],
    data_from: Option<Date>,
) -> Classification {
    let mut shifts: Vec<&ShiftInput> = inputs.iter().collect();
    shifts.sort_by_key(|s| s.start);

    let ambiguities = naive_ambiguities(policy, &shifts, data_from);
    if policy.ambiguity() == AmbiguityMode::Reject && !ambiguities.is_empty() {
        // 暦の日として重複のない昇順（同じ日なら、労働日の記録を残す）。
        let mut by_day: BTreeMap<i64, RecordDate> = BTreeMap::new();
        for a in &ambiguities {
            by_day
                .entry(a.date.day_number())
                .and_modify(|kept| {
                    if a.date < *kept {
                        *kept = a.date;
                    }
                })
                .or_insert(a.date);
        }
        return Classification {
            days: Vec::new(),
            periods: Vec::new(),
            ambiguities: Vec::new(),
            refused: Some(Refusal {
                code: RefusalCode::AmbiguousBoundary,
                dates: by_day.into_values().collect(),
            }),
        };
    }

    let mut periods: BTreeMap<MonthPeriod, PeriodResult> = BTreeMap::new();
    let mut out_days = Vec::new();
    for (_, mut day) in naive_days(policy, &shifts) {
        day.extra_within = (day.within - policy.scheduled_minutes_per_day()).max(0);
        let start = day.date.month_period(policy.month_start_day());
        let sum = periods.entry(start).or_insert(PeriodResult {
            start,
            total: 0,
            within: 0,
            overtime: 0,
            over60: 0,
            holiday: 0,
            night: 0,
        });
        sum.total += day.total;
        sum.within += day.within;
        sum.overtime += day.overtime;
        sum.over60 += day.over60;
        sum.holiday += day.holiday;
        sum.night += day.night;
        out_days.push(day);
    }
    Classification {
        days: out_days,
        periods: periods.into_values().collect(),
        ambiguities: if policy.ambiguity() == AmbiguityMode::UseDefault {
            ambiguities
        } else {
            Vec::new()
        },
        refused: None,
    }
}

// ---- 空との比較（失敗したときに中身が見えるよう assert_eq! で比べる）----

pub const NO_AMBIGUITIES: [Ambiguity; 0] = [];
pub const NO_CODES: [AmbiguityCode; 0] = [];
pub const NO_DAYS: [DayResult; 0] = [];
pub const NO_PERIODS: [PeriodResult; 0] = [];
pub const NO_DATES: [RecordDate; 0] = [];

/// 何も指定しない任意の項目（既定）を、すべて明示した方針。
pub fn with_defaults_spelled_out(mut policy: WorkPolicy) -> WorkPolicy {
    policy.week_start = Some(policy.week_start.unwrap_or(Weekday::Sunday));
    policy.weekly_limit_minutes = Some(policy.weekly_limit_minutes.unwrap_or(2400));
    policy.special_44h_confirmed = Some(policy.special_44h_confirmed.unwrap_or(false));
    policy.ambiguity = Some(policy.ambiguity.unwrap_or(AmbiguityMode::Reject));
    policy.worker_kind = Some(policy.worker_kind.unwrap_or(WorkerKind::Standard));
    policy.holiday_system = Some(policy.holiday_system.unwrap_or(HolidaySystem::Weekly));
    policy.night_window = Some(policy.night_window.unwrap_or(NightWindow::From22To05));
    policy
}
