//! `kintai report`: 月の期間ごとの台帳の項目（`docs/spec/cli.md`・`docs/spec/report.md`。AC-12.2）。
//!
//! 打刻CSV（全体）→ 打刻の対 → 勤務 → 従業員ごとに区分（`data_from` の既定は打刻CSV全体の最初の日付）→
//! 対象の月の期間の台帳の項目・曖昧さの記録・休憩の不足。打刻の異常か曖昧さで拒否した従業員は、その人の値を
//! 出さず `refused` を付け、終了コードを4にする（ほかの人の値は出す）。
//!
//! 出力は `text`（既定。人が読む表）・`json`（`kintai.report.v1`、1行）・`csv`（UTF-8、LF、見出し行あり。
//! 拒否した従業員の行は出さない）。従業員はID順。

use std::collections::BTreeMap;
use std::path::PathBuf;

use kintai_core::Date;
use kintai_core::punch::EmployeeId;
use kintai_jp_labor::report::{EmployeeReport, LedgerRow, MonthlyReport, ReportInput, Rounding};
use serde::Serialize;

use crate::args::{self, Args, Format, required};
use crate::case::EXIT_REFUSED;
use crate::case36::json_line;
use crate::common_out::{
    AmbiguityOut, BreakShortageOut, RefusedOut, refusal_text, refused_ambiguous, refused_anomalies,
};
use crate::failure::Failure;
use crate::inputs::{Punches, Work, YearMonth, period_last_day};
use crate::policy_file::PolicyFile;
use crate::text::{
    Align, CommandOutput, EXIT_OK, REVIEW_STATUS, footer, line, list, notice_codes, render_table,
};

/// 月次の集計の出力の `schema`。
pub const REPORT_SCHEMA: &str = "kintai.report.v1";

/// CSV の列（台帳の項目のあとに、表の確認日・確認の状態・注意を各行に付ける）。
pub const CSV_COLUMNS: [&str; 13] = [
    "employee_id",
    "period_start",
    "work_days",
    "work_minutes",
    "overtime_minutes",
    "over60_minutes",
    "holiday_minutes",
    "night_minutes",
    "within_minutes",
    "extra_within_minutes",
    "table_checked_on",
    "review_status",
    "notices",
];

/// `kintai report` の引数。
#[derive(Clone, Debug)]
pub struct ReportArgs {
    /// 方針ファイル。
    pub policy: PathBuf,
    /// 打刻CSV。
    pub punches: PathBuf,
    /// 対象の月。
    pub month: YearMonth,
    /// 打刻がそろっている最初の日（省略時は打刻CSV全体の最初の日付）。
    pub data_from: Option<Date>,
    /// 1人に絞る。
    pub employee: Option<EmployeeId>,
    /// 出力の形。
    pub format: Format,
}

impl ReportArgs {
    /// 引数を読む。
    ///
    /// # Errors
    ///
    /// 必須の引数がない、値の形が違う、余分な引数があるときは不正な入力。
    pub fn parse(mut raw: Args) -> Result<ReportArgs, Failure> {
        let policy = raw.path("--policy")?;
        let punches = raw.path("--punches")?;
        let month = raw.text("--month")?;
        let data_from = raw.text("--data-from")?;
        let employee = raw.text("--employee")?;
        let format = raw.text("--format")?;
        raw.finish()?;
        Ok(ReportArgs {
            policy: required(policy, "--policy")?,
            punches: required(punches, "--punches")?,
            month: args::month(&required(month, "--month")?, "--month")?,
            data_from: data_from
                .map(|text| args::date(&text, "--data-from"))
                .transpose()?,
            employee: employee
                .map(|text| args::employee(&text, "--employee"))
                .transpose()?,
            format: args::format(
                format.as_deref(),
                &[Format::Text, Format::Json, Format::Csv],
            )?,
        })
    }
}

