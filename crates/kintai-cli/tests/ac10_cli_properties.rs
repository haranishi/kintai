//! 壊した入力（方針ファイルの TOML・打刻・休暇・履歴の CSV・引数）でバイナリを動かす性質の試験（AC-10.5）。
//!
//! どんな入力でも、パニックやシグナルで終わらず、終了コードは 0〜4 のどれか。2・3 のときは標準出力が空で、
//! 標準エラーが `{"error": {"code", "message"}}` の1行。それ以外のときは標準エラーが空。
//! `demo` は書き込むので、引数の性質の試験には入れない（`ac12_demo_cli.rs` で試す）。

mod common;

use common::{Run, fixture_path, fixture_text, kintai, temp_dir, write_file};
use proptest::prelude::*;

/// 終了コードと出力の形が、決まりのとおりか。
fn check_shape(run: &Run) {
    match run.exit {
        0 | 1 | 4 => assert_eq!(run.stderr, "", "{run:?}"),
        2 | 3 => {
            assert_eq!(run.stdout, "", "{run:?}");
            let code = run.error_code();
            assert_eq!(run.exit == 2, code == "InvalidInput", "{code}");
        }
        other => panic!("決まりにない終了コード {other}: {run:?}"),
    }
}

/// バイト列の書き換え（置き換え・差し込み・削除）。
fn mutate(mut bytes: Vec<u8>, edits: &[(prop::sample::Index, u8, u8)]) -> Vec<u8> {
    for (index, byte, kind) in edits {
        let at = index.index(bytes.len().max(1)).min(bytes.len());
        match kind {
            0 => {
                if let Some(slot) = bytes.get_mut(at) {
                    *slot = *byte;
                }
            }
            1 => bytes.insert(at, *byte),
            _ => {
                if at < bytes.len() {
                    bytes.remove(at);
                }
            }
        }
    }
    bytes
}

fn edits() -> impl Strategy<Value = Vec<(prop::sample::Index, u8, u8)>> {
    prop::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0_u8..3), 1..6)
}

/// 書き換えるときに、よく効く文字（TOML と CSV の区切り・引用符・改行・数字）。
fn interesting_byte() -> impl Strategy<Value = u8> {
    prop::sample::select(b"=[]\"',\n\r#.:-0123456789 x\xff".to_vec())
}

fn edits_with_interesting_bytes() -> impl Strategy<Value = Vec<(prop::sample::Index, u8, u8)>> {
    prop::collection::vec(
        (any::<prop::sample::Index>(), interesting_byte(), 0_u8..3),
        1..6,
    )
}

fn report_with_policy_bytes(policy: &[u8]) -> Run {
    let dir = temp_dir("prop-policy");
    let policy = write_file(&dir, "policy.toml", policy);
    let punches = fixture_path("punches.csv");
    let run = kintai(&[
        "report",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
        "--format",
        "json",
    ]);
    let _ = std::fs::remove_dir_all(&dir);
    run
}

const VOCABULARY: [&str; 24] = [
    "report",
    "check",
    "leave",
    "explain",
    "retention",
    "case",
    "--policy",
    "--punches",
    "--leaves",
    "--history",
    "--month",
    "--as-of",
    "--date",
    "--employee",
    "--format",
    "--data-from",
    "--fail-on-violation",
    "--kind",
    "2026-10",
    "2026-10-08",
    "E001",
    "json",
    "csv",
    "wage-ledger",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// AC-10.5 任意のバイト列の方針ファイルは、不正な入力（パニックしない）。
    #[test]
    fn ac10_5_arbitrary_policy_bytes_are_invalid(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        let run = report_with_policy_bytes(&bytes);
        prop_assert_eq!(run.exit, 2, "{:?}", run);
        check_shape(&run);
    }

    /// AC-10.5 正しい方針ファイルを壊しても、決まった終了コードで終わる。
    #[test]
    fn ac10_5_mutated_policy_never_crashes(edits in edits_with_interesting_bytes()) {
        let bytes = mutate(fixture_text("policy.toml").into_bytes(), &edits);
        check_shape(&report_with_policy_bytes(&bytes));
    }

    /// AC-10.5 正しい打刻CSVを壊しても、決まった終了コードで終わる。
    #[test]
    fn ac10_5_mutated_punches_never_crash(edits in edits_with_interesting_bytes(), format in prop::sample::select(vec!["text", "json", "csv"])) {
        let dir = temp_dir("prop-punches");
        let punches = write_file(&dir, "punches.csv", mutate(fixture_text("punches.csv").into_bytes(), &edits));
        let policy = fixture_path("policy.toml");
        let run = kintai(&["report", "--policy", &policy, "--punches", &punches, "--month", "2026-10", "--format", format]);
        check_shape(&run);
        let explain = kintai(&["explain", "--policy", &policy, "--punches", &punches, "--employee", "E001", "--date", "2026-10-08"]);
        check_shape(&explain);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// AC-10.5 休暇CSVと履歴CSVを壊しても、決まった終了コードで終わる。
    #[test]
    fn ac10_5_mutated_leaves_and_history_never_crash(leave_edits in edits(), history_edits in edits_with_interesting_bytes()) {
        let dir = temp_dir("prop-csv");
        let leaves = write_file(&dir, "leaves.csv", mutate(fixture_text("leaves.csv").into_bytes(), &leave_edits));
        let history = write_file(&dir, "history.csv", mutate(fixture_text("history.csv").into_bytes(), &history_edits));
        let policy = fixture_path("policy.toml");
        let punches = fixture_path("punches.csv");
        check_shape(&kintai(&["leave", "--policy", &policy, "--leaves", &leaves, "--as-of", "2026-10-31"]));
        check_shape(&kintai(&[
            "check", "--policy", &policy, "--punches", &punches, "--month", "2026-10", "--history", &history,
            "--fail-on-violation",
        ]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// AC-10.5 引数を無作為に並べても、決まった終了コードで終わる（ファイルは合成データか、ないもの）。
    #[test]
    fn ac10_5_random_arguments_never_crash(
        words in prop::collection::vec(
            prop_oneof![
                prop::sample::select(VOCABULARY.to_vec()).prop_map(str::to_owned),
                prop::sample::select(vec!["policy.toml", "punches.csv", "leaves.csv", "history.csv", "missing.csv"])
                    .prop_map(fixture_path),
                "[ -~]{0,12}",
            ],
            0..10,
        ),
    ) {
        let args: Vec<&str> = words.iter().map(String::as_str).collect();
        prop_assume!(!args.contains(&"demo"));
        check_shape(&kintai(&args));
    }
}
