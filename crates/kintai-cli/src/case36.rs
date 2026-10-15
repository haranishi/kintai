//! `kintai.case36.v1`（36協定のケース）を読み、`kintai.limits36.v1` を出す
//! （`docs/spec/case-format-36-leave.md`）。
//!
//! 検査は `docs/spec/validation.md` の流儀で、最初に当たったものを返す。36協定のケースには対応外の
//! 指定がないので、誤りはどれも不正な入力（終了コード2）。
//! 1. 形と型: 知らない項目、必須の項目（トップレベルは `schema` `agreement` `months` `history`）、
//!    型（整数の欄は JSON の整数だけ。必須の項目の `null` は型の誤り、任意の項目の `null` は項目なし）
//! 2. 列挙と範囲: `kind`、協定の範囲と種類ごとの必須（[`Agreement::validate`]）、分の範囲
//! 3. 対応外: なし
//! 4. 日付の書式と整合: `start` の表記と入力の範囲（2000〜2099年）、協定月の並び、`history` の月
//! 5. 計算（[`judge`]）
//!
//! JSON の型を確かめる部品は、年休のケース（[`crate::case_leave`]）と共用する。

use kintai_core::Date;
use kintai_jp_labor::limits36::{
    Agreement, AgreementKind, HistoryMonth, Limits36Result, MonthHours, check_history_minutes,
    check_month_minutes, judge,
};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::case::{CaseOutput, EXIT_OK};
use crate::failure::Failure;

/// 36協定のケースの入力の `schema`。
pub const CASE36_SCHEMA: &str = "kintai.case36.v1";
/// 36協定の判定の結果の `schema`。
pub const LIMITS36_SCHEMA: &str = "kintai.limits36.v1";

const CASE_KEYS: [&str; 4] = ["schema", "agreement", "months", "history"];
const AGREEMENT_KEYS: [&str; 7] = [
    "kind",
    "year_start_month",
    "monthly_limit_minutes",
    "annual_limit_minutes",
    "special_monthly_total_limit_minutes",
    "special_annual_limit_minutes",
    "special_months_max",
];
const MONTH_KEYS: [&str; 3] = ["start", "overtime", "holiday"];
const HISTORY_KEYS: [&str; 2] = ["start", "total"];

// --- JSON の型を確かめる部品（年休のケースと共用。区分のケースの `dto` と同じ流儀） ---

/// オブジェクトであること。
pub(crate) fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, Failure> {
    value
        .as_object()
        .ok_or_else(|| Failure::invalid(format!("{path} はオブジェクトではありません")))
}

/// 配列であること。
pub(crate) fn array<'a>(value: &'a Value, path: &str) -> Result<&'a [Value], Failure> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| Failure::invalid(format!("{path} は配列ではありません")))
}

/// 知らない項目があれば不正な入力。
pub(crate) fn reject_unknown(
    map: &Map<String, Value>,
    known: &[&str],
    path: &str,
) -> Result<(), Failure> {
    if map.keys().any(|key| !known.contains(&key.as_str())) {
        return Err(Failure::invalid(format!("{path} に知らない項目があります")));
    }
    Ok(())
}

/// 必須の項目。ないときは不正な入力。`null` は型の誤りとして、読む側で不正な入力になる。
pub(crate) fn required<'a>(
    map: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a Value, Failure> {
    map.get(key)
        .ok_or_else(|| Failure::invalid(format!("{path}.{key} がありません")))
}

/// 任意の項目。ないとき、または `null` のときは `None`。
pub(crate) fn optional<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    map.get(key).filter(|value| !value.is_null())
}

/// 文字列であること。
pub(crate) fn string<'a>(value: &'a Value, path: &str) -> Result<&'a str, Failure> {
    value
        .as_str()
        .ok_or_else(|| Failure::invalid(format!("{path} は文字列ではありません")))
}

/// 整数であること（小数点も指数もない JSON の整数だけ。`-0` `1.0` `1e2` `true` は型の誤り）。
pub(crate) fn integer(value: &Value, path: &str) -> Result<i64, Failure> {
    if let Some(number) = value.as_i64() {
        return Ok(number);
    }
    if value.is_u64() {
        return Err(Failure::invalid(format!("{path} の値が範囲外です")));
    }
    Err(Failure::invalid(format!("{path} は整数ではありません")))
}

/// 入力の日付（`YYYY-MM-DD`、2000-01-01〜2099-12-31）を読む（手順4）。
pub(crate) fn date(text: &str, path: &str) -> Result<Date, Failure> {
    text.parse()
        .map_err(|error| Failure::invalid(format!("{path}: {error}")))
}

