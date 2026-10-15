//! 子プロセスを用いた出力・終了コードと、一時リポジトリでの挙動の検証。
//! `sh`・`grep`・`git` を呼ぶので、Unix系(macOS・Linux)だけで流す。
#![cfg(unix)]
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

// 試験は同じプロセスの複数のスレッドで同時に動く。時刻だけで名前を決めると、
// macOSのように時計の細かさが粗い環境で同じ名前になり、片方の後始末がもう片方の
// フォルダを消してしまう。連番を足し、既にあれば失敗させて取り違えを防ぐ。
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(
            "xtask-test-{}-{stamp}-{serial}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, name: &str, content: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

// 子プロセスでこの試験だけを再実行し、公開関数の標準出力を取得する。
#[test]
fn fixture_driver() {
    let Some(root) = std::env::var_os("XTASK_TEST_ROOT") else {
        return;
    };
    let args: Vec<String> =
        serde_json::from_str(&std::env::var("XTASK_TEST_ARGS").unwrap()).unwrap();
    println!("XTASK-BEGIN");
    let temp_dir = std::env::var_os("XTASK_TEST_TEMP_DIR").unwrap();
    match xtask::run_with_temp_dir(Path::new(&root), &args, Path::new(&temp_dir)) {
        Ok(code) => println!("XTASK-END:{code}"),
        Err(error) => panic!("{error}"),
    }
}
fn rust(root: &Path, args: &[&str]) -> (String, i32) {
    rust_env(root, args, &[])
}
fn rust_env(root: &Path, args: &[&str], env: &[(&str, &Path)]) -> (String, i32) {
    let temp = Fixture::new();
    rust_env_with_temp(root, args, env, &temp.0)
}
fn rust_env_with_temp(
    root: &Path,
    args: &[&str],
    env: &[(&str, &Path)],
    temp_dir: &Path,
) -> (String, i32) {
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_driver", "--nocapture"])
        .envs(env.iter().copied())
        .env("XTASK_TEST_ROOT", root)
        .env("XTASK_TEST_TEMP_DIR", temp_dir)
        .env("XTASK_TEST_ARGS", serde_json::to_string(args).unwrap())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    let (_, body) = text.split_once("XTASK-BEGIN\n").unwrap();
    let (out, tail) = body.split_once("XTASK-END:").unwrap();
    (
        out.to_owned(),
        tail.lines().next().unwrap().parse().unwrap(),
    )
}
#[test]
fn links_git_and_fallback() {
    let f = Fixture::new();
    f.write("README.md", "[missing](absent#heading)\n```\n[skip](absent2)\n```\n[local](#x) [web](https://example.org)\n[private](research/notes_secret.md)\n");
    f.write("docs/long.md", &"line\n".repeat(101));
    f.write("docs/images/long.md", &"line\n".repeat(101));
    f.write("bad#name.txt", "");
    f.write("research/notes_secret.md", "[excluded](absent)\n");
    f.write(".gitignore", "research/notes_*.md\n");
    f.write("target/excluded.md", "[excluded](absent)\n");
    // gitがない場合は非公開メモとtargetを除外し、画像内文書は行数制限の対象外にする。
    assert_eq!(
        rust(&f.0, &["check-links"]),
        (
            concat!(
                "bad#name.txt: ファイル名に使えない文字があります\n",
                "README.md:1: リンク切れ absent#heading\n",
                "docs/long.md: 101行あります(上限100行)\n",
                "検査したMarkdown: 3件、問題: 3件\n",
            )
            .into(),
            1
        )
    );
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&f.0)
        .status()
        .unwrap();
    assert!(status.success());
    // gitの一覧ではtargetも検査し、無視されるメモへのリンクを報告する。
    assert_eq!(
        rust(&f.0, &["check-links"]),
        (
            concat!(
                "bad#name.txt: ファイル名に使えない文字があります\n",
                "README.md:1: リンク切れ absent#heading\n",
                "README.md:6: 公開されないファイルへのリンク research/notes_secret.md\n",
                "docs/long.md: 101行あります(上限100行)\n",
                "target/excluded.md:1: リンク切れ absent\n",
                "検査したMarkdown: 4件、問題: 5件\n",
            )
            .into(),
            1
        )
    );
}
#[test]
fn trace_missing_and_prefix() {
    let f = Fixture::new();
    let ac = "AC-";
    f.write(
        "docs/spec/a.md",
        &format!(
            "- {ac}12.1 [rust]\n- {ac}12.10 [python]\n- {ac}12.2 [manual]\n- {ac}12.3 [other]\n"
        ),
    );
    f.write(
        "crates/a/src/lib.rs",
        &format!("// {ac}12.10 {ac}12.10\n// {ac}99.1\n"),
    );
    f.write("tools/a.py", &format!("# {ac}12.10\n"));
    // 番号の前方一致を避け、同じ行の重複は一度だけ数える。manualは対象外にする。
    let expected = format!(
        concat!(
            "条件        種類  参照  内訳（crates/tools）  例\n",
            "{ac}12.1   rust     0  0/0                   （なし）\n",
            "{ac}12.10  python     2  1/1                   crates/a/src/lib.rs:1\n",
            "{ac}12.3   other     0  0/0                   （なし）\n",
            "\n条件 4件（manual 1件は対象外）。参照の無い条件: 1件\n",
            "  {ac}12.1\n",
            "仕様に無い番号への参照: {ac}99.1\n",
        ),
        ac = ac
    );
    assert_eq!(rust(&f.0, &["check-spec-trace"]), (expected.clone(), 0));
    // strictだけが未参照条件を終了コードに反映する。
    assert_eq!(rust(&f.0, &["check-spec-trace", "--strict"]), (expected, 1));
}
#[test]
fn mutation_outcomes_and_cleanup() {
    let f = Fixture::new();
    f.write("value", "original\n");
    let temp = Fixture::new();
    let cases = [
        (
            "count",
            "true",
            "absent",
            "x",
            1,
            "[基準] 変更なし: 終了コード=0（0であること）\n[count] 適用できない: 見つかった数=0, 期待=1\n\n== 生き残り ==\ncount（適用できない）\n",
        ),
        (
            "survive",
            "true",
            "original",
            "changed",
            1,
            "[基準] 変更なし: 終了コード=0（0であること）\n[survive] 生き残り ← 試験が赤くならなかった\n\n== 生き残り ==\nsurvive\n",
        ),
        (
            "detected",
            "grep -q original value || { echo 'test sample ... FAILED'; exit 1; }",
            "original",
            "changed",
            0,
            "[基準] 変更なし: 終了コード=0（0であること）\n[detected] 検出 ← 赤になった試験: sample\n\n== 生き残り ==\nなし\n",
        ),
        (
            "compile",
            "grep -q original value || { echo 'could not compile'; exit 1; }",
            "original",
            "changed",
            1,
            "[基準] 変更なし: 終了コード=0（0であること）\n[compile] 変異がコンパイルできない ← 検出に数えない。変異の書き方を直すこと\n\n== 生き残り ==\ncompile（コンパイルできない）\n",
        ),
        (
            "baseline",
            "exit 1",
            "original",
            "changed",
            2,
            "[基準] 変更なし: 終了コード=1（0であること）\n基準が赤いので、変異の結果は信用できません。\n",
        ),
        (
            "timeout",
            "grep -q original value || exec sleep 2",
            "original",
            "changed",
            0,
            "[基準] 変更なし: 終了コード=0（0であること）\n[timeout] 検出 ← 赤になった試験: 時間切れ\n\n== 生き残り ==\nなし\n",
        ),
    ];
    for (label, shell, find, replacement, code, expected) in cases {
        // 時間切れを確かめる場合だけ1秒にする。ほかは、機械が混んでいても誤って時間切れにならない長さ。
        let timeout_sec = if label == "timeout" { 1 } else { 60 };
        let spec = serde_json::json!({"test_cmd": ["sh", "-c", shell], "timeout_sec": timeout_sec,
            "mutations": [{"label": label,"file":"value","find":find,"replace":replacement}]});
        f.write("spec.json", &spec.to_string());
        let spec_path = f.0.join("spec.json");
        // 適用不能・生き残り・検出・コンパイル失敗・基準失敗・時間切れを全文で確認する。
        let actual =
            rust_env_with_temp(&f.0, &["mutate", spec_path.to_str().unwrap()], &[], &temp.0);
        assert_eq!(actual, (expected.into(), code), "{label}");
        assert_eq!(fs::read_to_string(f.0.join("value")).unwrap(), "original\n");
        assert!(fs::read_dir(&temp.0).unwrap().next().is_none());
        assert!(!f.0.join(".tmp").exists());
    }
    f.write(
        "spec.json",
        r#"{"test_cmd":["missing-command-xtask"],"mutations":[]}"#,
    );
    assert!(
        xtask::run_with_temp_dir(
            &f.0,
            &[
                "mutate".into(),
                f.0.join("spec.json").to_str().unwrap().into()
            ],
            &temp.0
        )
        .is_err()
    );
    assert!(fs::read_dir(&temp.0).unwrap().next().is_none());
}

