//! 方針ファイル（TOML。`docs/spec/cli.md` の「方針ファイル」）を読む（AC-12.1）。
//!
//! 検査は `docs/spec/validation.md` の流儀で、最初に当たったものを返す。
//! 1. 形と型（[`PolicyFile::parse`]）: 大きさ（256MiB）→ 行の長さ（改行を除く4096バイト）→ UTF-8 →
//!    TOML として読めるか（同じ名前の項目は TOML の決まりで読めない）→ 知らない項目 → `schema` →
//!    必須の項目と型（`[workplace]` → `[agreement36]` → `[[employee]]` の順。従業員は10万人まで）
//! 2. 列挙と範囲（[`PolicyFile::parse`]）: 列挙の値、数値の範囲、協定の種類ごとの必須、
//!    従業員IDの形と重なり、年休の所定労働の条件、明示の付与の日数
//! 3. 対応外（[`PolicyFile::check_unsupported`]）: 表の順（`work_system` → 対象の従業員の `worker_kind`
//!    （ID順）→ 作業場のほかの指定）で、最後に `MonthStartDayMismatch`（TOML だけで判定する）
//! 4. 日付（[`PolicyFile::dates`]）: `hire_date` と `grants[].date` の書式と入力の範囲
//!
//! エラーの文には、行番号と、この版が決めた項目名だけを入れる。入力の値も、知らない項目の名前も入れない
//! （`toml` の部品の文は入力の一部を含みうるので使わない。AC-10.7）。整数は TOML の整数だけで、
//! 小数・真偽値・文字列は型の誤り。日付は文字列（TOML の日付の型は型の誤り）。

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use kintai_core::Date;
use kintai_core::punch::{EmployeeId, SecondsPolicy};
use kintai_jp_labor::leave::{
    Attendance, CalendarDate, GRANT_HALVES_RANGE, GrantCheck, GrantInput, LeaveError,
    LeaveUnsupportedCode, ValidWorkingTerms, WorkingTerms,
};
use kintai_jp_labor::limits36::{Agreement, AgreementKind, Limits36Error, ValidAgreement};
use kintai_jp_labor::report::Rounding;
use kintai_jp_labor::{PolicyError, UnsupportedCode, ValidPolicy, WorkPolicy, WorkerKind};
use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::case_leave::attendance_from;
use crate::case36::agreement_kind_from;
use crate::dto::{
    ambiguity_from, holiday_system_from, night_window_from, weekday_from, work_system_from,
    worker_kind_from,
};
use crate::failure::{EXIT_UNSUPPORTED, Failure};

/// 方針ファイルの `schema`。
pub const POLICY_SCHEMA: &str = "kintai.policy.v1";
/// 1行の長さの上限（バイト。改行の CR・LF を除く。`docs/spec/nfr.md`）。
pub const MAX_LINE_BYTES: usize = kintai_import::MAX_LINE_BYTES;
/// 従業員（`[[employee]]`）の数の上限（`docs/spec/nfr.md`）。
pub const MAX_EMPLOYEES: usize = kintai_import::MAX_EMPLOYEES;

const TOP_KEYS: [&str; 4] = ["schema", "workplace", "agreement36", "employee"];
const WORKPLACE_KEYS: [&str; 13] = [
    "work_system",
    "scheduled_minutes_per_day",
    "week_start",
    "weekly_limit_minutes",
    "special_44h_confirmed",
    "legal_holiday_weekday",
    "month_start_day",
    "ambiguity",
    "seconds",
    "rounding",
    "holiday_system",
    "holiday_swaps",
    "night_window",
];
/// `[agreement36]` の限度の項目（`Agreement` の順）。
const LIMIT_KEYS: [&str; 5] = [
    "monthly_limit_minutes",
    "annual_limit_minutes",
    "special_monthly_total_limit_minutes",
    "special_annual_limit_minutes",
    "special_months_max",
];
const AGREEMENT_KEYS: [&str; 8] = [
    "kind",
    "year_start_month",
    "month_start_day",
    "monthly_limit_minutes",
    "annual_limit_minutes",
    "special_monthly_total_limit_minutes",
    "special_annual_limit_minutes",
    "special_months_max",
];
const EMPLOYEE_KEYS: [&str; 8] = [
    "id",
    "worker_kind",
    "hire_date",
    "weekly_scheduled_days",
    "weekly_scheduled_minutes",
    "annual_scheduled_days",
    "attendance",
    "grants",
];
const GRANT_KEYS: [&str; 2] = ["date", "halves"];
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

// ---- 位置と誤り ----

/// バイトの位置 `offset` の行番号（1から）。先頭から数えるので、1回だけの誤り（UTF-8・TOML）に使う。
/// 値ごとの行番号は [`Source::line_at`]（行の始まりの表を二分探索する）で求める。
fn line_of(bytes: &[u8], offset: usize) -> u64 {
    let before = bytes.get(..offset).unwrap_or(bytes);
    // LF で分けた個数が、その位置の行番号（LF の数＋1）。
    u64::try_from(before.split(|byte| *byte == b'\n').count()).unwrap_or(u64::MAX)
}

/// 各行の始まりのバイト位置（1行目は0。昇順）。
fn line_starts(bytes: &[u8]) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        bytes
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'\n')
            .map(|(index, _)| index + 1),
    );
    starts
}

/// 行番号つきの不正な入力。
fn invalid_at(line: u64, message: impl fmt::Display) -> Failure {
    Failure::invalid(format!("方針ファイルの{line}行目: {message}"))
}

/// 行番号のない（ファイル全体の）不正な入力。
fn invalid_whole(message: impl fmt::Display) -> Failure {
    Failure::invalid(format!("方針ファイル: {message}"))
}

/// 値と、それが書かれた行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct At<T> {
    value: T,
    line: u64,
}

/// 読み込んだ文字列の行の始まりの表（行番号を求めるのに使う）。
///
/// 値を1つ読むたびにファイルの先頭から改行を数え直すと、従業員の数の2乗の時間になる（L2-01）。
/// 表は1回だけ作り、行番号は二分探索で求める。
#[derive(Clone, Copy)]
struct Source<'s> {
    starts: &'s [usize],
}

impl Source<'_> {
    /// バイトの位置 `offset` の行番号（1から）。
    fn line_at(self, offset: usize) -> u64 {
        let count = self.starts.partition_point(|start| *start <= offset);
        u64::try_from(count).unwrap_or(u64::MAX)
    }

    fn line<T>(self, item: &Spanned<T>) -> u64 {
        self.line_at(item.span().start)
    }
}

