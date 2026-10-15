# 実装指示書 xtask: 点検の道具を Rust（`cargo xtask`）に移す

あなたは実装担当です。日本の労働基準法に基づく勤怠のOSS「kintai」（Rust）の、点検の道具を Rust に移します。本人（保守者）が「Rust 製のはずなのに Python が混ざっている」と指摘したので、開発用の点検の道具だけを Rust にし、`cargo` だけで動くようにします。**別実装（`tools/oracle/` と `tools/difftest.py`、Python）は、独立して書かれていることに価値があるので、触らず、Python のまま残します。**

## 作業場所

作業場所は `__WS__`（以下 `$WS`）です。本体のリポジトリから調整役がコピーした、あなた専用のコピーです。このコピーの外には書かない。本体のリポジトリには触れない。

## 守ること（違反は失敗）

1. **ネットワークは使えない。** 依存は取得済み（`$WS/.cargo-home`）。新しい依存を足さない。`crates/xtask` が使ってよい外部の依存は **`serde`（derive）と `serde_json` だけ**（どちらも `Cargo.lock` にある）。`regex` `clap` `walkdir` `anyhow` などは使わない（引数の解析、再帰、文字列の検索は `std` で書く）。
2. **書き込んでよい場所**は、`$WS/crates/xtask/`（新規）、`$WS/.cargo/config.toml`（新規）、ルートの `Cargo.lock`（cargo が自動で直す）、`$WS/.agent-harness/runs/2026-10-15-phase0/` の報告書、`$WS/.tmp/`（一時ファイル）だけ。**既存の Python・シェルの道具（`tools/*.py` `tools/*.sh`）は消さず、直さない**（等価の確認に使う。消すのは調整役）。文書、CI、ほかの crate も触らない。
3. `unsafe` を使わない（ワークスペースが `forbid`）。パニックしうる書き方（`unwrap` `expect` `[]` の範囲外アクセス、桁あふれ）を、道具のコード（試験を除く）で使わない。
4. 品質の基準が、そのまま通ること: `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`（pedantic つき。ルートの `Cargo.toml` の `[workspace.lints]`）、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`、`cargo +1.88 check --workspace --all-targets`（MSRV 1.88。edition 2024）。
5. 各ファイルは SPDX の2行を先頭に付けなくてよい（Rust は `REUSE.toml` が表示する）。`Cargo.toml` の `license = "MIT OR Apache-2.0"`、`publish.workspace = true`、`[lints] workspace = true` を、ほかの crate と同じに書く。
6. 使い捨ての git リポジトリを `$WS/.tmp/` の中に作って試すのは構わない（`check-links` の git を使う動きを試すため）。`$WS` 自体では git の操作をしない。
7. 実データ・個人情報を使わない。

## 環境

```
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME=__WS__/.cargo-home CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=__WS__/.cargo-target
cd __WS__
```
`python3` と `rsync` が使える（等価の確認に Python の元の道具を動かす）。

## 作るもの

`crates/xtask`（バイナリ crate。`publish = false`）。`.cargo/config.toml` に別名 `xtask = "run --quiet --locked -p xtask --"` を置き、`cargo xtask <コマンド>` で動く。リポジトリのルートは、`env!("CARGO_MANIFEST_DIR")` の2つ上（Python の `ROOT = parent.parent` と同じ）。

| コマンド | 元の道具 | 動き |
|---|---|---|
| `cargo xtask check-links` | `tools/check-links.py` | 文書の相対リンク切れ、公開されない（`.gitignore` の）ファイルへのリンク、1ファイル100行の上限、ファイル名の禁止文字を検査する |
| `cargo xtask check-spec-trace [--strict]` | `tools/check-spec-trace.py` | 受け入れ条件（`AC-nn.n`）ごとに、試験からの参照があるかを表にする |
| `cargo xtask mutate <定義のJSON> [--only 文字列] [--keep]` | `tools/mutate.py` | ソースにわざと誤りを入れて、試験が赤くなるかを確かめる |
| `cargo xtask bench [回数]` | `tools/bench.sh` | 速さの測定の例（`crates/*/examples/bench_*.rs`）をまとめて流す |
| `cargo xtask check-all [--quick]` | `tools/check-all.sh` | PRの前に通す検査を、まとめて流す |

**元の道具の動きを、そのまま移す。** まず5つの元のファイル（`tools/check-links.py` `check-spec-trace.py` `mutate.py` `check-all.sh` `bench.sh`）を、最後まで読む。出力の文言（日本語）、終了コード、引数、除外する名前の一覧、エラーの扱いを、**同じにする**。次の点は特に注意する。

- **`check-links`**: git があるとき（`git ls-files -o -c --exclude-standard`）と、git のリポジトリでないとき（フォルダを歩く。`.git` `.cargo-home` `.cargo-target` `target` `node_modules` を除く。公開しない作業メモ `research/out-*.md` `research/notes_*.md` も除く）の両方。リンクの取り出し（コードブロックの中を飛ばす、`http:` などの外部リンクと `#` だけのリンクを飛ばす、`#見出し` を切る）、`.md` 以外は行数を見ない、`docs/images/` を行数の検査から外す、等も同じ。
- **`check-spec-trace`**: `docs/spec/*.md` の `- AC-nn.n [種類]` の行を集め、`crates/**/*.rs` と `tools/**/*.py` の中の `AC-nn.n` の参照を数える。参照の `AC-12.1` が `AC-12.10` に誤って一致しないこと（`(?!\d)` に当たる処理）。`manual` は対象外。`--strict` で、`rust` と `python` の条件に参照が無ければ終了コード1。出力の表の列の幅と並びも同じにする。
- **`mutate`**: 作業用の一時フォルダにリポジトリを写し（元は `rsync -a --delete --exclude ...`。**Rust では外部の `rsync` を使わず**、`std::fs` で再帰して写す。除く名前は、元の `EXCLUDES` と JSON の `exclude` の上書き。名前が一致する項目は、どの深さでも飛ばす）、基準（変更なし）を流してから、変異を1件ずつ適用して試験を流し、元に戻す。ファイルを戻すときは**更新時刻を引き継がない**（コピーして新しい時刻にする。引き継ぐと cargo が変更なしと判断する）。前後に1.1秒待つ。試験の種類の判定（`ok` `test` `compile` `timeout`）、赤くなった試験の名前の取り出し（`test X ... FAILED` と `failures:` の一覧。`could not compile` でコンパイルの失敗と判定）、時間切れ（JSONの `timeout_sec`、既定900秒。外部の `timeout` コマンドは使わず、`Child::try_wait` の繰り返しで見張る）、`count` の検査（見つかった数と期待が違えば「適用できない」）、生き残りの一覧、終了コード（生き残りがあれば1、基準が赤なら2）は同じ。`--keep` で一時フォルダを残す。途中で失敗しても一時フォルダを残さない（`--keep` のときを除く）。環境変数は `CARGO_TARGET_DIR=<一時フォルダ>/target` と、PATH の先頭に `$HOME/.cargo/bin`。**変異の定義 `tools/mutations/*.json` の形は変えない**（`test_cmd` `timeout_sec` `exclude` `mutations[].{label,file,find,replace,count}`）。
- **`bench`**: `tools/bench.sh` のとおり。環境（日時、OS、機械、Rust の版、プロファイルの説明）を先に出し、`kintai-cli` の release ビルド、`bench_jp_labor`、`bench_import` を順に流す。回数の既定は7、変えられる（3以上）。どれかが失敗したら終了コード1。
- **`check-all`**: `tools/check-all.sh` の段階を同じ順に流し、失敗した段階の名前を最後に一覧する。`--quick` で依存の検査と最小のRust（1.88）を省く。Python は**差分試験の段階にだけ**使う: `python3` が無ければ、その段階を「python3 が無いので省略」と出して飛ばす（失敗にしない）。差分試験の前に `cargo build -q -p kintai-cli` で実行ファイルを作り、`cargo metadata --format-version 1 --no-deps` の `target_directory`（`serde_json` で読む）から `debug/kintai` の場所を決めて `--bin` で渡す（元のシェルのとおり）。`uvx` が無ければ REUSE の段階を省く（元のとおり）。`cargo +1.88` の確認も、1.88 が未導入なら案内の文を出して省く。最後に `すべて通過しました。` か `失敗: ...`（終了コード1）。

## 試験

道具のコードは、純粋な関数（リンクの取り出し、行数の数え方、受け入れ条件の行の解析、参照の数え方、変異の適用と元に戻す、試験の出力の解析、引数の解析）に分け、`crates/xtask` の中の単体試験と `crates/xtask/tests/` の統合試験で確かめる。**実際の等価を確かめる**ため、次を必ずやる（結果を報告書に貼る）。

1. 現在のリポジトリで、`python3 tools/check-links.py` と `cargo xtask check-links` の標準出力・終了コードが**一致**する（git のリポジトリでない `$WS` では「フォルダを歩く」動きの比較。git を使う動きは、`$WS/.tmp/` に使い捨ての git リポジトリを作り、問題を含む文書を置いて、両方を比べる）。
2. 問題を含む合成の文書（リンク切れ、101行、`#` を含むファイル名、公開されないファイルへのリンク）を一時フォルダに作り、両者の出力が一致する。
3. `python3 tools/check-spec-trace.py` と `cargo xtask check-spec-trace` の出力が**一致**。`--strict` の終了コードも。参照の無い条件がある合成の状態でも比べる。
4. `tools/mutations/core.json` から**1件だけ**（`--only` で、短く速いもの）を選び、`python3 tools/mutate.py <json> --only <文字列>` と `cargo xtask mutate <json> --only <文字列>` の出力（基準の行、検出の行、「== 生き残り ==」）と終了コードが一致する。「適用できない」の動き（`count` 違い）と、「生き残り」の動き（試験が赤くならない変異）も、合成の小さなプロジェクト（`$WS/.tmp/` に作る）で確かめる。
5. `cargo xtask bench 3` が動く（速さの数字は一致しなくてよい）。`cargo xtask check-all --quick` が最後まで流れる。

## 報告

作業中は `$WS/.agent-harness/runs/2026-10-15-phase0/xtask-progress.md` に、段階ごとに1〜3行追記する。最後に `xtask-report.md` に次を書く: ①作った・変えたファイル ②等価の確認（上の1〜5のコマンドと結果。元の道具と違う点があれば、その理由） ③品質の基準のコマンドの出力（実際の末尾） ④読みで決めた点・仕様に無い所 ⑤やらなかったこと。
