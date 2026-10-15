//! `kintai case FILE`: 区分のケースを読み、結果の JSON を出す。
//!
//! 検査は `docs/spec/validation.md` の順（最初に当たったものを返す）。
//! 1. JSON として読めない、同じ名前の項目が2回ある（[`crate::strict_json`]）、`schema` が違う、
//!    型が違う、知らない項目がある、必須の項目がない（[`crate::dto::read_case`]）
//! 2. 列挙の値が定義にない（[`crate::dto::RawPolicy::to_work_policy`]）、数値が範囲外（[`WorkPolicy::validate`]）
//! 3. 対応外の指定（[`WorkPolicy::validate`]）
//! 4. 勤務と日付の検査（日時・日付の表記と範囲（`data_from` を含む）、開始と終了、重なり、休憩、長さ）
//! 5. 計算。曖昧さがあれば、`reject` で終了コード4（出力に `refused`）

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::dto::{ClassifyOut, RawShift, read_case};
use crate::failure::Failure;
use crate::strict_json::StrictValue;
use kintai_core::{Date, Instant};
use kintai_jp_labor::{PolicyError, ShiftInput, ShiftSet, WorkPolicy, classify};

/// 入力ファイルの大きさの上限（`docs/spec/nfr.md`）。
pub const MAX_INPUT_BYTES: u64 = 256 * 1024 * 1024;

/// 成功の終了コード。
pub const EXIT_OK: u8 = 0;
/// 曖昧さによる確定集計の拒否の終了コード。
pub const EXIT_REFUSED: u8 = 4;

/// 標準出力に出す1行（末尾の改行を含む）と、終了コード（0 か 4）。
#[derive(Debug, PartialEq, Eq)]
pub struct CaseOutput {
    /// 結果の JSON。
    pub stdout: String,
    /// 終了コード。
    pub exit: u8,
}

/// ファイルを読む（上限を超えたら不正な入力）。
fn read_file(path: &Path) -> Result<Vec<u8>, Failure> {
    let file = File::open(path).map_err(|_| Failure::invalid("ファイルを開けません"))?;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::invalid("ファイルを読めません"))?;
    if u64::try_from(bytes.len()).map_or(true, |len| len > MAX_INPUT_BYTES) {
        return Err(Failure::invalid("ファイルが256MBを超えています"));
    }
    Ok(bytes)
}

/// `kintai case FILE` の本体。
pub fn run_file(path: &Path) -> Result<CaseOutput, Failure> {
    let bytes = read_file(path)?;
    run_bytes(&bytes)
}

fn instant(text: &str, path: &str) -> Result<Instant, Failure> {
    text.parse()
        .map_err(|error| Failure::invalid(format!("{path}: {error}")))
}

/// 手順4のうち、日時の表記と範囲を読む。
fn shift_inputs(raw: &[RawShift<'_>]) -> Result<Vec<ShiftInput>, Failure> {
    raw.iter()
        .enumerate()
        .map(|(index, shift)| {
            let path = format!("shifts[{index}]");
            let breaks = shift
                .breaks
                .iter()
                .enumerate()
                .map(|(i, (start, end))| {
                    Ok((
                        instant(start, &format!("{path}.breaks[{i}][0]"))?,
                        instant(end, &format!("{path}.breaks[{i}][1]"))?,
                    ))
                })
                .collect::<Result<Vec<_>, Failure>>()?;
            Ok(ShiftInput {
                start: instant(shift.start, &format!("{path}.start"))?,
                end: instant(shift.end, &format!("{path}.end"))?,
                breaks,
            })
        })
        .collect()
}

fn validate(policy: &WorkPolicy) -> Result<kintai_jp_labor::ValidPolicy, Failure> {
    policy.validate().map_err(|error| match error {
        PolicyError::Unsupported(code) => Failure::unsupported(code),
        PolicyError::Invalid { .. } => Failure::invalid(error.to_string()),
    })
}

/// 入力のバイト列から、結果の JSON を作る。
pub fn run_bytes(bytes: &[u8]) -> Result<CaseOutput, Failure> {
    // 手順1（同じ名前の項目が2回あるものも、ここで不正な入力にする）
    let StrictValue(document) = serde_json::from_slice(bytes).map_err(|error| {
        Failure::invalid(format!(
            "JSON として読めません（{}行{}列）",
            error.line(),
            error.column()
        ))
    })?;
    // `schema` による振り分け。36協定と年休のケースは別のモジュールが読む（検査の流儀は同じ）。
    match document.get("schema").and_then(serde_json::Value::as_str) {
        Some("kintai.case36.v1") => return crate::case36::run(&document),
        Some("kintai.caseleave.v1") => return crate::case_leave::run(&document),
        _ => {}
    }
    let raw = read_case(&document)?;
    // 手順2・3
    let policy = validate(&raw.policy.to_work_policy()?)?;
    // 手順4
    let inputs = shift_inputs(&raw.shifts)?;
    let data_from = raw
        .data_from
        .map(|text| {
            text.parse::<Date>()
                .map_err(|error| Failure::invalid(format!("data_from: {error}")))
        })
        .transpose()?;
    let shifts = ShiftSet::new(&inputs).map_err(|error| Failure::invalid(error.to_string()))?;
    // 手順5
    let result = classify(&policy, &shifts, data_from)
        .map_err(|error| Failure::invalid(error.to_string()))?;
    let mut stdout = serde_json::to_string(&ClassifyOut::from(&result))
        .map_err(|_| Failure::invalid("結果を JSON にできません"))?;
    stdout.push('\n');
    let exit = if result.refused.is_some() {
        EXIT_REFUSED
    } else {
        EXIT_OK
    };
    Ok(CaseOutput { stdout, exit })
}

#[cfg(test)]
mod tests {
    use super::run_bytes;

    #[test]
    fn broken_json_is_invalid_input() {
        let failure = run_bytes(b"{\"schema\": ").unwrap_err();
        assert_eq!((failure.code, failure.exit), ("InvalidInput", 2));
        assert!(failure.message.contains("行"));
    }
}
