//! `kintai check`: 36協定の判定、休憩の不足、打刻の異常（`docs/spec/cli.md`。AC-12.3）。
//!
//! 協定月は、打刻がそろっている最初の日（`--data-from`。既定は打刻CSV全体の最初の日付）以後で最初に始まる
//! 協定月から、対象の月まで（実労働のない月は O も H も0）。それより前は `--history` の T。
//! `--data-from` は `report` と同じ意味で、区分の `IncompleteWeek` の判定にも使う。協定月の起算日は `workplace.month_start_day`（協定の種類が `none` でなければ
//! `agreement36.month_start_day` と同じ。違えば対応外で、ここまで来ない）。
//!
//! 違反と警告は、判定した協定月のすべての判定（`UnknownHistory` を含む）、判定する期間の休憩の不足、打刻の異常。
//! 終了コードは、`--fail-on-violation` があり違反か警告があれば1、そうでなく拒否（打刻の異常か曖昧さ）があれば4、
//! どちらもなければ0。出力は人が読む形だけ（`docs/spec/cli.md` の表に `--format` がない）。

use std::path::PathBuf;

use kintai_core::punch::EmployeeId;
use kintai_core::{Date, MonthPeriod, MonthStartDay};
use kintai_jp_labor::breaks::{BreakShortage, break_shortages};
use kintai_jp_labor::limits36::{
    AgreementKind, Finding, HistoryMonth, Limits36Error, Limits36Result, MonthHours,
    MonthJudgement, ValidAgreement, judge, months_from_periods,
};
use kintai_jp_labor::{Ambiguity, RecordDate, ValidPolicy};

use crate::args::{self, Args, required};
use crate::case::EXIT_REFUSED;
use crate::common_out::{RefusedOut, refusal_text, refused_ambiguous, refused_anomalies};
use crate::failure::Failure;
use crate::inputs::{History, Punches, Work, YearMonth, period_last_day};
use crate::policy_file::{Employee, PolicyFile};
use crate::text::{
    Align, CommandOutput, EXIT_OK, EXIT_VIOLATION, footer, line, list, render_table,
};

/// `kintai check` の引数。
#[derive(Clone, Debug)]
pub struct CheckArgs {
    /// 方針ファイル。
    pub policy: PathBuf,
    /// 打刻CSV。
    pub punches: PathBuf,
    /// 対象の月。
    pub month: YearMonth,
    /// 打刻がそろっている最初の日（省略時は打刻CSV全体の最初の日付）。
    pub data_from: Option<Date>,
    /// 36協定の履歴CSV。
    pub history: Option<PathBuf>,
    /// 違反と警告があれば終了コード1にする。
    pub fail_on_violation: bool,
}

impl CheckArgs {
    /// 引数を読む。
    ///
    /// # Errors
    ///
    /// 必須の引数がない、値の形が違う、余分な引数があるときは不正な入力。
    pub fn parse(mut raw: Args) -> Result<CheckArgs, Failure> {
        let policy = raw.path("--policy")?;
        let punches = raw.path("--punches")?;
        let month = raw.text("--month")?;
        let data_from = raw.text("--data-from")?;
        let history = raw.path("--history")?;
        let fail_on_violation = raw.flag("--fail-on-violation");
        raw.finish()?;
        Ok(CheckArgs {
            policy: required(policy, "--policy")?,
            punches: required(punches, "--punches")?,
            month: args::month(&required(month, "--month")?, "--month")?,
            data_from: data_from
                .map(|text| args::date(&text, "--data-from"))
                .transpose()?,
            history,
            fail_on_violation,
        })
    }
}

/// 1人分の判定。
struct EmployeeCheck {
    id: EmployeeId,
    /// 判定に渡した協定月（O と H）。
    months: Vec<MonthHours>,
    judgement: Option<Limits36Result>,
    refused: Option<RefusedOut>,
    break_shortages: Vec<BreakShortage>,
    ambiguities: Vec<Ambiguity>,
}

impl EmployeeCheck {
    fn findings(&self) -> usize {
        self.judgement.as_ref().map_or(0, |result| {
            result.months.iter().map(|month| month.findings.len()).sum()
        })
    }

    fn anomalies(&self) -> usize {
        self.refused
            .as_ref()
            .map_or(0, |refused| refused.anomalies.len())
    }
}

