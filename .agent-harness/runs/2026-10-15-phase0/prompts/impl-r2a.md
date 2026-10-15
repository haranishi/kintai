# 実装指示書 R2a: 36協定の上限・年次有給休暇・国民の祝日（kintai-jp-labor の純粋な計算）

あなたは実装担当です。日本の労働基準法に基づく勤怠のOSS「kintai」（Rust）の、第2周の前半を実装します。第1周（R1）で `kintai-core`、`kintai-jp-labor::{policy, shift, classify, provenance}`、`kintai-cli` の `case`（区分）ができています。**仕様を変えず**、仕様どおりに作り、仕様に疑問があれば止まらずに「疑問」として報告書に書いてください。

## 作業場所

作業場所は `__WS__`（以下 `$WS`）です。本体のリポジトリから調整役がコピーした、**あなた専用のコピー**です。並行して R2b が別のコピーで作業しています。このコピーの外（本体のリポジトリ、R2b のコピー）には**アクセスしない**。終わったら調整役が、あなたが作ったファイルを本体へ取り込みます。別実装（Python のオラクル）は、独立性を保つため、このコピーに入れていません。探さないでください。

## 守ること（違反は失敗）

1. **git の操作をしない。** このコピーは git のリポジトリではありません。
2. 書き込んでよい場所は、下の「触ってよいファイル」と、`$WS/.agent-harness/runs/2026-10-15-phase0/` の中だけ。`docs/` `Cargo.toml` `deny.toml` `tools/` は**読むだけ**（仕様の誤りは報告書へ）。
3. **新しい依存を足さない。** `kintai-core` と `kintai-jp-labor` の実行時の依存はゼロ（`kintai-jp-labor` は `kintai-core` のみ）。`kintai-cli` は `serde` `serde_json` `pico-args`。開発時のみ `proptest` `serde_json` `chrono`（すでに宣言された所だけ）。
4. **他の勤怠・人事・給与ソフト（GPL/AGPL のOSSを含む）のコード・画面・仕様書を探したり、見たりしない。** 仕様と法令の条文（`docs/legal/sources/statutes-2026-10-15.txt`）だけから書く。ネットワークは使えない。
5. 実データ・実在の人名を使わない。試験データは合成。
6. `unsafe` を使わない。ライブラリのコードで `unwrap()` `expect()` `[]` の範囲外アクセスなど、パニックしうる書き方をしない（試験は可）。整数は桁あふれを検査する。浮動小数点を使わない。

## 環境（ネットワークなし）

```
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME=__WS__/.cargo-home CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=__WS__/.cargo-target
cd __WS__
```
依存は取得済み。`cargo build/test/clippy/fmt/doc` と `cargo deny check`、`python3` が使える。

## 触ってよいファイル（これ以外は触らない）

- `crates/kintai-jp-labor/src/limits36.rs` `leave.rs` `holidays.rs`: 調整役が空の下書き（`//!` の説明行だけ）を置いた。`lib.rs` には `pub mod` の行が入っている。**`lib.rs` は触らない。`pub use` もしない。**
- `crates/kintai-jp-labor/tests/` の新しい試験ファイル（名前は `ac05_*.rs` `ac06_*.rs` `ac07_*.rs`）。
- `crates/kintai-cli/src/case36.rs` `case_leave.rs`（下の「CLIとの接続」）。
- `crates/kintai-cli/tests/` の新しい試験ファイル（名前は `ac09_36_*.rs` `ac09_leave_*.rs`）。

R1 が作ったコード（`kintai-core`、`policy.rs`、`classify.rs`、`dto.rs` など）に不足や不具合を見つけても、**直さず報告書に書く**。どうしても進めない場合に限り、最小の変更をして、「R1のコードを変えた」と理由を報告書に書く。

## 読むもの（この順）

1. `docs/spec/core.md`、`docs/spec/validation.md`（検査の順序、`null` と重複の規則。**36協定と年休のケースも同じ流儀**）
2. `docs/spec/limits36.md`、`docs/spec/case-format-36-leave.md`、`docs/spec/vectors/limits36-official.json`
3. `docs/spec/leave.md`、`docs/spec/vectors/leave-official.json`
4. `docs/spec/holidays.md`、`docs/legal/sources/syukujitsu-2026-10-15.csv`、`equinox-official-2026-10-15.csv`、`equinox-naoj-forecast-2020-2050.csv`
5. R1 の報告書 `.agent-harness/runs/2026-10-15-phase0/r1-report.md` と、R1 が作った公開API（型・関数・エラーの流儀。**それに揃える**）。`kintai-cli/src/{case,dto,failure}.rs` の書き方も揃える
6. 参考（根拠の引用）: `research/track-B.md`（36協定）、`research/track-C.md`（年休・祝日）

