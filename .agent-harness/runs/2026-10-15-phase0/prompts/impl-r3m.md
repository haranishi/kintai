# 実装指示書 R3m: 速さの測定と変異試験（部品の側）

あなたは実装担当です。日本の労働基準法に基づく勤怠のOSS「kintai」（Rust）の、第3周のうち、速さの測定と変異試験を担当します。部品（`kintai-core` `kintai-import` `kintai-jp-labor`）は、第1周と第2周でできています。**仕様を変えず**、仕様どおりに作り、疑問は報告書に書いてください。

## 作業場所

作業場所は `__WS__`（以下 `$WS`）です。調整役が本体からコピーした、**あなた専用のコピー**です。並行して R3 が、コマンドライン全体（`kintai-cli`）を別の場所で作っています。このコピーの外（本体のリポジトリ、R3 の場所）には**アクセスしない**。終わったら調整役が、あなたが作ったファイルを本体へ取り込みます。別実装（Python のオラクル）はこのコピーに入っていません。探さないでください。

## 守ること（違反は失敗）

1. git の操作をしない（このコピーは git のリポジトリではない）。
2. 書き込んでよい場所は、「触ってよいファイル」と `$WS/.agent-harness/runs/2026-10-15-phase0/` の中だけ。`docs/` `Cargo.toml`（ルート）`deny.toml` は**読むだけ**。
3. **新しい依存を足さない。** 許可リスト: 実行時 `encoding_rs` `serde`(derive) `serde_json` `toml` `pico-args`、試験のみ `proptest` `chrono`。
4. 他の勤怠・人事・給与ソフトのコードを探したり、見たりしない。ネットワークは使えない。
5. 実データ・実在の人名を使わない。測定用のデータは合成。
6. `unsafe` を使わない。ライブラリのコードでパニックしうる書き方をしない（試験は可）。浮動小数点を使わない。

## 環境（ネットワークなし）

```
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME=__WS__/.cargo-home CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=__WS__/.cargo-target
cd __WS__
```

## 触ってよいファイル

- `crates/kintai-jp-labor/examples/bench.rs`、`crates/kintai-import/examples/bench.rs`（新規または直す）。
- `crates/kintai-jp-labor/tests/mut_*.rs`、`crates/kintai-import/tests/mut_*.rs`、`crates/kintai-core/tests/mut_*.rs`（変異試験で生き残った誤りを赤くするための試験。名前は `mut_` で始める）。
- `tools/mutations/*.json`（新規。`r1-classify.json` は読むだけ）。
- `tools/bench.sh`（新規。測定をまとめて流すシェル。任意）。
- 部品のソース（`crates/*/src/**`）は、**直さず報告書に書く**。どうしても必要な最小の変更（遅い所の最適化）だけは許すが、「ソースを変えた」と理由を報告書に書く。

## 読むもの

1. `docs/spec/nfr.md`（速さの目標・決定性・安全）。`docs/spec/` の各仕様（変異の対象を選ぶため）
2. `tools/mutate.py` の冒頭（変異の定義の形）と `tools/mutations/r1-classify.json`（33件の例）
3. 前の周の報告書: `.agent-harness/runs/2026-10-15-phase0/{r1,r2a,r2b}-report.md` と、部品の公開API

## 作るもの

### 1. 速さの測定（AC-10.1〜10.3）
`nfr.md` の目標を測る `examples/bench.rs` を置く。合成データを自分で生成して、`--release` で測り、結果を標準出力に出す。

| 項目 | 目標 | 条件 |
|---|---|---|
| 区分の計算 | 100ミリ秒以内 | 1,000人×1か月（約3万日）、入出力を除く計算だけ |
| 36協定の年間判定 | 50ミリ秒以内 | 1,000人×12か月 |
| 打刻CSVの取込 | 3秒以内 | 100万行の合成データ（UTF-8） |
| 起動 | 50ミリ秒以内 | `kintai --version` |

目標を超えたら、遅い所を直す（ソースを変えたら、報告書に理由を書く。試験は変えない）。測定は3回以上行い、最良と中央値を記録する。機械、OS、Rustの版、`--release` のプロファイルを添える。起動時間は、ビルドした実行ファイルを `std::process::Command` で多数回動かして測る例（`examples/`）か、`tools/bench.sh` で測る。

### 2. 変異試験（生き残り0）
`tools/mutate.py` の使い方は、そのファイルの冒頭にある。`tools/mutations/` に、次の部品ごとのファイルを作る: `limits36.json`、`leave.json`、`holidays.json`、`report.json`、`punch.json`、`import.json`、`core.json`。各ファイルに、境界の誤りを**部品ごとに20件以上**入れる。例:

- 36協定: `4800 * n` を `4800 * n + 1`、`T < 6000` を `<=`、発動月の `> 2700` を `>=`、年の累計の起算月の替わり、履歴の使う月数、`UnknownHistory` の条件、協定年の途中の `year_overtime_known`
- 年休: 付与日の「次の初日」の規則（翌月1日）、失効の当日、半日の換算、古いロットからの消化、5日の判定（`>=` と `>`）、期間の重なり、出勤率 `below`、`as_of` より後の除外
- 祝日: 振替休日の規則、国民の休日、春分・秋分の式の定数、`OutOfRange` の境
- 月次の集計・保存期限: 30分丸めの閾値、「遅い方」の起算日、ID の検査、`night` を合計に足さない
- 打刻: 秒の切り上げ・切り捨て、遷移の抜け、24時間、同時刻、休憩の360・480分
- 取込: 文字コード、BOM、ヘッダ、列の過不足、入力の上限
- core: 日付の加減、うるう年、月の期間の初日

出発点に、前の周の担当が作った変異を使ってよい: R2a の `.agent-harness/runs/2026-10-15-phase0/r2a-self-mutate.py`（63件。ファイル・find・replace の形で入っている。どうやっても検出できない等価な変異2件の理由は `r2a-report.md`）、R2b の `r2b-mutations.json`（39件）、R1 の `tools/mutations/r1-classify.json`（33件）。定義は `tools/mutations/*.json` の形に写し、足りない境界を足す。

実行して、**生き残り（試験が赤くならない誤り）があれば試験を足して**赤くなるようにする（変異の側を弱めない）。試験は `tests/mut_*.rs` に足す。同じ変異が3回続けて生き残ったら、報告書に書いて止める。コンパイルエラーになる変異は直す（検出に数えない）。

### 3. その他
- 試験の名前かコメントに `AC-xx.y` を書く。AC-10.1〜10.3 の `[rust]` 条件に、参照があること。
- 測定用のデータを生成する関数は、`examples/` の中に閉じ込め、ライブラリの公開APIに足さない。

## 品質の基準（完了の条件）

次がすべて通ること。通った出力を報告書に貼る。
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check
python3 tools/check-spec-trace.py      # AC-10.1〜10.3 の [rust] 条件が参照されていること
python3 tools/mutate.py tools/mutations/<名前>.json   # 部品ごとに、生き残り0
```

## 報告

作業中は `$WS/.agent-harness/runs/2026-10-15-phase0/r3m-progress.md` に、段階ごとに1〜3行追記する。最後に `r3m-report.md` に次を書く: ①作った/変えたファイル（調整役が本体へ取り込む一覧。ソースを変えたなら明記） ②速さの測定の結果（表: 項目、目標、最良、中央値、機械） ③変異試験の結果（部品ごとの件数、検出、生き残り、足した試験） ④品質の基準のコマンドの出力（実際の末尾） ⑤仕様への疑問・曖昧な点 ⑥やらなかったこと ⑦次の周への申し送り。