/// 1人分の行（出力の材料）。
struct Row<'a> {
    employee: &'a EmployeeId,
    ledger: Option<&'a LedgerRow>,
    refused: Option<RefusedOut>,
    ambiguities: Vec<AmbiguityOut>,
    break_shortages: Vec<BreakShortageOut>,
}

/// 計算の結果（出力の形によらない）。
struct Computed<'a> {
    month: YearMonth,
    period_start: String,
    period_end: String,
    data_from: Option<Date>,
    rounding: Rounding,
    rows: Vec<Row<'a>>,
}

#[derive(Serialize)]
struct ReportOut<'a> {
    schema: &'static str,
    month: String,
    period_start: &'a str,
    period_end: &'a str,
    data_from: Option<String>,
    rounding: &'static str,
    employees: Vec<EmployeeOut<'a>>,
    notices: Vec<&'static str>,
    table_checked_on: &'static str,
    review_status: &'static str,
}

#[derive(Serialize)]
struct EmployeeOut<'a> {
    employee_id: &'a str,
    ledger: Vec<LedgerOut>,
    refused: Option<&'a RefusedOut>,
    ambiguities: &'a [AmbiguityOut],
    break_shortages: &'a [BreakShortageOut],
}

#[derive(Serialize)]
struct LedgerOut {
    period_start: String,
    work_days: i64,
    work_minutes: i64,
    overtime_minutes: i64,
    over60_minutes: i64,
    holiday_minutes: i64,
    night_minutes: i64,
    within_minutes: i64,
    extra_within_minutes: i64,
    rounded: Option<RoundedOut>,
}

/// `monthly_30min` で丸めた時間（時間）。
#[derive(Serialize)]
struct RoundedOut {
    overtime: i64,
    holiday: i64,
    night: i64,
}

impl From<&LedgerRow> for LedgerOut {
    fn from(row: &LedgerRow) -> LedgerOut {
        LedgerOut {
            period_start: row.period.to_string(),
            work_days: row.work_days,
            work_minutes: row.work_minutes,
            overtime_minutes: row.overtime_minutes,
            over60_minutes: row.over60_minutes,
            holiday_minutes: row.holiday_minutes,
            night_minutes: row.night_minutes,
            within_minutes: row.within_minutes,
            extra_within_minutes: row.extra_within_minutes,
            rounded: row.rounded.map(|hours| RoundedOut {
                overtime: hours.overtime,
                holiday: hours.holiday,
                night: hours.night,
            }),
        }
    }
}

fn rows<'a>(
    works: &'a [(EmployeeId, Work)],
    monthly: &'a MonthlyReport,
) -> Result<Vec<Row<'a>>, Failure> {
    let reports: BTreeMap<&EmployeeId, &EmployeeReport> = monthly
        .employees
        .iter()
        .map(|report| (&report.employee, report))
        .collect();
    works
        .iter()
        .map(|(employee, work)| match (work, reports.get(employee)) {
            (Work::Classified { .. }, Some(report)) => Ok(Row {
                employee,
                ledger: report.ledger.first(),
                refused: report.refused.as_ref().map(refused_ambiguous),
                ambiguities: report.ambiguities.iter().map(AmbiguityOut::from).collect(),
                break_shortages: report
                    .break_shortages
                    .iter()
                    .map(BreakShortageOut::from)
                    .collect(),
            }),
            (Work::Anomalies(anomalies), _) => Ok(Row {
                employee,
                ledger: None,
                refused: Some(refused_anomalies(anomalies)),
                ambiguities: Vec::new(),
                break_shortages: Vec::new(),
            }),
            // 区分した従業員は、必ず月次の集計にいる。いなければ黙って空の行にせず、誤りにする。
            (Work::Classified { .. }, None) => Err(Failure::invalid(
                "月次の集計に、区分した従業員がいません（内部の不一致）",
            )),
        })
        .collect()
}

