# R1 報告書: kintai-core・kintai-jp-labor の区分・`kintai case`（実装担当: Claude Opus）

## 第1周は完了し、品質の基準と変異試験がすべて通った

第1周の実装は完了した。品質の基準の6コマンドと Rust 1.88 の `check` がすべて終了コード0で、試験は129件すべて合格した。境界の規則に誤りを入れる変異試験も33件すべて検出し、生き残りは0件だった。

仕様は作業中に2回更新された（02:41〜02:42 と 02:52〜02:53）。実装は 02:53 の版と、調整役の連絡1〜4に合わせてある。別実装（Python）との差分試験の前に、2点をそろえる必要がある。一つは、別実装が同じ版の仕様で書かれていること。もう一つは、JSON の読み方の端（数の `-0` と `true`、`NaN`、同じ名前の項目）で、詳細は④の1と2に書いた。

## 調整役の連絡1〜4は、すべて反映した

1. 検査: 同じ名前の項目が2回ある JSON は、`crates/kintai-cli/src/strict_json.rs` が手順1で拒否する。任意の項目の `null` は欠けと同じに扱い、必須の項目の `null` は型の誤りにした。`legal_holiday_weekday` の欠けと `null` だけは手順3の `LegalHolidayUnspecified` になる。接する休憩と接する勤務は許す。接する勤務はつながず、別の勤務として数える（始業の日が違えば労働日も別になる）。日時の範囲は 2000-01-01T00:00〜2099-12-31T23:59 で、2100-01-01T00:00 に終わる勤務は不正な入力になる。区切りは `T` と空白の両方を読む。
2. 出力: `periods` は実労働のある期間だけを出す。`ambiguities` は reject では空の配列で、`notices` の欄は作っていない。
3. 日付の型: `Date` と `Instant` の型は 1900-01-01〜2200-12-31 を持てる。入力の範囲（2000〜2099年）を見るのは入口だけで、`Date::parse`・`Instant::parse` と `FromStr` がそれに当たる。公開APIの形は⑥にまとめた。
4. 試験データ: 更新後の `official-examples.json` の10件（公的な7件と `derived-incomplete-week-*` の3件）が、ライブラリでも実際のバイナリでも通る。`refused`・`ambiguities`・`exit_code`・`data_from` も比べている。通らない数値例はなかった。

## ① 作った・変えたファイル

`Cargo.toml`（ルートと各クレート）、`Cargo.lock`、`docs/` は変えていない。依存も足していない。

- `crates/kintai-core/src/`: `lib.rs`、`calendar.rs`、`date.rs`、`instant.rs`、`interval.rs`、`period.rs`、`weekday.rs`、`error.rs`
- `crates/kintai-core/tests/`: `ac02_calendar.rs`、`ac02_instant_interval.rs`、`ac02_notation.rs`、`ac02_periods.rs`
- `crates/kintai-jp-labor/src/`: `lib.rs`、`provenance.rs`、`policy.rs`、`shift.rs`、`classify.rs`
- `crates/kintai-jp-labor/tests/`: `common/mod.rs`（1分ずつ数える試験用の素朴な実装を含む）、`ac03_official_vectors.rs`、`ac03_rules.rs`、`ac03_properties.rs`、`ac03_incomplete_week.rs`、`ac04_over60.rs`、`ac11_policy.rs`、`ac13_shifts.rs`、`provenance.rs`
- `crates/kintai-cli/`: `build.rs`、`src/main.rs`・`case.rs`・`dto.rs`・`failure.rs`・`strict_json.rs`、`tests/common/mod.rs`・`ac09_case_cli.rs`・`ac09_properties.rs`・`ac11_unsupported_cli.rs`・`ac13_data_from_cli.rs`
- `tools/mutations/r1-classify.json`（変異33件）
- この作業フォルダ: `r1-progress.md`、`r1-report.md`、`r1-gates.txt`（コマンドの出力）、`r1-mutate.log`（変異試験のログ）

## ② 受け入れ条件と試験の対応

