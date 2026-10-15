# xtask 実装報告

## ① 作った・変えたファイル

- 新規: `crates/xtask/Cargo.toml`、`src/{main,lib,links,trace,mutate,pipeline}.rs`、`tests/commands.rs`。
- 新規: `.cargo/config.toml`。`xtask = "run --quiet --locked -p xtask --"`。
- 自動更新: `Cargo.lock` にxtaskのエントリを追加。外部依存は既存のserde（derive）とserde_jsonのみ。
- 工程記録: この報告書と `xtask-progress.md`。検証スクリプト・ログ・合成データ・一時コピーはWSの `.tmp/`。Cargoの保存先は指示書指定の `.cargo-home/` と `.cargo-target/`。
- 既存Python・シェル、他crate、ルートCargo.toml、CI、既存文書は変更しない。ネットワーク・WS本体のgit操作・公開・外部送信は実施しない。

## ② 等価の確認

比較は標準出力のバイト列と終了コードを照合した。Pythonツールのソースは変更していない。

### 1. 現リポジトリ（gitなし）

```
python3 -B tools/check-links.py
cargo xtask check-links
```

双方の標準出力・終了コード0が一致。最終再確認: `検査したMarkdown: 75件、問題: 0件`。比較スクリプトは `.tmp/verify.py`、証拠は `.tmp/check-links-{python,rust}.log` と `.tmp/equivalence.json`。

### 2. 合成文書、gitあり／なし

```
XTASK_COMPARE_PYTHON=1 cargo test -p xtask --test commands
```

`links_git_and_fallback` は、リンク切れ、101行の文書、禁止文字を含むファイル名、公開されないメモへのリンクを比較。gitなしの走査と、WS内に `git init -q` した使い捨てリポジトリの双方で、標準出力・終了コード1が一致した。コードフェンス、外部リンク、見出しのみのリンク、docs/imagesの行数除外、target除外も確認した。

### 3. 仕様参照表

```
python3 -B tools/check-spec-trace.py
cargo xtask check-spec-trace
python3 -B tools/check-spec-trace.py --strict
cargo xtask check-spec-trace --strict
```

現リポジトリでは表全体の出力が一致し、通常・strictとも終了0。条件116件、manual3件、参照の無い条件0件。
合成状態 `trace_missing_and_prefix` では、短い番号と末尾に0が続く番号を区別し、同じ行の重複参照・crates/tools内訳・manual除外・未知番号も比較。通常終了0、strict終了1を含め一致した。
証拠: `.tmp/check-spec-trace-{python,rust}.log`、`.tmp/check-spec-trace-strict-{python,rust}.log`。

### 4. 変異試験

```
TMPDIR="$WS/.tmp" python3 -B tools/mutate.py tools/mutations/core.json --only 1970-01-01
TMPDIR="$WS/.tmp" cargo xtask mutate tools/mutations/core.json --only 1970-01-01
```

双方で終了0。基準行・検出行・生き残り一覧を含む標準出力全文が一致した。

```
[基準] 変更なし: 終了コード=0（0であること）
[core: 1970-01-01 の曜日を金曜にする] 検出 ← 赤になった試験: ac02_1_every_input_date_matches_chrono, ac02_1_every_representable_date_matches_chrono, ac02_1_weekday_offsets, ac02_7_week_first_day

== 生き残り ==
なし
```

合成プロジェクトの比較も実施した。

| ケース | 出力と終了コード |
|---|---|
| count不一致 | 「適用できない」、生き残り一覧、終了1が一致 |
| 生き残り | 「生き残り ← 試験が赤くならなかった」、終了1が一致 |
| 試験失敗 | 試験名sampleを検出、終了0が一致 |
| コンパイル失敗 | 「検出に数えない」、終了1が一致 |
| 基準が赤い | 基準不成立の説明、終了2が一致 |
| 時間切れ | 「時間切れ」を検出として表示、終了0が一致 |
| CRLFのソース | Pythonの改行正規化と同じ置換を行い、終了0が一致 |
| 引数なし | 元の説明全文と終了2が一致 |
| シグナル終了 | 基準の終了コード-15と最終終了2が一致 |

Rust単独の統合試験では、JSONのexclude上書き・深い階層の同名除外、--only、--keep、元ファイルの復元、途中のプロセス起動エラーでの一時コピー削除も確認した。
証拠: `.tmp/core-equivalence-final.log`、`.tmp/core-mutation-{python,rust}.log`、`.tmp/equivalence-fixtures-final.log`。

### 5. bench / check-all

```
cargo xtask bench 3
cargo xtask check-all --quick
```

