//! `kintai leave`: 年休のロット・残数・5日取得義務・管理簿（`docs/spec/cli.md`・`docs/spec/leave.md`。AC-12.4）。
//!
//! 従業員の属性（雇入れ日・所定労働の条件・出勤率・明示の付与）は方針ファイルの `[[employee]]`、取得は休暇CSV。
//! 対象は方針ファイルの従業員（ID順。`--employee` で1人）。1人分の計算と項目名は、ケース
//! （`kintai.caseleave.v1` → `kintai.leave.v1`）と同じ。
//!
//! 年休の対応外（`LeaveHourly` → `LeaveSplitGrant` → `LeaveOutOfTable`）は、対象の従業員ごと（ID順）に見て、
//! 1人でもあれば終了コード3（エラーに従業員IDを含める）。

use std::path::PathBuf;

use kintai_core::Date;
use kintai_core::punch::EmployeeId;
use kintai_jp_labor::leave::{
    CalendarDate, FiveDayStatus, LeaveError, LeaveInput, LeaveResult, LeaveUnit, TakenCheck,
    TakenInput, check_unsupported, compute,
};
use kintai_jp_labor::report::TABLE_CHECKED_ON;
use serde::Serialize;

use crate::args::{self, Args, Format, required};
use crate::case36::json_line;
use crate::failure::Failure;
use crate::inputs::Leaves;
use crate::policy_file::{Employee, EmployeeDates, PolicyFile, leave_unsupported_for};
use crate::text::{
    Align, CommandOutput, EXIT_OK, REVIEW_STATUS, days, footer, line, list, notice_codes,
    render_table,
};

/// 年休の出力の `schema`。
pub const LEAVE_REPORT_SCHEMA: &str = "kintai.leavereport.v1";

/// `kintai leave` の引数。
#[derive(Clone, Debug)]
pub struct LeaveArgs {
    /// 方針ファイル。
    pub policy: PathBuf,
    /// 休暇CSV。
    pub leaves: PathBuf,
    /// 基準の日付。
    pub as_of: Date,
    /// 1人に絞る。
    pub employee: Option<EmployeeId>,
    /// 出力の形。
    pub format: Format,
}

impl LeaveArgs {
    /// 引数を読む。
    ///
    /// # Errors
    ///
    /// 必須の引数がない、値の形が違う、余分な引数があるときは不正な入力。
    pub fn parse(mut raw: Args) -> Result<LeaveArgs, Failure> {
        let policy = raw.path("--policy")?;
        let leaves = raw.path("--leaves")?;
        let as_of = raw.text("--as-of")?;
        let employee = raw.text("--employee")?;
        let format = raw.text("--format")?;
        raw.finish()?;
        Ok(LeaveArgs {
            policy: required(policy, "--policy")?,
            leaves: required(leaves, "--leaves")?,
            as_of: args::date(&required(as_of, "--as-of")?, "--as-of")?,
            employee: employee
                .map(|text| args::employee(&text, "--employee"))
                .transpose()?,
            format: args::format(format.as_deref(), &[Format::Text, Format::Json])?,
        })
    }
}

#[derive(Serialize)]
struct LeaveReportOut<'a> {
    schema: &'static str,
    as_of: String,
    employees: Vec<EmployeeLeaveOut<'a>>,
    notices: Vec<&'static str>,
    table_checked_on: &'static str,
    review_status: &'static str,
}

/// 1人分。`employee_id` のあとは `kintai.leave.v1` と同じ項目と並び。
#[derive(Serialize)]
struct EmployeeLeaveOut<'a> {
    employee_id: &'a str,
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