type Value<'i> = Spanned<DeValue<'i>>;

/// 表（`[workplace]` など）と、その場所。根の表は行番号を持たない。
#[derive(Clone, Copy)]
struct TableAt<'t, 'i> {
    table: &'t DeTable<'i>,
    line: Option<u64>,
}

impl<'t, 'i> TableAt<'t, 'i> {
    fn missing(self, path: &str) -> Failure {
        match self.line {
            Some(line) => invalid_at(line, format!("{path} がありません")),
            None => invalid_whole(format!("{path} がありません")),
        }
    }

    fn required(self, key: &str, path: &str) -> Result<&'t Value<'i>, Failure> {
        self.table.get(key).ok_or_else(|| self.missing(path))
    }

    fn optional(self, key: &str) -> Option<&'t Value<'i>> {
        self.table.get(key)
    }

    /// 知らない項目があれば、ファイルの中で最初のものの行で不正な入力にする（項目の名前は出さない）。
    fn reject_unknown(self, src: Source<'_>, known: &[&str], path: &str) -> Result<(), Failure> {
        let first = self
            .table
            .iter()
            .filter(|(key, _)| !known.contains(&key.get_ref().as_ref()))
            .map(|(key, _)| key.span().start)
            .min();
        match first {
            Some(offset) => Err(invalid_at(
                src.line_at(offset),
                format!("{path} に知らない項目があります"),
            )),
            None => Ok(()),
        }
    }
}

fn string<'t>(src: Source<'_>, value: &'t Value<'_>, path: &str) -> Result<At<&'t str>, Failure> {
    let line = src.line(value);
    match value.get_ref() {
        DeValue::String(text) => Ok(At {
            value: text.as_ref(),
            line,
        }),
        _ => Err(invalid_at(line, format!("{path} は文字列ではありません"))),
    }
}

fn integer(src: Source<'_>, value: &Value<'_>, path: &str) -> Result<At<i64>, Failure> {
    let line = src.line(value);
    let DeValue::Integer(number) = value.get_ref() else {
        return Err(invalid_at(line, format!("{path} は整数ではありません")));
    };
    let radix = number.radix();
    // 基数は TOML の 2・8・10・16 だけだが、範囲外の基数で `from_str_radix` がパニックしないよう確かめる。
    if !(2..=36).contains(&radix) {
        return Err(invalid_at(line, format!("{path} は整数ではありません")));
    }
    i64::from_str_radix(number.as_str(), radix)
        .map(|value| At { value, line })
        .map_err(|_| invalid_at(line, format!("{path} の値が範囲外です")))
}

fn boolean(src: Source<'_>, value: &Value<'_>, path: &str) -> Result<At<bool>, Failure> {
    let line = src.line(value);
    match value.get_ref() {
        DeValue::Boolean(flag) => Ok(At { value: *flag, line }),
        _ => Err(invalid_at(line, format!("{path} は真偽値ではありません"))),
    }
}

fn table<'t, 'i>(
    src: Source<'_>,
    value: &'t Value<'i>,
    path: &str,
) -> Result<TableAt<'t, 'i>, Failure> {
    let line = src.line(value);
    match value.get_ref() {
        DeValue::Table(inner) => Ok(TableAt {
            table: inner,
            line: Some(line),
        }),
        _ => Err(invalid_at(line, format!("{path} は表ではありません"))),
    }
}

fn array<'t, 'i>(
    src: Source<'_>,
    value: &'t Value<'i>,
    path: &str,
) -> Result<At<&'t [Value<'i>]>, Failure> {
    let line = src.line(value);
    match value.get_ref() {
        DeValue::Array(items) => Ok(At { value: items, line }),
        _ => Err(invalid_at(line, format!("{path} は配列ではありません"))),
    }
}

fn optional_string<'t>(
    src: Source<'_>,
    owner: TableAt<'t, '_>,
    key: &str,
    path: &str,
) -> Result<Option<At<&'t str>>, Failure> {
    owner
        .optional(key)
        .map(|value| string(src, value, &format!("{path}.{key}")))
        .transpose()
}

fn optional_integer(
    src: Source<'_>,
    owner: TableAt<'_, '_>,
    key: &str,
    path: &str,
) -> Result<Option<At<i64>>, Failure> {
    owner
        .optional(key)
        .map(|value| integer(src, value, &format!("{path}.{key}")))
        .transpose()
}

fn required_string<'t>(
    src: Source<'_>,
    owner: TableAt<'t, '_>,
    key: &str,
    path: &str,
) -> Result<At<&'t str>, Failure> {
    let full = format!("{path}.{key}");
    string(src, owner.required(key, &full)?, &full)
}

fn required_integer(
    src: Source<'_>,
    owner: TableAt<'_, '_>,
    key: &str,
    path: &str,
) -> Result<At<i64>, Failure> {
    let full = format!("{path}.{key}");
    integer(src, owner.required(key, &full)?, &full)
}

// ---- 手順1: 形と型 ----

/// 型を確かめた `[workplace]`（列挙の値は、まだ文字列のまま）。
struct RawWorkplace<'t> {
    work_system: At<&'t str>,
    scheduled_minutes_per_day: At<i64>,
    week_start: Option<At<&'t str>>,
    weekly_limit_minutes: Option<At<i64>>,
    special_44h_confirmed: Option<At<bool>>,
    legal_holiday_weekday: Option<At<&'t str>>,
    month_start_day: At<i64>,
    ambiguity: Option<At<&'t str>>,
    seconds: Option<At<&'t str>>,
    rounding: Option<At<&'t str>>,
    holiday_system: Option<At<&'t str>>,
    holiday_swaps: usize,
    night_window: Option<At<&'t str>>,
}

/// 型を確かめた `[agreement36]`。
struct RawAgreement<'t> {
    line: Option<u64>,
    kind: At<&'t str>,
    year_start_month: At<i64>,
    month_start_day: Option<At<i64>>,
    limits: [Option<At<i64>>; 5],
}

/// 型を確かめた明示の付与。
struct RawGrant<'t> {
    date: At<&'t str>,
    halves: At<i64>,
}

/// 型を確かめた `[[employee]]` の1件。
struct RawEmployee<'t> {
    index: usize,
    line: u64,
    id: At<&'t str>,
    worker_kind: Option<At<&'t str>>,
    hire_date: At<&'t str>,
    weekly_scheduled_days: At<i64>,
    weekly_scheduled_minutes: At<i64>,
    annual_scheduled_days: Option<At<i64>>,
    attendance: Vec<At<&'t str>>,
    grants: Option<Vec<RawGrant<'t>>>,
}

