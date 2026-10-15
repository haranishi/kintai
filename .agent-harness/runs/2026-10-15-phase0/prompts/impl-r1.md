# 実装指示書 R1: kintai-core と、時間の区分（kintai-jp-labor::classify）

あなたは実装担当です。リポジトリ `~/Projects/kintai`（Rust、日本の労働基準法に基づく勤怠のOSS）の第1周を実装します。設計と仕様は決まっています。**仕様を変えず**、仕様どおりに作り、仕様に疑問があれば止まらずに「疑問」として報告書に書いて、仕様どおりに実装してください。

## 守ること（違反は失敗）

1. **git の操作をしない**（commit・push・branch・stash・add すべて）。
2. 書き込んでよい場所は、`crates/` と `tools/` と `.agent-harness/runs/2026-10-15-phase0/` の中だけ。`docs/` と `Cargo.toml`（ルート）と `deny.toml` と `.github/` は**読むだけ**（仕様の誤りは報告書へ）。
3. **新しい依存を足さない。** 使ってよい依存は次だけ。実行時: `kintai-cli` に `serde`(derive)・`serde_json`・`pico-args`。`kintai-core` と `kintai-jp-labor` の実行時の依存はゼロ（`kintai-jp-labor` は `kintai-core` のみ）。開発時のみ: `chrono`・`proptest`（`kintai-core`）、`proptest`・`serde_json`（`kintai-jp-labor`）。`Cargo.toml` の依存の行を増やす必要があるときは、その理由を報告書に書き、最小の変更にとどめる（すでに取得済みの依存だけが使える）。
4. **他の勤怠・人事・給与ソフト（GPL/AGPLのOSSを含む）のコード・画面・仕様書を探したり、見たりしない。** 仕様と法令の条文（`docs/legal/sources/statutes-2026-10-15.txt`）だけから書く。ネットワークは使えない。
5. 実データ・実在の人名を使わない。試験データは合成。
6. `unsafe` を使わない。ライブラリのコードで `unwrap()`・`expect()`・配列の範囲外アクセスなど、パニックしうる書き方をしない（試験は可）。整数は桁あふれを検査する。浮動小数点を使わない。

## 環境（ネットワークなし）

```
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME=~/Projects/kintai/.cargo-home CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=~/Projects/kintai/.cargo-target
```
依存は取得済み。`cargo build/test/clippy/fmt/doc` と `cargo deny check` が使える。`python3` が使える。

## 読むもの（この順）

1. `docs/spec/core.md`（日付・分・区間）
2. `docs/spec/classify.md`、`docs/spec/over60.md`、`docs/spec/unsupported.md`、`docs/spec/validation.md`（**検査の順序・対応外の指定・曖昧さの厳密な定義。別実装と一致させる要**）
3. `docs/spec/case-format.md`（入出力のJSON。`kintai case` の区分のケース）
4. `docs/spec/vectors/official-examples.json`（公的な数値例7件。期待値つき）
5. `docs/architecture.md`（設計の理由）
6. 参考: `.agent-harness/runs/2026-10-15-phase0/research/track-A.md`（根拠の引用。読み物）

## 作るもの

### 1. `crates/kintai-core`（外部依存なし）
`docs/spec/core.md` の型と規則（日付、曜日、時点、区間、表記、月の期間、週の起算）。受け入れ条件 AC-02.1〜AC-02.7 を満たす。公開APIには `///` の説明を付ける（`missing_docs` が警告で、clippy は `-D warnings`）。