/// 判定する期間（協定月の最初から、対象の月の終わりまで）。
#[derive(Clone, Copy)]
struct Range {
    first: MonthPeriod,
    last: MonthPeriod,
    from: Date,
    until: Date,
}

impl Range {
    fn contains(self, date: Date) -> bool {
        self.from <= date && date < self.until
    }
}

/// 打刻がそろっている最初の日以後で、最初に始まる協定月。
fn first_agreement_month(
    first_date: Date,
    start_day: MonthStartDay,
) -> Result<MonthPeriod, Failure> {
    let period = first_date.month_period(start_day);
    if period.first_day_number() == first_date.day_number() {
        return Ok(period);
    }
    period
        .next_first_day()
        .map(|next| next.month_period(start_day))
        .map_err(|_| Failure::invalid("協定月の初日が、日付の範囲の外です"))
}

/// 判定する期間。打刻が1件もなければ判定できない（`--data-from` があっても、不正な入力）。
fn range(
    punches: &Punches,
    data_from: Option<Date>,
    month: YearMonth,
    start_day: MonthStartDay,
) -> Result<Range, Failure> {
    let first_date = punches.first_date().ok_or_else(|| {
        Failure::invalid("打刻CSVに打刻が1件もないため、36協定を判定する協定月を決められません")
    })?;
    let first = first_agreement_month(data_from.unwrap_or(first_date), start_day)?;
    let last = month.period(start_day);
    if first > last {
        return Err(Failure::invalid(
            "対象の月の期間が、打刻がそろっている最初の日（--data-from。既定は打刻CSVの最初の日付）より前に始まります（その月の打刻がそろっていないため、36協定を判定できません）",
        ));
    }
    let bounds = first
        .first_day()
        .and_then(|from| Ok((from, last.next_first_day()?)));
    let (from, until) =
        bounds.map_err(|_| Failure::invalid("協定月の初日が、日付の範囲の外です"))?;
    Ok(Range {
        first,
        last,
        from,
        until,
    })
}

/// 履歴の誤りを、履歴CSVの行番号つきの不正な入力にする。
fn history_error(error: Limits36Error, rows: &[(u64, HistoryMonth)]) -> Failure {
    let at = |index: usize| {
        rows.get(index)
            .map_or_else(String::new, |(line, _)| format!("{line}行目の"))
    };
    match error {
        Limits36Error::HistoryNotAgreementMonth { index } => Failure::invalid(format!(
            "履歴CSVの{}列 month_start: 協定月の初日ではありません（日は月の起算日）",
            at(index)
        )),
        Limits36Error::HistoryNotBefore { index } => Failure::invalid(format!(
            "履歴CSVの{}列 month_start: 打刻から判定する最初の協定月以後の月です（履歴は、それより前の月だけ）",
            at(index)
        )),
        Limits36Error::DuplicateHistory { index } => Failure::invalid(format!(
            "履歴CSVの{}列 month_start: 同じ従業員の同じ月が2回あります",
            at(index)
        )),
        other => Failure::invalid(other.to_string()),
    }
}

fn check_employee(
    agreement: &ValidAgreement,
    history: &History,
    employee: &Employee,
    work: Work,
    range: Range,
) -> Result<EmployeeCheck, Failure> {
    let id = employee.id.clone();
    let (shifts, classification) = match work {
        Work::Anomalies(anomalies) => {
            return Ok(EmployeeCheck {
                id,
                months: Vec::new(),
                judgement: None,
                refused: Some(refused_anomalies(&anomalies)),
                break_shortages: Vec::new(),
                ambiguities: Vec::new(),
            });
        }
        Work::Classified {
            shifts,
            classification,
        } => (shifts, classification),
    };
    let break_shortages = break_shortages(&shifts)
        .map_err(|error| Failure::invalid(error.to_string()))?
        .into_iter()
        .filter(|shortage| range.contains(shortage.date))
        .collect();
    if let Some(refusal) = &classification.refused {
        return Ok(EmployeeCheck {
            id,
            months: Vec::new(),
            judgement: None,
            refused: Some(refused_ambiguous(refusal)),
            break_shortages,
            ambiguities: Vec::new(),
        });
    }
    let months = months_from_periods(&classification.periods, range.first, range.last)
        .map_err(|error| Failure::invalid(error.to_string()))?;
    let rows = history.of(&employee.id);
    let entries: Vec<HistoryMonth> = rows.iter().map(|(_, entry)| *entry).collect();
    let judgement =
        judge(agreement, &months, &entries).map_err(|error| history_error(error, rows))?;
    let ambiguities = classification
        .ambiguities
        .iter()
        .filter(|record| match record.date {
            RecordDate::LaborDay(date) => range.contains(date),
            // 週の記録は、週の7日のどれかが判定する期間に入れば出す（月次の集計と同じ）。
            RecordDate::WeekStart(week) => (0..7).any(|offset| {
                Date::from_day_number(week.first_day_number().saturating_add(offset))
                    .is_ok_and(|date| range.contains(date))
            }),
        })
        .copied()
        .collect();
    Ok(EmployeeCheck {
        id,
        months,
        judgement: Some(judgement),
        refused: None,
        break_shortages,
        ambiguities,
    })
}

