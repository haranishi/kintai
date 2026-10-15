//! 入力ファイル（打刻・休暇・履歴のCSV）の読み込みと、打刻から区分までのつなぎ。
//!
//! 打刻は、CSV（`kintai-import`）→ 打刻の対（`kintai_core::punch::pair`。異常の検出と秒の方針）→
//! 勤務（[`ShiftSet`]）→ 区分（[`classify()`]）の順に通す。打刻の異常は、渡された打刻の全体で見る
//! （範囲の外の異常でも、その従業員の確定集計を拒否する。`docs/spec/punch.md`）。
//!
//! CSV の従業員IDは、すべて方針ファイルの `[[employee]]` にあること（なければ不正な入力）。
//! エラーの文は、行番号と列名だけを示す（AC-10.7）。

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use kintai_core::punch::{
    AnomalyKind, EmployeeId, PairedShift, PairingOutcome, PunchError, PunchKind, pair,
};
use kintai_core::{Date, MonthPeriod, MonthStartDay};
use kintai_import::{ImportedPunches, read_history, read_leaves, read_limited, read_punches};
use kintai_jp_labor::leave::LeaveUnit;
use kintai_jp_labor::limits36::HistoryMonth;
use kintai_jp_labor::{Classification, ShiftInput, ShiftSet, ValidPolicy, classify};

use crate::failure::Failure;
use crate::policy_file::PolicyFile;

/// ファイルを読む（256MiB まで。`docs/spec/nfr.md`）。`what` はエラーの文に出すファイルの名前。
///
/// # Errors
///
/// 開けない・読めない・大きすぎるときは不正な入力（入出力の誤りの中身は出さない）。
pub fn read_input(path: &Path, what: &str) -> Result<Vec<u8>, Failure> {
    let file = File::open(path).map_err(|_| Failure::invalid(format!("{what}を開けません")))?;
    read_limited(file).map_err(|error| Failure::invalid(format!("{what}: {error}")))
}

/// 対象の月（`--month YYYY-MM`。2000-01〜2099-12）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct YearMonth {
    year: i32,
    month: u32,
}

impl YearMonth {
    /// `YYYY-MM` を読む。形が違う、月が1〜12の外、年が2000〜2099の外なら `None`。
    #[must_use]
    pub fn parse(text: &str) -> Option<YearMonth> {
        let bytes: [u8; 7] = text.as_bytes().try_into().ok()?;
        let [y1, y2, y3, y4, b'-', m1, m2] = bytes else {
            return None;
        };
        let digits = |items: &[u8]| -> Option<u32> {
            let mut value = 0_u32;
            for byte in items {
                if !byte.is_ascii_digit() {
                    return None;
                }
                value = value * 10 + u32::from(byte - b'0');
            }
            Some(value)
        };
        let year = i32::try_from(digits(&[y1, y2, y3, y4])?).ok()?;
        let month = digits(&[m1, m2])?;
        let in_range = (2000..=2099).contains(&year) && (1..=12).contains(&month);
        in_range.then_some(YearMonth { year, month })
    }

    /// その月の `start_day` に始まる月の期間（起算日が16なら、10月は10月16日〜11月15日）。
    #[must_use]
    pub fn period(self, start_day: MonthStartDay) -> MonthPeriod {
        // 起算日は1〜28なので、どの月にもある。年は2000〜2099なので、日付の型に収まる。
        match Date::from_ymd(self.year, self.month, start_day.get()) {
            Ok(first) => first.month_period(start_day),
            Err(_) => Date::INPUT_MIN.month_period(start_day),
        }
    }
}

impl std::fmt::Display for YearMonth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

/// 月の期間の最後の日（含む）。
#[must_use]
pub fn period_last_day(period: MonthPeriod) -> Option<Date> {
    Date::from_day_number(period.next_first_day_number().checked_sub(1)?).ok()
}