/// 型を確かめた方針ファイル。
struct RawPolicy<'t> {
    workplace: RawWorkplace<'t>,
    agreement: RawAgreement<'t>,
    employees: Vec<RawEmployee<'t>>,
}

fn read_workplace<'t>(
    src: Source<'_>,
    owner: TableAt<'t, '_>,
) -> Result<RawWorkplace<'t>, Failure> {
    let path = "workplace";
    owner.reject_unknown(src, &WORKPLACE_KEYS, path)?;
    Ok(RawWorkplace {
        work_system: required_string(src, owner, "work_system", path)?,
        scheduled_minutes_per_day: required_integer(src, owner, "scheduled_minutes_per_day", path)?,
        week_start: optional_string(src, owner, "week_start", path)?,
        weekly_limit_minutes: optional_integer(src, owner, "weekly_limit_minutes", path)?,
        special_44h_confirmed: owner
            .optional("special_44h_confirmed")
            .map(|value| boolean(src, value, "workplace.special_44h_confirmed"))
            .transpose()?,
        legal_holiday_weekday: optional_string(src, owner, "legal_holiday_weekday", path)?,
        month_start_day: required_integer(src, owner, "month_start_day", path)?,
        ambiguity: optional_string(src, owner, "ambiguity", path)?,
        seconds: optional_string(src, owner, "seconds", path)?,
        rounding: optional_string(src, owner, "rounding", path)?,
        holiday_system: optional_string(src, owner, "holiday_system", path)?,
        holiday_swaps: match owner.optional("holiday_swaps") {
            Some(value) => array(src, value, "workplace.holiday_swaps")?.value.len(),
            None => 0,
        },
        night_window: optional_string(src, owner, "night_window", path)?,
    })
}

fn read_agreement<'t>(
    src: Source<'_>,
    owner: TableAt<'t, '_>,
) -> Result<RawAgreement<'t>, Failure> {
    let path = "agreement36";
    owner.reject_unknown(src, &AGREEMENT_KEYS, path)?;
    let kind = required_string(src, owner, "kind", path)?;
    let year_start_month = required_integer(src, owner, "year_start_month", path)?;
    let month_start_day = optional_integer(src, owner, "month_start_day", path)?;
    let mut limits = [None; 5];
    for (slot, key) in limits.iter_mut().zip(LIMIT_KEYS) {
        *slot = optional_integer(src, owner, key, path)?;
    }
    Ok(RawAgreement {
        line: owner.line,
        kind,
        year_start_month,
        month_start_day,
        limits,
    })
}

fn read_grant<'t>(
    src: Source<'_>,
    value: &'t Value<'_>,
    path: &str,
) -> Result<RawGrant<'t>, Failure> {
    let owner = table(src, value, path)?;
    owner.reject_unknown(src, &GRANT_KEYS, path)?;
    Ok(RawGrant {
        date: required_string(src, owner, "date", path)?,
        halves: required_integer(src, owner, "halves", path)?,
    })
}

fn read_employee<'t>(
    src: Source<'_>,
    value: &'t Value<'_>,
    index: usize,
) -> Result<RawEmployee<'t>, Failure> {
    let path = format!("employee[{index}]");
    let owner = table(src, value, &path)?;
    owner.reject_unknown(src, &EMPLOYEE_KEYS, &path)?;
    let id = required_string(src, owner, "id", &path)?;
    let worker_kind = optional_string(src, owner, "worker_kind", &path)?;
    let hire_date = required_string(src, owner, "hire_date", &path)?;
    let weekly_scheduled_days = required_integer(src, owner, "weekly_scheduled_days", &path)?;
    let weekly_scheduled_minutes = required_integer(src, owner, "weekly_scheduled_minutes", &path)?;
    let annual_scheduled_days = optional_integer(src, owner, "annual_scheduled_days", &path)?;
    let attendance_path = format!("{path}.attendance");
    let attendance = array(
        src,
        owner.required("attendance", &attendance_path)?,
        &attendance_path,
    )?
    .value
    .iter()
    .enumerate()
    .map(|(i, item)| string(src, item, &format!("{attendance_path}[{i}]")))
    .collect::<Result<Vec<_>, Failure>>()?;
    let grants = owner
        .optional("grants")
        .map(|list| {
            array(src, list, &format!("{path}.grants"))?
                .value
                .iter()
                .enumerate()
                .map(|(i, item)| read_grant(src, item, &format!("{path}.grants[{i}]")))
                .collect::<Result<Vec<_>, Failure>>()
        })
        .transpose()?;
    Ok(RawEmployee {
        index,
        line: owner.line.unwrap_or(1),
        id,
        worker_kind,
        hire_date,
        weekly_scheduled_days,
        weekly_scheduled_minutes,
        annual_scheduled_days,
        attendance,
        grants,
    })
}

/// 従業員の数の上限（10万人。`docs/spec/nfr.md`）。
fn check_employee_count(count: usize, line: u64) -> Result<(), Failure> {
    if count > MAX_EMPLOYEES {
        Err(invalid_at(
            line,
            format!("従業員が多すぎます（上限は {MAX_EMPLOYEES} 人）"),
        ))
    } else {
        Ok(())
    }
}

fn read_raw<'t>(src: Source<'_>, root: &'t DeTable<'_>) -> Result<RawPolicy<'t>, Failure> {
    let root = TableAt {
        table: root,
        line: None,
    };
    root.reject_unknown(src, &TOP_KEYS, "方針ファイルの最上位")?;
    let schema = string(src, root.required("schema", "schema")?, "schema")?;
    if schema.value != POLICY_SCHEMA {
        return Err(invalid_at(
            schema.line,
            format!("schema が {POLICY_SCHEMA} ではありません"),
        ));
    }
    let workplace = read_workplace(
        src,
        table(src, root.required("workplace", "[workplace]")?, "workplace")?,
    )?;
    let agreement = read_agreement(
        src,
        table(
            src,
            root.required("agreement36", "[agreement36]")?,
            "agreement36",
        )?,
    )?;
    let list = array(src, root.required("employee", "[[employee]]")?, "employee")?;
    check_employee_count(list.value.len(), list.line)?;
    let employees = list
        .value
        .iter()
        .enumerate()
        .map(|(index, value)| read_employee(src, value, index))
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(RawPolicy {
        workplace,
        agreement,
        employees,
    })
}

// ---- 手順2: 列挙と範囲 ----

fn enum_value<T>(at: At<&str>, path: &str, from: fn(&str) -> Option<T>) -> Result<T, Failure> {
    from(at.value).ok_or_else(|| invalid_at(at.line, format!("{path} の値が定義にありません")))
}

