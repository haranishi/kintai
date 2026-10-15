//! 複数のコマンドで使う出力の部品: 確定集計の拒否（`refused`）、打刻の異常、曖昧さの記録、休憩の不足。
//!
//! `refused` の形は、曖昧さ（`AmbiguousBoundary`）でも打刻の異常（`PunchAnomaly`）でも同じ
//! `{"code", "dates", "anomalies"}`。`dates` は重複のない昇順。`anomalies` は打刻の異常のときだけ中身がある
//! （種類・日付・打刻CSVの行番号。`docs/spec/cli.md` の「拒否」）。

use std::collections::BTreeSet;

use kintai_core::punch::AnomalyKind;
use kintai_jp_labor::breaks::BreakShortage;
use kintai_jp_labor::{Ambiguity, Refusal};
use serde::Serialize;

use crate::inputs::AnomalyRecord;

/// 打刻の異常による拒否のコード。
pub const PUNCH_ANOMALY: &str = "PunchAnomaly";

/// 確定集計の拒否。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RefusedOut {
    /// `AmbiguousBoundary` か `PunchAnomaly`。
    pub code: &'static str,
    /// 記録のある日付（重複なし、昇順）。
    pub dates: Vec<String>,
    /// 打刻の異常（時刻の順）。曖昧さの拒否では空。
    pub anomalies: Vec<AnomalyOut>,
}

/// 打刻の異常（1件）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AnomalyOut {
    /// 種類（`OpenShift` など）。
    pub kind: &'static str,
    /// 日付。
    pub date: String,
    /// 打刻CSVの行番号。
    pub line: Option<u64>,
}

/// 曖昧さの記録（1件。`kintai.classify.v1` の `ambiguities` と同じ形）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AmbiguityOut {
    /// 日付（労働日、または週の起算日）。
    pub date: String,
    /// コード。
    pub code: &'static str,
}

/// 休憩の不足の警告（1件）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BreakShortageOut {
    /// 労働日。
    pub date: String,
    /// 実労働（分）。
    pub work_minutes: i64,
    /// 実際の休憩（分）。
    pub break_minutes: i64,
    /// 必要な休憩（分）。
    pub required_minutes: i64,
    /// 不足（分）。
    pub shortage_minutes: i64,
}

impl From<&Ambiguity> for AmbiguityOut {
    fn from(record: &Ambiguity) -> AmbiguityOut {
        AmbiguityOut {
            date: record.date.to_string(),
            code: record.code.code(),
        }
    }
}

impl From<&BreakShortage> for BreakShortageOut {
    fn from(shortage: &BreakShortage) -> BreakShortageOut {
        BreakShortageOut {
            date: shortage.date.to_string(),
            work_minutes: shortage.work_minutes,
            break_minutes: shortage.break_minutes,
            required_minutes: shortage.required_minutes,
            shortage_minutes: shortage.shortage_minutes,
        }
    }
}

/// 曖昧さによる拒否。
#[must_use]
pub fn refused_ambiguous(refusal: &Refusal) -> RefusedOut {
    RefusedOut {
        code: refusal.code.code(),
        dates: refusal.dates.iter().map(ToString::to_string).collect(),
        anomalies: Vec::new(),
    }
}

/// 打刻の異常による拒否。
#[must_use]
pub fn refused_anomalies(anomalies: &[AnomalyRecord]) -> RefusedOut {
    let dates: BTreeSet<_> = anomalies.iter().map(|anomaly| anomaly.date).collect();
    RefusedOut {
        code: PUNCH_ANOMALY,
        dates: dates.iter().map(ToString::to_string).collect(),
        anomalies: anomalies
            .iter()
            .map(|anomaly| AnomalyOut {
                kind: anomaly.kind.code(),
                date: anomaly.date.to_string(),
                line: anomaly.line,
            })
            .collect(),
    }
}

/// 打刻の異常の説明（人が読む出力）。
#[must_use]
pub const fn anomaly_label(kind: AnomalyKind) -> &'static str {
    match kind {
        AnomalyKind::DuplicateIn => "勤務中か休憩中に、また in がある",
        AnomalyKind::OutWithoutIn => "対になる in のない out",
        AnomalyKind::BreakStartWithoutIn => "勤務中でないときの break_start",
        AnomalyKind::BreakEndWithoutStart => "対になる break_start のない break_end",
        AnomalyKind::OutDuringBreak => "休憩中の out",
        AnomalyKind::OpenShift => "勤務中のまま終わっている",
        AnomalyKind::SameTimestamp => "同じ時刻の打刻があり、順序が決まらない",
        AnomalyKind::LongShift => "24時間以上の勤務",
    }
}

fn anomaly_kind(code: &str) -> Option<AnomalyKind> {
    AnomalyKind::ALL
        .into_iter()
        .find(|kind| kind.code() == code)
}

/// 拒否の説明（人が読む出力の1行。従業員IDは呼ぶ側で前に付ける）。
#[must_use]
pub fn refusal_text(refused: &RefusedOut) -> String {
    if refused.code == PUNCH_ANOMALY {
        let items: Vec<String> = refused
            .anomalies
            .iter()
            .map(|anomaly| {
                let label = anomaly_kind(anomaly.kind).map_or("", anomaly_label);
                let line = anomaly
                    .line
                    .map_or_else(String::new, |line| format!("、打刻CSVの{line}行目"));
                format!("{} {}（{label}{line}）", anomaly.kind, anomaly.date)
            })
            .collect();
        format!(
            "確定集計を拒否（PunchAnomaly。打刻の異常: {}。直してから集計してください）",
            items.join("、")
        )
    } else {
        format!(
            "確定集計を拒否（{}。日付: {}。休日・週・月の境界をまたぐ勤務か、打刻がそろっていない週があり、方針の ambiguity が reject です）",
            refused.code,
            refused.dates.join("、")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{refusal_text, refused_anomalies};
    use crate::inputs::AnomalyRecord;
    use kintai_core::punch::AnomalyKind;

    #[test]
    fn anomaly_dates_are_unique_and_sorted() {
        let date = |text: &str| text.parse().unwrap();
        let refused = refused_anomalies(&[
            AnomalyRecord {
                kind: AnomalyKind::SameTimestamp,
                date: date("2026-10-05"),
                line: Some(9),
            },
            AnomalyRecord {
                kind: AnomalyKind::OpenShift,
                date: date("2026-10-03"),
                line: Some(4),
            },
            AnomalyRecord {
                kind: AnomalyKind::LongShift,
                date: date("2026-10-05"),
                line: Some(7),
            },
        ]);
        assert_eq!(refused.code, "PunchAnomaly");
        assert_eq!(refused.dates, vec!["2026-10-03", "2026-10-05"]);
        assert_eq!(refused.anomalies.len(), 3);
        let text = refusal_text(&refused);
        assert!(
            text.contains(
                "SameTimestamp 2026-10-05（同じ時刻の打刻があり、順序が決まらない、打刻CSVの9行目）"
            ),
            "{text}"
        );
    }
}
