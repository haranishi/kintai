# 実装指示書 R2b: 打刻の対・CSV取込・休憩の不足・月次の集計・explain（入出力と説明の部品）

あなたは実装担当です。日本の労働基準法に基づく勤怠のOSS「kintai」（Rust）の、第2周の後半を実装します。第1周（R1）で `kintai-core`、`kintai-jp-labor::{policy, shift, classify, provenance}`、`kintai-cli` の `case`（区分）ができています。**仕様を変えず**、仕様どおりに作り、仕様に疑問があれば止まらずに「疑問」として報告書に書いてください。

## 作業場所

作業場所は `__WS__`（以下 `$WS`）です。本体のリポジトリから調整役がコピーした、**あなた専用のコピー**です。並行して R2a が別のコピーで作業しています。このコピーの外（本体のリポジトリ、R2a のコピー）には**アクセスしない**。終わったら調整役が、あなたが作ったファイルを本体へ取り込みます。別実装（Python のオラクル）は、独立性を保つため、このコピーに入れていません。探さないでください。

## 守ること（違反は失敗）

1. **git の操作をしない。** このコピーは git のリポジトリではありません。
2. 書き込んでよい場所は、下の「触ってよいファイル」と、`$WS/.agent-harness/runs/2026-10-15-phase0/` の中だけ。`docs/` `Cargo.toml`（ルート）`deny.toml` `tools/` は**読むだけ**（仕様の誤りは報告書へ）。
3. **新しい依存を足さない。** `kintai-import` が `csv` `encoding_rs` を使うのは許可済み（`Cargo.toml` に宣言済みなら、そのまま使う）。`kintai-core` と `kintai-jp-labor` の実行時の依存はゼロ。開発時のみ `proptest` `serde_json` `chrono`（すでに宣言された所だけ）。
4. **他の勤怠・人事・給与ソフト（GPL/AGPL のOSSを含む）のコード・画面・仕様書を探したり、見たりしない。** 仕様と法令の条文だけから書く。ネットワークは使えない。
5. 実データ・実在の人名を使わない。試験データは合成。
6. `unsafe` を使わない。ライブラリのコードで `unwrap()` `expect()` `[]` の範囲外アクセスなど、パニックしうる書き方をしない（試験は可）。整数は桁あふれを検査する。浮動小数点を使わない。

## 環境（ネットワークなし）

```
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME=__WS__/.cargo-home CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=__WS__/.cargo-target
cd __WS__
```

## 触ってよいファイル（これ以外は触らない）

- `crates/kintai-core/src/punch.rs`（調整役が空の下書きを置いた。`lib.rs` に `pub mod punch;` は入っている）と、`crates/kintai-core/tests/` の新しい試験ファイル（名前は `ac01_*.rs`）。
- `crates/kintai-import/` の全部（`src/` `tests/` `examples/`、`Cargo.toml` の依存の宣言は理由を報告書に書けば最小の変更可）。
- `crates/kintai-jp-labor/src/breaks.rs` `report.rs` `explain.rs`（調整役が空の下書きを置いた。`lib.rs` に `pub mod` の行は入っている）と、`crates/kintai-jp-labor/tests/` の新しい試験ファイル（名前は `ac01_*.rs` `ac08_*.rs` `explain_*.rs`）。
- **`lib.rs`（core と jp-labor）と `kintai-cli` は触らない。** `pub use` もしない。並行して R2a が `limits36.rs` `leave.rs` `holidays.rs` と `kintai-cli` の `case36.rs` `case_leave.rs` を作ります。

R1 が作ったコードに不足や不具合を見つけても、**直さず報告書に書く**。どうしても進めない場合に限り、最小の変更をして、「R1のコードを変えた」と理由を報告書に書く。

## 読むもの

1. `docs/spec/punch.md`、`docs/spec/report.md`、`docs/spec/nfr.md`（入力の上限・個人情報・決定性）
2. `docs/spec/validation.md`、`docs/spec/classify.md`、`docs/spec/core.md`
3. `docs/spec/cli.md`（打刻CSV・休暇CSV・履歴CSVの列、`explain` の中身）
4. R1の報告書 `.agent-harness/runs/2026-10-15-phase0/r1-report.md` と、R1が作った公開API（型・関数・エラーの流儀。**それに揃える**）。日付の型は入力の範囲外（2099年を超える）の値も持てる
5. 参考: `research/track-A.md` のQ7〜Q9、`research/track-B.md` のQ5

## 作るもの

### 1. `kintai-core::punch`（AC-01.1〜01.5）
打刻のイベント（従業員ID・時点（秒まで）・種別 `in` `out` `break_start` `break_end`）の列を、従業員ごとに時刻順に並べて、`docs/spec/punch.md` の状態遷移で勤務（始業・終業・休憩の区間）に対にする。異常の種類（`DuplicateIn` `OutWithoutIn` `BreakStartWithoutIn` `BreakEndWithoutStart` `OutDuringBreak` `OpenShift` `SameTimestamp` `LongShift`）を返す。秒の方針 `worker_favorable`（開始側は切り捨て・終了側は切り上げ）と `reject`。結果は決定的な並び（従業員ID順、時刻順）。`kintai-core` は外部の依存を持たない。秒を持つ時点の型が必要なら、`punch.rs` の中に作り、公開のAPIを増やしすぎない。