fn optional_enum<T>(
    at: Option<At<&str>>,
    path: &str,
    from: fn(&str) -> Option<T>,
) -> Result<Option<T>, Failure> {
    at.map(|at| enum_value(at, path, from)).transpose()
}

/// 作業場の検査済みの値（手順2まで）。
struct Workplace {
    policy: WorkPolicy,
    seconds: SecondsPolicy,
    rounding: Rounding,
}

fn validate_workplace(raw: &RawWorkplace<'_>) -> Result<Workplace, Failure> {
    let policy = WorkPolicy {
        work_system: enum_value(raw.work_system, "workplace.work_system", work_system_from)?,
        scheduled_minutes_per_day: raw.scheduled_minutes_per_day.value,
        week_start: optional_enum(raw.week_start, "workplace.week_start", weekday_from)?,
        weekly_limit_minutes: raw.weekly_limit_minutes.map(|at| at.value),
        special_44h_confirmed: raw.special_44h_confirmed.map(|at| at.value),
        legal_holiday_weekday: optional_enum(
            raw.legal_holiday_weekday,
            "workplace.legal_holiday_weekday",
            weekday_from,
        )?,
        month_start_day: raw.month_start_day.value,
        ambiguity: optional_enum(raw.ambiguity, "workplace.ambiguity", ambiguity_from)?,
        worker_kind: None,
        holiday_system: optional_enum(
            raw.holiday_system,
            "workplace.holiday_system",
            holiday_system_from,
        )?,
        holiday_swaps: raw.holiday_swaps,
        night_window: optional_enum(
            raw.night_window,
            "workplace.night_window",
            night_window_from,
        )?,
    };
    let seconds = optional_enum(raw.seconds, "workplace.seconds", SecondsPolicy::from_code)?
        .unwrap_or_default();
    let rounding =
        optional_enum(raw.rounding, "workplace.rounding", Rounding::from_code)?.unwrap_or_default();
    // 数値の範囲。対応外（手順3）は、ここでは見ない。
    if let Err(PolicyError::Invalid { field, expected }) = policy.validate() {
        let line = match field {
            "scheduled_minutes_per_day" => Some(raw.scheduled_minutes_per_day.line),
            "month_start_day" => Some(raw.month_start_day.line),
            _ => raw.weekly_limit_minutes.map(|at| at.line),
        };
        let message = format!("workplace.{field} の値が範囲外です（{expected}）");
        return Err(match line {
            Some(line) => invalid_at(line, message),
            None => invalid_whole(message),
        });
    }
    Ok(Workplace {
        policy,
        seconds,
        rounding,
    })
}

/// 協定の検査済みの値（手順2まで）。
struct AgreementValues {
    agreement: ValidAgreement,
    month_start_day: Option<i64>,
}

fn agreement_error(raw: &RawAgreement<'_>, error: Limits36Error) -> Failure {
    let table_message = |message: String| match raw.line {
        Some(line) => invalid_at(line, message),
        None => invalid_whole(message),
    };
    match error {
        Limits36Error::AgreementOutOfRange { field, expected } => {
            let line = if field == "year_start_month" {
                Some(raw.year_start_month.line)
            } else {
                LIMIT_KEYS
                    .iter()
                    .position(|key| *key == field)
                    .and_then(|at| raw.limits.get(at).copied().flatten())
                    .map(|at| at.line)
            };
            let message = format!("agreement36.{field} の値が範囲外です（{expected}）");
            match line {
                Some(line) => invalid_at(line, message),
                None => table_message(message),
            }
        }
        Limits36Error::MissingLimit { field } => table_message(format!(
            "agreement36.{field} は、この協定の種類では必須です"
        )),
        other => invalid_whole(other),
    }
}

fn validate_agreement(raw: &RawAgreement<'_>) -> Result<AgreementValues, Failure> {
    let kind = enum_value(raw.kind, "agreement36.kind", agreement_kind_from)?;
    let [
        monthly,
        annual,
        special_total,
        special_annual,
        special_months,
    ] = raw.limits.map(|at| at.map(|at| at.value));
    let agreement = Agreement {
        kind,
        year_start_month: raw.year_start_month.value,
        monthly_limit_minutes: monthly,
        annual_limit_minutes: annual,
        special_monthly_total_limit_minutes: special_total,
        special_annual_limit_minutes: special_annual,
        special_months_max: special_months,
    }
    .validate()
    .map_err(|error| agreement_error(raw, error))?;
    match raw.month_start_day {
        Some(at) if !(1..=28).contains(&at.value) => Err(invalid_at(
            at.line,
            "agreement36.month_start_day の値が範囲外です（1〜28）",
        )),
        None if kind != AgreementKind::None => {
            let message = "agreement36.month_start_day は、kind が none でないとき必須です";
            Err(match raw.line {
                Some(line) => invalid_at(line, message),
                None => invalid_whole(message),
            })
        }
        _ => Ok(AgreementValues {
            agreement,
            month_start_day: raw.month_start_day.map(|at| at.value),
        }),
    }
}

/// 方針ファイルの従業員（`[[employee]]` の1件。手順2まで検査済み）。
#[derive(Clone, Debug)]
pub struct Employee {
    /// ファイルの中で何番目か（0から）。
    pub index: usize,
    /// 従業員ID。
    pub id: EmployeeId,
    /// 従業員の区分（既定は `standard`）。
    pub worker_kind: WorkerKind,
    /// 所定労働の条件（範囲は検査済み）。
    pub terms: WorkingTerms,
    /// 検査済みの所定労働の条件。
    pub valid_terms: ValidWorkingTerms,
    /// 付与ごとの出勤率。
    pub attendance: Vec<Attendance>,
    hire_date: At<String>,
    grants: Option<Vec<(At<String>, i64)>>,
}

impl Employee {
    /// 年休の対応外の判定（手順3）に使う、明示の付与（日付は書式が正しいときだけ）。
    #[must_use]
    pub fn grant_checks(&self) -> Option<Vec<GrantCheck>> {
        self.grants.as_ref().map(|grants| {
            grants
                .iter()
                .map(|(date, halves)| GrantCheck {
                    date: CalendarDate::parse(&date.value),
                    halves: *halves,
                })
                .collect()
        })
    }
}

