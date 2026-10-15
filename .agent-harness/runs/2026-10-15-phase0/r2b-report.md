# R2b 報告書: 打刻の対・CSV取込・休憩の不足・月次の集計・explain（実装担当: Claude Opus）

## 第2周の後半は完了し、品質の基準が通った（cargo deny の勧告の検査だけは、この作業コピーでは動かない）

指示書の5つの部品（`kintai-core::punch`、`kintai-import`、`kintai-jp-labor::{breaks, report, explain}`）を作り、試験を足した。全体の試験は 226 件が合格、失敗は0件（R1 の終了時は129件）。担当の受け入れ条件（AC-01.1〜01.8、AC-08.1〜08.7、AC-10.3〜10.7）は、すべて試験から参照がある。100万行の打刻CSVの取込は 114〜140 ミリ秒で、目標の3秒を大きく下回った。境界に誤りを入れる変異試験は、39件すべてを検出し、生き残りは0件（1回目に生き残った2件は、試験を足して赤くした）。

`cargo deny check` は、bans・licenses・sources は通信なしで通った。advisories（勧告）だけは、この作業コピーの勧告の一覧に git の情報がなく、通信なしでも動かなかった。代わりに、使っている部品の版を勧告のファイルと手で突き合わせ、該当がないことを確かめた（③）。本体のリポジトリで流し直してほしい。

仕様に書かれていない所を、読みで決めた点がある。特に確認してほしいのは次の4つで、詳しくは④に書いた。

1. 秒を分に丸めた結果、続けて打刻した2つの勤務が1分だけ重なるとき、前の勤務の終わりを次の始業で切った（④の1）。
2. 打刻の異常のある従業員は、渡された打刻の全期間で確定集計を拒否する（④の4）。
3. 休暇CSVの `hours`（対応外）は値として返し、拒否は使う側に任せた。CSVの検査の順は、JSONのケースの順と同じではない（④の8）。
4. `report.md` の `leave_days` は作っていない。指示書の項目の一覧にも、`kintai report` の引数にも休暇の入力がないため（④の10）。

## ① 作った・変えたファイル（調整役が本体へ取り込む一覧）

R1 のコード（`lib.rs`、`policy.rs`、`shift.rs`、`classify.rs`、`provenance.rs`、`kintai-cli` など）は変えていない。依存は足していない。

- `crates/kintai-core/src/punch.rs`（下書きを置き換え）
- `crates/kintai-core/tests/ac01_punch.rs`、`ac01_punch_properties.rs`
- `crates/kintai-import/Cargo.toml`（`csv` の宣言を外した。理由は下）
- `crates/kintai-import/src/lib.rs`（ファイルの大きさの境界の単体試験を1つ含む）
- `crates/kintai-import/tests/ac01_import.rs`、`ac10_limits.rs`、`ac10_properties.rs`
- `crates/kintai-import/tests/fixtures/synthetic/` の合成CSV 6本（UTF-8＋BOM＋CRLF、Shift_JIS＋CRLF、Shift_JIS の日本語の値、見出しなし、休暇、履歴）
- `crates/kintai-import/examples/bench.rs`
- `crates/kintai-jp-labor/src/breaks.rs`、`report.rs`、`explain.rs`（下書きを置き換え）
- `crates/kintai-jp-labor/tests/ac01_breaks.rs`、`ac08_report.rs`、`ac08_properties.rs`、`explain_day.rs`、`explain_properties.rs`
- `Cargo.lock`（`csv` を外したので、cargo が `csv` と `csv-core` の項目を自動で消した。ほかは変わらない）
- この作業フォルダ: `r2b-progress.md`、`r2b-report.md`、`r2b-mutations.json`（変異39件）、`r2b-mutate.log`（最終の変異試験）、`r2b-mutate-run1.log`（1回目）、`r2b-gates.txt`（品質の基準の出力）