## 作るもの

### 1. `kintai-jp-labor::limits36`（AC-05.1〜05.15）

`limits36.md` の判定1〜9と `case-format-36-leave.md` のとおり。間違えやすい点:

- `year_start_month`（1〜12）は `none` でも必須。限度は0以上で、法定の最大（2700・21600・5999・43200・6回）を超えたら不正な入力。その種類で使わない限度の項目は、範囲だけ検査して使わない。
- O と H は別々に持つ。T＝O＋H。**平均は割らず、合計と `4800×n` を比べる。**
- 発動月は O>2700 の月。協定の月限度が2700より小さいとき、その限度を超え2700以下の月は発動月に数えず `GeneralMonthlyLimitExceeded`。
- 累計の違反（`AnnualOvertimeExceeded` `GeneralAnnualLimitExceeded` `SpecialAnnualExceeded`）は、超えた月から協定年の終わりまで毎月付く。`SpecialCountExceeded` は、回数が超えた**発動月**だけ（判定8）。
- `months` の最初の月が協定年の途中なら、その年の月は `year_overtime_known` が偽。偽のときは下限値がすでに超えている違反だけを出す（判定9）。
- 履歴が足りない n は `sums` が `null`、`findings` に `UnknownHistory`（`n` つき）。`findings` の並びは、コードの表の順、同じコードは `n` の昇順。`notices` は常に `["OwnEmployerOnly"]`。
- `history`: 必須の配列（空でもよい）。協定月の初日でない日付、`months` の最初の月以後の月、同じ月の2回は不正な入力。並びは問わない。`months` は空でもよい（出力の `months` も空）。

### 2. `kintai-jp-labor::leave`（AC-06.1〜06.12）

`leave.md` と `case-format-36-leave.md` のとおり。間違えやすい点:

- **日付の規則**（`leave.md` の「日付の規則」）。`期間の終わり(S, n)` と `次の初日(S, n)` を関数として作り、付与日 g_k＝`次の初日(g0, 12k)`（**g0 から数える**）、失効日＝`次の初日(付与日, 24)`、5日の期間＝付与日〜`期間の終わり(付与日, 12)` を、すべてこの関数で出す。雇入れ2023-08-29 の例（g0〜g4、失効、期間）を試験にする。
- 付与日数の優先順位: ①週5日以上か週1800分以上→通常 ②年間所定労働日数があれば区分（週の日数より優先） ③週の日数の行（週0日は `LeaveOutOfTable`）。付与表（通常7行・比例28セル）は `docs/legal/sources/statutes-2026-10-15.txt` の労基法39条と労基則24条の3から作り、試験で全セルを条文の写しと突き合わせる。
- 出勤率 `below` の年は付与しない（勤続年数は進む）。`unknown` は付与して `AttendanceUnconfirmed` を1回。`grants` を明示したときは `attendance` を使わず、注意も付けない。
- `as_of` より後の付与と取得は出さず、数えない（残数の検査もしない）。`unit` は `full`（2半日）と `half`（1半日）。1日の取得は、残り半日のロットと次のロットにまたがってよい。残数が足りない記録は不正な入力。
- 5日取得義務: 付与が10日（20半日）以上のときだけ対象。状態名は `Met` `InProgress` `NotMet` `NotApplicable`。`needed_halves`＝max(0, 10−`taken_halves`)、`Met` と `NotApplicable` は0。期間が重なるときは、両方に数える。
- 管理簿: 付与ごとに、基準日、付与日数、**5日の期間に入る取得**（そのロットから消化した分とは限らない）、取得日数の計。
- 対応外を見る順: `LeaveHourly`（`unit` が `hours`）→ `LeaveSplitGrant` → `LeaveOutOfTable`。`LeaveSplitGrant` は `grants` で、10日（20半日）未満の付与の次の付与が、`次の初日(その付与日, 12)` より前にあるとき。第二基準日の結合の特例は扱わない。
- 必須は `hire_date` `weekly_scheduled_days` `weekly_scheduled_minutes` `attendance`。`annual_scheduled_days` と `grants` は、ない・`null` のどちらも指定なし。