/// 打刻の異常（出力の1件）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnomalyRecord {
    /// 種類。
    pub kind: AnomalyKind,
    /// 日付（その打刻の日。`OpenShift` と `LongShift` は勤務の始業の日）。
    pub date: Date,
    /// 打刻CSVの行番号。
    pub line: Option<u64>,
}

/// 1人分の、打刻から区分までの結果。
#[derive(Clone, Debug)]
pub enum Work {
    /// 打刻に異常があり、確定集計を拒否した（`refused.code` は `PunchAnomaly`）。異常は時刻の順。
    Anomalies(Vec<AnomalyRecord>),
    /// 勤務に対にして、区分した（曖昧さによる拒否は `classification.refused`）。
    Classified {
        /// 勤務。
        shifts: ShiftSet,
        /// 区分の結果。
        classification: Classification,
    },
}

/// 読み込んで対にした打刻。
#[derive(Clone, Debug)]
pub struct Punches {
    imported: ImportedPunches,
    pairings: BTreeMap<EmployeeId, PairingOutcome>,
    first_date: Option<Date>,
}

/// 打刻の対の誤りを、行番号つきの不正な入力にする。
fn punch_error(error: PunchError, imported: &ImportedPunches) -> Failure {
    let at = |index: usize| {
        imported
            .line(index)
            .map_or_else(String::new, |line| format!("{line}行目"))
    };
    let message = match error {
        PunchError::NonZeroSeconds { index } => format!(
            "{}の列 timestamp: 秒が0ではありません（方針の seconds が reject です）",
            at(index)
        ),
        PunchError::TooManyShifts { index } => format!(
            "{}: 1人が1つの暦の月に始める勤務が{}件を超えています（入力の上限）",
            at(index),
            kintai_core::punch::MAX_SHIFTS_PER_MONTH
        ),
        PunchError::OutOfRange { index } => format!(
            "{}の列 timestamp: 秒を分にすると、日時が範囲の外になります",
            at(index)
        ),
        PunchError::InvalidEmployeeId => "従業員IDの形が違います".to_owned(),
    };
    Failure::invalid(format!("打刻CSVの{message}"))
}

/// 勤務に変える（打刻の対の結果は、区分の入力の条件を満たす）。
fn shift_inputs(shifts: &[PairedShift]) -> Vec<ShiftInput> {
    shifts
        .iter()
        .map(|shift| ShiftInput {
            start: shift.span().start(),
            end: shift.span().end(),
            breaks: shift
                .breaks()
                .iter()
                .map(|rest| (rest.start(), rest.end()))
                .collect(),
        })
        .collect()
}

impl Punches {
    /// 打刻CSVを読み、従業員IDを方針と照らし、従業員ごとに対にする。
    ///
    /// # Errors
    ///
    /// CSV の誤り（行番号と列名つき）、方針にない従業員ID、秒の方針が `reject` で秒のある打刻、
    /// 1人1か月1000件を超える勤務は、不正な入力。
    pub fn load(path: &Path, policy: &PolicyFile) -> Result<Punches, Failure> {
        let bytes = read_input(path, "打刻CSV")?;
        let imported =
            read_punches(&bytes).map_err(|error| Failure::invalid(format!("打刻CSVの{error}")))?;
        if let Some(index) = imported
            .punches
            .iter()
            .position(|punch| !policy.contains(&punch.employee))
        {
            return Err(not_in_policy("打刻CSV", imported.line(index)));
        }
        let pairings = pair(&imported.punches, policy.seconds())
            .map_err(|error| punch_error(error, &imported))?
            .into_iter()
            .map(|pairing| (pairing.employee, pairing.outcome))
            .collect();
        let first_date = imported
            .punches
            .iter()
            .map(|punch| punch.time.floor().date())
            .min();
        Ok(Punches {
            imported,
            pairings,
            first_date,
        })
    }

    /// 打刻CSV全体の最初の日付（`--data-from` の既定）。打刻がなければ `None`。
    #[must_use]
    pub const fn first_date(&self) -> Option<Date> {
        self.first_date
    }