/// 結果を1行の JSON（末尾の改行つき）にする。
pub(crate) fn json_line<T: Serialize>(value: &T) -> Result<String, Failure> {
    let mut line =
        serde_json::to_string(value).map_err(|_| Failure::invalid("結果を JSON にできません"))?;
    line.push('\n');
    Ok(line)
}

// --- 36協定のケース ---

/// 型を確かめた協定月（日付は、まだ文字列のまま。読むのは手順4）。
struct RawMonth<'a> {
    start: &'a str,
    overtime: i64,
    holiday: i64,
}

/// 型を確かめた履歴の1件。
struct RawHistory<'a> {
    start: &'a str,
    total: i64,
}

/// 型を確かめた36協定のケース。
struct RawCase36<'a> {
    kind: &'a str,
    year_start_month: i64,
    limits: [Option<i64>; 5],
    months: Vec<RawMonth<'a>>,
    history: Vec<RawHistory<'a>>,
}

fn optional_integer(
    map: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<i64>, Failure> {
    optional(map, key)
        .map(|value| integer(value, &format!("{path}.{key}")))
        .transpose()
}

fn read_month(value: &Value, index: usize) -> Result<RawMonth<'_>, Failure> {
    let path = format!("months[{index}]");
    let map = object(value, &path)?;
    reject_unknown(map, &MONTH_KEYS, &path)?;
    Ok(RawMonth {
        start: string(required(map, "start", &path)?, &format!("{path}.start"))?,
        overtime: integer(
            required(map, "overtime", &path)?,
            &format!("{path}.overtime"),
        )?,
        holiday: integer(required(map, "holiday", &path)?, &format!("{path}.holiday"))?,
    })
}

fn read_history(value: &Value, index: usize) -> Result<RawHistory<'_>, Failure> {
    let path = format!("history[{index}]");
    let map = object(value, &path)?;
    reject_unknown(map, &HISTORY_KEYS, &path)?;
    Ok(RawHistory {
        start: string(required(map, "start", &path)?, &format!("{path}.start"))?,
        total: integer(required(map, "total", &path)?, &format!("{path}.total"))?,
    })
}

/// 手順1: JSON の形・型・知らない項目・必須の項目を確かめて読む。
fn read_case(document: &Value) -> Result<RawCase36<'_>, Failure> {
    let map = object(document, "入力")?;
    reject_unknown(map, &CASE_KEYS, "入力")?;
    if string(required(map, "schema", "入力")?, "schema")? != CASE36_SCHEMA {
        return Err(Failure::invalid(
            "schema が kintai.case36.v1 ではありません",
        ));
    }
    let agreement = object(required(map, "agreement", "入力")?, "agreement")?;
    reject_unknown(agreement, &AGREEMENT_KEYS, "agreement")?;
    let kind = string(required(agreement, "kind", "agreement")?, "agreement.kind")?;
    let year_start_month = integer(
        required(agreement, "year_start_month", "agreement")?,
        "agreement.year_start_month",
    )?;
    let mut limits = [None; 5];
    for (slot, key) in limits.iter_mut().zip(AGREEMENT_KEYS.iter().skip(2)) {
        *slot = optional_integer(agreement, key, "agreement")?;
    }
    let months = array(required(map, "months", "入力")?, "months")?
        .iter()
        .enumerate()
        .map(|(index, value)| read_month(value, index))
        .collect::<Result<_, _>>()?;
    let history = array(required(map, "history", "入力")?, "history")?
        .iter()
        .enumerate()
        .map(|(index, value)| read_history(value, index))
        .collect::<Result<_, _>>()?;
    Ok(RawCase36 {
        kind,
        year_start_month,
        limits,
        months,
        history,
    })
}

/// 協定の種類（`none` `general` `special`）。定義にない値は `None`（方針ファイルの読み込みと共用する）。
pub(crate) fn agreement_kind_from(text: &str) -> Option<AgreementKind> {
    match text {
        "none" => Some(AgreementKind::None),
        "general" => Some(AgreementKind::General),
        "special" => Some(AgreementKind::Special),
        _ => None,
    }
}

fn agreement_kind(text: &str) -> Result<AgreementKind, Failure> {
    agreement_kind_from(text)
        .ok_or_else(|| Failure::invalid("agreement.kind の値が定義にありません"))
}

/// 出力 `kintai.limits36.v1`。項目の並びは仕様の例のとおり。
#[derive(Serialize)]
struct Limits36Out {
    schema: &'static str,
    months: Vec<MonthOut>,
    notices: Vec<&'static str>,
}