fn validate_employee(raw: &RawEmployee<'_>) -> Result<Employee, Failure> {
    let path = format!("employee[{}]", raw.index);
    let id = EmployeeId::new(raw.id.value).map_err(|_| {
        invalid_at(
            raw.id.line,
            format!("{path}.id の形が違います（英数字で始まり、英数字と . _ - だけの64文字まで）"),
        )
    })?;
    let worker_kind = optional_enum(
        raw.worker_kind,
        &format!("{path}.worker_kind"),
        worker_kind_from,
    )?
    .unwrap_or(WorkerKind::Standard);
    let terms = WorkingTerms {
        weekly_scheduled_days: raw.weekly_scheduled_days.value,
        weekly_scheduled_minutes: raw.weekly_scheduled_minutes.value,
        annual_scheduled_days: raw.annual_scheduled_days.map(|at| at.value),
    };
    let valid_terms = terms.validate().map_err(|error| {
        let line = match error {
            LeaveError::OutOfRange {
                field: "weekly_scheduled_days",
                ..
            } => raw.weekly_scheduled_days.line,
            LeaveError::OutOfRange {
                field: "weekly_scheduled_minutes",
                ..
            } => raw.weekly_scheduled_minutes.line,
            _ => raw.annual_scheduled_days.map_or(raw.line, |at| at.line),
        };
        match error {
            LeaveError::OutOfRange { field, expected } => invalid_at(
                line,
                format!("{path}.{field} の値が範囲外です（{expected}）"),
            ),
            other => invalid_at(line, other),
        }
    })?;
    let attendance = raw
        .attendance
        .iter()
        .enumerate()
        .map(|(i, at)| enum_value(*at, &format!("{path}.attendance[{i}]"), attendance_from))
        .collect::<Result<Vec<_>, Failure>>()?;
    let grants = raw
        .grants
        .as_ref()
        .map(|grants| {
            grants
                .iter()
                .enumerate()
                .map(|(i, grant)| {
                    if GRANT_HALVES_RANGE.contains(&grant.halves.value) {
                        Ok((
                            At {
                                value: grant.date.value.to_owned(),
                                line: grant.date.line,
                            },
                            grant.halves.value,
                        ))
                    } else {
                        Err(invalid_at(
                            grant.halves.line,
                            format!("{path}.grants[{i}].halves は1〜200の整数です"),
                        ))
                    }
                })
                .collect::<Result<Vec<_>, Failure>>()
        })
        .transpose()?;
    Ok(Employee {
        index: raw.index,
        id,
        worker_kind,
        terms,
        valid_terms,
        attendance,
        hire_date: At {
            value: raw.hire_date.value.to_owned(),
            line: raw.hire_date.line,
        },
        grants,
    })
}

// ---- 読み込んだ方針 ----

/// 方針ファイル（手順1・2まで検査済み）。
#[derive(Clone, Debug)]
pub struct PolicyFile {
    workplace: WorkPolicy,
    seconds: SecondsPolicy,
    rounding: Rounding,
    agreement: ValidAgreement,
    agreement_month_start_day: Option<i64>,
    employees: Vec<Employee>,
    by_id: BTreeMap<EmployeeId, usize>,
}

/// 従業員ごとの日付（手順4で読んだもの）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmployeeDates {
    /// 雇入れ日。
    pub hire_date: Date,
    /// 明示の付与（なければ法定の付与日）。
    pub grants: Option<Vec<GrantInput>>,
}

/// 1行の長さを確かめる（改行の CR・LF を除いて [`MAX_LINE_BYTES`] まで）。
fn check_line_lengths(bytes: &[u8]) -> Result<(), Failure> {
    for (number, line) in (1_u64..).zip(bytes.split(|byte| *byte == b'\n')) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > MAX_LINE_BYTES {
            return Err(invalid_at(
                number,
                format!("行が長すぎます（上限は改行を除いて {MAX_LINE_BYTES} バイト）"),
            ));
        }
    }
    Ok(())
}

impl PolicyFile {
    /// ファイルを読み、手順1・2を検査する。
    ///
    /// # Errors
    ///
    /// 開けない・読めない・256MiB を超える、または手順1・2の誤りなら不正な入力。
    pub fn load(path: &Path) -> Result<PolicyFile, Failure> {
        let bytes = crate::inputs::read_input(path, "方針ファイル")?;
        PolicyFile::parse(&bytes)
    }

    /// バイト列を読み、手順1・2を検査する（大きさの上限は読む側が見る）。
    ///
    /// # Errors
    ///
    /// 手順1・2の誤りは不正な入力（行番号と項目名つき）。
    pub fn parse(bytes: &[u8]) -> Result<PolicyFile, Failure> {
        let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
        check_line_lengths(body)?;
        let text = std::str::from_utf8(body).map_err(|error| {
            invalid_at(
                line_of(body, error.valid_up_to()),
                "UTF-8 として読めません（方針ファイルは UTF-8 で書いてください）",
            )
        })?;
        let starts = line_starts(body);
        let src = Source { starts: &starts };
        let root = DeTable::parse(text).map_err(|error| match error.span() {
            Some(span) => invalid_at(line_of(body, span.start), "TOML として読めません"),
            None => invalid_whole("TOML として読めません"),
        })?;
        // 手順1
        let raw = read_raw(src, root.get_ref())?;
        // 手順2
        let workplace = validate_workplace(&raw.workplace)?;
        let agreement = validate_agreement(&raw.agreement)?;
        let mut employees = Vec::with_capacity(raw.employees.len());
        let mut by_id = BTreeMap::new();
        for entry in &raw.employees {
            let employee = validate_employee(entry)?;
            if by_id.insert(employee.id.clone(), employees.len()).is_some() {
                return Err(invalid_at(
                    entry.id.line,
                    format!(
                        "employee[{}].id が、前の [[employee]] と同じです",
                        entry.index
                    ),
                ));
            }
            employees.push(employee);
        }
        Ok(PolicyFile {
            workplace: workplace.policy,
            seconds: workplace.seconds,
            rounding: workplace.rounding,
            agreement: agreement.agreement,
            agreement_month_start_day: agreement.month_start_day,
            employees,
            by_id,
        })
    }

    /// 秒の扱い（`workplace.seconds`）。
    #[must_use]
    pub const fn seconds(&self) -> SecondsPolicy {
        self.seconds
    }

    /// 丸め（`workplace.rounding`）。
    #[must_use]
    pub const fn rounding(&self) -> Rounding {
        self.rounding
    }

    /// 36協定（`[agreement36]`）。
    #[must_use]
    pub const fn agreement(&self) -> &ValidAgreement {
        &self.agreement
    }

    /// 従業員IDが `[[employee]]` にあるか。
    #[must_use]
    pub fn contains(&self, id: &EmployeeId) -> bool {
        self.by_id.contains_key(id)
    }

