# 実装指示書 R3: コマンドライン全体（方針ファイル、report・check・leave・explain・retention・demo）

あなたは実装担当です。日本の労働基準法に基づく勤怠のOSS「kintai」（Rust）の、第3周のうち、コマンドライン全体を実装します。部品（`kintai-core`、`kintai-import`、`kintai-jp-labor::{policy, shift, classify, limits36, leave, holidays, breaks, report, explain}`）と、`kintai case`（3種類のケース）は、第1周と第2周でできています。この周で、それらをコマンドとして組み上げます。**仕様を変えず**、仕様どおりに作り、仕様に疑問があれば止まらずに「疑問」として報告書に書いてください。

## 作業場所

作業場所は `__WS__`（以下 `$WS`）です。調整役が本体からコピーした、**あなた専用のコピー**です。並行して R3m が別のコピーで、速さの測定と変異試験を作っています。このコピーの外（本体のリポジトリ、R3m の場所）には**アクセスしない**。終わったら調整役が、あなたが作ったファイルを本体へ取り込みます。別実装（Python のオラクル）は、独立性を保つため、このコピーに入れていません。探さないでください。

## 守ること（違反は失敗）

1. **git の操作をしない**（このコピーは git のリポジトリではない）。
2. 書き込んでよい場所は、「触ってよいファイル」と `$WS/.agent-harness/runs/2026-10-15-phase0/` の中だけ。`docs/` `Cargo.toml`（ルート）`deny.toml` `.github/` は**読むだけ**（仕様の誤りは報告書へ）。
3. **新しい依存を足さない。** 許可リスト: 実行時 `csv` `encoding_rs` `serde`(derive) `serde_json` `toml` `pico-args`、試験のみ `proptest` `chrono`。`Cargo.toml` の宣言の変更は最小にとどめ、理由を報告書に書く。
4. **他の勤怠・人事・給与ソフト（GPL/AGPL のOSSを含む）のコード・画面・仕様書を探したり、見たりしない。** 別実装の Python は、このコピーに入っていない。探さない。ネットワークは使えない。
5. 実データ・実在の人名を使わない。試験データは合成（`tests/fixtures/synthetic/` に置く）。
6. `unsafe` を使わない。ライブラリのコードでパニックしうる書き方をしない（試験は可）。整数は桁あふれを検査する。浮動小数点を使わない。外へ通信しない。
7. 現在の時刻、乱数、環境変数、ロケールを計算に使わない（`docs/spec/nfr.md`）。基準の日付は引数で受ける（`retention` の `--completed` `--wage-due`、`leave` の `--as-of`）。

## 環境（ネットワークなし）

```
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME=__WS__/.cargo-home CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=__WS__/.cargo-target
cd __WS__
```

## 触ってよいファイル

- `crates/kintai-cli/src/**` と `crates/kintai-cli/tests/**`、`crates/kintai-cli/examples/**`（`Cargo.toml` の依存の宣言は理由つきで最小）。
- `tests/fixtures/synthetic/**`（合成の方針・打刻・休暇・履歴）。

**並行して R3m が別のコピーで、速さの測定（`examples/bench.rs`）と変異試験（`tools/mutations/`）を作ります。部品の中のファイルは R3m が追加の試験を足すので、触らない。** 部品（`kintai-core` `kintai-import` `kintai-jp-labor`）に不足や不具合を見つけたら、**直さず報告書に書く**。どうしても進めない場合に限り、最小の変更をして、理由を報告書に書く。

## 読むもの（この順）

1. `docs/spec/cli.md`（**コマンドの意味・方針ファイル・出力の形。この周の中心**）
2. `docs/spec/report.md`、`punch.md`、`nfr.md`、`validation.md`、`classify.md`
3. `docs/spec/case-format.md`、`case-format-36-leave.md`、`limits36.md`、`leave.md`
4. 前の周の報告書: `.agent-harness/runs/2026-10-15-phase0/{r1,r2a,r2b}-report.md` と、部品の公開API（型・関数・エラー）。`kintai-cli/src/{case,dto,failure,strict_json}.rs` の書き方に**揃える**
5. `docs/phase1/` は読まなくてよい（次の段階）

## 作るもの

