//! `kintai explain`: その日の区間ごとの区分・割増の下限・根拠の条文・表の確認日（`docs/spec/cli.md`。AC-12.5）。
//!
//! 区分は、その従業員の打刻CSV全体の勤務を区分した結果から取る（週と月の累計のため。`data_from` は
//! `--data-from`、省略時は打刻CSV全体の最初の日付。`report` と同じ）。区間の割り当ては `kintai_jp_labor::explain` のとおりで、所定外法定内は
//! 日の法定内の分のうち時刻の後ろの分。国民の祝日は説明にだけ使い、法定休日の判定には使わない
//! （`docs/spec/holidays.md`）。出力に祝日の表の出典を付ける。

use std::path::PathBuf;

use kintai_core::punch::EmployeeId;
use kintai_core::{Date, Instant, Weekday};
use kintai_jp_labor::explain::{Category, DayExplanation, Segment, explain_day};
use kintai_jp_labor::holidays::{HolidayLookup, SOURCE_NOTE, holiday_on};
use kintai_jp_labor::report::TABLE_CHECKED_ON;
use kintai_jp_labor::{DayResult, Provenance, ShiftSet, ValidPolicy};
use serde::Serialize;

use crate::args::{self, Args, Format, required};
use crate::case::EXIT_REFUSED;
use crate::case36::json_line;
use crate::common_out::{RefusedOut, refusal_text, refused_ambiguous, refused_anomalies};
use crate::failure::Failure;
use crate::inputs::{Punches, Work};
use crate::policy_file::PolicyFile;
use crate::text::{
    Align, CommandOutput, EXIT_OK, REVIEW_STATUS, footer, line, list, notice_codes, render_table,
};

/// 説明の出力の `schema`。
pub const EXPLAIN_SCHEMA: &str = "kintai.explain.v1";

/// `kintai explain` の引数。
#[derive(Clone, Debug)]
pub struct ExplainArgs {
    /// 方針ファイル。
    pub policy: PathBuf,
    /// 打刻CSV。
    pub punches: PathBuf,
    /// 従業員。
    pub employee: EmployeeId,
    /// 労働日。
    pub date: Date,
    /// 打刻がそろっている最初の日（省略時は打刻CSV全体の最初の日付）。
    pub data_from: Option<Date>,
    /// 出力の形。
    pub format: Format,
}

impl ExplainArgs {
    /// 引数を読む。
    ///
    /// # Errors
    ///
    /// 必須の引数がない、値の形が違う、余分な引数があるときは不正な入力。
    pub fn parse(mut raw: Args) -> Result<ExplainArgs, Failure> {
        let policy = raw.path("--policy")?;
        let punches = raw.path("--punches")?;
        let employee = raw.text("--employee")?;
        let date = raw.text("--date")?;
        let data_from = raw.text("--data-from")?;
        let format = raw.text("--format")?;
        raw.finish()?;
        Ok(ExplainArgs {
            policy: required(policy, "--policy")?,
            punches: required(punches, "--punches")?,
            employee: args::employee(&required(employee, "--employee")?, "--employee")?,
            date: args::date(&required(date, "--date")?, "--date")?,
            data_from: data_from
                .map(|text| args::date(&text, "--data-from"))
                .transpose()?,
            format: args::format(format.as_deref(), &[Format::Text, Format::Json])?,
        })
    }
}

#[derive(Serialize)]
struct ExplainOut<'a> {
    schema: &'static str,
    employee_id: &'a str,
    date: String,
    weekday: &'static str,
    legal_holiday: bool,
    national_holiday: Option<NationalHolidayOut>,
    holiday_table: HolidayTableOut,
    day: Option<DayOut>,
    segments: Vec<SegmentOut>,
    notes: Vec<NoteOut>,
    refused: Option<&'a RefusedOut>,
    notices: Vec<&'static str>,
    table_checked_on: &'static str,
    review_status: &'static str,
}

#[derive(Serialize)]
struct NationalHolidayOut {
    name: &'static str,
    kind: &'static str,
}