    /// 従業員（ID順）。
    pub fn employees_by_id(&self) -> impl Iterator<Item = &Employee> {
        self.by_id
            .values()
            .filter_map(|index| self.employees.get(*index))
    }

    /// 計算の対象の従業員（ID順）。`employee` があれば、その1人だけ。
    ///
    /// # Errors
    ///
    /// `employee` が `[[employee]]` になければ不正な入力。
    pub fn scope(&self, employee: Option<&EmployeeId>) -> Result<Vec<&Employee>, Failure> {
        match employee {
            None => Ok(self.employees_by_id().collect()),
            Some(id) => self
                .by_id
                .get(id)
                .and_then(|index| self.employees.get(*index))
                .map(|found| vec![found])
                .ok_or_else(|| {
                    Failure::invalid(
                        "--employee の従業員IDが、方針ファイルの [[employee]] にありません",
                    )
                }),
        }
    }

    /// 手順3: 対応外の指定を、表の順に見る。`scope` は計算の対象の従業員（ID順）。
    ///
    /// `work_system` → 対象の従業員の `worker_kind`（ID順。エラーに従業員IDを含める）→
    /// 作業場のほかの指定（`holiday_system` → `holiday_swaps` → `night_window` → `legal_holiday_weekday`
    /// の欠け → 週44時間の確認なし）→ `MonthStartDayMismatch`（協定の種類が `none` でなく、協定の
    /// `month_start_day` が作業場のものと違う）。
    ///
    /// # Errors
    ///
    /// 対応外なら、そのコードの [`Failure`]（終了コード3）。
    pub fn check_unsupported(&self, scope: &[&Employee]) -> Result<ValidPolicy, Failure> {
        if let Some(code) = self.workplace.work_system.unsupported() {
            return Err(Failure::unsupported(code));
        }
        for employee in scope {
            if let Some(code) = employee.worker_kind.unsupported() {
                return Err(unsupported_for(
                    code.code(),
                    &code.to_string(),
                    &employee.id,
                ));
            }
        }
        let valid = self.workplace.validate().map_err(|error| match error {
            PolicyError::Unsupported(code) => Failure::unsupported(code),
            PolicyError::Invalid { .. } => invalid_whole(error),
        })?;
        let month_start_day = i64::from(valid.month_start_day().get());
        if self.agreement.kind() != AgreementKind::None
            && self.agreement_month_start_day != Some(month_start_day)
        {
            return Err(Failure::unsupported(UnsupportedCode::MonthStartDayMismatch));
        }
        Ok(valid)
    }

    /// 手順4: 全員の `hire_date` と `grants[].date` を読む（ファイルの順に、最初の誤りを返す）。
    ///
    /// # Errors
    ///
    /// 書式の誤りや、2000-01-01〜2099-12-31 の外の日付は不正な入力（行番号つき）。
    pub fn dates(&self) -> Result<BTreeMap<EmployeeId, EmployeeDates>, Failure> {
        let mut dates = BTreeMap::new();
        for employee in &self.employees {
            let path = format!("employee[{}]", employee.index);
            let hire_date = parse_date(&employee.hire_date, &format!("{path}.hire_date"))?;
            let grants = employee
                .grants
                .as_ref()
                .map(|grants| {
                    grants
                        .iter()
                        .enumerate()
                        .map(|(i, (date, halves))| {
                            Ok(GrantInput {
                                date: parse_date(date, &format!("{path}.grants[{i}].date"))?,
                                halves: *halves,
                            })
                        })
                        .collect::<Result<Vec<_>, Failure>>()
                })
                .transpose()?;
            dates.insert(employee.id.clone(), EmployeeDates { hire_date, grants });
        }
        Ok(dates)
    }
}

fn parse_date(at: &At<String>, path: &str) -> Result<Date, Failure> {
    Date::parse(&at.value).map_err(|error| invalid_at(at.line, format!("{path}: {error}")))
}

/// 従業員ごとの対応外（終了コード3）。エラーに従業員IDを含める（`docs/spec/cli.md` の「拒否」）。
#[must_use]
pub fn unsupported_for(code: &'static str, message: &str, id: &EmployeeId) -> Failure {
    Failure {
        code,
        message: format!("{message}（従業員ID: {id}）"),
        exit: EXIT_UNSUPPORTED,
    }
}

/// 年休の対応外（終了コード3）を、従業員ID つきで作る。
#[must_use]
pub fn leave_unsupported_for(code: LeaveUnsupportedCode, id: &EmployeeId) -> Failure {
    unsupported_for(code.code(), &code.to_string(), id)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_EMPLOYEES, PolicyFile, Source, check_employee_count, check_line_lengths, line_of,
        line_starts,
    };
    use kintai_core::punch::{EmployeeId, SecondsPolicy};
    use kintai_jp_labor::report::Rounding;

    const BASE: &str = r#"schema = "kintai.policy.v1"

[workplace]
work_system = "standard"
scheduled_minutes_per_day = 420
legal_holiday_weekday = "sun"
month_start_day = 1

[agreement36]
kind = "general"
year_start_month = 4
month_start_day = 1
monthly_limit_minutes = 2700
annual_limit_minutes = 21600