### 1. 方針ファイル（TOML）の読み込み（AC-12.1）
`cli.md` の例のとおり。`schema = "kintai.policy.v1"`、`[workplace]`、`[agreement36]`、`[[employee]]`。知らない項目、必須の欠け、範囲外の値は不正な入力。`MonthStartDayMismatch` は、`agreement36.kind` が `none` でなく `agreement36.month_start_day` が `workplace.month_start_day` と違うときの対応外（TOMLだけで判定）。TOMLの読み込みで、入力の上限（`nfr.md`）、パニックしないこと、エラーに入力の値を含めないこと（行番号と項目名だけ）を守る。

### 2. コマンド（AC-12.2〜12.8）
`cli.md` の表とコマンドの意味のとおり。`report`（`text` `json` `csv`）、`check`（`--history` `--fail-on-violation`）、`leave`、`explain`、`retention`、`demo`。`case`（3種類）と `--version` `--help` は、すでにある。

- `report` `check` `explain` の流れ: 打刻CSV（`kintai-import`）→ 打刻の対（`kintai-core::punch`、異常の検出と秒の方針）→ 勤務 → 従業員ごとに `classify`（`data_from` の既定は CSV 全体の最初の日付）→ 月の期間ごとの台帳項目（`report`）、36協定（`limits36`）、休憩の不足（`breaks`）。打刻に異常のある従業員は、その人の確定集計を拒否する（黙って除外しない。異常の一覧を出す）。
- 出力の形（JSON のフィールド名、CSV の列、text の表）は、前の周の部品の出力の型と、`report.md` の項目名に揃える。CSV の列は `employee_id,period_start,work_days,work_minutes,overtime_minutes,over60_minutes,holiday_minutes,night_minutes,within_minutes,extra_within_minutes`、UTF-8、LF、見出し行あり。**JSON・CSV の形は、報告書に表で書く**（調整役が `docs/spec/` に取り込む）。
- すべての出力に `table_checked_on`、`review_status: "unreviewed"`、`notices`（`report.md`）を付ける。text の出力の末尾にも、参考値であることを1行で示す。
- 出力は決定的（同じ入力でバイト列が一致）。従業員はID順、日付は昇順。
- エラーは標準エラーに `{"error": {"code": ..., "message": ...}}`、終了コードは 0・1・2・3・4（`case-format.md`）。
- `demo`: 合成データを、`--out` のフォルダ（省略時は新しい一時フォルダ）に書いて、各コマンドの出力を一通り標準出力に出す。そこにだけ書く（Unix系ではフォルダ0700、ファイル0600。AC-10.8）。合成データには、通常の月、夜勤、法定休日の労働、60時間超、36協定の特別条項、年休の失効、休憩の不足、境界をまたぐ勤務（拒否の例）を含める。実在の人名・会社名は使わない（従業員IDは `E001` `E002` …）。
- `tests/fixtures/synthetic/` に、デモと同じ内容の方針・打刻・休暇・履歴を置く（試験と `getting-started.md` が使う）。

### 3. その他
- 試験の名前かコメントに `AC-xx.y` を書く。AC-12 の `[rust]` 条件と、AC-10.4〜10.8（決定性・壊れた入力・上限・エラー文・権限）に、参照があること。AC-10.1〜10.3（速さ）は R3m が担当する。
- `kintai-cli` の公開しない関数にも、境界の試験を足す。壊した入力（TOML・CSV・引数）で、パニックしないこと（`proptest`）。

## 品質の基準（完了の条件）

次がすべて通ること。通った出力を報告書に貼る。
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check
python3 tools/check-spec-trace.py      # AC-12 と AC-10.4〜10.8 の [rust] 条件が参照されていること
python3 tools/check-links.py
```
MSRV（1.88）の確認は、R1 の報告書に書かれた方法に合わせる。別実装との差分試験は、調整役が本体で流す。

## 報告

作業中は `$WS/.agent-harness/runs/2026-10-15-phase0/r3-progress.md` に、段階ごとに1〜3行追記する（途中で止まっても引き継げるように）。最後に `r3-report.md` に次を書く: ①作った/変えたファイル ②各受け入れ条件がどの試験で確かめられているか（表） ③品質の基準のコマンドの出力（実際の末尾） ④JSON・CSVの出力の形（表） ⑤仕様への疑問・曖昧な点 ⑥やらなかったこと ⑦次の周への申し送り。