#[derive(Serialize)]
struct HolidayTableOut {
    status: &'static str,
    notices: Vec<&'static str>,
    source: &'static str,
}

/// その日の区分の合計（`kintai.classify.v1` の `days` の1件と同じ項目）。
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
struct SegmentOut {
    start: String,
    end: String,
    minutes: i64,
    category: &'static str,
    night: bool,
    premium: PremiumOut,
}

#[derive(Serialize)]
struct PremiumOut {
    percent: Option<u32>,
    basis: Vec<BasisOut>,
}

/// 根拠の条文（法令名・条項・e-Gov の版ID と URL・確認日）。
#[derive(Serialize)]
struct BasisOut {
    law: &'static str,
    article: &'static str,
    revision_id: &'static str,
    url: &'static str,
    confirmed_on: &'static str,
}

#[derive(Serialize)]
struct NoteOut {
    code: &'static str,
    message: &'static str,
}

impl From<&DayResult> for DayOut {
    fn from(day: &DayResult) -> DayOut {
        DayOut {
            date: day.date.to_string(),
            total: day.total,
            within: day.within,
            extra_within: day.extra_within,
            overtime: day.overtime,
            over60: day.over60,
            holiday: day.holiday,
            night: day.night,
            night_within: day.night_within,
            night_overtime: day.night_overtime,
            night_over60: day.night_over60,
            night_holiday: day.night_holiday,
        }
    }
}

impl From<&Provenance> for BasisOut {
    fn from(provenance: &Provenance) -> BasisOut {
        BasisOut {
            law: provenance.law.name,
            article: provenance.article,
            revision_id: provenance.law.revision_id,
            url: provenance.law.url,
            confirmed_on: provenance.confirmed_on,
        }
    }
}

impl From<&Segment> for SegmentOut {
    fn from(segment: &Segment) -> SegmentOut {
        SegmentOut {
            start: segment.interval.start().to_string(),
            end: segment.interval.end().to_string(),
            minutes: segment.minutes(),
            category: segment.category.code(),
            night: segment.night,
            premium: PremiumOut {
                percent: segment.premium.percent,
                basis: segment.premium.basis.iter().map(BasisOut::from).collect(),
            },
        }
    }
}

/// 曜日の値（方針ファイルと同じ `sun`〜`sat`）。
const fn weekday_code(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "mon",
        Weekday::Tuesday => "tue",
        Weekday::Wednesday => "wed",
        Weekday::Thursday => "thu",
        Weekday::Friday => "fri",
        Weekday::Saturday => "sat",
        Weekday::Sunday => "sun",
    }
}

/// 曜日の漢字1文字。
const fn weekday_label(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "月",
        Weekday::Tuesday => "火",
        Weekday::Wednesday => "水",
        Weekday::Thursday => "木",
        Weekday::Friday => "金",
        Weekday::Saturday => "土",
        Weekday::Sunday => "日",
    }
}

/// 説明の材料。
struct Explained<'a> {
    employee: &'a EmployeeId,
    date: Date,
    legal_holiday_weekday: Weekday,
    holiday: HolidayLookup,
    day: Option<DayResult>,
    explanation: Option<DayExplanation>,
    refused: Option<RefusedOut>,
    data_from: Option<Date>,
    /// 前日に始まった勤務の続きが、この暦日にあるか（その分は前日の労働日に数える）。
    continued_from_previous_day: bool,
}

