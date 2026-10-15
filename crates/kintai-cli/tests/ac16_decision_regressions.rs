//! AC-16.1: public decisions become executable, synthetic data cases.
mod common;

use common::{fixture_path, kintai};
use serde_json::Value;

#[test]
fn ac16_1_public_decisions_replay_against_the_real_binary() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/contracts/decisions.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], "kintai.regressions.v1");
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    for case in corpus["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(case["spec"].as_str().unwrap())
                .is_file(),
            "{id}: missing spec"
        );
        let mut args = vec![
            "report",
            "--policy",
            &policy,
            "--punches",
            &punches,
            "--month",
            "2026-10",
            "--format",
            "json",
        ];
        args.extend(
            case["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|arg| arg.as_str().unwrap()),
        );
        let out = kintai(&args);
        assert_eq!(
            Some(i64::from(out.exit)),
            case["exit"].as_i64(),
            "{id}: {out:?}"
        );
        let document = if out.exit == 2 || out.exit == 3 {
            assert_eq!(out.stdout, "", "{id}");
            serde_json::from_str::<Value>(&out.stderr).unwrap()
        } else {
            assert_eq!(out.stderr, "", "{id}");
            out.output()
        };
        for (pointer, expected) in case["expected"].as_object().unwrap() {
            assert_eq!(document.pointer(pointer), Some(expected), "{id}: {pointer}");
        }
        if let Some(secret) = case["no_echo"].as_str() {
            assert!(!out.stderr.contains(secret), "{id}: input leaked");
        }
    }
}