/// `kintai check` の本体。
///
/// # Errors
///
/// 不正な入力（終了コード2）か対応外（終了コード3）。
pub fn run(args: &CheckArgs) -> Result<CommandOutput, Failure> {
    let policy = PolicyFile::load(&args.policy)?;
    let scope = policy.scope(None)?;
    let valid: ValidPolicy = policy.check_unsupported(&scope)?;
    let punches = Punches::load(&args.punches, &policy)?;
    let history = match &args.history {
        Some(path) => History::load(path, &policy)?,
        None => History::default(),
    };
    policy.dates()?;
    let data_from = args.data_from.or(punches.first_date());
    let range = range(&punches, data_from, args.month, valid.month_start_day())?;
    let mut checks = Vec::with_capacity(scope.len());
    for employee in &scope {
        let work = punches.work(&valid, &employee.id, data_from)?;
        checks.push(check_employee(
            policy.agreement(),
            &history,
            employee,
            work,
            range,
        )?);
    }
    let findings: usize = checks.iter().map(EmployeeCheck::findings).sum();
    let shortages: usize = checks.iter().map(|check| check.break_shortages.len()).sum();
    let anomalies: usize = checks.iter().map(EmployeeCheck::anomalies).sum();
    let refused = checks
        .iter()
        .filter(|check| check.refused.is_some())
        .count();
    let violations = findings + shortages + anomalies;
    let exit = if args.fail_on_violation && violations > 0 {
        EXIT_VIOLATION
    } else if refused > 0 {
        EXIT_REFUSED
    } else {
        EXIT_OK
    };
    let mut stdout = header(args.month, data_from, range, policy.agreement());
    for check in &checks {
        employee_text(&mut stdout, check);
    }
    stdout.push('\n');
    trailing_sections(&mut stdout, &checks, range);
    line(
        &mut stdout,
        &format!(
            "違反と警告: {violations}件（36協定の判定 {findings}件、休憩の不足 {shortages}件、打刻の異常 {anomalies}件）。確定集計を拒否した従業員: {refused}人"
        ),
    );
    line(
        &mut stdout,
        "（--fail-on-violation を付けると、違反か警告があるとき終了コード1で終わります）",
    );
    stdout.push_str(&footer(&[(
        "OwnEmployerOnly",
        "入力された自社の時間（履歴を含む）だけを判定しています。月100時間未満と2〜6か月平均80時間は、副業・兼業先や転職前の時間も通算して適用されます（労働基準法第38条第1項）",
    )]));
    Ok(CommandOutput { stdout, exit })
}

fn agreement_text(agreement: &ValidAgreement) -> String {
    let start = agreement.year_start_month();
    match agreement.kind() {
        AgreementKind::None => format!("none（協定なし。起算月 {start}月）"),
        AgreementKind::General => format!(
            "general（一般条項。起算月 {start}月。月の限度 {}分、年の限度 {}分）",
            agreement.monthly_limit_minutes().unwrap_or(0),
            agreement.annual_limit_minutes().unwrap_or(0)
        ),
        AgreementKind::Special => format!(
            "special（特別条項つき。起算月 {start}月。月の限度 {}分、年の限度 {}分。特別条項: 月の上限 {}分、年の上限 {}分、{}回）",
            agreement.monthly_limit_minutes().unwrap_or(0),
            agreement.annual_limit_minutes().unwrap_or(0),
            agreement.special_monthly_total_limit_minutes().unwrap_or(0),
            agreement.special_annual_limit_minutes().unwrap_or(0),
            agreement.special_months_max().unwrap_or(0)
        ),
    }
}