#[test]
fn mutation_copy_exclusions_and_keep() {
    let f = Fixture::new();
    f.write("nested/skip/secret", "excluded");
    f.write("skip/secret", "excluded");
    f.write("ui/retained", "retained by override");
    f.write("value", "original");
    let spec = serde_json::json!({"exclude": ["skip"],
        "test_cmd": ["sh", "-c", "test ! -e nested/skip && test ! -e skip && test -f ui/retained"],
        "mutations": [{"label":"chosen", "file":"value", "find":"original", "replace":"changed"},
                      {"label":"other", "file":"missing", "find":"x", "replace":"y"}]});
    f.write("spec.json", &spec.to_string());
    let temp = Fixture::new();
    let result = rust_env_with_temp(
        &f.0,
        &[
            "mutate",
            f.0.join("spec.json").to_str().unwrap(),
            "--only",
            "chosen",
            "--keep",
        ],
        &[],
        &temp.0,
    );
    assert_eq!(result, ("[基準] 変更なし: 終了コード=0（0であること）\n[chosen] 生き残り ← 試験が赤くならなかった\n\n== 生き残り ==\nchosen\n".into(), 1));
    let bases = fs::read_dir(&temp.0)
        .unwrap()
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(bases.len(), 1);
    let work = bases[0].path().join("repo");
    assert_eq!(fs::read_to_string(work.join("value")).unwrap(), "original");
    assert!(!work.join("nested/skip").exists());
    assert!(work.join("ui/retained").exists());
}