fn employee_out<'a>(id: &'a EmployeeId, result: &LeaveResult) -> EmployeeLeaveOut<'a> {
    EmployeeLeaveOut {
        employee_id: id.as_str(),
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

fn leave_error(error: LeaveError, employee: &Employee, rows: &[(u64, Date, LeaveUnit)]) -> Failure {
    match error {
        LeaveError::InsufficientBalance { index } => {
            let at = rows
                .get(index)
                .map_or_else(String::new, |(line, _, _)| format!("{line}行目の"));
            Failure::invalid(format!(
                "休暇CSVの{at}列 date: その日に使える年次有給休暇の残数が足りません"
            ))
        }
        LeaveError::Unsupported(code) => leave_unsupported_for(code, &employee.id),
        other => Failure::invalid(format!("employee[{}]: {other}", employee.index)),
    }
}

/// `kintai leave` の本体。
///
/// # Errors
///
/// 不正な入力（終了コード2）か対応外（終了コード3）。
pub fn run(args: &LeaveArgs) -> Result<CommandOutput, Failure> {
    let policy = PolicyFile::load(&args.policy)?;
    let scope = policy.scope(args.employee.as_ref())?;
    policy.check_unsupported(&scope)?;
    let leaves = Leaves::load(&args.leaves, &policy)?;
    // 年休の対応外（手順3）。日付は読んだ後なので、範囲の中の日付として比べる。
    let as_of = CalendarDate::from_date(args.as_of);
    for employee in &scope {
        let taken: Vec<TakenCheck> = leaves
            .of(&employee.id)
            .iter()
            .map(|(_, date, unit)| TakenCheck {
                date: Some(CalendarDate::from_date(*date)),
                unit: *unit,
            })
            .collect();
        check_unsupported(
            Some(as_of),
            &employee.valid_terms,
            employee.grant_checks().as_deref(),
            &taken,
        )
        .map_err(|code| leave_unsupported_for(code, &employee.id))?;
    }
    let dates = policy.dates()?;
    let mut results = Vec::with_capacity(scope.len());
    for employee in &scope {
        let rows = leaves.of(&employee.id);
        // 手順4で全員の日付を読んだので、必ずある。
        let EmployeeDates { hire_date, grants } = dates.get(&employee.id).ok_or_else(|| {
            Failure::invalid(format!("employee[{}] の日付を読めません", employee.index))
        })?;
        let input = LeaveInput {
            as_of: args.as_of,
            hire_date: *hire_date,
            terms: employee.terms,
            attendance: employee.attendance.clone(),
            grants: grants.clone(),
            taken: rows
                .iter()
                .map(|(_, date, unit)| TakenInput {
                    date: *date,
                    unit: *unit,
                })
                .collect(),
        };
        let result = compute(&input).map_err(|error| leave_error(error, employee, rows))?;
        results.push((*employee, *hire_date, result));
    }
    let stdout = match args.format {
        Format::Json => json_line(&LeaveReportOut {
            schema: LEAVE_REPORT_SCHEMA,
            as_of: args.as_of.to_string(),
            employees: results
                .iter()
                .map(|(employee, _, result)| employee_out(&employee.id, result))
                .collect(),
            notices: notice_codes(),
            table_checked_on: TABLE_CHECKED_ON,
            review_status: REVIEW_STATUS.code(),
        })?,
        Format::Text | Format::Csv => text(args.as_of, &results),
    };
    Ok(CommandOutput {
        stdout,
        exit: EXIT_OK,
    })
}

fn status_label(status: FiveDayStatus) -> &'static str {
    match status {
        FiveDayStatus::Met => "Met（5日以上）",
        FiveDayStatus::InProgress => "InProgress（期間の途中）",
        FiveDayStatus::NotMet => "NotMet（期間が過ぎて5日に満たない）",
        FiveDayStatus::NotApplicable => "NotApplicable（10日未満の付与）",
    }
}

fn employee_text(out: &mut String, employee: &Employee, hire_date: Date, result: &LeaveResult) {
    let annual = employee
        .terms
        .annual_scheduled_days
        .map_or_else(String::new, |days| format!("・年{days}日"));
    line(
        out,
        &format!(
            "\n{}（雇入れ {hire_date}。週{}日・週{}分{annual}）",
            employee.id,
            employee.terms.weekly_scheduled_days,
            employee.terms.weekly_scheduled_minutes
        ),
    );
    line(
        out,
        "  ロット（付与ごと。古いロットから消化。失効日の当日から使えない）:",
    );
    if result.lots.is_empty() {
        line(out, "    なし（基準日までの付与はありません）");
        line(out, &format!("  残数: {}", days(result.balance_halves)));
        return;
    }
    lots_text(out, result);
    five_day_text(out, result);
    register_text(out, result);
    notices_text(out, result);
}