benchは終了0で全項目が目標以内。計算の中央値7.092/8.311ms、年間判定1.025ms、起動1.738ms、取込124.403ms（数値一致は要件外）。証拠: `.tmp/bench.log`。
check-allは最後のまとめまで実行した。Rust・リンク・仕様参照・差分試験は通過し、差分900件、不一致0件、オラクル例外0件。
任意のREUSEだけ終了失敗となったため、check-all全体は終了1。最初の実行はuvxの既定tools保存先への書き込みが環境に拒否された（書き込みは成功していない）。UV_TOOL_DIR/UV_TOOL_BIN_DIR/UV_CACHE_DIR/TMPDIRをすべてWS内へ指定した再実行では、オフラインキャッシュにreuseが無く、依存解決不能だった。ネットワークで取得していない。REUSE失敗を隠した合格扱いはしていない。

最終のまとめ:

```
== ライセンス表示(REUSE)
  × No solution found when resolving tool dependencies:
  ╰─▶ Because there are no versions of reuse[charset-normalizer] and
      you require reuse[charset-normalizer], we can conclude that your
      requirements are unsatisfiable.
   -> 失敗

== まとめ
失敗: ライセンス表示(REUSE)
```

証拠: `.tmp/check-all-final.log`。実行環境は `CARGO_NET_OFFLINE=true`、`UV_OFFLINE=true`、`CARGO_HOME=$WS/.cargo-home`、`CARGO_TARGET_DIR=$WS/.cargo-target`。
外部コマンドを合成した統合試験では、python3/uvx/1.88/cargo-deny不在時の省略、失敗の段階順、metadataのtarget_directory（空白入り）を使った--bin引数も確認した。

## ③ 品質の基準

最終コードに対して次を実行し、すべて終了0。ワークスペースの試験はドキュメント試験を含め合計449件成功（xtaskの単体4件・統合7件を含む）。

```
fmt: 0
clippy: 0
test: 0
doc: 0
msrv: 0
```

`cargo fmt --all -- --check` の実際の末尾:

```
（標準出力・標準エラーとも出力なし）
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings` の実際の末尾:

```
    Checking xtask v0.0.1 ($WS/crates/xtask)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.26s
```

`cargo test --workspace` の実際の末尾:

```
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

all doctests ran in 0.46s; merged doctests compilation took 0.20s
   Doc-tests xtask

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` の実際の末尾:

```
 Documenting xtask v0.0.1 ($WS/crates/xtask)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s
   Generated $WS/.cargo-target/doc/kintai/index.html and 4 other files
```

`cargo +1.88 check --workspace --all-targets` の実際の末尾:

```
    Checking xtask v0.0.1 ($WS/crates/xtask)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.57s
```

## ④ 読みで決めた点・仕様に無い所

- 公開関数run(root,args)でロジックを試験し、本番バイナリはCARGO_MANIFEST_DIRの2階層上をcanonicalizeして使う。合成比較ではテストドライバの子プロセスから同じ関数を呼び、Python側はimport後のROOTだけを合成ルートへ設定する。ツール本体のファイルは編集しない。
- mutateの一時フォルダは指示書の「WS外に書かない」に従いWS/.tmp内に作る。コピー先自身は再帰コピーから外す。--keep時もこの場所に残し、それ以外はRAIIで削除する。
- 更新時刻は引き継がずfs::copyで戻す。変異の前後は元と同じ1.1秒待つ。出力はファイルに受けてChild::try_waitで監視し、パイプ詰まりを避ける。
- check-allの仕様参照検査は旧Pythonファイルの存在でなくdocs/specの存在で判断する。将来、調整役が旧道具を消した後もRustの検査を続けるため。
- 仕様参照検査が追加したRust自身の表示文字列を試験の参照として数えないよう、benchの条件番号の定数を分割して同一の表示を生成する。試験の合成番号も組み立て、既存の参照表を変えない。
- 通常のcargo testはPython不要。XTASK_COMPARE_PYTHON=1を明示した検証時だけ、既存Pythonツールと統合試験の出力を比較する。
- 不正なファイル・JSON・起動不能の例外はRustの診断を標準エラーへ出して終了1とする。Python固有のトレースバック形式は再現しない。引数なしmutateの説明と終了2は再現した。
- 作業データ・ビルド出力はSSD共通ルールより優先される今回の明示指示の場所を使用した。空き容量を確認し、WS外への移動は行わない。

## ⑤ やらなかったこと

- 全変異の実行、check-allの非quick実走行、依存の新規取得は実施しない。1.88全target確認は別途実行し合格した。
- REUSEの実検査合格は確認できていない（上記オフライン環境の制約）。元のシェルcheck-all/benchとの出力全文比較は要件どおり対象外で、Rustの実実行と合成外部コマンド試験で確認した。
- 既存道具の削除・変更、oracle/difftestの移植、他crateや文書・CIの変更、commit/push/PR、外部通信、独立エージェントへの委譲は行わない。