[[employee]]
id = "E001"
hire_date = "2024-04-01"
weekly_scheduled_days = 5
weekly_scheduled_minutes = 2100
attendance = []
"#;

    fn parse(text: &str) -> Result<PolicyFile, (&'static str, u8, String)> {
        PolicyFile::parse(text.as_bytes()).map_err(|f| (f.code, f.exit, f.message))
    }

    fn message(text: &str) -> String {
        parse(text).unwrap_err().2
    }

    #[test]
    fn base_policy_reads_with_defaults() {
        let policy = parse(BASE).unwrap();
        assert_eq!(policy.seconds(), SecondsPolicy::WorkerFavorable);
        assert_eq!(policy.rounding(), Rounding::None);
        let scope = policy.scope(None).unwrap();
        let valid = policy.check_unsupported(&scope).unwrap();
        assert_eq!(valid.weekly_limit_minutes(), 2400);
        assert!(policy.contains(&EmployeeId::new("E001").unwrap()));
        assert_eq!(policy.dates().unwrap().len(), 1);
    }

    /// AC-10.6 行の長さは、改行を除いて4096バイトまで。
    #[test]
    fn ac10_6_line_length_boundary() {
        let ok = format!("# {}\n", "x".repeat(4094));
        assert!(check_line_lengths(ok.as_bytes()).is_ok());
        let crlf = format!("# {}\r\n", "x".repeat(4094));
        assert!(check_line_lengths(crlf.as_bytes()).is_ok());
        let long = format!("a = 1\n# {}\n", "x".repeat(4095));
        let error = check_line_lengths(long.as_bytes()).unwrap_err();
        assert_eq!(error.code, "InvalidInput");
        assert!(error.message.contains("2行目"), "{}", error.message);
    }

    /// AC-10.6 従業員は10万人まで。
    #[test]
    fn ac10_6_employee_count_boundary() {
        assert!(check_employee_count(MAX_EMPLOYEES, 1).is_ok());
        assert!(check_employee_count(MAX_EMPLOYEES + 1, 1).is_err());
    }

    #[test]
    fn line_numbers_count_lf() {
        assert_eq!(line_of(b"a\nb\nc", 0), 1);
        assert_eq!(line_of(b"a\nb\nc", 2), 2);
        assert_eq!(line_of(b"a\nb\nc", 4), 3);
        assert_eq!(line_of(b"a\nb\nc", 99), 3);
    }

    /// 行の始まりの表を二分探索した行番号は、先頭から数えた行番号と、どの位置でも一致する
    /// （空の行・CRLF・末尾の改行・範囲の外の位置を含む）。
    #[test]
    fn line_table_agrees_with_counting() {
        for text in ["", "a", "a\nb\nc", "\n\n", "ab\r\n\r\ncd\r\n", "x\n"] {
            let bytes = text.as_bytes();
            let starts = line_starts(bytes);
            let source = Source { starts: &starts };
            for offset in 0..=bytes.len() + 2 {
                assert_eq!(
                    source.line_at(offset),
                    line_of(bytes, offset),
                    "{text:?} {offset}"
                );
            }
        }
    }

    /// L2-01・AC-10.6 上限の10万人の方針を読める。行番号を値ごとに先頭から数え直すと2乗の時間に
    /// なる（8,000人で約14秒、10万人で約36分の見込みだった）。表を使うと、Apple M4 Pro で
    /// リリースのビルドが0.22秒、最適化なしの試験のビルドが2.1秒。下の上限（10秒）は、最適化なしの
    /// ビルドと遅い機械での余裕を見たもの。
    #[test]
    fn l2_01_hundred_thousand_employees_parse_quickly() {
        use std::fmt::Write as _;
        let mut text = BASE.split("[[employee]]").next().unwrap().to_owned();
        for index in 0..MAX_EMPLOYEES {
            write!(
                text,
                "[[employee]]\nid = \"E{index:06}\"\nhire_date = \"2024-04-01\"\nweekly_scheduled_days = 5\nweekly_scheduled_minutes = 2400\nattendance = [\"meets\"]\n"
            )
            .unwrap();
        }
        let started = std::time::Instant::now();
        let policy = PolicyFile::parse(text.as_bytes()).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(policy.employees_by_id().count(), MAX_EMPLOYEES);
        assert!(policy.dates().is_ok());
        eprintln!("10万人の方針の読み込み: {elapsed:?}");
        assert!(elapsed.as_secs() < 10, "{elapsed:?}");
    }

    #[test]
    fn toml_integers_in_any_radix_and_with_underscores() {
        let text = BASE
            .replace(
                "weekly_scheduled_minutes = 2100",
                "weekly_scheduled_minutes = 2_100",
            )
            .replace(
                "scheduled_minutes_per_day = 420",
                "scheduled_minutes_per_day = 0x1A4",
            )
            .replace("year_start_month = 4", "year_start_month = +4");
        let policy = parse(&text).unwrap();
        assert_eq!(policy.employees[0].terms.weekly_scheduled_minutes, 2100);
        assert_eq!(policy.workplace.scheduled_minutes_per_day, 420);
        assert_eq!(policy.agreement().year_start_month(), 4);
    }

    /// AC-12.1 型の誤り・知らない項目・必須の欠け・範囲外は、不正な入力（行番号つき）。
    #[test]
    fn ac12_1_shape_errors_are_invalid_input() {
        let cases = [
            (
                BASE.replace(
                    "month_start_day = 1\n\n[agreement36]",
                    "month_start_day = 1.0\n\n[agreement36]",
                ),
                "7行目",
            ),
            (
                BASE.replace("work_system = \"standard\"", "work_system = 1"),
                "4行目",
            ),
            (
                BASE.replace("[workplace]\n", "[workplace]\nsurprise = 1\n"),
                "4行目",
            ),
            (BASE.replace("work_system = \"standard\"\n", ""), "3行目"),
            (
                BASE.replace(
                    "schema = \"kintai.policy.v1\"",
                    "schema = \"kintai.policy.v2\"",
                ),
                "1行目",
            ),
            (
                BASE.replace("hire_date = \"2024-04-01\"", "hire_date = 2024-04-01"),
                "18行目",
            ),
            (
                BASE.replace(
                    "month_start_day = 1\n\n[agreement36]",
                    "month_start_day = 29\n\n[agreement36]",
                ),
                "7行目",
            ),
            (
                BASE.replace("weekly_scheduled_days = 5", "weekly_scheduled_days = 8"),
                "19行目",
            ),
            (
                BASE.replace("attendance = []", "attendance = [\"meets\", \"maybe\"]"),
                "21行目",
            ),
            (
                BASE.replace(
                    "monthly_limit_minutes = 2700",
                    "monthly_limit_minutes = 2701",
                ),
                "13行目",
            ),
            (BASE.replace("annual_limit_minutes = 21600\n", ""), "9行目"),
            (BASE.replace("id = \"E001\"", "id = \"=E001\""), "17行目"),
        ];
        for (text, line) in cases {
            let (code, exit, message) = parse(&text).unwrap_err();
            assert_eq!((code, exit), ("InvalidInput", 2), "{text}");
            assert!(message.contains(line), "{line}: {message}\n{text}");
        }
    }

    /// AC-10.7 エラーの文に、入力の値や知らない項目の名前を出さない。
    #[test]
    fn ac10_7_messages_do_not_echo_the_input() {
        let marker = "SECRETMARK";
        let texts = [
            BASE.replace("[workplace]\n", &format!("[workplace]\n{marker} = 1\n")),
            BASE.replace(
                "work_system = \"standard\"",
                &format!("work_system = \"{marker}\""),
            ),
            BASE.replace(
                "hire_date = \"2024-04-01\"",
                &format!("hire_date = \"{marker}\""),
            ),
            BASE.replace("id = \"E001\"", &format!("id = \"{marker} x\"")),
            format!("{BASE}\n{marker} = = 1\n"),
            BASE.replace(
                "work_system = \"standard\"",
                &format!("work_system = '{marker}"),
            ),
        ];
        for text in texts {
            let failure = PolicyFile::parse(text.as_bytes())
                .and_then(|policy| policy.dates().map(|_| policy))
                .unwrap_err();
            assert!(!failure.message.contains(marker), "{}", failure.message);
        }
    }

    #[test]
    fn duplicate_ids_and_duplicate_keys_are_invalid() {
        let two = format!(
            "{BASE}\n[[employee]]\nid = \"E001\"\nhire_date = \"2024-04-01\"\nweekly_scheduled_days = 5\nweekly_scheduled_minutes = 2100\nattendance = []\n"
        );
        assert!(message(&two).contains("24行目"));
        let duplicated_key = BASE.replace(
            "month_start_day = 1\n\n[agreement36]",
            "month_start_day = 1\nmonth_start_day = 1\n\n[agreement36]",
        );
        assert!(message(&duplicated_key).contains("TOML として読めません"));
    }

    #[test]
    fn utf8_bom_is_accepted_and_bad_bytes_are_rejected() {
        let mut with_bom = b"\xEF\xBB\xBF".to_vec();
        with_bom.extend_from_slice(BASE.as_bytes());
        assert!(PolicyFile::parse(&with_bom).is_ok());
        let mut broken = BASE.as_bytes().to_vec();
        broken.extend_from_slice(b"# \xFF\n");
        let failure = PolicyFile::parse(&broken).unwrap_err();
        assert!(failure.message.contains("UTF-8"), "{}", failure.message);
    }

    /// AC-12.1 対応外: 表の順。作業場の `work_system` → 従業員の `worker_kind`（ID つき）→ ほかの指定 →
    /// `MonthStartDayMismatch`。範囲の誤りは、対応外より先に不正な入力になる。
    #[test]
    fn ac12_1_unsupported_codes_in_table_order() {
        let check = |text: &str| {
            let policy = parse(text).unwrap();
            let scope = policy.scope(None).unwrap();
            let failure = policy.check_unsupported(&scope).unwrap_err();
            (failure.code, failure.exit, failure.message)
        };
        let flex_and_minor = BASE
            .replace("work_system = \"standard\"", "work_system = \"flex\"")
            .replace("id = \"E001\"", "id = \"E001\"\nworker_kind = \"minor\"");
        assert_eq!(check(&flex_and_minor).0, "Flex");
        let minor_and_four_weeks = BASE
            .replace(
                "work_system = \"standard\"",
                "work_system = \"standard\"\nholiday_system = \"four_weeks\"",
            )
            .replace("id = \"E001\"", "id = \"E001\"\nworker_kind = \"minor\"");
        let (code, exit, minor_message) = check(&minor_and_four_weeks);
        assert_eq!((code, exit), ("Minor", 3));
        assert!(
            minor_message.contains("E001") && minor_message.contains("労働基準法第60条"),
            "{minor_message}"
        );
        let no_holiday = BASE.replace("legal_holiday_weekday = \"sun\"\n", "");
        assert_eq!(check(&no_holiday).0, "LegalHolidayUnspecified");
        let mismatch = BASE.replace(
            "month_start_day = 1\nmonthly_limit_minutes",
            "month_start_day = 16\nmonthly_limit_minutes",
        );
        assert_eq!(check(&mismatch).0, "MonthStartDayMismatch");
        // kind = none なら、協定の起算日は比べない（無くてもよい）。
        let none = BASE
            .replace("kind = \"general\"", "kind = \"none\"")
            .replace(
                "month_start_day = 1\nmonthly_limit_minutes",
                "month_start_day = 16\nmonthly_limit_minutes",
            );
        let policy = parse(&none).unwrap();
        assert!(
            policy
                .check_unsupported(&policy.scope(None).unwrap())
                .is_ok()
        );
        let missing = BASE.replace(
            "month_start_day = 1\nmonthly_limit_minutes",
            "monthly_limit_minutes",
        );
        assert!(message(&missing).contains("kind が none でないとき必須"));
        // 週44時間は、範囲の中だが確認なしは対応外。範囲外（2500）は不正な入力。
        let weekly44 = BASE.replace(
            "month_start_day = 1\n\n[agreement36]",
            "month_start_day = 1\nweekly_limit_minutes = 2640\n\n[agreement36]",
        );
        assert_eq!(check(&weekly44).0, "Weekly44WithoutConfirmation");
        let weekly_bad = BASE.replace(
            "month_start_day = 1\n\n[agreement36]",
            "month_start_day = 1\nweekly_limit_minutes = 2500\n\n[agreement36]",
        );
        assert_eq!(parse(&weekly_bad).unwrap_err().1, 2);
    }

    /// `--employee` で対象を絞ると、ほかの人の `worker_kind` は見ない（`docs/spec/cli.md` の「拒否」）。
    #[test]
    fn scope_limits_the_worker_kind_check() {
        let text = format!(
            "{BASE}\n[[employee]]\nid = \"E002\"\nworker_kind = \"driver\"\nhire_date = \"2024-04-01\"\nweekly_scheduled_days = 5\nweekly_scheduled_minutes = 2100\nattendance = []\n"
        );
        let policy = parse(&text).unwrap();
        let all = policy.scope(None).unwrap();
        assert_eq!(
            policy.check_unsupported(&all).unwrap_err().code,
            "AutomobileDriver"
        );
        let one = policy
            .scope(Some(&EmployeeId::new("E001").unwrap()))
            .unwrap();
        assert!(policy.check_unsupported(&one).is_ok());
        assert!(
            policy
                .scope(Some(&EmployeeId::new("E999").unwrap()))
                .is_err()
        );
    }

    /// 手順4: 日付の書式と範囲。
    #[test]
    fn dates_are_checked_in_step_4() {
        let bad = BASE.replace("hire_date = \"2024-04-01\"", "hire_date = \"1999-12-31\"");
        let policy = parse(&bad).unwrap();
        let failure = policy.dates().unwrap_err();
        assert_eq!(failure.code, "InvalidInput");
        assert!(failure.message.contains("18行目"), "{}", failure.message);
        let grants = BASE.replace(
            "attendance = []",
            "attendance = []\ngrants = [{date = \"2024-04-01\", halves = 20}]",
        );
        let dates = parse(&grants).unwrap().dates().unwrap();
        assert_eq!(
            dates
                .values()
                .next()
                .unwrap()
                .grants
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        let zero = BASE.replace(
            "attendance = []",
            "attendance = []\ngrants = [{date = \"2024-04-01\", halves = 0}]",
        );
        assert!(message(&zero).contains("halves は1〜200"));
    }
}