/// `kintai explain` の本体。
///
/// # Errors
///
/// 不正な入力（終了コード2）か対応外（終了コード3）。拒否（曖昧さか打刻の異常）は、出力に `refused` を付けて
/// 終了コードを4にする。
pub fn run(args: &ExplainArgs) -> Result<CommandOutput, Failure> {
    let policy = PolicyFile::load(&args.policy)?;
    let scope = policy.scope(Some(&args.employee))?;
    let valid: ValidPolicy = policy.check_unsupported(&scope)?;
    let punches = Punches::load(&args.punches, &policy)?;
    policy.dates()?;
    let mut explained = Explained {
        employee: &args.employee,
        date: args.date,
        legal_holiday_weekday: valid.legal_holiday_weekday(),
        holiday: holiday_on(args.date),
        day: None,
        explanation: None,
        refused: None,
        data_from: None,
        continued_from_previous_day: false,
    };
    let data_from = args.data_from.or(punches.first_date());
    explained.data_from = data_from;
    match punches.work(&valid, &args.employee, data_from)? {
        Work::Anomalies(anomalies) => explained.refused = Some(refused_anomalies(&anomalies)),
        Work::Classified {
            shifts,
            classification,
        } => {
            if let Some(refusal) = &classification.refused {
                explained.refused = Some(refused_ambiguous(refusal));
            } else {
                explained.continued_from_previous_day = continues_into(&shifts, args.date);
                explained.day = classification
                    .days
                    .iter()
                    .find(|day| day.date == args.date)
                    .copied();
                explained.explanation = Some(
                    explain_day(&valid, &shifts, &classification, args.date)
                        .map_err(|error| Failure::invalid(error.to_string()))?,
                );
            }
        }
    }
    let exit = if explained.refused.is_some() {
        EXIT_REFUSED
    } else {
        EXIT_OK
    };
    let stdout = match args.format {
        Format::Json => json(&explained)?,
        Format::Text | Format::Csv => text(&explained),
    };
    Ok(CommandOutput { stdout, exit })
}

fn json(explained: &Explained<'_>) -> Result<String, Failure> {
    let weekday = explained.date.weekday();
    let out = ExplainOut {
        schema: EXPLAIN_SCHEMA,
        employee_id: explained.employee.as_str(),
        date: explained.date.to_string(),
        weekday: weekday_code(weekday),
        legal_holiday: weekday == explained.legal_holiday_weekday,
        national_holiday: explained.holiday.holiday.map(|holiday| NationalHolidayOut {
            name: holiday.name,
            kind: holiday.kind.code(),
        }),
        holiday_table: HolidayTableOut {
            status: explained.holiday.status.code(),
            notices: explained
                .holiday
                .notices
                .iter()
                .map(|notice| notice.code())
                .collect(),
            source: SOURCE_NOTE,
        },
        day: explained.day.as_ref().map(DayOut::from),
        segments: explained
            .explanation
            .iter()
            .flat_map(|explanation| explanation.segments.iter().map(SegmentOut::from))
            .collect(),
        notes: explained
            .explanation
            .iter()
            .flat_map(|explanation| {
                explanation.notes.iter().map(|note| NoteOut {
                    code: note.code(),
                    message: note.message(),
                })
            })
            .collect(),
        refused: explained.refused.as_ref(),
        notices: notice_codes(),
        table_checked_on: TABLE_CHECKED_ON,
        review_status: REVIEW_STATUS.code(),
    };
    json_line(&out)
}

/// 前の日に始まった勤務が、暦日 `date` の 0:00 より後まで続くか（L2-13）。勤務は1440分未満なので、
/// 続いて来るのは前日に始まった勤務だけ。
fn continues_into(shifts: &ShiftSet, date: Date) -> bool {
    let Ok(midnight) = Instant::new(date, 0) else {
        return false;
    };
    shifts
        .shifts()
        .iter()
        .any(|shift| shift.labor_day() < date && shift.span().end() > midnight)
}

/// 時刻の表示。労働日の翌日以降の時刻は `翌` を付ける。
fn clock(instant: Instant, labor_day: Date) -> String {
    let prefix = if instant.date() > labor_day {
        "翌"
    } else {
        ""
    };
    format!("{prefix}{:02}:{:02}", instant.hour(), instant.minute())
}

fn category_label(category: Category, night: bool) -> String {
    if night {
        format!("{}・深夜", category.label())
    } else {
        category.label().to_owned()
    }
}

fn segment_rows(explanation: &DayExplanation) -> Vec<Vec<String>> {
    explanation
        .segments
        .iter()
        .map(|segment| {
            let percent = segment
                .premium
                .percent
                .map_or_else(|| "示さない".to_owned(), |percent| format!("{percent}%"));
            let basis: Vec<String> = segment
                .premium
                .basis
                .iter()
                .map(Provenance::citation)
                .collect();
            vec![
                format!(
                    "{}〜{}",
                    clock(segment.interval.start(), explanation.date),
                    clock(segment.interval.end(), explanation.date)
                ),
                format!("{}分", segment.minutes()),
                category_label(segment.category, segment.night),
                percent,
                basis.join("、"),
            ]
        })
        .collect()
}