`csv` を外した理由: `csv` の部品は、空の行を黙って飛ばし、閉じていない引用符も誤りにせずに読む。そのため「行番号と列名だけを示すエラー」（AC-10.7）の行番号がずれ、壊れた行を厳密に拒否できない。データの値はすべて ASCII で、改行を含む値も要らないので、引用符に対応した1行ずつの読み方を `lib.rs` に書いた（約60行）。`csv` が要るなら、R3 が `kintai-cli` 側で宣言すればよい。

## ② 受け入れ条件と試験の対応

| 条件 | 確かめている試験（ファイル: 主な試験） |
|---|---|
| AC-01.1 | core `ac01_punch.rs`: 休憩つきの勤務・夜勤・複数日・並びによらない・従業員ID順。性質: 正しい打刻の列は必ず対になり、区分の計算の入力の条件を満たす |
| AC-01.2 | core `ac01_punch.rs`: 遷移の表の外の9通り、24時間ちょうど（1439分は可）、秒の切り上げで24時間になる勤務、同時刻、異常の時点と番号、全件の一覧。性質: 任意の打刻でパニックしない |
| AC-01.3 | core `ac01_3_anomalous_employee_gets_no_shifts`（型でも、`PairingOutcome::Refused` は勤務を持てない） |
| AC-01.4 | core: 開始側の切り捨て・終了側の切り上げ、1分未満の休憩の消去、同じ1分の中の終業と始業、切った終わりと休憩。性質: 実際に働いた秒は、どれも丸めた実労働に入る |
| AC-01.5 | core: `reject` で秒のある打刻は最も小さい番号の誤り、秒が0なら丸めは何もしない（性質） |
| AC-01.6 | jp-labor `ac01_breaks.rs`: 360・361・480・481分、8時間1分に45分は15分不足、勤務の間の空き、夜勤の労働日、打刻からの通し。性質: 数え直しと一致 |
| AC-01.7 | jp-labor `ac01_7_no_automatic_deduction`（勤務も区分の結果も変わらない）、警告の文 |
| AC-01.8 | import `ac01_import.rs`: UTF-8＋BOM＋CRLF、Shift_JIS、Shift_JIS の日本語の値は形の誤り、読めないバイト、見出しなし・足りない・重なる・余分な列。性質: 列の並び・CRLF・BOM・引用符によらず同じ値 |
| AC-08.1 | jp-labor `ac08_report.rs`: 項目の合計、公的な60時間超の例（延長4380・60時間超780・休日120）、労働日の月の期間。性質: 区分の結果の期間の合計と一致 |
| AC-08.2 | `ac08_2_overtime_excludes_legal_holiday_work`（日曜12時間は休日720・延長0、土曜は延長240） |
| AC-08.3 | 29分と30分、60時間超は丸めない、元の分は変えない。性質: 差は30分以内 |
| AC-08.4 | `ac08_4_no_daily_rounding_value`（値は2つだけ。日ごとの丸めの名前は全部 `None`。列挙の値が増えるとコンパイルで気づく） |
| AC-08.5 | 遅い方（支払期日が遅い・完了の日が遅い）、厚生労働省の例（4/30 完結・5/15 支払 → 5/15 起算）、状況の記録、民法の期間の端（2月29日・2月28日） |
| AC-08.6 | `ac08_6_meta_is_always_present`（従業員0人・拒否だけでも付く）。保存期限と explain の出力にも付く |
| AC-08.7 | core `ac08_7_employee_id_format`、import `ac08_7_employee_id_in_csv`、jp-labor `ac08_7_report_uses_checked_employee_ids` |
| AC-10.3 | import `examples/bench.rs`（100万行を測る） |
| AC-10.4 | core `ac10_4_same_input_same_result`・並びを変えても同じ（性質）、import の同じ入力は同じ結果（性質） |
| AC-10.5 | import 性質: 任意のバイト列、CSVらしいバイト列、正しいCSVを壊したもの。core 性質: 任意の打刻 |
| AC-10.6 | import `ac10_limits.rs`: 256MiB ちょうどと＋1（`read_limited` と各読み取り。取込の大きさの検査の境界は `lib.rs` の単体試験）、4096と4097バイト、10万人と10万1人、日付の範囲の両端。core: 1か月1000件と1001件 |
| AC-10.7 | import `ac10_7_messages_have_no_row_content`（9通り）と性質（どの列を壊しても目印が文に出ない）、入出力の誤りの中身も出さない |
| （explain） | jp-labor `explain_day.rs`: 厚生労働省の例1・例2・60時間超の例の区間と率、2023-04-01 の境界、法定内の深夜、週の超過、法定休日をまたぐ勤務、拒否・不一致、率の表、根拠の照合。性質: 区間の分が区分の結果と一致 |