### 2. `crates/kintai-jp-labor`（`kintai-core` のみに依存）
- `provenance`: 法令の根拠を表す型（法令名、条項、e-GovのURLと版ID、確認日 `2026-10-15`）。この周で使う根拠（労基法32条・37条・35条・36条5項ほか、割増賃金令、労基則20条）を定数で持つ。
- `policy`: `docs/spec/case-format.md` の `policy` を表す型と検査。`WorkPolicy::validate` が、範囲外を不正な入力、対応外を `Unsupported`（`docs/spec/unsupported.md` のコード）で返す。検査の順序と、`worker_kind` `holiday_system` `night_window` を含む対応外の指定は `docs/spec/validation.md` のとおり（**順序を変えない**）。
- `classify`: `docs/spec/classify.md` の手順の実装。入力は方針と勤務（開始・終了・休憩の区間）。出力は日ごとと月の期間ごとの結果、曖昧さの記録、拒否。**区間の演算で実装する。1分ずつ回すループにしない**（速さのため。別の担当が、1分ずつ数える別実装を書き、差分を取る）。
  - 実労働の区間を暦日の0:00で分ける、平日型・法定休日の分類、労働日ごとの先頭からの割り当て、週の枠、月の期間の60時間超（時刻順）、所定外法定内、深夜（22:00以上5:00未満。区分ごとの内数、60時間超の深夜）を実装する。
  - 勤務が法定休日の暦日・週の境界・月の期間の境界をまたぐとき（定義は classify.md の「境界」）は、`ambiguity = reject` で拒否（結果の `refused`）、`use_default` で曖昧さの記録つきで計算する。
- 結果の型は、`case-format.md` の出力JSONと1対1に対応させる（フィールド名・意味）。出力の並びは決定的にする（`BTreeMap` か整列した `Vec`。`HashMap` の走査順に依存しない）。

### 3. `crates/kintai-cli`（最小）
- `kintai --version`（版とgitの短いハッシュ。ハッシュが取れなければ `unknown`）、`kintai --help`。
- `kintai case FILE`: 区分のケース（`schema = "kintai.case.v1"`）を読み、結果のJSON（`kintai.classify.v1`）を標準出力へ。JSONは `serde_json` で、フィールドの並びは `case-format.md` の例の順、インデントなしの1行 + 末尾の改行。エラーは標準エラーに `{"error": {"code": ..., "message": ...}}`、終了コードは `docs/spec/case-format.md` の表（0・2・3・4）。`schema` が未知なら不正な入力（2）。この周は区分のケースだけでよい（36協定・年休は次の周）。
- DTO（JSON用の型）は `kintai-cli` に置き、`kintai-core`・`kintai-jp-labor` を `serde` に依存させない。

## 試験（受け入れ条件との対応）

試験の名前かコメントに `AC-xx.y` を書く（`python3 tools/check-spec-trace.py` が参照を数える）。必須の試験:
- AC-02.x: `crates/kintai-core/tests/` で `chrono`（開発時のみ）と2000〜2099年の全日付を突き合わせる。表記の往復と不正な表記の拒否。
- AC-03.x / AC-04.x / AC-11.x: `crates/kintai-jp-labor/tests/` に、①公的な数値例（`vectors/official-examples.json` を `env!("CARGO_MANIFEST_DIR")` から読む）、②仕様の各規則の小さな手書きの例（境界: 480分ちょうど・481分、週の上限ちょうど、22:00ちょうど・5:00ちょうど、3600分ちょうど・3601分、24:00に終わる勤務はまたがない、月の起算日が1と16、など）、③`proptest` の性質（`total = within + overtime + holiday`、`night = night_within + night_overtime + night_holiday`、`over60 <= overtime`、深夜 <= 総実労働、休憩を足しても実労働が増えない、順序を入れ替えても結果が同じ）。
- AC-09.x の一部（`case` の入出力、エラーの形、終了コード）: `crates/kintai-cli/tests/`（実際のバイナリを `std::process::Command` で動かす）。
- ライブラリの全公開関数に、少なくとも1つの試験。

## 品質の基準（完了の条件）

次がすべて通ること。通った出力を報告書に貼る。
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check
python3 tools/check-spec-trace.py        # AC-02, 03, 04, 09.1〜09.4, 11 の [rust] 条件が参照されていること
```
`tools/check-links.py` は `docs/` を対象にするので、触らなくてよい。

## 報告

作業中は `.agent-harness/runs/2026-10-15-phase0/r1-progress.md` に、段階ごとに1〜3行追記する（途中で止まっても引き継げるように）。最後に `.agent-harness/runs/2026-10-15-phase0/r1-report.md` に次を書く: ①作った/変えたファイル ②各受け入れ条件がどの試験で確かめられているか（表） ③品質の基準のコマンドの出力（要約でなく実際の末尾） ④仕様への疑問・曖昧な点・仕様と実装を一致させられなかった点 ⑤やらなかったこと ⑥次の周への申し送り。