## 最終再確認の証拠

```
Compiling xtask v0.0.1 ($WS/crates/xtask)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.27s
     Running tests/commands.rs (.cargo-target/debug/deps/commands-04619d8f3fb01e66)

running 7 tests
test fixture_driver ... ok
test trace_missing_and_prefix ... ok
test links_git_and_fallback ... ok
test pipelines_skip_optional_tools_and_use_metadata ... ok
test mutation_copy_exclusions_and_keep ... ok
test mutation_crlf_and_usage ... ok
test mutation_outcomes_and_cleanup ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 20.59s
```

````text
python 0 [基準] 変更なし: 終了コード=0（0であること）
[core: 1970-01-01 の曜日を金曜にする] 検出 ← 赤になった試験: ac02_1_every_input_date_matches_chrono, ac02_1_every_representable_date_matches_chrono, ac02_1_weekday_offsets, ac02_7_week_first_day

== 生き残り ==
なし
 
rust 0 [基準] 変更なし: 終了コード=0（0であること）
[core: 1970-01-01 の曜日を金曜にする] 検出 ← 赤になった試験: ac02_1_every_input_date_matches_chrono, ac02_1_every_representable_date_matches_chrono, ac02_1_weekday_offsets, ac02_7_week_first_day

== 生き残り ==
なし
 
stdout_equal= True exit_equal= True
````

最終のリンク・仕様表比較:

```json
[
  {
    "case": "check-links",
    "stdout_equal": true,
    "python_exit": 0,
    "rust_exit": 0,
    "tail": [
      "検査したMarkdown: 75件、問題: 0件"
    ]
  },
  {
    "case": "check-spec-trace",
    "stdout_equal": true,
    "python_exit": 0,
    "rust_exit": 0,
    "tail": [
      "AC-13.6   python     2  0/2                   tools/difftest.py:18",
      "",
      "条件 116件（manual 3件は対象外）。参照の無い条件: 0件"
    ]
  },
  {
    "case": "check-spec-trace-strict",
    "stdout_equal": true,
    "python_exit": 0,
    "rust_exit": 0,
    "tail": [
      "AC-13.6   python     2  0/2                   tools/difftest.py:18",
      "",
      "条件 116件（manual 3件は対象外）。参照の無い条件: 0件"
    ]
  }
]
```


## その2

### 直したこと

- 変更した実装は `crates/xtask/src/{lib,links,mutate}.rs` と `crates/xtask/tests/commands.rs` の4ファイル。
- `python()`、`equivalent()`、`XTASK_COMPARE_PYTHON` の分岐を削除した。リンク検査（gitあり／なし）、仕様表（通常／strict）、変異（適用不能・生き残り・検出・コンパイル失敗・基準失敗・時間切れ・CRLF・引数なし・シグナル終了）の出力全文と終了コードを固定の期待値で確認する。試験の意図を日本語コメントで補った。
- `run_with_temp_dir(root, args, temp_dir)` を追加し、`run` は `std::env::temp_dir()` を渡す。mutateは渡された親に `kintai-mut-*` を作り、フィクスチャも `std::env::temp_dir()` に作る。`set_var`、`unsafe`、新しい依存は使っていない。
- 後始末は、合成リポジトリと分けた専用の親フォルダが空になることで確認する。起動エラーでも削除され、`--keep` では親に1件残り、コピー内のソースが復元されることを確認する。合成リポジトリ内に `.tmp` ができないことも確認する。
- 英語の説明コメントを日本語化した。check-allの合成データから不要な旧 `check-spec-trace.py` を取り除いた。残す `difftest.py` と仕様参照の `tools/**/*.py` 走査は維持した。

### 確かめた結果

