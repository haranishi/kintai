//! The browser adapter preserves the calculation contract and protects its output file.
mod common;

use common::{fixture_path, kintai, temp_dir};
use serde_json::Value;
use std::fs;

fn review(dir: &std::path::Path, extra: &[&str]) -> common::Run {
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let out = dir.join("review.html");
    let mut args = vec![
        "review",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
        "--out",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    kintai(&args)
}

fn embedded(html: &str) -> Value {
    let data = html
        .split_once("id=\"report-data\">")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    serde_json::from_str(data).unwrap()
}

/// AC-14.1: the actual engine JSON, including refusals, is preserved without recalculation.
#[test]
fn ac14_1_html_preserves_the_original_report_and_refusal_exit() {
    let dir = temp_dir("review");
    let run = review(&dir, &[]);
    assert_eq!(run.exit, 4, "{run:?}");
    assert_eq!(run.stderr, "");
    let html = fs::read_to_string(dir.join("review.html")).unwrap();
    let original = kintai(&[
        "report",
        "--policy",
        &fixture_path("policy.toml"),
        "--punches",
        &fixture_path("punches.csv"),
        "--month",
        "2026-10",
        "--format",
        "json",
    ]);
    assert_eq!(embedded(&html), original.output());
    assert!(!html.contains("KINTAI_REPORT_JSON"));
    assert!(!html.contains("/* KINTAI_"));
    fs::remove_dir_all(dir).unwrap();
}

/// AC-14.1: employee scope is applied before HTML creation, not just hidden in the browser.
#[test]
fn scoped_review_does_not_embed_other_employees() {
    let dir = temp_dir("review-scope");
    assert_eq!(review(&dir, &["--employee", "E001"]).exit, 0);
    let html = fs::read_to_string(dir.join("review.html")).unwrap();
    let data = embedded(&html);
    assert_eq!(data["employees"].as_array().unwrap().len(), 1);
    assert_eq!(data["employees"][0]["employee_id"], "E001");
    assert!(!html.contains("E005"));
    fs::remove_dir_all(dir).unwrap();
}

/// AC-14.2: existing output is never truncated, including a symlink to an existing file.
#[test]
fn ac14_2_existing_output_is_preserved_and_error_redacts_the_path() {
    let dir = temp_dir("private-output-sentinel");
    let out = dir.join("review.html");
    fs::write(&out, "keep this content").unwrap();
    let run = review(&dir, &[]);
    assert_eq!(run.exit, 2);
    assert_eq!(run.error_code(), "InvalidInput");
    assert_eq!(fs::read_to_string(&out).unwrap(), "keep this content");
    assert!(!run.stderr.contains("private-output-sentinel"));
    assert_eq!(run.stdout, "");
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn ac14_2_new_output_is_private_and_symlinks_are_not_followed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = temp_dir("review-mode");
    assert_eq!(review(&dir, &[]).exit, 4);
    let out = dir.join("review.html");
    assert_eq!(
        fs::metadata(&out).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::remove_file(&out).unwrap();
    let target = dir.join("target.html");
    fs::write(&target, "untouched").unwrap();
    symlink(&target, &out).unwrap();
    assert_eq!(review(&dir, &[]).exit, 2);
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_arguments_do_not_create_an_output() {
    for extra in [
        vec!["--format", "json"],
        vec!["--employee", "PRIVATE_INPUT_SENTINEL"],
        vec!["--unknown"],
    ] {
        let dir = temp_dir("review-invalid");
        let run = review(&dir, &extra);
        assert_eq!(run.exit, 2, "{run:?}");
        assert!(!run.stderr.contains("PRIVATE_INPUT_SENTINEL"));
        assert!(!dir.join("review.html").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}

/// AC-14.3: the report uses an inert JSON block; JavaScript renders text with DOM APIs.
#[test]
fn ac14_3_document_is_offline_and_has_no_html_injection_sinks() {
    let dir = temp_dir("review-offline");
    review(&dir, &[]);
    let html = fs::read_to_string(dir.join("review.html")).unwrap();
    for forbidden in [
        "innerHTML",
        "document.write",
        "localStorage",
        "fetch(",
        "src=\"https:",
        "href=\"https:",
    ] {
        assert!(!html.contains(forbidden), "{forbidden}");
    }
    assert!(html.contains("connect-src 'none'"));
    fs::remove_dir_all(dir).unwrap();
}
