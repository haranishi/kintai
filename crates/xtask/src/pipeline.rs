use crate::{command, links, output, trace};
use std::{fs, path::Path};
fn execute(root: &Path, program: &str, args: &[&str]) -> bool {
    match command(root, program, args).status() {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("{program}: {error}");
            false
        }
    }
}
fn capture(root: &Path, program: &str, args: &[&str]) -> String {
    output(root, program, args)
        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_owned())
        .unwrap_or_default()
}
fn available(root: &Path, program: &str) -> bool {
    output(
        root,
        "sh",
        &["-c", "command -v \"$1\" >/dev/null", "sh", program],
    )
    .is_ok_and(|o| o.status.success())
}
pub(super) fn bench(root: &Path, args: &[String]) -> i32 {
    let runs = args.first().map_or("7", String::as_str);
    println!("== 測定の環境");
    println!("日時: {}", capture(root, "date", &["+%Y-%m-%d %H:%M %Z"]));
    println!("OS: {}", capture(root, "uname", &["-srm"]));
    if capture(root, "uname", &["-s"]) == "Darwin" {
        let cpu = capture(root, "sysctl", &["-n", "machdep.cpu.brand_string"]);
        let cores = capture(root, "sysctl", &["-n", "hw.ncpu"]);
        let memory = capture(root, "sysctl", &["-n", "hw.memsize"])
            .parse::<u64>()
            .unwrap_or(0)
            / 1_073_741_824;
        println!("機械: {cpu} / {cores} コア / {memory} GiB");
    } else if let Ok(info) = fs::read_to_string("/proc/cpuinfo") {
        let cpu = info
            .lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split_once(':'))
            .map_or("", |(_, v)| v.strip_prefix(' ').unwrap_or(v));
        println!("機械: {cpu} / {} コア", capture(root, "nproc", &[]));
    }
    println!("Rust: {}", capture(root, "rustc", &["--version"]));
    println!(
        "プロファイル: release（ワークスペースに [profile.release] がないので Cargo の既定。opt-level=3、lto=false、codegen-units=16、debug=false）"
    );
    let mut failed = false;
    println!("\n== 起動を測るための kintai の release ビルド");
    failed |= !execute(
        root,
        "cargo",
        &["build", "--release", "-q", "-p", "kintai-cli"],
    );
    println!(concat!(
        "\n== 区分の計算・36協定の年間判定・起動（AC-",
        "10.1・10.2）"
    ));
    failed |= !execute(
        root,
        "cargo",
        &[
            "run",
            "--release",
            "-q",
            "-p",
            "kintai-jp-labor",
            "--example",
            "bench_jp_labor",
            "--",
            "--runs",
            runs,
        ],
    );
    println!(concat!("\n== 打刻CSVの取込（AC-", "10.3）"));
    failed |= !execute(
        root,
        "cargo",
        &[
            "run",
            "--release",
            "-q",
            "-p",
            "kintai-import",
            "--example",
            "bench_import",
            "--",
            "--runs",
            runs,
        ],
    );
    println!(
        "\n{}",
        if failed {
            "目標を超えたか、測れなかった項目があります。"
        } else {
            "すべて目標以内でした。"
        }
    );
    i32::from(failed)
}
fn step(failed: &mut Vec<String>, name: &str, action: impl FnOnce() -> bool) {
    println!("\n== {name}");
    if action() {
        println!("   -> 通過");
    } else {
        println!("   -> 失敗");
        failed.push(name.to_owned());
    }
}
pub(super) fn check_all(root: &Path, quick: bool) -> i32 {
    let mut failed = Vec::new();
    for (name, args) in [
        ("Rust: 書式", vec!["fmt", "--all", "--", "--check"]),
        (
            "Rust: clippy",
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("Rust: 試験", vec!["test", "--workspace"]),
    ] {
        step(&mut failed, name, || execute(root, "cargo", &args));
    }
    step(&mut failed, "Rust: 文書", || {
        command(root, "cargo", &["doc", "--workspace", "--no-deps"])
            .env("RUSTDOCFLAGS", "-D warnings")
            .status()
            .is_ok_and(|s| s.success())
    });
    if !quick {
        if capture(root, "rustup", &["toolchain", "list"])
            .lines()
            .any(|l| l.starts_with("1.88"))
        {
            step(&mut failed, "Rust: 最小の1.88", || {
                execute(root, "cargo", &["+1.88", "check", "--workspace"])
            });
        } else {
            println!("(最小の1.88が未導入: rustup toolchain install 1.88 --profile minimal)");
        }
        if available(root, "cargo-deny") {
            step(&mut failed, "依存: cargo deny", || {
                execute(root, "cargo", &["deny", "check"])
            });
        } else {
            println!("(cargo-denyが未導入: cargo install cargo-deny)");
        }
    }
    step(&mut failed, "文書: リンクと行数", || {
        links::run(root).is_ok_and(|c| c == 0)
    });
    if root.join("docs/spec").is_dir() {
        step(&mut failed, "仕様: 条件と試験の対応", || {
            trace::run(root, false).is_ok_and(|c| c == 0)
        });
    }
    differential(root, &mut failed);
    if available(root, "uvx") {
        step(&mut failed, "ライセンス表示(REUSE)", || {
            execute(
                root,
                "uvx",
                &["--from", "reuse[charset-normalizer]", "reuse", "lint"],
            )
        });
    }
    println!("\n== まとめ");
    if failed.is_empty() {
        println!("すべて通過しました。");
    } else {
        for name in &failed {
            println!("失敗: {name}");
        }
    }
    i32::from(!failed.is_empty())
}

fn differential(root: &Path, failed: &mut Vec<String>) {
    if root.join("tools/difftest.py").is_file() {
        if available(root, "python3") {
            step(
                failed,
                "実行ファイルの組み立て(差分試験用)",
                || execute(root, "cargo", &["build", "-q", "-p", "kintai-cli"]),
            );
            step(failed, "別実装との差分試験(少数)", || {
                let metadata = output(
                    root,
                    "cargo",
                    &["metadata", "--format-version", "1", "--no-deps"],
                );
                let parsed = metadata
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());
                let target = parsed
                    .as_ref()
                    .and_then(|v| v.get("target_directory"))
                    .and_then(serde_json::Value::as_str);
                target.is_some_and(|target| {
                    execute(
                        root,
                        "python3",
                        &[
                            "tools/difftest.py",
                            "--cases",
                            "300",
                            "--seed",
                            "1",
                            "--bin",
                            &Path::new(target).join("debug/kintai").to_string_lossy(),
                        ],
                    )
                })
            });
        } else {
            println!("python3 が無いので省略");
        }
    }
}