    /// 1人分を区分する。打刻がない従業員は、勤務なしとして区分する。
    ///
    /// # Errors
    ///
    /// 打刻から組み立てた勤務が日時の範囲の外になったとき（2099-12-31 23:59:30 の終業の切り上げなど）は
    /// 不正な入力。
    pub fn work(
        &self,
        policy: &ValidPolicy,
        employee: &EmployeeId,
        data_from: Option<Date>,
    ) -> Result<Work, Failure> {
        let shifts = match self.pairings.get(employee) {
            Some(PairingOutcome::Refused(anomalies)) => {
                return Ok(Work::Anomalies(
                    anomalies
                        .iter()
                        .map(|anomaly| AnomalyRecord {
                            kind: anomaly.kind,
                            date: anomaly.time.floor().date(),
                            line: self.imported.line(anomaly.index),
                        })
                        .collect(),
                ));
            }
            Some(PairingOutcome::Paired(shifts)) => shift_inputs(shifts),
            None => Vec::new(),
        };
        let shifts = ShiftSet::new(&shifts).map_err(|_| self.out_of_range(employee))?;
        let classification = classify(policy, &shifts, data_from)
            .map_err(|error| Failure::invalid(error.to_string()))?;
        Ok(Work::Classified {
            shifts,
            classification,
        })
    }
}

impl Punches {
    /// 打刻から組み立てた勤務が日時の範囲の外になったときの誤り。従業員IDは文に出さず（AC-10.7・L2-02）、
    /// 秒を分に切り上げると範囲の外（2100-01-01T00:00）になる打刻（2099-12-31 23:59 台の `out` か
    /// `break_start`）の行番号を示す。
    fn out_of_range(&self, employee: &EmployeeId) -> Failure {
        let line = self
            .imported
            .punches
            .iter()
            .position(|punch| {
                &punch.employee == employee
                    && matches!(punch.kind, PunchKind::Out | PunchKind::BreakStart)
                    && punch
                        .time
                        .ceil()
                        .is_ok_and(|minute| !minute.is_input_range())
            })
            .and_then(|index| self.imported.line(index));
        let at = line.map_or_else(String::new, |line| format!("の{line}行目の列 timestamp"));
        Failure::invalid(format!(
            "打刻CSV{at}: 秒を分に切り上げると、打刻から組み立てた勤務の日時が範囲（2000-01-01T00:00〜2099-12-31T23:59）の外になります"
        ))
    }
}

fn not_in_policy(what: &str, line: Option<u64>) -> Failure {
    let at = line.map_or_else(String::new, |line| format!("{line}行目の"));
    Failure::invalid(format!(
        "{what}の{at}列 employee_id: 方針ファイルの [[employee]] にない従業員IDです"
    ))
}

/// 36協定の履歴（`--history`。`employee_id,month_start,total_minutes`）。従業員ごと、ファイルの順。
#[derive(Clone, Debug, Default)]
pub struct History {
    by_employee: BTreeMap<EmployeeId, Vec<(u64, HistoryMonth)>>,
}

impl History {
    /// 履歴CSVを読み、従業員IDを方針と照らす。
    ///
    /// # Errors
    ///
    /// CSV の誤り（行番号と列名つき）と、方針にない従業員IDは不正な入力。
    pub fn load(path: &Path, policy: &PolicyFile) -> Result<History, Failure> {
        let bytes = read_input(path, "履歴CSV")?;
        let rows =
            read_history(&bytes).map_err(|error| Failure::invalid(format!("履歴CSVの{error}")))?;
        let mut by_employee: BTreeMap<EmployeeId, Vec<(u64, HistoryMonth)>> = BTreeMap::new();
        for row in rows {
            if !policy.contains(&row.employee) {
                return Err(not_in_policy("履歴CSV", Some(row.line)));
            }
            by_employee.entry(row.employee).or_default().push((
                row.line,
                HistoryMonth {
                    start: row.month_start,
                    total: row.total_minutes,
                },
            ));
        }
        Ok(History { by_employee })
    }