/// `kintai report` の本体。
///
/// # Errors
///
/// 不正な入力（終了コード2）か対応外（終了コード3）。曖昧さや打刻の異常による拒否はエラーにせず、
/// 出力に `refused` を付けて終了コードを4にする。
pub fn run(args: &ReportArgs) -> Result<CommandOutput, Failure> {
    let policy = PolicyFile::load(&args.policy)?;
    let scope = policy.scope(args.employee.as_ref())?;
    let valid = policy.check_unsupported(&scope)?;
    let punches = Punches::load(&args.punches, &policy)?;
    policy.dates()?;
    let period = args.month.period(valid.month_start_day());
    let data_from = args.data_from.or(punches.first_date());
    let works = scope
        .iter()
        .map(|employee| {
            Ok((
                employee.id.clone(),
                punches.work(&valid, &employee.id, data_from)?,
            ))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let inputs: Vec<ReportInput<'_>> = works
        .iter()
        .filter_map(|(employee, work)| match work {
            Work::Classified {
                shifts,
                classification,
            } => Some(ReportInput {
                employee,
                shifts,
                classification,
            }),
            Work::Anomalies(_) => None,
        })
        .collect();
    let monthly = MonthlyReport::build(&valid, policy.rounding(), Some(period), &inputs)
        .map_err(|error| Failure::invalid(error.to_string()))?;
    let computed = Computed {
        month: args.month,
        period_start: period.to_string(),
        period_end: period_last_day(period).map_or_else(String::new, |day| day.to_string()),
        data_from,
        rounding: policy.rounding(),
        rows: rows(&works, &monthly)?,
    };
    let exit = if computed.rows.iter().any(|row| row.refused.is_some()) {
        EXIT_REFUSED
    } else {
        EXIT_OK
    };
    let stdout = match args.format {
        Format::Text => text(&computed),
        Format::Json => json(&computed)?,
        Format::Csv => csv(&computed),
    };
    Ok(CommandOutput { stdout, exit })
}

fn json(computed: &Computed<'_>) -> Result<String, Failure> {
    let out = ReportOut {
        schema: REPORT_SCHEMA,
        month: computed.month.to_string(),
        period_start: &computed.period_start,
        period_end: &computed.period_end,
        data_from: computed.data_from.map(|date| date.to_string()),
        rounding: computed.rounding.code(),
        employees: computed
            .rows
            .iter()
            .map(|row| EmployeeOut {
                employee_id: row.employee.as_str(),
                ledger: row.ledger.map(LedgerOut::from).into_iter().collect(),
                refused: row.refused.as_ref(),
                ambiguities: &row.ambiguities,
                break_shortages: &row.break_shortages,
            })
            .collect(),
        notices: notice_codes(),
        table_checked_on: kintai_jp_labor::report::TABLE_CHECKED_ON,
        review_status: REVIEW_STATUS.code(),
    };
    json_line(&out)
}

fn csv(computed: &Computed<'_>) -> String {
    let mut out = String::new();
    line(&mut out, &CSV_COLUMNS.join(","));
    let notices = notice_codes().join(";");
    for row in &computed.rows {
        let Some(ledger) = row.ledger.filter(|_| row.refused.is_none()) else {
            continue;
        };
        // 値はどれも ASCII の英数字と `-` `.` `_` `;` だけなので、引用符は要らない（従業員IDの先頭は英数字）。
        let cells = [
            row.employee.to_string(),
            ledger.period.to_string(),
            ledger.work_days.to_string(),
            ledger.work_minutes.to_string(),
            ledger.overtime_minutes.to_string(),
            ledger.over60_minutes.to_string(),
            ledger.holiday_minutes.to_string(),
            ledger.night_minutes.to_string(),
            ledger.within_minutes.to_string(),
            ledger.extra_within_minutes.to_string(),
            kintai_jp_labor::report::TABLE_CHECKED_ON.to_owned(),
            REVIEW_STATUS.code().to_owned(),
            notices.clone(),
        ];
        line(&mut out, &cells.join(","));
    }
    out
}

fn ledger_cells(ledger: &LedgerRow, rounded: bool) -> Vec<String> {
    let mut cells = vec![
        ledger.work_days.to_string(),
        ledger.work_minutes.to_string(),
        ledger.overtime_minutes.to_string(),
        ledger.over60_minutes.to_string(),
        ledger.holiday_minutes.to_string(),
        ledger.night_minutes.to_string(),
        ledger.within_minutes.to_string(),
        ledger.extra_within_minutes.to_string(),
    ];
    if rounded {
        cells.push(ledger.rounded.map_or_else(String::new, |hours| {
            format!("{}・{}・{}", hours.overtime, hours.holiday, hours.night)
        }));
    }
    cells
}

fn text(computed: &Computed<'_>) -> String {
    let mut out = String::new();
    line(&mut out, "月次の集計（kintai report）");
    line(
        &mut out,
        &format!(
            "対象の月: {}（{}〜{}）",
            computed.month, computed.period_start, computed.period_end
        ),
    );
    let data_from = computed.data_from.map_or_else(
        || "なし（打刻がありません）".to_owned(),
        |date| date.to_string(),
    );
    line(
        &mut out,
        &format!("打刻がそろっている最初の日（data_from）: {data_from}"),
    );
    line(
        &mut out,
        &format!("丸め（rounding）: {}", computed.rounding.code()),
    );
    line(
        &mut out,
        "単位は分。延長は法定外の時間外で、法定休日の労働を含みません。深夜は、ほかの列と重なる独立の項目です。",
    );
    let rounded = computed.rounding == Rounding::Monthly30Min;
    let mut headers = vec![
        "従業員",
        "労働日数",
        "労働時間",
        "延長",
        "うち60時間超",
        "休日労働",
        "深夜",
        "法定内",
        "うち所定外法定内",
    ];
    let mut aligns = vec![Align::Left];
    aligns.extend([Align::Right; 8]);
    if rounded {
        headers.push("丸めた時間（時間。延長・休日・深夜）");
        aligns.push(Align::Right);
    }
    let table_rows: Vec<Vec<String>> = computed
        .rows
        .iter()
        .filter(|row| row.refused.is_none())
        .filter_map(|row| {
            row.ledger.map(|ledger| {
                let mut cells = vec![row.employee.to_string()];
                cells.extend(ledger_cells(ledger, rounded));
                cells
            })
        })
        .collect();
    out.push('\n');
    out.push_str(&render_table(&headers, &aligns, &table_rows));
    out.push('\n');
    sections(&mut out, computed);
    out.push_str(&footer(&[]));
    out
}

fn sections(out: &mut String, computed: &Computed<'_>) {
    line(out, "確定集計を拒否した従業員（その人の値は出しません）:");
    let refused: Vec<String> = computed
        .rows
        .iter()
        .filter_map(|row| {
            row.refused
                .as_ref()
                .map(|refused| format!("  {}: {}", row.employee, refusal_text(refused)))
        })
        .collect();
    list(out, &refused);
    line(
        out,
        "曖昧さの記録（ambiguity が use_default のとき。製品の既定の読みで計算した日）:",
    );
    let ambiguities: Vec<String> = computed
        .rows
        .iter()
        .flat_map(|row| {
            row.ambiguities
                .iter()
                .map(move |record| format!("  {} {} {}", row.employee, record.date, record.code))
        })
        .collect();
    list(out, &ambiguities);
    line(out, "警告（休憩の不足。休憩は自動で控除していません）:");
    let shortages: Vec<String> = computed
        .rows
        .iter()
        .flat_map(|row| {
            row.break_shortages.iter().map(move |shortage| {
                format!(
                    "  {} {}: 実労働{}分に対して休憩が{}分です（必要{}分、{}分不足。根拠: {}）",
                    row.employee,
                    shortage.date,
                    shortage.work_minutes,
                    shortage.break_minutes,
                    shortage.required_minutes,
                    shortage.shortage_minutes,
                    kintai_jp_labor::breaks::LSA_34_1.citation()
                )
            })
        })
        .collect();
    list(out, &shortages);
}