fn holiday_text(explained: &Explained<'_>) -> String {
    let status = explained.holiday.status.code();
    match explained.holiday.holiday {
        Some(holiday) => format!(
            "国民の祝日: {}（{}。祝日の表: {status}）。祝日は法定休日ではありません（法定休日は方針の曜日で決めます）",
            holiday.name,
            holiday.kind.code()
        ),
        None => format!("国民の祝日: なし（祝日の表: {status}）"),
    }
}

fn text(explained: &Explained<'_>) -> String {
    let mut out = String::new();
    let weekday = explained.date.weekday();
    line(&mut out, "区間ごとの区分と割増の下限（kintai explain）");
    line(
        &mut out,
        &format!(
            "従業員: {}  労働日: {}（{}）",
            explained.employee,
            explained.date,
            weekday_label(weekday)
        ),
    );
    let legal = if weekday == explained.legal_holiday_weekday {
        "この日は法定休日（方針の曜日）です"
    } else {
        "この日は法定休日ではありません"
    };
    line(
        &mut out,
        &format!(
            "法定休日: 方針では{}曜日。{legal}",
            weekday_label(explained.legal_holiday_weekday)
        ),
    );
    line(&mut out, &holiday_text(explained));
    let data_from = explained.data_from.map_or_else(
        || "なし（打刻がありません）".to_owned(),
        |date| date.to_string(),
    );
    line(
        &mut out,
        &format!("打刻がそろっている最初の日（data_from）: {data_from}"),
    );
    if let Some(refused) = &explained.refused {
        line(
            &mut out,
            &format!("{}: {}", explained.employee, refusal_text(refused)),
        );
    }
    if let Some(day) = &explained.day {
        line(
            &mut out,
            &format!(
                "区分の合計（分）: 実労働 {} ＝ 法定内 {}（うち所定外法定内 {}）＋ 時間外 {}（うち60時間超 {}）＋ 法定休日 {}。深夜 {}",
                day.total,
                day.within,
                day.extra_within,
                day.overtime,
                day.over60,
                day.holiday,
                day.night
            ),
        );
    }
    if let Some(explanation) = &explained.explanation {
        if explanation.segments.is_empty() && explained.continued_from_previous_day {
            line(
                &mut out,
                "区間: この日に始まった勤務はありません（前日に始まった勤務の続きは、前日の労働日に数えています）",
            );
        } else if explanation.segments.is_empty() {
            line(&mut out, "区間: この日の実労働はありません");
        } else {
            line(&mut out, "区間（割増の下限は、賃金の上乗せの率の最低限）:");
            out.push_str(&render_table(
                &["時刻", "長さ", "区分", "割増の下限", "根拠の条文"],
                &[
                    Align::Left,
                    Align::Right,
                    Align::Left,
                    Align::Right,
                    Align::Left,
                ],
                &segment_rows(explanation),
            ));
        }
        line(&mut out, "注:");
        let notes: Vec<String> = explanation
            .notes
            .iter()
            .map(|note| format!("  - {}: {}", note.code(), note.message()))
            .collect();
        list(&mut out, &notes);
    }
    line(&mut out, &format!("祝日の表の{SOURCE_NOTE}"));
    out.push_str(&footer(&[]));
    out
}

#[cfg(test)]
mod tests {
    use super::{clock, weekday_code};
    use kintai_core::Weekday;

    #[test]
    fn clock_marks_the_next_day() {
        let day = "2026-10-08".parse().unwrap();
        assert_eq!(clock("2026-10-08T09:05".parse().unwrap(), day), "09:05");
        assert_eq!(clock("2026-10-09T00:00".parse().unwrap(), day), "翌00:00");
        assert_eq!(weekday_code(Weekday::Sunday), "sun");
    }
}