    /// 1人分の履歴（行番号つき、ファイルの順）。
    #[must_use]
    pub fn of(&self, employee: &EmployeeId) -> &[(u64, HistoryMonth)] {
        self.by_employee.get(employee).map_or(&[], Vec::as_slice)
    }
}

/// 年休の取得の記録（`--leaves`。`employee_id,date,unit`）。従業員ごと、ファイルの順。
#[derive(Clone, Debug, Default)]
pub struct Leaves {
    by_employee: BTreeMap<EmployeeId, Vec<(u64, Date, LeaveUnit)>>,
}

impl Leaves {
    /// 休暇CSVを読み、従業員IDを方針と照らす。`hours`（時間単位）は値として読む（拒否は使う側）。
    ///
    /// # Errors
    ///
    /// CSV の誤り（行番号と列名つき）と、方針にない従業員IDは不正な入力。
    pub fn load(path: &Path, policy: &PolicyFile) -> Result<Leaves, Failure> {
        let bytes = read_input(path, "休暇CSV")?;
        let rows =
            read_leaves(&bytes).map_err(|error| Failure::invalid(format!("休暇CSVの{error}")))?;
        let mut by_employee: BTreeMap<EmployeeId, Vec<(u64, Date, LeaveUnit)>> = BTreeMap::new();
        for row in rows {
            if !policy.contains(&row.employee) {
                return Err(not_in_policy("休暇CSV", Some(row.line)));
            }
            let unit = match row.unit {
                kintai_import::LeaveUnit::Full => LeaveUnit::Full,
                kintai_import::LeaveUnit::Half => LeaveUnit::Half,
                kintai_import::LeaveUnit::Hours => LeaveUnit::Hours,
            };
            by_employee
                .entry(row.employee)
                .or_default()
                .push((row.line, row.date, unit));
        }
        Ok(Leaves { by_employee })
    }

    /// 1人分の取得（行番号つき、ファイルの順）。
    #[must_use]
    pub fn of(&self, employee: &EmployeeId) -> &[(u64, Date, LeaveUnit)] {
        self.by_employee.get(employee).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::{YearMonth, period_last_day};
    use kintai_core::MonthStartDay;

    /// `--month` の境界（年は2000〜2099、月は01〜12、形は `YYYY-MM` だけ）。
    #[test]
    fn year_month_boundaries() {
        assert!(YearMonth::parse("2000-01").is_some());
        assert!(YearMonth::parse("2099-12").is_some());
        for bad in [
            "1999-12",
            "2100-01",
            "2026-00",
            "2026-13",
            "2026-1",
            "2026/10",
            "２０２６-10",
            "2026-10-01",
            "",
            "+026-10",
            "2026-1a",
        ] {
            assert!(YearMonth::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(YearMonth::parse("2026-10").unwrap().to_string(), "2026-10");
    }

    /// 起算日が16なら、10月の期間は10月16日〜11月15日。12月は年をまたぐ。
    #[test]
    fn month_periods_follow_the_start_day() {
        let october = YearMonth::parse("2026-10").unwrap();
        let first = october.period(MonthStartDay::FIRST);
        assert_eq!(first.to_string(), "2026-10-01");
        assert_eq!(period_last_day(first).unwrap().to_string(), "2026-10-31");
        let sixteenth = october.period(MonthStartDay::new(16).unwrap());
        assert_eq!(sixteenth.to_string(), "2026-10-16");
        assert_eq!(
            period_last_day(sixteenth).unwrap().to_string(),
            "2026-11-15"
        );
        let december = YearMonth::parse("2099-12")
            .unwrap()
            .period(MonthStartDay::new(28).unwrap());
        assert_eq!(period_last_day(december).unwrap().to_string(), "2100-01-27");
    }
}