fn executable(f: &Fixture, name: &str, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    f.write(name, text);
    fs::set_permissions(f.0.join(name), fs::Permissions::from_mode(0o755)).unwrap();
}
#[test]
fn pipelines_skip_optional_tools_and_use_metadata() {
    let f = Fixture::new();
    let home = f.0.join("home");
    let bin = home.join(".cargo/bin");
    fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink("/bin/sh", bin.join("sh")).unwrap();
    executable(&f, "home/.cargo/bin/git", "#!/bin/sh\nexit 1\n");
    executable(
        &f,
        "home/.cargo/bin/cargo",
        concat!(
            "#!/bin/sh\nprintf '%s\\n' \"cargo $*\" >> calls.txt\n",
            "if [ \"$1\" = metadata ]; then printf '{\"target_directory\":\"%s/alternate target\"}' \"$PWD\"; fi\n"
        ),
    );
    f.write("tools/difftest.py", "");
    let env = [("HOME", home.as_path()), ("PATH", bin.as_path())];
    let (text, code) = rust_env(&f.0, &["check-all", "--quick"], &env);
    assert_eq!(code, 0);
    assert!(text.contains("python3 が無いので省略"));
    assert!(text.contains("すべて通過しました。"));
    assert!(
        !fs::read_to_string(f.0.join("calls.txt"))
            .unwrap()
            .contains("metadata")
    );
    executable(
        &f,
        "home/.cargo/bin/python3",
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > python-args.txt\n",
    );
    assert_eq!(rust_env(&f.0, &["check-all", "--quick"], &env).1, 0);
    let args = fs::read_to_string(f.0.join("python-args.txt")).unwrap();
    assert!(args.contains("--bin\n"));
    assert!(args.contains(&format!(
        "{}/alternate target/debug/kintai\n",
        f.0.display()
    )));
    executable(&f, "home/.cargo/bin/cargo", "#!/bin/sh\nexit 1\n");
    let (text, code) = rust_env(&f.0, &["check-all"], &env);
    assert_eq!(code, 1);
    assert!(text.contains("最小の1.88が未導入"));
    assert!(
        text.contains("失敗: Rust: 書式\n失敗: Rust: clippy\n失敗: Rust: 試験\n失敗: Rust: 文書\n")
    );
    let (text, code) = rust_env(&f.0, &["bench", "3"], &env);
    assert_eq!(code, 1);
    assert!(text.contains("目標を超えたか、測れなかった項目があります。"));
}

