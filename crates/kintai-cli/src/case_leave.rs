//! `kintai.caseleave.v1`（年次有給休暇のケース）を読み、`kintai.leave.v1` を出す
//! （`docs/spec/case-format-36-leave.md`）。
//!
//! 検査は `docs/spec/validation.md` の流儀で、最初に当たったものを返す。
//! 1. 形と型: 知らない項目、必須の項目（トップレベルは `schema` `as_of` `employee` `taken`）、型
//! 2. 列挙と範囲: `attendance` と `unit` の値、所定労働の条件の範囲、`grants[].halves`（1〜200）
//! 3. 対応外（終了コード3）: `LeaveHourly` → `LeaveSplitGrant` → `LeaveOutOfTable`。日付は書式が
//!    正しければ範囲を問わずに `as_of` と比べ、`as_of` より後の取得と付与は入れない（[`check_unsupported`]）
//! 4. 日付の書式と入力の範囲（2000〜2099年）
//! 5. 計算（[`compute`]。残数が足りない取得は不正な入力）

use kintai_jp_labor::leave::{
    Attendance, CalendarDate, GrantCheck, GrantInput, LeaveError, LeaveInput, LeaveResult,
    LeaveUnit, LeaveUnsupportedCode, TakenCheck, TakenInput, WorkingTerms, check_unsupported,
    compute,
};
use serde::Serialize;
use serde_json::Value;

use crate::case::{CaseOutput, EXIT_OK};
use crate::case36::{
    array, date, integer, json_line, object, optional, reject_unknown, required, string,
};
use crate::failure::{EXIT_UNSUPPORTED, Failure};

/// 年休のケースの入力の `schema`。
pub const CASELEAVE_SCHEMA: &str = "kintai.caseleave.v1";
/// 年休の計算の結果の `schema`。
pub const LEAVE_SCHEMA: &str = "kintai.leave.v1";

const CASE_KEYS: [&str; 4] = ["schema", "as_of", "employee", "taken"];
const EMPLOYEE_KEYS: [&str; 6] = [
    "hire_date",
    "weekly_scheduled_days",
    "weekly_scheduled_minutes",
    "annual_scheduled_days",
    "attendance",
    "grants",
];
const GRANT_KEYS: [&str; 2] = ["date", "halves"];
const TAKEN_KEYS: [&str; 2] = ["date", "unit"];

/// 型を確かめた明示の付与（日付は、まだ文字列のまま）。
struct RawGrant<'a> {
    date: &'a str,
    halves: i64,
}

/// 型を確かめた取得の記録（単位も、まだ文字列のまま）。
struct RawTaken<'a> {
    date: &'a str,
    unit: &'a str,
}

/// 型を確かめた年休のケース。
struct RawCaseLeave<'a> {
    as_of: &'a str,
    hire_date: &'a str,
    weekly_scheduled_days: i64,
    weekly_scheduled_minutes: i64,
    annual_scheduled_days: Option<i64>,
    attendance: Vec<&'a str>,
    grants: Option<Vec<RawGrant<'a>>>,
    taken: Vec<RawTaken<'a>>,
}

fn read_grant(value: &Value, index: usize) -> Result<RawGrant<'_>, Failure> {
    let path = format!("employee.grants[{index}]");
    let map = object(value, &path)?;
    reject_unknown(map, &GRANT_KEYS, &path)?;
    Ok(RawGrant {
        date: string(required(map, "date", &path)?, &format!("{path}.date"))?,
        halves: integer(required(map, "halves", &path)?, &format!("{path}.halves"))?,
    })
}

fn read_taken(value: &Value, index: usize) -> Result<RawTaken<'_>, Failure> {
    let path = format!("taken[{index}]");
    let map = object(value, &path)?;
    reject_unknown(map, &TAKEN_KEYS, &path)?;
    Ok(RawTaken {
        date: string(required(map, "date", &path)?, &format!("{path}.date"))?,
        unit: string(required(map, "unit", &path)?, &format!("{path}.unit"))?,
    })
}

