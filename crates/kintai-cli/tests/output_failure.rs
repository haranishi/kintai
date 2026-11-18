//! A report must not look successfully saved when stdout rejects the bytes.
#![cfg(target_os = "linux")]

mod common;

use std::fs::OpenOptions;
use std::process::Command;

/// AC-12.8: report output failures are observable to a calling shell or batch job.
#[test]
fn report_to_full_device_fails_with_a_redacted_error() {
    for format in ["text", "json", "csv"] {
        let full = OpenOptions::new().write(true).open("/dev/full").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_kintai"))
            .args([
                "report",
                "--policy",
                &common::fixture_path("policy.toml"),
                "--punches",
                &common::fixture_path("punches.csv"),
                "--month",
                "2026-10",
                "--employee",
                "E001",
                "--format",
                format,
            ])
            .stdout(full)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "format: {format}");
        let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], "InvalidInput");
        let message = error["error"]["message"].as_str().unwrap();
        assert!(message.contains("標準出力"));
        assert!(!message.contains("E001"));
    }
}