#[test]
fn mutation_crlf_and_usage() {
    let f = Fixture::new();
    f.write("value", "original\r\n");
    let spec = serde_json::json!({"test_cmd": ["sh", "-c", "grep -q original value"],
        "mutations": [{"label":"newline", "file":"value", "find":"original\n", "replace":"changed\n"}]});
    f.write("spec.json", &spec.to_string());
    assert_eq!(
        rust(&f.0, &["mutate", f.0.join("spec.json").to_str().unwrap()]),
        ("[基準] 変更なし: 終了コード=0（0であること）\n[newline] 検出 ← 赤になった試験: (名前を取れなかった)\n\n== 生き残り ==\nなし\n".into(), 0)
    );
    assert_eq!(
        fs::read_to_string(f.0.join("value")).unwrap(),
        "original\r\n"
    );
    // 引数なしの説明と終了コードも固定する。
    let expected = "変異試験の道具: わざと誤りを入れて、試験が赤くなるかを確かめる。\n\n使い方:  cargo xtask mutate tools/mutations/<名前>.json [--only 文字列] [--keep]\n\n仕様のJSON:\n  {\n    \"test_cmd\": [\"cargo\", \"test\", \"-q\", \"-p\", \"kintai-jp-labor\", \"--no-fail-fast\"],\n    \"timeout_sec\": 900,\n    \"mutations\": [\n      {\"label\": \"按分: 四捨五入を切捨てに\", \"file\": \"crates/kintai-jp-labor/src/compose.rs\",\n       \"find\": \"+ 50) / 100\", \"replace\": \"/ 100\"}\n    ]\n  }\n\nリポジトリ全体をコピーした作業用の場所で試すので、本物のファイルは変わらない。\nファイルを戻すときは更新時刻を引き継がない(コピーして新しい時刻にする)。引き継ぐと、Cargoが「変更なし」と\n判断して前の変異を含むビルドを使い回し、結果が嘘になる。生き残り(試験が赤くならない変異)が\nあれば終了コード1。基準(変異なし)が赤い場合は、変異の結果を信用できないので終了コード2。\nコンパイルエラーは「検出」に数えず、直すべき変異として一覧に出す。\n";
    assert_eq!(rust(&f.0, &["mutate"]), (format!("{expected}\n"), 2));
    f.write(
        "spec.json",
        r#"{"test_cmd":["sh","-c","kill -TERM $$"],"mutations":[]}"#,
    );
    // シグナルによる基準の終了コードを負数で表示する。
    assert_eq!(rust(&f.0, &["mutate", f.0.join("spec.json").to_str().unwrap()]),
        ("[基準] 変更なし: 終了コード=-15（0であること）\n基準が赤いので、変異の結果は信用できません。\n".into(), 2));
}