| 条件 | 確かめている試験（ファイル: 主な試験） |
|---|---|
| AC-02.1 | `ac02_calendar.rs`: 2000〜2099年の全36,525日と、1900〜2200年の全109,938日を chrono と突き合わせ。加減と差も全日付 |
| AC-02.2〜02.4 | `ac02_instant_interval.rs`: 分解と合成の往復・加減の往復（proptest）、端が接する・1分重なる、0:00での分割の合計（proptest） |
| AC-02.5・02.6 | `ac02_notation.rs`・`ac02_calendar.rs`: 全日付の往復、2月30日・25時・24:00・60分の拒否、入力の範囲外の拒否、計算の結果は2200年まで作れること |
| AC-02.7 | `ac02_periods.rs`: 起算日1と16、月と年をまたぐ場合、全日付・全起算日での包含 |
| AC-03.1〜03.9、03.11、03.12 | `ac03_rules.rs`: 480分ちょうど・481分、週の上限ちょうど、二重計上なし、44時間、法定休日、22:00・5:00ちょうど、所定外法定内、日をまたぐ勤務、月をまたぐ週、3つの境界と24:00 |
| AC-03.10、04.6 | `ac03_official_vectors.rs`（全10件。素朴な実装とも一致）、CLI の `ac09_case_cli.rs` |
| AC-03.13 | `ac03_properties.rs`: 区間の演算と1分ずつ数える素朴な実装が無作為の入力300件で全件一致。合計の関係、順序の入れ替え、休憩の追加 |
| AC-03.15、03.16 | `ac03_incomplete_week.rs`、CLI の `ac13_data_from_cli.rs` |
| AC-04.1〜04.5、04.7 | `ac04_over60.rs`: 3600分と3601分、期間ごとの戻り、法定休日を数えない、後ろの分から超える、`night_over60`、起算日16 |
| AC-09.1〜09.4 | CLI の `ac09_case_cli.rs`・`ac09_properties.rs`（無作為のケース・壊したJSON・任意のバイト列でも終了コードは0・2・3・4だけ） |
| AC-11.1〜11.4 | `ac11_policy.rs`、CLI の `ac11_unsupported_cli.rs`（表の24行。全コードの条文） |
| AC-13.1〜13.5 | `ac11_policy.rs`・`ac13_shifts.rs`・`ac03_incomplete_week.rs`、CLI の `ac11_unsupported_cli.rs`・`ac13_data_from_cli.rs` |

ライブラリの公開関数は、すべて試験から呼んでいる（名前の照合で確認）。

## ③ 品質の基準のコマンドの出力

全文は `r1-gates.txt` にある。各コマンドの末尾は次のとおり。

```
$ cargo fmt --all -- --check                      → 出力なし [終了コード 0]
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s    [終了コード 0]
$ cargo test --workspace
   Doc-tests kintai_jp_labor
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
（試験バイナリ20個と文書の例3つの集計: 合格 129 件、失敗 0 件）              [終了コード 0]
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
   Generated ~/Projects/kintai/.cargo-target/doc/kintai/index.html and 3 other files  [終了コード 0]
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok                            [終了コード 0]
$ python3 tools/check-spec-trace.py
条件 114件（manual 3件は対象外）。参照の無い条件: 69件                       [終了コード 0]
$ cargo +1.88 check --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s    [終了コード 0]
```

参照の無い69件は、この周の範囲外（AC-01・05〜08・10・12、AC-09.5〜09.7）と、別実装側の [python] の2件（AC-03.14・13.6）である。範囲内の [rust] の条件（AC-02、03、04、09.1〜09.4、11.1〜11.4、13.1〜13.5）は、すべて参照がある。変異試験（`python3 tools/mutate.py tools/mutations/r1-classify.json`）は、基準が緑で33件すべて検出、生き残りは0件だった。

## ④ 仕様への疑問と、読みで決めた点