/// 手順1: JSON の形・型・知らない項目・必須の項目を確かめて読む。
fn read_case(document: &Value) -> Result<RawCaseLeave<'_>, Failure> {
    let map = object(document, "入力")?;
    reject_unknown(map, &CASE_KEYS, "入力")?;
    if string(required(map, "schema", "入力")?, "schema")? != CASELEAVE_SCHEMA {
        return Err(Failure::invalid(
            "schema が kintai.caseleave.v1 ではありません",
        ));
    }
    let as_of = string(required(map, "as_of", "入力")?, "as_of")?;
    let path = "employee";
    let employee = object(required(map, "employee", "入力")?, path)?;
    reject_unknown(employee, &EMPLOYEE_KEYS, path)?;
    let hire_date = string(required(employee, "hire_date", path)?, "employee.hire_date")?;
    let weekly_scheduled_days = integer(
        required(employee, "weekly_scheduled_days", path)?,
        "employee.weekly_scheduled_days",
    )?;
    let weekly_scheduled_minutes = integer(
        required(employee, "weekly_scheduled_minutes", path)?,
        "employee.weekly_scheduled_minutes",
    )?;
    let annual_scheduled_days = optional(employee, "annual_scheduled_days")
        .map(|value| integer(value, "employee.annual_scheduled_days"))
        .transpose()?;
    let attendance = array(
        required(employee, "attendance", path)?,
        "employee.attendance",
    )?
    .iter()
    .enumerate()
    .map(|(index, value)| string(value, &format!("employee.attendance[{index}]")))
    .collect::<Result<_, _>>()?;
    let grants = optional(employee, "grants")
        .map(|value| {
            array(value, "employee.grants")?
                .iter()
                .enumerate()
                .map(|(index, grant)| read_grant(grant, index))
                .collect::<Result<Vec<_>, Failure>>()
        })
        .transpose()?;
    let taken = array(required(map, "taken", "入力")?, "taken")?
        .iter()
        .enumerate()
        .map(|(index, value)| read_taken(value, index))
        .collect::<Result<_, _>>()?;
    Ok(RawCaseLeave {
        as_of,
        hire_date,
        weekly_scheduled_days,
        weekly_scheduled_minutes,
        annual_scheduled_days,
        attendance,
        grants,
        taken,
    })
}

/// 出勤率（`meets` `below` `unknown`）。定義にない値は `None`（方針ファイルの読み込みと共用する）。
pub(crate) fn attendance_from(text: &str) -> Option<Attendance> {
    match text {
        "meets" => Some(Attendance::Meets),
        "below" => Some(Attendance::Below),
        "unknown" => Some(Attendance::Unknown),
        _ => None,
    }
}

fn attendance(text: &str, index: usize) -> Result<Attendance, Failure> {
    attendance_from(text).ok_or_else(|| {
        Failure::invalid(format!(
            "employee.attendance[{index}] の値が定義にありません"
        ))
    })
}

fn unit(text: &str, index: usize) -> Result<LeaveUnit, Failure> {
    match text {
        "full" => Ok(LeaveUnit::Full),
        "half" => Ok(LeaveUnit::Half),
        "hours" => Ok(LeaveUnit::Hours),
        _ => Err(Failure::invalid(format!(
            "taken[{index}].unit の値が定義にありません"
        ))),
    }
}

/// 年休の対応外（終了コード3）。説明に、扱いの名前と根拠の条文を含める。
fn unsupported(code: LeaveUnsupportedCode) -> Failure {
    Failure {
        code: code.code(),
        message: code.to_string(),
        exit: EXIT_UNSUPPORTED,
    }
}

fn failure(error: LeaveError) -> Failure {
    match error {
        LeaveError::Unsupported(code) => unsupported(code),
        other => Failure::invalid(other.to_string()),
    }
}

/// 出力 `kintai.leave.v1`。項目の並びは仕様の例のとおり。
#[derive(Serialize)]
struct LeaveOut {
    schema: &'static str,
    lots: Vec<LotOut>,
    balance_halves: i64,
    five_day: Vec<FiveDayOut>,
    register: Vec<RegisterOut>,
    notices: Vec<&'static str>,
}

#[derive(Serialize)]
struct LotOut {
    grant_date: String,
    granted_halves: i64,
    expires: String,
    used_halves: i64,
    remaining_halves: i64,
    expired_halves: i64,
}

#[derive(Serialize)]
struct FiveDayOut {
    base_date: String,
    window_end: String,
    taken_halves: i64,
    status: &'static str,
    needed_halves: i64,
}

#[derive(Serialize)]
struct RegisterOut {
    base_date: String,
    granted_halves: i64,
    taken: Vec<TakenOut>,
    taken_halves: i64,
}

#[derive(Serialize)]
struct TakenOut {
    date: String,
    unit: &'static str,
}

impl From<&LeaveResult> for LeaveOut {
    fn from(result: &LeaveResult) -> LeaveOut {
        LeaveOut {
            schema: LEAVE_SCHEMA,
            lots: result
                .lots
                .iter()
                .map(|lot| LotOut {
                    grant_date: lot.grant_date.to_string(),
                    granted_halves: lot.granted_halves,
                    expires: lot.expires.to_string(),
                    used_halves: lot.used_halves,
                    remaining_halves: lot.remaining_halves,
                    expired_halves: lot.expired_halves,
                })
                .collect(),
            balance_halves: result.balance_halves,
            five_day: result
                .five_day
                .iter()
                .map(|entry| FiveDayOut {
                    base_date: entry.base_date.to_string(),
                    window_end: entry.window_end.to_string(),
                    taken_halves: entry.taken_halves,
                    status: entry.status.code(),
                    needed_halves: entry.needed_halves,
                })
                .collect(),
            register: result
                .register
                .iter()
                .map(|entry| RegisterOut {
                    base_date: entry.base_date.to_string(),
                    granted_halves: entry.granted_halves,
                    taken: entry
                        .taken
                        .iter()
                        .map(|record| TakenOut {
                            date: record.date.to_string(),
                            unit: record.unit.code(),
                        })
                        .collect(),
                    taken_halves: entry.taken_halves,
                })
                .collect(),
            notices: result.notices.iter().map(|notice| notice.code()).collect(),
        }
    }
}