次の6項目はすべて終了0。xtaskは単体4件・統合7件成功。ワークスペース全体はドキュメント試験を含め合計449件成功。

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p xtask
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets
```

ログは `.tmp/xtask-part2/{fmt,clippy,test-xtask,test-workspace,doc,msrv}.log`、終了コード一覧は `quality-results.json` に保存した。

修正前に保存した出力と、修正後の次の3コマンドの標準出力バイト列・終了コードはすべて一致（すべて終了0）。変更前後のファイルは `.tmp/xtask-part2/{links,trace,mutate}-{before,after}.stdout`、判定は `comparison-results.json`。

```text
cargo xtask check-links
cargo xtask check-spec-trace
cargo xtask mutate tools/mutations/core.json --only 1970-01-01
```

リンク検査は「検査したMarkdown: 76件、問題: 0件」。変異試験は1970-01-01の曜日変異を検出し、生き残り「なし」。`TMPDIR=$WS/.tmp` で実行した後の `kintai-mut-*` と `xtask-test-*` はいずれも残留0件。WS直下の項目名を開始前後で比較し、新しい項目0件。WSには `.git` がなく、gitによる未追跡判定は行わず、`top-before.json` の一覧と比較した。WS本体のgit操作は行っていない。

### 読みで決めた点

- 「システムの一時フォルダ」はコードが `std::env::temp_dir()` を使用して `TMPDIR` に従う意味で適用した。今回の実行は「WS外へ書かない」の制約を守るため `TMPDIR=$WS/.tmp` を指定した。コードはリポジトリ内の `.tmp` に固定していない。Cargo保存先も指定の `.cargo-home` と `.cargo-target` を継続し、ネットワークを使わず実施した。
- 仕様表の期待値内の合成条件番号は `AC-` と番号を分けた固定文字列で組み立て、自分自身を実リポジトリの参照に数えさせない。
- 引数なしmutateの説明は出力互換のため、旧Pythonコマンドを含む文章も期待値に固定した。スクリプトの読み込みや実行は行わず、存在に依存しない。Rustdocの標準的な `# Errors` 見出しはclippyの規約として残し、その説明文は日本語にした。
- 初回の固定値試験で仕様表の見出しの空白が1文字不足していたため期待値を訂正し、上記の全検証を通した。製品出力は変えていない。
- 実装以外の書き込みは指示で指定された進捗・報告書、Cargo既定の指定済み保存先、`.tmp` の検証証拠のみ。旧ツール削除、他crate変更、ネットワーク利用、依存追加、commit/pushは実施していない。今回の依頼範囲に未完了項目はない。

## その3: 本体に取り込んだあと、調整役が直したこと

### 一括検査で見つかった試験の不安定さ

本体で `cargo xtask check-all`（完全版）を流すと、xtaskの統合試験7件のうち2件が「No such file or directory」で落ちた。単独の `cargo test --workspace` では通っていた。

**原因**: 試験の一時フォルダの名前が `xtask-test-<プロセス番号>-<時刻のナノ秒>` で、macOSの時計はマイクロ秒単位でしか進まない。同時に動く試験どうしが、同じ名前のフォルダを作ることがあった。片方の後始末（`remove_dir_all`）が、もう片方の使っているフォルダを消す。

**確かめ方**:

- 直列に20回流しても落ちなかった。再現には、試験の実行ファイルを4並列で流す必要があった。
- 修正前の試験を4並列×25回（計100回）流すと、10回落ちた。落ちた回の失敗は4種類で、どれも「別の試験のフォルダが混ざった」と読める。別の試験の出力が自分の結果として返った、ファイルの内容が別の試験のものに変わっていた（`original\r\n`）、作業フォルダの中の `stdout` や `value` が消えていた、実行中のコマンドが全部失敗した、である。
- 落ちた回のフォルダ名のうち、記録に残った2つは、同じ名前が2回作られていた名前と一致した。
- 修正後の同じ条件の100回では、0回だった。

**直したこと**（`crates/xtask/tests/commands.rs`）:

- フォルダ名にプロセス内の連番を足した。作るときは `create_dir` にして、既にあれば失敗させる。
- 時間切れを確かめる場面だけ `timeout_sec` を1秒にして、ほかは60秒にした。機械が混んでいても、誤って時間切れにならない。
- 試験ファイル全体を `#![cfg(unix)]` にした。`sh` `grep` `git` を呼ぶので、Windowsでは流さない。

### そのほか

- `mutate` のヘルプ文に残っていた旧コマンドの案内（`python3 tools/mutate.py`、`shutil.copy`）を、`cargo xtask mutate` と実際の動作の説明に直した。前の節にある「旧Pythonコマンドを含む文章も期待値に固定した」は、この直しで古くなった。
- コメントに残っていたPythonへの言及を、動作の説明に書き直した。
- 旧Pythonの道具5本（`check-links.py` `check-spec-trace.py` `mutate.py` `check-all.sh` `bench.sh`）を削除する前に、最終のコードでもう一度、出力と終了コードを比べた。リンク検査、仕様対応（通常と `--strict`）、変異試験（`core.json` の1件）の4つが、すべて完全に一致した。
- 本体で `cargo xtask check-all`（完全版）を流し、書式、clippy、全試験（449件）、文書、Rust 1.88、cargo deny、リンクと行数、仕様の対応、差分試験、REUSEの全段階が通った。