公開した関数は、すべて試験から呼んでいる。R1 の根拠の表にない条文（34条1項、33条1項、38条1項、108条、109条、附則143条1項、労基則20条1項・2項、54条1項・2項、56条1項2号・5号・2項・3項、24条の7、附則71条、安衛則52条の7の3第2項）は、使うファイルに定数を置き、試験で手元の条文（`statutes-2026-10-15.txt`）の文言と照合した。

## ③ 品質の基準のコマンドの出力

全文は `r2b-gates.txt` にある。各コマンドの末尾は次のとおり。

```
$ cargo fmt --all -- --check                       → 出力なし [終了コード 0]
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.35s      [終了コード 0]
$ cargo test --workspace
   Doc-tests kintai_jp_labor
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
（試験の結果の行33個の集計: 合格 226 件、失敗 0 件）                          [終了コード 0]
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
   Generated …/.cargo-target/doc/kintai/index.html and 3 other files        [終了コード 0]
$ cargo deny --offline check
[ERROR] failed to get 'FETCH_HEAD' metadata … fatal: not a git repository     [終了コード 1]
  （勧告の一覧 .cargo-home/advisory-dbs/advisory-db-… に .git がない。通信なしでは更新もできない）
$ cargo deny --offline check bans licenses sources
bans ok, licenses ok, sources ok                                            [終了コード 0]
$ （手作業）Cargo.lock の版と、勧告のファイルの patched・unaffected の突き合わせ
勧告のファイルがある部品は8つ（9件）で、使っている版はどれも patched の範囲  → 該当なし
$ python3 tools/check-spec-trace.py
条件 114件（manual 3件は対象外）。参照の無い条件: 49件                        [終了コード 0]
  （R1 の終了時は69件。残りは R2a・R3・別実装の担当の条件で、AC-01・08 と AC-10.3〜10.7 は0件）
$ cargo +1.88 check --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.68s      [終了コード 0]
```

速さ（release、Apple M4 Pro、rustc 1.99.0）: 100万行・33,250,027 バイトの打刻CSVの取込は 114〜140 ミリ秒（3回）。参考に、同じ打刻の対（1,000人）は 62〜66 ミリ秒。

変異試験（`python3 tools/mutate.py r2b-mutations.json`。作業用の写しは `TMPDIR` で作業コピーの中に置いた）: 打刻10件・取込11件・休憩4件・月次8件・explain 6件の、計39件。1回目は37件を検出し、2件が生き残った。「従業員IDの2文字目以降に `=` を許す」は、先頭の `=` しか試していなかったため。「256MiB ちょうどのファイルを拒否する」は、取込の関数に 256MiB ちょうどを渡す試験がなかったため。途中の `=` `+` `@` などの例と、大きさの検査を小さな関数に切り出した単体試験（巨大な入力を作らずに境界を見る）を足し、流し直して39件すべてを検出した（`r2b-mutate.log`。1回目は `r2b-mutate-run1.log`）。変異の側は弱めていない。

## ④ 仕様への疑問・曖昧な点・読みで決めた点

一致させられなかった仕様の文は、ない。次は、仕様が決めていない所を読みで決めたもの。

