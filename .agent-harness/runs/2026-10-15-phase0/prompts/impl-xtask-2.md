# 実装指示書 xtask その2: Pythonとの比較を試験から外し、一時フォルダをシステムの場所にする

あなたは実装担当です。前の作業（`prompts/impl-xtask.md`、報告書 `xtask-report.md`）で作った `crates/xtask` を直します。作業場所は `__WS__`（以下 `$WS`）です。前と同じ約束を守る: `$WS` の外に書かない、ネットワークなし、新しい依存を足さない（`serde` と `serde_json` だけ）、`$WS` で git の操作をしない、`unsafe` を使わない、`crates/xtask/**` 以外は触らない。環境は前と同じ（`CARGO_HOME=$WS/.cargo-home` `CARGO_NET_OFFLINE=true` `CARGO_TARGET_DIR=$WS/.cargo-target`）。

## 背景

調整役が `crates/xtask` を本体へ取り込み、元のPythonの道具（`tools/check-links.py` `check-spec-trace.py` `mutate.py` `check-all.sh` `bench.sh`）を**消す**ことにしました。等価の確認は、すでにあなたと調整役が済ませています。そのため次を直します。

## 直すこと

1. **試験からPythonとの比較を外す。** `crates/xtask/tests/commands.rs` の `python()` 関数と、`XTASK_COMPARE_PYTHON` の分岐（`equivalent()` の中）を消す。比較の代わりに、各場面の**期待する標準出力と終了コードを、試験の中に固定の文字列として書く**（いま Rust 版が出す出力は、元のPythonと一致することを確かめ済みなので、その出力を期待値にしてよい。ただし、場面ごとに「何を確かめる試験か」が読めるようにする）。試験が `tools/*.py` の存在に依存しないこと。`tools/difftest.py` と `tools/oracle/` は残るので、`check-spec-trace` が `tools/**/*.py` を数える動きは残す。
2. **一時フォルダを、リポジトリの中の `.tmp/` でなく、システムの一時フォルダ（`std::env::temp_dir()`。`TMPDIR` を尊重する）にする。** 元のPython（`tempfile.mkdtemp(prefix="kintai-mut-")`）と同じ。対象:
   - `src/mutate.rs` の作業フォルダ。作る場所を引数で渡せる形にする（例: `lib.rs` に `pub fn run_with_temp_dir(root, args, temp_dir)` を足し、`run` は `std::env::temp_dir()` を渡す）。環境変数の書き換え（`std::env::set_var`）は edition 2024 では `unsafe` なので使わない。
   - `tests/commands.rs` のフィクスチャ（`Fixture::new()`）と、変異試験の後始末の確認（いま `f.0.join(".tmp")` を見ている所）。後始末の確認は、`run_with_temp_dir` に渡した専用の一時フォルダが空に戻ることで確かめる。
   - 試験の実行で、リポジトリの中に `.tmp/` などのファイルが**できない**こと。
3. コードの中の英語のコメント（`lib.rs` の `lines` の説明、`links.rs` の `resolve` の説明、試験のコメントなど）を、日本語にそろえる。
4. `Cargo.toml` の `description` は日本語のままでよい。

## 確かめること

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo test -p xtask`（`cargo test --workspace` も）、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`、`cargo +1.88 check --workspace --all-targets` がすべて通る。
- `cargo xtask check-links` と `cargo xtask check-spec-trace` が、このコピーの現状で、変更前と**同じ出力**（`$WS/.tmp/` に変更前の出力を先に保存して比べる）。
- `cargo xtask mutate tools/mutations/core.json --only 1970-01-01` が、変更前と同じ出力で終わり、`TMPDIR` を `$WS/.tmp/` に向けると、その中に `kintai-mut-*` が残らない。
- 試験を流した後、`$WS` 直下に `.tmp/` 以外の新しい未追跡のファイルが増えていない（試験が `$WS/.tmp/` を使うのでなく、システムの一時フォルダを使うため）。

## 報告

`$WS/.agent-harness/runs/2026-10-15-phase0/xtask-progress.md` に追記し、`xtask-report.md` の末尾に「その2」の節を足す（直したこと、確かめた結果、読みで決めた点）。終わったら、短く返す。