1. **仕様の版**（最重要）: 作業中に仕様が2回変わった。実装は 02:53 の版に従う。02:02 の版との違いは、対応外の表（`DeferredIndustry` がなくなり、`AutomobileDriver` や `HolidaySwap` などが加わって全25コードになった）、`holiday_swaps`、`data_from` と `IncompleteWeek`、同じ名前の項目の拒否、日付の型の範囲である。別実装が古い版で書かれていると、差分試験はこれらで必ず食い違う。
2. **JSON の読み方の端**: `serde_json` と Python の `json.loads`（既定）は、次の入力で答えが分かれる。`-0` は Rust では小数（-0.0）になり不正な入力、Python では整数0になる。`true` は Python では整数としても通る（`bool` は `int` の子）。`NaN` と孤立サロゲート（`"\ud800"`）は Python だけが読める。同じ名前の項目は、Python では後の値が残る。仕様に「整数は小数点と指数のない JSON の整数」「RFC 8259 の JSON だけを受け付ける」と書くか、別実装で `type(x) is int`・`parse_constant`・`object_pairs_hook` を使ってそろえる必要がある。
3. **入力の範囲の外の日付が出力に出る**: 起算日16の月の期間の初日（例 `1999-12-16`）と、`IncompleteWeek` の週の起算日（例 `1999-12-26`）は、2000年より前になりうる。型の範囲（1900〜2200年）の中なので、そのまま出力している。仕様はこの出力を明記していない。
4. **翌日にかかる分の帰属**（文面どおりの読み）: 法定休日の分も、その勤務の労働日の行に入れる。例えば日曜が法定休日で土22:00→日2:00 の勤務なら、日曜の120分は土曜の行の `holiday` に入る。日→月の勤務では、月曜の分が「労働日=日曜」の平日型として日曜の480分の枠と日曜の週で数えられる。どちらも曖昧さの記録が付く入力だが、`use_default` の答えは専門家の確認が要る。
5. **`data_from` より前の勤務**: 拒否の規定がないので受け付ける。その勤務の週は、週の起算日が `data_from` より前なので `IncompleteWeek` になる。
6. **必須かどうかが表にない項目**: `shifts` は必須（欠けたら不正な入力）、`special_44h_confirmed` は任意で既定は偽、と読んだ。`data_from` が範囲外（例 `1999-12-31`）なら、日時と同じく手順4の不正な入力にした。
7. **表から外れた値**: `worker_kind` の `construction` と `sugar_manufacturing` は、今の表と列挙のどちらにもないので不正な入力になる。「通常制として計算できる」の意図なら、`standard` を使うと仕様に書くとよい。
8. **条文の手元の写し**: 38条の3・38条の4、労基則18条・23条の本文と、割増賃金令の本則は `statutes-2026-10-15.txt` にない。この5つの根拠の定数は、手元の写しと照合できていない。版IDと URL は照合済み（`tests/provenance.rs`）。
9. **文書の食い違い**（小さいもの）: `architecture.md` は chrono との突き合わせを「1970〜2100年」と書くが、core.md は入力が2000〜2099年、型が1900〜2200年である。case-format.md の出力の例は、`ambiguity` が reject なのに `ambiguities` に記録がある（例として見れば支障はない）。

仕様と実装を一致させられなかった点は、ない。

## ⑤ やらなかったこと

- 36協定・年休のケース（AC-09.5〜09.7）、方針ファイル（TOML）、`report` `check` `leave` `explain` `demo`（AC-12）、`kintai-import`（AC-01・08）は次の周以降に残した。`MonthStartDayMismatch` は方針ファイルでだけ判定するので、コードと根拠は持つが、`case` では出ない。
- 非機能（AC-10）は扱っていない。速さは作業用の場所で試しに測っただけで、release ビルドの1,000人×31日（各日 9〜21時台の勤務と休憩1時間）の区分は、最良 9.0 ms だった（目標は100 ms）。`examples/bench.rs` と入力の上限（1人1か月1,000件など）は未実装。

## ⑥ 次の周への申し送り

**日付の公開API**（年休の担当向け）: 失効日など計算の結果は `Date::from_ymd` と `checked_add_days` で作る。どちらも1900-01-01〜2200-12-31 を許し、その外は `CoreError::OutOfRange` を返す。入力の文字列は `Date::parse`（または `str::parse`）で読み、2000〜2099年の外を拒否する。範囲は `Date::MIN`・`MAX`（型）と `INPUT_MIN`・`INPUT_MAX`（入力）、`is_input_range()` で分かる。ほかに `year`・`month`・`day`・`weekday`・`days_in_month`・`days_since`・`day_number`・`week(Weekday)`・`month_period(MonthStartDay)` がある。`Instant` も同じ形で、`parse`・`MIN`/`MAX`・`INPUT_MIN`/`INPUT_MAX`・`checked_add_minutes`・`date`・`minute_of_day` を持つ。`Week` と `MonthPeriod` の `first_day()` は `Result` を返し、表示（`YYYY-MM-DD`）は範囲によらずできる。

**区分の公開API**: `WorkPolicy::validate()`（手順2→3）→ `ShiftSet::new(&[ShiftInput])`（手順4。日時の入力範囲も見る）→ `classify(&ValidPolicy, &ShiftSet, data_from: Option<Date>)`（手順5）の順に呼ぶ。曖昧さの記録の日付は `RecordDate`（`LaborDay(Date)` か `WeekStart(Week)`）で、並びと重複の判定は暦の日で行う。

**CLI**: `kintai case` の読み込みは `strict_json` → `dto::read_case`（型）→ 列挙の読み替え → `validate` → 日時の解析、の順にしてある。36協定・年休のケースも、この流れで `schema` によって分ければよい。`--version` のハッシュは `build.rs` が `.git` のファイルを読んで決める（git コマンドは呼ばない）。いまは `.git` がないので `unknown` になる。環境変数 `KINTAI_GIT_HASH` で上書きできる。

**検証の工程へ**: `tools/mutations/r1-classify.json` をそのまま使える。差分試験の前に、④の1と2を別実装の担当とそろえること。