1. **丸めた勤務の1分の重なり**: `worker_favorable` では、終業が18:00:30（切り上げて18:01）、次の始業が18:00:45（切り捨てて18:00）だと、2つの勤務が1分重なる。R1 の勤務の検査は重なりを不正な入力にするので、前の勤務の終わりを次の始業（18:00）で切った。その1分は次の勤務の分になり、働いた時間は数から漏れない（性質の試験で確かめた）。前の勤務の最後の休憩が切った終わりに届くときは、その休憩の始まりで勤務を終える。つなぐ・異常にする、の案もある。
2. **24時間の判定は丸めたあとの長さ**: 23時間59分59秒の勤務も、終業を切り上げると1440分になるので `LongShift` にした。区分の計算に1440分以上の勤務を渡さないため。
3. **同時刻**: 同じ従業員の秒まで同じ打刻は、種別が違っても `SameTimestamp` にした（終業と次の始業が同時刻でも）。打刻だけで「接する2つの勤務」を書くには、1秒以上ずらす必要がある。
4. **「その期間の確定集計を拒否」**: `pair` は対象の月を知らないので、渡された打刻のどこかに異常があれば、その従業員の勤務をすべて返さない。CLI が CSV 全体を渡すと、9月の異常で10月の集計も拒否される。月の外の勤務は週の数えに要るので、月で切り分けるのは R3 の判断になる。
5. **表にない遷移の異常の名前**: 休憩中の `break_start` は `BreakStartWithoutIn`、勤務中の `break_end` は `BreakEndWithoutStart`、休憩中の `in` は `DuplicateIn` にした。異常のあとの立て直し（重なった `in` は前の勤務を捨てて始め直す、など）は、異常を続けて出しすぎないための実装の選択。
6. **秒の扱いの置き場所**: `punch.md` は「取込で分にする」と書くが、指示書に合わせて `pair` の引数（`SecondsPolicy`）にした。取込は秒のまま読む。
7. **入力の上限の読み**: 256MB は 256MiB（268,435,456 バイト）、4KB は改行（CR・LF）を除く4096バイトで、文字コードを読む前のバイトで数える。1か月1,000件の勤務は、暦の月ごとの `in` の数で数え、`pair` の誤り（不正な入力）にした。履歴CSVの `total_minutes` は、仕様に範囲がないので 0〜44,640（31日×1440分）にした。
8. **CSVの読み方と検査の順**: 引用符は使えるが、値の中の改行は不可。空の行はファイルの末尾だけ可。改行でない CR は不可。前後の空白は取り除かない。見出しの列名は大文字・小文字を区別する。見出しだけのファイルは0件、空のファイルは「1行目に見出しがない」。誤りは、大きさ → 行の長さ → 文字コード → 見出し → データの行（上から。1つの行では `employee_id` が先）の順に、最初の1つを返す。JSONのケースの「対応外 → 日付の書式」の順はCSVには決まっていないので、休暇CSVで `hours`（対応外）より下の行に日付の誤りがあると、取込は日付の誤り（終了コード2）を返す。`hours` は `LeaveUnit::Hours` として返し、`LeaveHourly` で拒否するのは使う側（R3）。
9. **文字コード**: BOM があれば UTF-8 だけで読む。BOM がなく UTF-8 として正しければ UTF-8、そうでなければ Shift_JIS（encoding_rs の SHIFT_JIS。Windows の CP932 を含む）。正しい値はすべて ASCII なので、どちらで読むかは、誤りの種類（文字コードの誤りか、値の形の誤りか）にだけ効く。
10. **`leave_days`**: `report.md` の表の注記に `leave_days` があるが、指示書の項目の一覧、R3 の CSV の列、`kintai report` の引数のどれにもない。休暇の入力がないので作っていない。半日の数え方も決まっていない。
11. **保存期限**: (a) 期限は民法の数え方で、起算日の翌日から数え、n 年後の応当日の前日（応当日がなければ月末）にした。ふつうは「起算日の応当日の当日」と同じだが、平年の2月28日から数えて期限がうるう年に入ると1日違う（2027-02-28 の5年 → 2032-02-29）。仕様の「応当日の当日まで保存」がこの端まで意図しているか、確認してほしい。(b) 労働時間の状況の記録は、`report.md` のとおり完了の日から数え（`--wage-due` は使わない）、本則も3年にした。AC-08.5 の「遅い方」と、調査Bの「遅い日から5年」とは違う。(c) 年休管理簿は、`--completed` を期間の満了の日として、支払期日との遅い方（労基則56条3項の準用）。「期間中」の保存は表せないので、満了後の期限だけを出す。
12. **explain の区間の割り当て**: 所定外法定内は、日の法定内の分のうち、時刻の後ろの分にした（先頭から所定労働時間までが所定内）。仕様は「日ごとに所定労働時間を引く簡易な値」とだけ書き、日の中の位置を決めていない。厚生労働省の例1（17〜18時が所定外）とは一致する。2023-04-01 より前は、60時間超の深夜（75%）の率も出さない（50%を前提にした率のため）。深夜の率の根拠には、区分の根拠に37条4項と労基則20条の該当の項を足した。注意として `ExtraWithinIsContractual` と、その日の曖昧さの記録も付けた。
13. **根拠の定数の置き場所**: 新しい条文の定数は `provenance.rs`（R1 のファイル）に足せないので、`breaks.rs`・`report.rs`・`explain.rs` に置いた。次の周で `provenance::ALL` に移すとよい。
14. **境界の日時**: 2099-12-31 23:59:30 の終業は、切り上げると 2100-01-01T00:00 になり、R1 の勤務の検査で不正な入力になる。
15. このコピーの `cli.md` には、調整役の連絡にあった「打刻の異常を `refused`（`PunchAnomaly`）で返す」記述はまだ入っていない。`PairingOutcome::Refused` に異常の一覧があるので、R3 はそのまま使える。