/// 厳密に読んだ JSON（同じ名前の項目の拒否は済み）から、結果の JSON を作る。
///
/// # Errors
///
/// 不正な入力は [`Failure::invalid`]、対応外（`LeaveHourly` `LeaveSplitGrant` `LeaveOutOfTable`）は
/// 終了コード3の [`Failure`]。
pub fn run(document: &Value) -> Result<CaseOutput, Failure> {
    // 手順1
    let raw = read_case(document)?;
    // 手順2
    let attendance = raw
        .attendance
        .iter()
        .enumerate()
        .map(|(index, text)| attendance(text, index))
        .collect::<Result<Vec<_>, Failure>>()?;
    let units = raw
        .taken
        .iter()
        .enumerate()
        .map(|(index, record)| unit(record.unit, index))
        .collect::<Result<Vec<_>, Failure>>()?;
    let terms = WorkingTerms {
        weekly_scheduled_days: raw.weekly_scheduled_days,
        weekly_scheduled_minutes: raw.weekly_scheduled_minutes,
        annual_scheduled_days: raw.annual_scheduled_days,
    };
    let valid_terms = terms.validate().map_err(failure)?;
    if let Some(index) = raw.grants.as_ref().and_then(|grants| {
        grants
            .iter()
            .position(|grant| !kintai_jp_labor::leave::GRANT_HALVES_RANGE.contains(&grant.halves))
    }) {
        return Err(failure(LeaveError::GrantHalvesOutOfRange { index }));
    }
    // 手順3（日付は書式だけを見て比べる。範囲の誤りと書式の誤りは手順4）
    let grant_checks: Option<Vec<GrantCheck>> = raw.grants.as_ref().map(|grants| {
        grants
            .iter()
            .map(|grant| GrantCheck {
                date: CalendarDate::parse(grant.date),
                halves: grant.halves,
            })
            .collect()
    });
    let taken_checks: Vec<TakenCheck> = raw
        .taken
        .iter()
        .zip(&units)
        .map(|(record, &unit)| TakenCheck {
            date: CalendarDate::parse(record.date),
            unit,
        })
        .collect();
    check_unsupported(
        CalendarDate::parse(raw.as_of),
        &valid_terms,
        grant_checks.as_deref(),
        &taken_checks,
    )
    .map_err(unsupported)?;
    // 手順4
    let as_of = date(raw.as_of, "as_of")?;
    let hire_date = date(raw.hire_date, "employee.hire_date")?;
    let grants = raw
        .grants
        .as_ref()
        .map(|grants| {
            grants
                .iter()
                .enumerate()
                .map(|(index, grant)| {
                    Ok(GrantInput {
                        date: date(grant.date, &format!("employee.grants[{index}].date"))?,
                        halves: grant.halves,
                    })
                })
                .collect::<Result<Vec<_>, Failure>>()
        })
        .transpose()?;
    let taken = raw
        .taken
        .iter()
        .zip(&units)
        .enumerate()
        .map(|(index, (record, &unit))| {
            Ok(TakenInput {
                date: date(record.date, &format!("taken[{index}].date"))?,
                unit,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    // 手順5
    let result = compute(&LeaveInput {
        as_of,
        hire_date,
        terms,
        attendance,
        grants,
        taken,
    })
    .map_err(failure)?;
    Ok(CaseOutput {
        stdout: json_line(&LeaveOut::from(&result))?,
        exit: EXIT_OK,
    })
}

#[cfg(test)]
mod tests {
    use super::run;
    use serde_json::json;

    #[test]
    fn hourly_record_is_unsupported_even_with_a_broken_date() {
        let case = json!({
            "schema": "kintai.caseleave.v1", "as_of": "2026-01-01",
            "employee": {"hire_date": "2024-04-01", "weekly_scheduled_days": 5,
                "weekly_scheduled_minutes": 2400, "attendance": []},
            "taken": [{"date": "2025-02-30", "unit": "hours"}]
        });
        let failure = run(&case).unwrap_err();
        assert_eq!((failure.code, failure.exit), ("LeaveHourly", 3));
        assert!(failure.message.contains("労働基準法第39条第4項"));
    }
}