fn lots_text(out: &mut String, result: &LeaveResult) {
    let lots: Vec<Vec<String>> = result
        .lots
        .iter()
        .map(|lot| {
            vec![
                lot.grant_date.to_string(),
                days(lot.granted_halves),
                lot.expires.to_string(),
                days(lot.used_halves),
                days(lot.remaining_halves),
                days(lot.expired_halves),
            ]
        })
        .collect();
    let mut aligns = vec![Align::Left, Align::Right, Align::Left];
    aligns.extend([Align::Right; 3]);
    out.push_str(&indent(&render_table(
        &["付与日", "付与", "失効日", "使用", "残り", "失効"],
        &aligns,
        &lots,
    )));
    line(out, &format!("  残数: {}", days(result.balance_halves)));
}

fn five_day_text(out: &mut String, result: &LeaveResult) {
    line(
        out,
        "  年5日の取得義務（10日以上の付与ごと。期間は付与日から1年）:",
    );
    let five: Vec<Vec<String>> = result
        .five_day
        .iter()
        .map(|entry| {
            vec![
                entry.base_date.to_string(),
                entry.window_end.to_string(),
                days(entry.taken_halves),
                status_label(entry.status).to_owned(),
                days(entry.needed_halves),
            ]
        })
        .collect();
    out.push_str(&indent(&render_table(
        &["基準日", "期間の終わり", "取得", "状態", "あと"],
        &[
            Align::Left,
            Align::Left,
            Align::Right,
            Align::Left,
            Align::Right,
        ],
        &five,
    )));
}

fn register_text(out: &mut String, result: &LeaveResult) {
    line(out, "  管理簿（基準日ごとに、5日の期間に入る取得）:");
    let register: Vec<String> = result
        .register
        .iter()
        .map(|entry| {
            let taken: Vec<String> = entry
                .taken
                .iter()
                .map(|record| {
                    let unit = if record.unit == LeaveUnit::Half {
                        "半日"
                    } else {
                        "1日"
                    };
                    format!("{} {unit}", record.date)
                })
                .collect();
            let taken = if taken.is_empty() {
                "取得なし".to_owned()
            } else {
                taken.join("、")
            };
            format!(
                "    {}（付与{}）: {taken}。計{}",
                entry.base_date,
                days(entry.granted_halves),
                days(entry.taken_halves)
            )
        })
        .collect();
    list(out, &register);
}

fn notices_text(out: &mut String, result: &LeaveResult) {
    let notices: Vec<String> = result
        .notices
        .iter()
        .map(|notice| {
            format!(
                "    {}: 出勤率が未確認の付与があります（付与したものとして数えています）",
                notice.code()
            )
        })
        .collect();
    line(out, "  注意:");
    if notices.is_empty() {
        line(out, "    なし");
    }
    for notice in &notices {
        line(out, notice);
    }
}

/// 表に、もう1段の字下げを付ける。
fn indent(table: &str) -> String {
    let mut out = String::with_capacity(table.len());
    for row in table.lines() {
        out.push_str("  ");
        out.push_str(row);
        out.push('\n');
    }
    out
}

fn text(as_of: Date, results: &[(&Employee, Date, LeaveResult)]) -> String {
    let mut out = String::new();
    line(&mut out, "年次有給休暇（kintai leave）");
    line(&mut out, &format!("基準日（as_of）: {as_of}"));
    line(
        &mut out,
        "日数は日（半日は0.5日）。基準日より後の付与と取得は数えません。",
    );
    for (employee, hire_date, result) in results {
        employee_text(&mut out, employee, *hire_date, result);
    }
    out.push('\n');
    line(
        &mut out,
        "NotMet は、期間が過ぎて5日に満たないことを示すだけで、法違反とは限りません（休業などで取得できなかった場合は違反ではありません。基発1228第15号 答6）。",
    );
    line(
        &mut out,
        "管理簿の保存: 付与の期間中と満了後5年が本則（労働基準法施行規則第24条の7）、当分の間は3年（労働基準法施行規則附則第71条）。製品の既定は5年です（期限の目安は kintai retention --kind leave-register）。",
    );
    out.push_str(&footer(&[]));
    out
}
