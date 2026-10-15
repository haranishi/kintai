//! `kintai demo`（AC-12.7・AC-10.8）の試験。
//!
//! `--out` を省いた場合の一時フォルダは、環境変数 `TMPDIR` を試験用のフォルダに向けて確かめる
//! （試験が、作業場所の外に書かないように）。

mod common;

use std::path::Path;

use common::{failed_code, fixture, kintai, kintai_env, temp_dir};

const FILES: [&str; 4] = ["history.csv", "leaves.csv", "policy.toml", "punches.csv"];

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn assert_demo_output(stdout: &str, dir: &Path) {
    assert!(stdout.contains(dir.to_str().unwrap()), "{stdout}");
    for command in [
        "$ kintai report --policy policy.toml --punches punches.csv --month 2026-10\n",
        "--format csv\n",
        "--format json\n",
        "$ kintai check --policy policy.toml --punches punches.csv --month 2026-10 --history history.csv\n",
        "$ kintai leave --policy policy.toml --leaves leaves.csv --as-of 2026-10-31\n",
        "$ kintai explain --policy policy.toml --punches punches.csv --employee E001 --date 2026-10-08\n",
        "$ kintai retention --kind wage-ledger --completed 2026-10-31 --wage-due 2026-11-25\n",
    ] {
        assert!(stdout.contains(command), "{command}\n{stdout}");
    }
    for title in [
        "月次の集計（kintai report）",
        "36協定の判定・休憩の不足・打刻の異常（kintai check）",
        "年次有給休暇（kintai leave）",
        "区間ごとの区分と割増の下限（kintai explain）",
        "保存の期限の目安（kintai retention）",
        "employee_id,period_start,work_days",
        "{\"schema\":\"kintai.report.v1\"",
    ] {
        assert!(stdout.contains(title), "{title}");
    }
    // 合成データに含めた場面が、出力に現れる。
    for scene in [
        "E005: 確定集計を拒否（AmbiguousBoundary",
        "MultiMonthSumExceeded(n=2)",
        "E003 2026-10-14: 実労働540分に対して休憩が0分です",
        "2024-10-01  10日  2026-10-01  6.5日   0日  3.5日",
        "国民の祝日: スポーツの日",
        "statutory_current: 2029-11-25",
    ] {
        assert!(stdout.contains(scene), "{scene}");
    }
    assert!(stdout.contains("（終了コード 4:"), "拒否の例の終了コード");
    assert!(stdout.contains("（終了コード 0: 成功）"));
    // AC-12.10・L2-12 デモの出力も、最後の1行は参考値の1行。
    assert_eq!(
        stdout.lines().last(),
        Some("この出力は参考値です（専門家の確認前。review_status: unreviewed）。")
    );
}

/// AC-12.7 `--out` のフォルダだけに書き、すべてのコマンドの出力が揃う。書いた中身は合成データと同じ。
#[test]
fn ac12_7_demo_writes_only_to_out_and_shows_every_command() {
    let base = temp_dir("demo-out");
    let out = base.join("new");
    let run = kintai(&["demo", "--out", out.to_str().unwrap()]);
    assert_eq!(run.exit, 0, "{run:?}");
    assert_eq!(run.stderr, "");
    assert_eq!(names(&out), FILES);
    assert_eq!(names(&base), ["new"], "--out の外には書かない");
    for name in FILES {
        assert_eq!(
            std::fs::read(out.join(name)).unwrap(),
            std::fs::read(fixture(name)).unwrap(),
            "{name} は tests/fixtures/synthetic と同じ"
        );
    }
    assert_demo_output(&run.stdout, &out);
}

/// AC-10.8 作るフォルダは0700、ファイルは0600（Unix系）。
#[cfg(unix)]
#[test]
fn ac10_8_demo_creates_private_folder_and_files() {
    use std::os::unix::fs::PermissionsExt;
    let out = temp_dir("demo-perm").join("private");
    let run = kintai(&["demo", "--out", out.to_str().unwrap()]);
    assert_eq!(run.exit, 0, "{run:?}");
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&out), 0o700);
    for name in FILES {
        assert_eq!(mode(&out.join(name)), 0o600, "{name}");
    }
}

/// AC-12.7 `--out` を省くと、新しい一時フォルダに書き、そのパスを示す（ほかには書かない）。
#[test]
fn ac12_7_demo_without_out_uses_a_new_temporary_folder() {
    let tmp = temp_dir("demo-tmp");
    let run = kintai_env(&["demo"], &[("TMPDIR", tmp.to_str().unwrap())]);
    assert_eq!(run.exit, 0, "{run:?}");
    let created = names(&tmp);
    assert_eq!(created.len(), 1, "{created:?}");
    assert!(created[0].starts_with("kintai-demo-"), "{created:?}");
    let dir = tmp.join(&created[0]);
    assert_eq!(names(&dir), FILES);
    assert_demo_output(&run.stdout, &dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    // 2回目は別のフォルダ（上書きしない）。
    let again = kintai_env(&["demo"], &[("TMPDIR", tmp.to_str().unwrap())]);
    assert_eq!(again.exit, 0);
    assert_eq!(names(&tmp).len(), 2);
}

/// `--out` は、存在しないか空のフォルダだけ。ファイルや、中身のあるフォルダは不正な入力（何も書かない）。
#[test]
fn demo_refuses_non_empty_folders_and_files() {
    let base = temp_dir("demo-refuse");
    let full = base.join("full");
    std::fs::create_dir(&full).unwrap();
    std::fs::write(full.join("keep.txt"), b"x").unwrap();
    let run = kintai(&["demo", "--out", full.to_str().unwrap()]);
    assert_eq!(failed_code(&run, 2), "InvalidInput");
    assert_eq!(names(&full), ["keep.txt"]);
    let file = base.join("file.txt");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(
        failed_code(&kintai(&["demo", "--out", file.to_str().unwrap()]), 2),
        "InvalidInput"
    );
    let missing_parent = base.join("no").join("such");
    assert_eq!(
        failed_code(
            &kintai(&["demo", "--out", missing_parent.to_str().unwrap()]),
            2
        ),
        "InvalidInput"
    );
    let empty = base.join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert_eq!(kintai(&["demo", "--out", empty.to_str().unwrap()]).exit, 0);
    assert_eq!(names(&empty), FILES);
    assert_eq!(failed_code(&kintai(&["demo", "--out"]), 2), "InvalidInput");
    assert_eq!(failed_code(&kintai(&["demo", "extra"]), 2), "InvalidInput");
}

/// AC-10.4 デモの出力も、書く場所の表示を除けば毎回同じ。
#[test]
fn ac10_4_demo_output_is_deterministic() {
    let base = temp_dir("demo-twice");
    let first_dir = base.join("a");
    let second_dir = base.join("b");
    let first = kintai(&["demo", "--out", first_dir.to_str().unwrap()]);
    let second = kintai(&["demo", "--out", second_dir.to_str().unwrap()]);
    let normalize = |text: &str, dir: &Path| text.replace(dir.to_str().unwrap(), "<DIR>");
    assert_eq!(
        normalize(&first.stdout, &first_dir),
        normalize(&second.stdout, &second_dir)
    );
}
