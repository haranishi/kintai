//! 試験の共通の部品: ケースを一時ファイルに書き、実際のバイナリ `kintai` を動かす。
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 実行の結果。
#[derive(Debug, PartialEq, Eq)]
pub struct Run {
    pub stdout: String,
    pub stderr: String,
    pub exit: i32,
}

impl Run {
    /// 標準エラーの JSON の `error.code`。
    pub fn error_code(&self) -> String {
        let value: Value = serde_json::from_str(&self.stderr)
            .unwrap_or_else(|e| panic!("標準エラーが JSON でない（{e}）: {:?}", self.stderr));
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 1, "{value}");
        let error = value["error"].as_object().unwrap();
        assert_eq!(error.len(), 2, "{value}");
        assert!(error["message"].is_string());
        error["code"].as_str().unwrap().to_owned()
    }

    /// 標準出力の JSON。
    pub fn output(&self) -> Value {
        serde_json::from_str(&self.stdout).unwrap()
    }
}

pub fn kintai(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_kintai"))
        .args(args)
        .output()
        .unwrap();
    Run {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        exit: output.status.code().unwrap(),
    }
}

/// バイト列を一時ファイルに書いて `kintai case FILE` を動かす。
pub fn run_case_bytes(bytes: &[u8]) -> Run {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("kintai-case-{}-{n}.json", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let run = kintai(&["case", path.to_str().unwrap()]);
    std::fs::remove_file(&path).unwrap();
    run
}

pub fn run_case(case: &Value) -> Run {
    run_case_bytes(serde_json::to_string(case).unwrap().as_bytes())
}

/// 通常制の方針（必須項目だけ。法定休日は日曜）。
pub fn base_policy() -> Value {
    json!({
        "work_system": "standard",
        "scheduled_minutes_per_day": 480,
        "legal_holiday_weekday": "sun",
        "month_start_day": 1
    })
}

pub fn case_with(policy: Value, shifts: Value) -> Value {
    let mut case = serde_json::Map::new();
    case.insert("schema".to_owned(), json!("kintai.case.v1"));
    case.insert("policy".to_owned(), policy);
    case.insert("shifts".to_owned(), shifts);
    Value::Object(case)
}

/// 月曜 9:00〜17:00 の勤務1つ。
pub fn one_shift() -> Value {
    json!([{"start": "2026-10-05T09:00", "end": "2026-10-05T17:00"}])
}

pub fn valid_case() -> Value {
    case_with(base_policy(), one_shift())
}

/// 方針の項目を1つ差し替えた（`null` なら消した）ケース。
pub fn with_policy_item(key: &str, value: Value) -> Value {
    let mut policy = base_policy();
    if value.is_null() {
        policy.as_object_mut().unwrap().remove(key);
    } else {
        policy[key] = value;
    }
    case_with(policy, one_shift())
}

/// 公的な数値例のケース（id の順）。
pub fn official_cases() -> Vec<(String, Value)> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/spec/vectors/official-examples.json"
    );
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["id"].as_str().unwrap().to_owned(), v.clone()))
        .collect()
}

pub fn assert_invalid(case: &Value) {
    let run = run_case(case);
    assert_eq!(run.exit, 2, "{case}\n{run:?}");
    assert_eq!(run.error_code(), "InvalidInput", "{case}");
    assert_eq!(run.stdout, "", "{case}");
}

pub fn assert_unsupported(case: &Value, code: &str) {
    let run = run_case(case);
    assert_eq!(run.exit, 3, "{case}\n{run:?}");
    assert_eq!(run.error_code(), code, "{case}");
    assert_eq!(run.stdout, "", "{case}");
}

// ---- コマンドライン全体（第3周）の試験の部品 ----

/// 合成データ（リポジトリの `tests/fixtures/synthetic/`）の場所。
pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/synthetic")
        .join(name)
}

/// 合成データの中身。
pub fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).unwrap()
}

/// 試験ごとの新しい空のフォルダ（`CARGO_TARGET_TMPDIR` の下）。
pub fn temp_dir(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("kintai-{label}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// ファイルを書いて、その場所（文字列）を返す。
pub fn write_file(dir: &std::path::Path, name: &str, contents: impl AsRef<[u8]>) -> String {
    let path = dir.join(name);
    std::fs::write(&path, contents).unwrap();
    path.to_str().unwrap().to_owned()
}

/// 環境変数を足して `kintai` を動かす（`demo` の一時フォルダを試験の場所に向けるため）。
pub fn kintai_env(args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kintai"));
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().unwrap();
    Run {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        exit: output.status.code().unwrap(),
    }
}

/// 合成データの場所（文字列）。
pub fn fixture_path(name: &str) -> String {
    fixture(name).to_str().unwrap().to_owned()
}

/// 方針ファイルから、指定の従業員の `[[employee]]` だけを残す（合成データを小さくして試すため）。
pub fn policy_with_employees(ids: &[&str]) -> String {
    let text = fixture_text("policy.toml");
    let mut parts = text.split("\n# E00");
    let mut out = parts.next().unwrap().to_owned();
    out.push('\n');
    for part in parts {
        let id = format!("E00{}", part.chars().next().unwrap());
        if ids.contains(&id.as_str()) {
            out.push_str("# E00");
            out.push_str(part);
            out.push('\n');
        }
    }
    out
}

/// CSV から、指定の従業員の行だけを残す（見出しは残す）。
pub fn csv_with_employees(text: &str, ids: &[&str]) -> String {
    let mut lines = text.lines();
    let mut out = lines.next().unwrap().to_owned();
    out.push('\n');
    for line in lines {
        if ids.iter().any(|id| line.starts_with(&format!("{id},"))) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// 標準エラーが `{"error": {...}}` の1行で、標準出力が空であることを確かめ、コードを返す。
pub fn failed_code(run: &Run, exit: i32) -> String {
    assert_eq!(run.exit, exit, "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    assert!(
        run.stderr.ends_with('\n') && !run.stderr.trim_end().contains('\n'),
        "{run:?}"
    );
    run.error_code()
}