fn header(
    month: YearMonth,
    data_from: Option<Date>,
    range: Range,
    agreement: &ValidAgreement,
) -> String {
    let mut out = String::new();
    line(
        &mut out,
        "36協定の判定・休憩の不足・打刻の異常（kintai check）",
    );
    let last_day = period_last_day(range.last).map_or_else(String::new, |day| day.to_string());
    line(
        &mut out,
        &format!("対象の月: {month}（{}〜{last_day}）", range.last),
    );
    line(
        &mut out,
        &format!(
            "判定する協定月: {} から {} まで（打刻がそろっている最初の日以後に始まる協定月から。それより前の月は履歴CSVの T）",
            range.first, range.last
        ),
    );
    let data_from = data_from.map_or_else(String::new, |date| date.to_string());
    line(
        &mut out,
        &format!("打刻がそろっている最初の日（data_from）: {data_from}"),
    );
    line(&mut out, &format!("協定: {}", agreement_text(agreement)));
    line(
        &mut out,
        "単位は分。O＝時間外（法定休日の労働を含まない）、H＝法定休日の労働、T＝O＋H。直近nか月の合計は 4800×n 分（平均80時間）と比べます。",
    );
    out
}

/// 判定の説明（人が読む出力の1行）。
fn finding_text(finding: Finding) -> String {
    let what = match finding {
        Finding::NoAgreement => "36協定がないのに、時間外か休日労働があります".to_owned(),
        Finding::MonthlyOvertimeExceeded => {
            "一般条項だけの協定で、時間外が月45時間（2700分）を超えています".to_owned()
        }
        Finding::GeneralMonthlyLimitExceeded => "時間外が、協定の月の限度を超えています".to_owned(),
        Finding::AnnualOvertimeExceeded => {
            "協定年の時間外の累計が、360時間（21600分）を超えています".to_owned()
        }
        Finding::GeneralAnnualLimitExceeded => {
            "協定年の時間外の累計が、協定の年の限度を超えています".to_owned()
        }
        Finding::SpecialCountExceeded => {
            "限度時間を超えた月の数が、特別条項の回数を超えています".to_owned()
        }
        Finding::SpecialMonthlyTotalExceeded => {
            "限度時間を超えた月の時間外と休日労働の合計が、特別条項の月の上限を超えています"
                .to_owned()
        }
        Finding::SpecialAnnualExceeded => {
            "協定年の時間外の累計が、特別条項の年の上限を超えています".to_owned()
        }
        Finding::MonthlyTotalReached100h => {
            "時間外と休日労働の合計が、月100時間（6000分）に達しています（100時間未満であること）"
                .to_owned()
        }
        Finding::MultiMonthSumExceeded { n } => format!(
            "直近{n}か月の時間外と休日労働の合計が、{}分（平均80時間）を超えています",
            4800 * i64::from(n)
        ),
        Finding::UnknownHistory { n } => format!(
            "直近{n}か月に、打刻も履歴もない月があり、判定できません（0分とはみなしません。--history で前の月の T を渡します）"
        ),
    };
    let basis: Vec<String> = finding
        .provenances()
        .iter()
        .map(kintai_jp_labor::Provenance::citation)
        .collect();
    let code = match finding.n() {
        Some(n) => format!("{}(n={n})", finding.code()),
        None => finding.code().to_owned(),
    };
    format!("{code}: {what}（根拠: {}）", basis.join("、"))
}

fn month_row(hours: Option<&MonthHours>, month: &MonthJudgement) -> Vec<String> {
    let lower = if month.year_overtime_known {
        ""
    } else {
        "（下限）"
    };
    let sums = month
        .sums
        .iter()
        .map(|sum| sum.map_or_else(|| "不明".to_owned(), |value| value.to_string()));
    let codes: Vec<String> = month
        .findings
        .iter()
        .map(|finding| match finding.n() {
            Some(n) => format!("{}(n={n})", finding.code()),
            None => finding.code().to_owned(),
        })
        .collect();
    let (overtime, holiday) = hours.map_or_else(
        || (String::new(), String::new()),
        |hours| (hours.overtime.to_string(), hours.holiday.to_string()),
    );
    let mut cells = vec![
        month.start.to_string(),
        overtime,
        holiday,
        month.total.to_string(),
    ];
    cells.extend(sums);
    cells.push(format!("{}{lower}", month.special_count));
    cells.push(format!("{}{lower}", month.year_overtime));
    cells.push(if codes.is_empty() {
        "なし".to_owned()
    } else {
        codes.join(" ")
    });
    cells
}