#[derive(Serialize)]
struct MonthOut {
    start: String,
    total: i64,
    sums: SumsOut,
    special_count: u32,
    year_overtime: i64,
    year_overtime_known: bool,
    findings: Vec<FindingOut>,
}

/// 直近 n か月の合計（足りなければ `null`）。
#[derive(Serialize)]
struct SumsOut {
    #[serde(rename = "2")]
    two: Option<i64>,
    #[serde(rename = "3")]
    three: Option<i64>,
    #[serde(rename = "4")]
    four: Option<i64>,
    #[serde(rename = "5")]
    five: Option<i64>,
    #[serde(rename = "6")]
    six: Option<i64>,
}

#[derive(Serialize)]
struct FindingOut {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u32>,
}

impl From<&Limits36Result> for Limits36Out {
    fn from(result: &Limits36Result) -> Limits36Out {
        Limits36Out {
            schema: LIMITS36_SCHEMA,
            months: result
                .months
                .iter()
                .map(|month| {
                    let [two, three, four, five, six] = month.sums;
                    MonthOut {
                        start: month.start.to_string(),
                        total: month.total,
                        sums: SumsOut {
                            two,
                            three,
                            four,
                            five,
                            six,
                        },
                        special_count: month.special_count,
                        year_overtime: month.year_overtime,
                        year_overtime_known: month.year_overtime_known,
                        findings: month
                            .findings
                            .iter()
                            .map(|finding| FindingOut {
                                code: finding.code(),
                                n: finding.n(),
                            })
                            .collect(),
                    }
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
/// 不正な入力は [`Failure::invalid`]（36協定のケースに対応外はない）。
pub fn run(document: &Value) -> Result<CaseOutput, Failure> {
    // 手順1
    let raw = read_case(document)?;
    // 手順2
    let [
        monthly,
        annual,
        special_total,
        special_annual,
        special_months,
    ] = raw.limits;
    let agreement = Agreement {
        kind: agreement_kind(raw.kind)?,
        year_start_month: raw.year_start_month,
        monthly_limit_minutes: monthly,
        annual_limit_minutes: annual,
        special_monthly_total_limit_minutes: special_total,
        special_annual_limit_minutes: special_annual,
        special_months_max: special_months,
    }
    .validate()
    .map_err(|error| Failure::invalid(error.to_string()))?;
    for (index, month) in raw.months.iter().enumerate() {
        check_month_minutes(index, month.overtime, month.holiday)
            .map_err(|error| Failure::invalid(error.to_string()))?;
    }
    for (index, entry) in raw.history.iter().enumerate() {
        check_history_minutes(index, entry.total)
            .map_err(|error| Failure::invalid(error.to_string()))?;
    }
    // 手順4（日付の書式と入力の範囲。並びと月の整合は judge が見る）
    let months = raw
        .months
        .iter()
        .enumerate()
        .map(|(index, month)| {
            Ok(MonthHours {
                start: date(month.start, &format!("months[{index}].start"))?,
                overtime: month.overtime,
                holiday: month.holiday,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let history = raw
        .history
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            Ok(HistoryMonth {
                start: date(entry.start, &format!("history[{index}].start"))?,
                total: entry.total,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    // 手順4（協定月の整合）と手順5
    let result = judge(&agreement, &months, &history)
        .map_err(|error| Failure::invalid(error.to_string()))?;
    Ok(CaseOutput {
        stdout: json_line(&Limits36Out::from(&result))?,
        exit: EXIT_OK,
    })
}

#[cfg(test)]
mod tests {
    use super::{integer, run};
    use serde_json::json;

    #[test]
    fn integers_are_json_integers_only() {
        assert_eq!(integer(&json!(5), "x").unwrap(), 5);
        assert!(integer(&json!(1.0), "x").is_err());
        assert!(integer(&json!(true), "x").is_err());
        assert!(integer(&json!("1"), "x").is_err());
        assert!(integer(&json!(u64::MAX), "x").is_err());
    }

    #[test]
    fn empty_months_give_empty_output() {
        let case = json!({
            "schema": "kintai.case36.v1",
            "agreement": {"kind": "none", "year_start_month": 4},
            "months": [],
            "history": [{"start": "2021-03-16", "total": 0}, {"start": "2021-02-01", "total": 0}]
        });
        let output = run(&case).unwrap();
        assert_eq!(
            output.stdout,
            "{\"schema\":\"kintai.limits36.v1\",\"months\":[],\"notices\":[\"OwnEmployerOnly\"]}\n"
        );
    }
}