### 3. `kintai-jp-labor::holidays`（AC-07.1〜07.6）

`holidays.md` のとおり。2020〜2027年は公式CSVの表を、**Rustのソースに埋め込む**（`syukujitsu-2026-10-15.csv` から2020年以降の行を `const` の表にする。出典、取得日、SHA-256 のコメントを付ける）。2028〜2099年は規則から生成し `Estimated`、2020年より前は `OutOfRange`。春分・秋分は `floor(20.8431 + 0.242194*(y-1980) - floor((y-1980)/4))`（春分）と `floor(23.2488 + 0.242194*(y-1980) - floor((y-1980)/4))`（秋分）。**整数の固定小数点で実装する**（浮動小数点を使わない）。公式の16件と、国立天文台の予想表の62件（`equinox-naoj-forecast-2020-2050.csv`）の全件と一致することを試験にする。振替休日（3条2項）と国民の休日（3条3項）は、2条の祝日に再帰して混ぜない。種別（`Statutory` `Substitute` `Citizens`）を返す。祝日は法定休日の判定に使わない。

### CLIとの接続

調整役が、`kintai case` の `schema` による振り分け（`kintai.case36.v1`→`case36::run`、`kintai.caseleave.v1`→`case_leave::run`）を済ませています。`case36.rs` と `case_leave.rs` には、不正な入力を返すだけの下書きの `run` があります。**`run` の名前と型は変えず**、中身を作ってください。R1 の区分のケースの流儀（JSON の読み方、重複したキーの拒否、未知の項目の拒否、`null` の扱い、エラーの形、終了コード、出力の整形）に**揃える**。検査の順序は `validation.md` と同じ（形と型 → 列挙と範囲 → 対応外 → 日付の書式と整合 → 計算）。`kintai.case36.v1` → `kintai.limits36.v1`、`kintai.caseleave.v1` → `kintai.leave.v1`。試験は、実際のバイナリを `std::process::Command` で動かす。

## 試験

試験の名前かコメントに `AC-xx.y` を書く（`python3 tools/check-spec-trace.py` が数える）。必須:

- 公的な数値例: `docs/spec/vectors/limits36-official.json` と `leave-official.json` を `env!("CARGO_MANIFEST_DIR")` から読む。36協定の `expect` に書かれた項目は全部検査する。`derived-` の36協定の期待値は `findings_by_month`（キー＝月の初日、値＝その月の findings のコード（n は省略）の配列。書いていない月は findings が空）と `special_count_by_month`（書いた月だけ一致）。年休の `derived-` も含める。通らない vector があれば、仕様と vector のどちらが誤りかを報告書に書く（vectors は調整役が直す）。
- 仕様の各規則の境界: Tが5999・6000分、合計が4800×nちょうどと+1、発動月が6回目と7回目、協定月の限度が小さい場合、協定年の替わり、履歴不足、年の途中から始まる入力、付与日の応当日なし、失効の前日と当日、半日、5日の達成がちょうど10半日、期間が重なる、`as_of` と付与日が同じ日、`grants` の明示など。
- `proptest`: 36協定は、Tを等しく分割しても和は不変、協定年の替わりで ΣO が戻る。年休は、残数が常に非負で付与−使用−失効に等しい、取得を足しても期間の取得合計は単調増加。
- 祝日: 規則から生成した2020〜2027年と公式CSVの全件一致（143件）、春分・秋分16件と予想表62件、3条の休日が再帰しないこと。
- 壊した入力（型違い、未知の項目、重複したキー、`null`、範囲外）で、パニックせず `InvalidInput` を返すこと。

## 品質の基準（完了の条件）

次がすべて通ること。通った出力を報告書に貼る。
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check
python3 tools/check-spec-trace.py      # AC-05, 06, 07, 09.5〜09.7 の [rust] 条件が参照されていること
```
MSRV（1.88）の確認は、R1 の報告書に書かれた方法に合わせる。

## 報告

作業中は `.agent-harness/runs/2026-10-15-phase0/r2a-progress.md` に、段階ごとに1〜3行追記する（途中で止まっても引き継げるように）。最後に `r2a-report.md` に次を書く: ①作った/変えたファイル（調整役が本体へ取り込む一覧。R1のコードを変えたなら明記） ②各受け入れ条件がどの試験で確かめられているか（表） ③品質の基準のコマンドの出力（実際の末尾） ④仕様への疑問・曖昧な点・一致させられなかった点 ⑤やらなかったこと ⑥次の周への申し送り。