## ⑤ やらなかったこと

- AC-10.1・10.2（区分と36協定の速さ）、AC-10.8（`demo --out` の権限）、コマンドライン（AC-12）は R3 の担当として残した。
- `tools/mutations/` への変異の仕様の追加は R3 の担当なので、この作業フォルダの `r2b-mutations.json` に置いた（そのまま使える）。
- `cargo deny check` の advisories は、この作業コピーでは動かない（③）。
- AC-01.9 [manual]: 日ごとの切り捨てや15分丸めの機能は、公開APIにない。`SecondsPolicy` は `worker_favorable` と `reject`、`Rounding` は `none` と `monthly_30min` だけ。
- 作業上の逸脱が1つある。品質の基準の出力を一時的に `/tmp` の2つのファイルに書いてしまい、すぐに消した（中身は cargo deny と cargo doc の出力だけ）。以後は作業フォルダの中だけに書いた。変異試験の道具の一時フォルダも、`TMPDIR` で作業コピーの中（`.cargo-target/mutate-tmp`）に向けた。

## ⑥ 次の周（R3）への申し送り

**つなぎ方**: `kintai_import::read_limited(file)` → `read_punches(&bytes)` で `ImportedPunches { punches, lines }` を得る → `kintai_core::punch::pair(&punches, seconds)`（従業員ID順）→ `Paired` の各 `PairedShift` を `ShiftInput { start: span().start(), end: span().end(), breaks: breaks().iter().map(|b| (b.start(), b.end())).collect() }` にして `ShiftSet::new` → `classify(&policy, &set, data_from)`。`pair` の誤りと異常の `index` は、`imported.line(index)` で行番号に直せる。`--data-from` の既定（CSV全体の最初の日付）は、打刻の `time.floor().date()` の最小。

**部品**: 月次の集計は `report::MonthlyReport::build(&policy, rounding, Some(period), &[ReportInput { employee, shifts, classification }])`（従業員ID順、拒否・曖昧さ・休憩の不足・注意・`table_checked_on`・`review_status` つき）。1人分だけなら `ledger_for`。休憩の不足は `breaks::break_shortages(&set)`、説明は `explain::explain_day(&policy, &set, &classification, date)`、保存期限は `report::retention(kind, completed, wage_due)`。方針ファイルの値は `SecondsPolicy::from_code`・`Rounding::from_code`・`RetentionKind::from_code`・`PunchKind::from_code`・`LeaveUnit::from_code` で読める（知らない値は `None`）。出力の名前は `code()` を使う。

**エラーの文**: `ImportError` の文は行番号と列名だけで、入力の中身を含まない。`PunchError` の文は `punches[番号]` の形なので、CLI では行番号に直して出すとよい。