fn employee_text(out: &mut String, check: &EmployeeCheck) {
    out.push('\n');
    if let Some(refused) = &check.refused {
        line(out, &format!("{}: {}", check.id, refusal_text(refused)));
        return;
    }
    line(out, &check.id.to_string());
    let Some(result) = &check.judgement else {
        return;
    };
    let headers = [
        "協定月",
        "O",
        "H",
        "T",
        "2か月",
        "3か月",
        "4か月",
        "5か月",
        "6か月",
        "発動月の数",
        "年の時間外",
        "判定",
    ];
    let mut aligns = vec![Align::Left];
    aligns.extend([Align::Right; 10]);
    aligns.push(Align::Left);
    let rows: Vec<Vec<String>> = result
        .months
        .iter()
        .enumerate()
        .map(|(index, month)| month_row(check.months.get(index), month))
        .collect();
    out.push_str(&render_table(&headers, &aligns, &rows));
    for month in &result.months {
        for finding in &month.findings {
            line(
                out,
                &format!("    {} {}", month.start, finding_text(*finding)),
            );
        }
    }
    if result.months.iter().any(|month| !month.year_overtime_known) {
        line(
            out,
            "    （下限）: 協定年の最初の月から打刻がないため、年の時間外と発動月の数は下限です。年と回数は、確実に超えたものだけを判定します（出ないことは適合を意味しません）。",
        );
    }
}

fn trailing_sections(out: &mut String, checks: &[EmployeeCheck], range: Range) {
    let until = range
        .until
        .checked_add_days(-1)
        .map_or_else(|_| String::new(), |day| day.to_string());
    line(
        out,
        &format!(
            "休憩の不足（{}〜{until}。休憩は自動で控除していません）:",
            range.from
        ),
    );
    let shortages: Vec<String> = checks
        .iter()
        .flat_map(|check| {
            check
                .break_shortages
                .iter()
                .map(move |shortage| format!("  {} {shortage}", check.id))
        })
        .collect();
    list(out, &shortages);
    line(out, "打刻の異常（打刻CSVの全体で見ます）:");
    let anomalies: Vec<String> = checks
        .iter()
        .filter(|check| check.anomalies() > 0)
        .filter_map(|check| {
            check
                .refused
                .as_ref()
                .map(|refused| format!("  {}: {}", check.id, refusal_text(refused)))
        })
        .collect();
    list(out, &anomalies);
    line(out, "曖昧さの記録（ambiguity が use_default のとき）:");
    let ambiguities: Vec<String> = checks
        .iter()
        .flat_map(|check| {
            check
                .ambiguities
                .iter()
                .map(move |record| format!("  {} {} {}", check.id, record.date, record.code.code()))
        })
        .collect();
    list(out, &ambiguities);
}

#[cfg(test)]
mod tests {
    use super::{finding_text, first_agreement_month};
    use kintai_core::MonthStartDay;
    use kintai_jp_labor::limits36::Finding;

    /// 打刻CSVの最初の日付以後で、最初に始まる協定月（起算日1と16、年をまたぐ場合）。
    #[test]
    fn first_agreement_month_starts_on_or_after_the_first_date() {
        let date = |text: &str| text.parse().unwrap();
        let first = MonthStartDay::FIRST;
        let sixteen = MonthStartDay::new(16).unwrap();
        for (from, start_day, expected) in [
            ("2026-10-01", first, "2026-10-01"),
            ("2026-09-27", first, "2026-10-01"),
            ("2026-12-02", first, "2027-01-01"),
            ("2026-10-16", sixteen, "2026-10-16"),
            ("2026-10-15", sixteen, "2026-10-16"),
            ("2026-10-17", sixteen, "2026-11-16"),
            ("2026-12-17", sixteen, "2027-01-16"),
        ] {
            let month = first_agreement_month(date(from), start_day).unwrap();
            assert_eq!(month.to_string(), expected, "{from}");
        }
    }

    #[test]
    fn findings_are_explained_with_the_basis() {
        let text = finding_text(Finding::MultiMonthSumExceeded { n: 3 });
        assert!(text.starts_with("MultiMonthSumExceeded(n=3): "), "{text}");
        assert!(
            text.contains("14400分") && text.contains("労働基準法第36条第6項第3号"),
            "{text}"
        );
        let text = finding_text(Finding::NoAgreement);
        assert!(text.contains("労働基準法第36条第1項"), "{text}");
    }
}