### 2. `kintai-import`（AC-01.8、AC-10.5〜10.7 の取込側）
- 打刻CSV（`employee_id,timestamp,kind`）、休暇CSV（`employee_id,date,unit`）、履歴CSV（`employee_id,month_start,total_minutes`）を読む。UTF-8（BOMがあれば取り除く）とShift_JIS（`encoding_rs`）、改行はCRLFもLF。ヘッダ行は必須（列名がなければ拒否）。列の順序はヘッダで決める。余分な列は拒否。
- 従業員IDは `[A-Za-z0-9][A-Za-z0-9._-]{0,63}`。それ以外は拒否（AC-08.7）。
- 入力の上限（`nfr.md`）: ファイル256MB、1行4KB、従業員10万人、日付は2000-01-01〜2099-12-31。超えたら不正な入力。
- **エラーの文に、入力の行の中身を含めない。** 行番号と列名だけを示す（AC-10.7）。
- どんなバイト列でもパニックしない（`proptest` で任意のバイト列と、壊れたCSVを与える。AC-10.5）。
- 100万行を3秒以内に読む（AC-10.3）。速さの測定用に `crates/kintai-import/examples/bench.rs` を作る（合成データを生成して、読む時間を表示する）。

### 3. `kintai-jp-labor::breaks`（AC-01.6、01.7）
労働日ごとの休憩の不足の警告（`BreakShortage`、不足の分数つき）。必要な休憩は、実労働 L が360分以下で0、361〜480で45、481以上で60。実際の休憩は、勤務の内側の休憩と、同じ労働日の勤務と勤務の間の空きの合計。自動で控除しない。

### 4. `kintai-jp-labor::report`（AC-08.1〜08.6）
`docs/spec/report.md`。区分の結果（R1の `Classification`）から、従業員・月の期間ごとの台帳の項目（`work_days` `work_minutes` `overtime_minutes` `over60_minutes` `holiday_minutes` `night_minutes` `within_minutes` `extra_within_minutes`）を作る。丸め（`none` と `monthly_30min` だけ。日ごとの丸めは型として存在させない）。保存の目安（`retention`: 種類ごとに `statutory_current` `statutory_base` `product_default` の3つの期限の日付。起算日は完了日と賃金の支払期日の遅い方。`kintai retention` が使う関数）。出力に `table_checked_on` と `review_status: "unreviewed"` が必ず付く。

### 5. `kintai-jp-labor::explain`
ある従業員のある労働日について、区間ごと（連続して同じ区分・同じ深夜の属性の区間）の説明の構造を返す。区分（法定内・所定外法定内・時間外・時間外60時間超・法定休日）、深夜か、割増の下限（%）、根拠（`provenance` の条文）。割増の下限の表は次のとおり。法定内0%（労基法32条）、時間外25%（37条1項、割増賃金令）、時間外60時間超50%（37条1項ただし書）、法定休日35%（37条1項、割増賃金令）、深夜は加算して25%（37条4項）、時間外＋深夜50%・時間外60時間超＋深夜75%（労基則20条1項）・法定休日＋深夜60%（労基則20条2項）。2023-04-01より前の日付では、時間外60時間超の率を出さず、注意の文を付ける。文章の整形はCLI側。この周は構造とその試験まで。

## 試験

試験の名前かコメントに `AC-xx.y` を書く。必須: 各ACの境界の例（360/361/480/481分、秒の切り上げ・切り捨て、同時刻、24時間、30分丸めの29分と30分、保存期限の「遅い方」、IDの形式、ヘッダなし、Shift_JIS、BOM）、`proptest`（任意のバイト列でパニックしない／対にした勤務の実労働＋休憩＝始業から終業／丸めは元の値以下か1時間未満の差）。AC-01・08・10 の `[rust]` 条件（この周で担当する分）に参照があること。

## 品質の基準（完了の条件）

次がすべて通ること。通った出力を報告書に貼る。
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check
python3 tools/check-spec-trace.py      # AC-01, 08, 10 の [rust] 条件（この周で担当する分）が参照されていること
```
MSRV（1.88）の確認は、R1 の報告書に書かれた方法に合わせる。

## 報告

作業中は `.agent-harness/runs/2026-10-15-phase0/r2b-progress.md` に1〜3行ずつ追記する。最後に `r2b-report.md` に次を書く: ①作った/変えたファイル（調整役が本体へ取り込む一覧。R1のコードを変えたなら明記） ②各受け入れ条件がどの試験で確かめられているか（表） ③品質の基準のコマンドの出力（実際の末尾） ④仕様への疑問・曖昧な点・一致させられなかった点 ⑤やらなかったこと ⑥次の周への申し送り。
