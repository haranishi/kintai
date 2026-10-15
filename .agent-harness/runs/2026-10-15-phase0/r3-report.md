# R3 報告書: コマンドライン全体（方針ファイル、report・check・leave・explain・retention・demo）（実装担当: Claude Opus）

## 第3周のコマンドラインは完成し、品質の基準はリンク検査を除いて通った

方針ファイル（TOML）の読み込みと、`report` `check` `leave` `explain` `retention` `demo` を作った。合成データ（5人、2026-09-27〜10-31）を `tests/fixtures/synthetic/` に置き、`demo` はそれをそのまま埋め込んで使う。試験は90件を足し（初回82件、追加の修正で8件）、全体の418件が合格、失敗は0件。AC-12.1〜12.10 と AC-10.4〜10.8 は、すべて試験から参照がある。

品質の基準のうち2つは、このコピーの事情で通っていない。`check-links.py` は既存の7件で失敗する（`docs/testing.md` と `docs/benchmarks.md` が無い、調査記録の `downloads/`）。どれも触ってよい範囲の外で、私の変更とは関係しない（L2 の対応のあと、本体の最新に同期したコピーでは、別の4件で失敗する。末尾の節）。`cargo deny check` の勧告の検査は、前の周と同じく勧告の一覧に git の情報がなく動かない。bans・licenses・sources は通り、今回使い始めた TOML の部品は手で勧告と照合して該当がなかった。

調整役に決めてほしかった点は次の3つで、どれも仕様に採られた（詳しくは⑤）。

1. **CSV の末尾に3列を足した**（`table_checked_on,review_status,notices`）。指示書の列の一覧（10列）と、「すべての出力にメタ情報を付ける」（AC-08.6）がぶつかるため、台帳の10列の並びは変えずに後ろへ足した。
2. **`check` と `retention` は人が読む形だけ**にした。`cli.md` の表に `--format` がないため。`features.md` は「出力はJSONが基本」と書いており、食い違う。
3. **対象の従業員は、方針ファイルの `[[employee]]` 全員**（`--employee` で1人）。打刻のない人も0の行で出す。`worker_kind` の対応外は、その全員のうち1人でもあれば終了コード3になる。

作業上の逸脱が1つある。作業中に誤って `git status` を1回実行した。リポジトリではないので失敗し、何も変わっていない。

**査読 L2 への対応（07:55〜08:25）**: コマンド側の指摘8件（L2-01・02・04・09〜13）を直した。試験は全体で438件が合格し、失敗は0件（同期後の出発点427件に11件を足した）。L2 の変異26件と取込の変異30件は、すべて試験が見つけた。内容・試験・変えたファイル・品質の基準の結果は、末尾の節「L2への対応」。

**追加の修正（調整役の依頼3点。07:05〜07:13）**: `check` と `explain` に `--data-from D` を足した（`report` と同じ意味と既定）。`check` では、判定する最初の協定月も `--data-from` 以後で決める（読みは⑤の7）。AC-12.9・AC-12.10 の試験（`ac12_output_shape.rs`）を足し、既存の試験に条件の番号を書いた。`kintai --help` に `--data-from` を書き足した。

## ① 作った・変えたファイル（調整役が本体へ取り込む一覧）

`Cargo.toml`（ルートと各クレート）と `Cargo.lock` は変えていない。依存も足していない（`toml` と `pico-args` は R1 から宣言済み）。部品（core・import・jp-labor）は変えていない。

- 新規 `crates/kintai-cli/src/`: `policy_file.rs`（方針ファイル）、`args.rs`（引数）、`inputs.rs`（ファイル・打刻の対・区分・履歴・休暇）、`text.rs`（表・末尾の注意）、`common_out.rs`（拒否・異常・曖昧さ・休憩の不足の出力）、`report.rs`、`check.rs`、`leave.rs`、`explain.rs`、`retention.rs`、`demo.rs`
- 変更 `crates/kintai-cli/src/`: `main.rs`（振り分けと `--help`）。`dto.rs`・`case36.rs`・`case_leave.rs` は、列挙の読み替えを `*_from`（値がなければ `None`）に分けて方針ファイルと共用しただけで、ケースの振る舞いは変えていない
- 新規 `crates/kintai-cli/tests/`: `ac12_report_cli.rs`、`ac12_check_cli.rs`、`ac12_leave_cli.rs`、`ac12_explain_cli.rs`、`ac12_demo_cli.rs`、`ac12_errors_cli.rs`、`ac12_output_shape.rs`（追加の修正）、`ac10_cli_properties.rs`。変更: `common/mod.rs`（末尾に部品を足した）
- 新規 `tests/fixtures/synthetic/`: `policy.toml`、`punches.csv`（440件）、`leaves.csv`、`history.csv`
- 作業フォルダ: `r3-progress.md`、この報告書、`r3-gates.txt`（品質の基準の全出力）、`r3-self-mutate.log`（自分のコードへの変異19件と、出力の形の変異8件）
- 取り込まないもの: `$WS/.tmp/`（打刻CSVを作ったスクリプト `gen_punches.py`、変異のスクリプト、デモの試し出力）

## ② 受け入れ条件と試験の対応

| 条件 | 確かめている試験（ファイル: 主な試験） |
|---|---|
| AC-12.1 | `policy_file.rs` の単体試験（型・知らない項目・欠け・範囲・行番号、対応外の表の順、`--employee` で対応外の範囲を絞る、手順4の日付）。`ac12_errors_cli.rs` の `ac12_1_policy_file_errors_through_the_command`（2が12通り、3が8コード、ID つき）。`ac12_check_cli.rs` の `MonthStartDayMismatch` |
| AC-12.2 | `ac12_report_cli.rs`: JSON の全員の値（手で数えた値）、CSV の列と行、text の表、`--employee`、打刻のない月と人、`--data-from`、打刻の異常、`use_default`、秒の `reject`、方針にないID、`rounding = "none"` |
| AC-12.3 | `ac12_check_cli.rs`: 違反3件で1／フラグなしで拒否の4、違反なしで0、履歴なしの `UnknownHistory` は違反、休憩の不足だけでも1、打刻の異常は1と4、協定なし、範囲外の月、履歴の誤りの行番号、`use_default` の記録、`--data-from`（`IncompleteWeek` の拒否と回避、最初の協定月） |
| AC-12.4 | `ac12_leave_cli.rs`: 5人の1人分が `kintai case` の結果と項目も値も一致、text の各欄、時間単位（ID つき3、`--employee` で回避、基準日より後は数えない）、表の外、残数不足の行番号、明示の付与 |
| AC-12.5 | `ac12_explain_cli.rs`: E001 10/08 の5区間（区分・%・根拠・確認日・版ID・URL）、祝日は法定休日でない（スポーツの日）、法定休日35%、60時間超50%、拒否で4、労働のない日、`--data-from`（`IncompleteWeek` の拒否と回避） |
| AC-12.6 | `ac12_errors_cli.rs` の `ac12_6_case_handles_three_schemas`（と既存の `ac09_*`） |
| AC-12.7 | `ac12_demo_cli.rs`: `--out` だけに書く・中身は合成データと同じ・全コマンドの出力と場面、`--out` なしは新しい一時フォルダ（`TMPDIR` を試験の場所に向ける）、空でないフォルダ・ファイル・親のないパスは2。`demo.rs` の単体試験 |
| AC-12.8 | `ac12_errors_cli.rs`: 引数の誤り19通り（`--data-from` の誤りを含む）が JSON で2、表のとおり0・1・3・4。`args.rs` の単体試験。性質の試験の形の検査 |
| AC-12.9 | `ac12_output_shape.rs`: `report` `leave` `explain` の JSON の項目名を、出力の文字列から書かれた順のまま読み、`cli-output.md` の並びと比べる（入れ子の各オブジェクトまで。`leave` の1人分と `explain` の `day` は、ケースの出力と同じ並び）。`report` の CSV の見出しと13列。値の確認は `ac12_report_cli.rs` `ac12_leave_cli.rs` `ac12_explain_cli.rs` |
| AC-12.10 | `ac12_output_shape.rs`: 5つのコマンドの7通りの人が読む出力で、注意の見出しと4つのコード、表の確認日、確認の状態、最後の行が参考値の1行。`text.rs` の単体試験と、各コマンドの text の試験 |
| AC-10.4 | `ac12_report_cli.rs`: 3つの形で2回同じバイト列、打刻の行を逆順にしても同じ。`ac12_demo_cli.rs`: 書く場所の表示を除いて同じ |
| AC-10.5 | `ac10_cli_properties.rs`: 任意のバイト列の方針は2、壊した方針・打刻・休暇・履歴、無作為の引数（各48件）。終了コードは0〜4、2・3は JSON |
| AC-10.6 | `policy_file.rs` の単体試験（4096/4097バイト、10万/10万1人）。`ac12_errors_cli.rs`: 1行4096バイトちょうどは読め1バイト超で2、範囲外の日付と月、256MiB＋1バイトの方針ファイル |
| AC-10.7 | `policy_file.rs` の単体試験と `ac12_errors_cli.rs`（打刻CSV5通り・方針4通りで目印が文に出ない）、方針にない従業員IDを文に出さない（report・check・leave） |
| AC-10.8 | `ac12_demo_cli.rs`: フォルダ0700・ファイル0600（`--out` と一時フォルダ） |

ほかに、AC-08.2・08.5・08.6・08.7・01.5・03.15 をコマンドから確かめる試験がある。自分のコードの境界に19件の変異を入れ、17件を試験が見つけた。生き残った1件は、CSV で拒否した人の行を除く条件で、拒否した人は台帳の行を持たないので結果が変わらない（等価）。1件は指定の誤りで対象外。追加の修正では、出力の型の項目の順を入れ替える変異と、末尾の参考値の行を消す変異の8件を入れ、AC-12.9・AC-12.10 の試験がすべて見つけた。

## ③ 品質の基準のコマンドの出力

全文は `r3-gates.txt`（追加の修正3点のあとで流し直したもの）。環境は指示書のとおりで、`TMPDIR` も `$WS/.tmp` に向けた。末尾は次のとおり。

```
$ cargo fmt --all -- --check                                    → 出力なし [終了コード 0]
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.39s      [終了コード 0]
$ cargo test --workspace
    all doctests ran in 0.54s; merged doctests compilation took 0.33s
    （試験の結果の行53個の集計: 合格 418 件、失敗 0 件）                       [終了コード 0]
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    Generated …/.cargo-target/doc/kintai/index.html and 3 other files      [終了コード 0]
$ cargo deny check            （1回だけ流した）
    [ERROR] failed to fetch advisory database … fatal: not a git repository [終了コード 1]
$ cargo deny --offline check bans licenses sources
    bans ok, licenses ok, sources ok                                        [終了コード 0]
$ python3 tools/check-spec-trace.py
    条件 116件（manual 3件は対象外）。参照の無い条件: 6件
      AC-03.14 AC-06.11 AC-05.11 AC-10.1 AC-10.2 AC-13.6                 [終了コード 0]
$ python3 tools/check-links.py   （GIT_CEILING_DIRECTORIES で $WS の外を探さない）
    docs/faq.md:34: リンク切れ benchmarks.md
    検査したMarkdown: 69件、問題: 7件                                      [終了コード 1]
$ cargo +1.88 check --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.27s      [終了コード 0]
```

参照の無い6件は、別実装の側の4件（AC-03.14・05.11・06.11・13.6）と、R3m の担当の2件（AC-10.1・10.2）。`cargo deny` の bans は `winnow` の2つの版（0.7.15 と 1.0.4。`toml` が持ち込む。`Cargo.lock` は前から同じ）を警告するだけ。リンク検査の7件は、`CONTRIBUTING.md`・`README.md`・`docs/development.md` から `docs/testing.md` へ、`docs/faq.md` から `docs/benchmarks.md` へのリンク（どちらもこのコピーに無い）と、調査記録の `downloads/` `notes_B.md`（本体では git が除外するもの）。

## ④ JSON・CSV の出力の形

エラーは従来どおり標準エラーに `{"error":{"code","message"}}` の1行。JSON は1行で、末尾に改行。日付は `YYYY-MM-DD`、時点は `YYYY-MM-DDTHH:MM`、分と半日は整数。

**`report --format json`（`kintai.report.v1`）**

| 項目 | 中身 |
|---|---|
| `schema` `month` | `"kintai.report.v1"`、`--month` の `YYYY-MM` |
| `period_start` `period_end` | 対象の月の期間の初日と最後の日（含む） |
| `data_from` | 使った値（`--data-from` か打刻CSV全体の最初の日付。打刻がなければ `null`） |
| `rounding` | `none` か `monthly_30min` |
| `employees[]` | 従業員ID順。`employee_id`、`ledger`、`refused`、`ambiguities`、`break_shortages` |
| `ledger[]`（0か1件） | `period_start` `work_days` `work_minutes` `overtime_minutes` `over60_minutes` `holiday_minutes` `night_minutes` `within_minutes` `extra_within_minutes` `rounded`（`{overtime, holiday, night}` は時間。`none` では `null`）。拒否なら空 |
| `refused` | `null` か `{code, dates, anomalies}`。`code` は `AmbiguousBoundary` か `PunchAnomaly`、`dates` は重複なし昇順、`anomalies[]` は `{kind, date, line}`（打刻の異常のときだけ） |
| `ambiguities[]` | `{date, code}`（`use_default` で、期間に入る記録） |
| `break_shortages[]` | `{date, work_minutes, break_minutes, required_minutes, shortage_minutes}`（期間の労働日） |
| `notices` `table_checked_on` `review_status` | 4つの注意、`"2026-10-15"`、`"unreviewed"` |

**`report --format csv`**: UTF-8、LF、見出し行あり。列は `employee_id,period_start,work_days,work_minutes,overtime_minutes,over60_minutes,holiday_minutes,night_minutes,within_minutes,extra_within_minutes,table_checked_on,review_status,notices`（`notices` は4つのコードを `;` でつなぐ）。1人1行（対象の月の期間）。拒否した人の行は出さない（終了コード4で分かる）。値は ASCII の英数字と `-` `.` `_` `;` だけで、引用符は使わない。

**`leave --format json`（`kintai.leavereport.v1`）**: `schema`、`as_of`、`employees[]`、`notices`、`table_checked_on`、`review_status`。`employees[]` は `employee_id` のあとに `kintai.leave.v1` と同じ `lots` `balance_halves` `five_day` `register` `notices`（`AttendanceUnconfirmed`）を同じ順で持つ。

**`explain --format json`（`kintai.explain.v1`）**

| 項目 | 中身 |
|---|---|
| `employee_id` `date` `weekday` | 従業員、労働日、曜日（`sun`〜`sat`） |
| `legal_holiday` | その曜日が方針の法定休日か |
| `national_holiday` | `null` か `{name, kind}`（`Statutory` `Substitute` `Citizens`） |
| `holiday_table` | `{status, notices, source}`（`Official`／`Estimated`／`OutOfRange`、`EquinoxUnverified`、出典の1行） |
| `day` | その日の区分の合計（`kintai.classify.v1` の `days` と同じ12項目）か `null` |
| `segments[]` | `{start, end, minutes, category, night, premium}`。`category` は `within` `extra_within` `overtime` `over60` `holiday`。`premium` は `{percent, basis[]}`（`percent` は2023-04-01より前の60時間超で `null`）、`basis[]` は `{law, article, revision_id, url, confirmed_on}` |
| `notes[]` | `{code, message}`（`ExtraWithinIsContractual`、`Over60RateNotShown`、曖昧さのコード） |
| `refused` `notices` `table_checked_on` `review_status` | `report` と同じ |

**`check` と `retention`（人が読む形だけ）**: `check` は従業員ごとに協定月の表（O・H・T・直近2〜6か月の合計・発動月の数・年の時間外・判定。下限のときは「（下限）」）、判定の説明と根拠、休憩の不足、打刻の異常、曖昧さの記録、違反と警告の件数、`OwnEmployerOnly` を含む注意。`retention` は `statutory_current: 日付` `statutory_base: 日付` `product_default: 日付` の3行を、この名前で出す（機械でも読める）。どの text の出力も、注意・`table_checked_on`・`review_status` のあと、最後の1行が「この出力は参考値です（専門家の確認前。review_status: unreviewed）。」。

**終了コード**: `report` と `explain` は拒否があれば4。`check` は、`--fail-on-violation` で違反か警告があれば1、そうでなく拒否があれば4。`leave` と `retention` は0か2・3。`demo` は書けたら0（各コマンドの終了コードは出力の中に示す）。

## ⑤ 仕様への疑問・曖昧な点（読みで決めたもの）

一致させられなかった仕様の文はない。次は、仕様が決めていない所を読みで決めた点。1・2・3・5・6・8〜11・13 は、調整役が仕様（`cli.md`・`cli-output.md`）に採った。4・12・14 と、7 の新しい読み（`check` の最初の協定月）は未決。

1. **CSV のメタ情報**（最重要）: 冒頭の1。代わりの案は「CSV は10列のまま、メタ情報は JSON と text だけ」と仕様に書くこと。
2. **`check` と `retention` の形**: 冒頭の2。`--format json` を足すなら、`check` は `kintai.limits36.v1` の `months` を従業員ごとに並べる形が自然。
3. **対象の従業員の範囲**: 冒頭の3。「打刻CSVにいる人だけ」の読みもありうる。`report` で打刻のない人を0の行で出すのは、台帳から人が黙って抜けないようにするため。
4. **複数のファイルの検査の順**: 仕様は JSON のケースの順だけを決める。コマンドでは「方針の手順1→2 → `--employee` → 方針の対応外 → CSV の取込（形・値・上限）→ CSV の従業員ID → 打刻の対（秒の `reject`、1か月1000件）→ 年休の対応外（`leave`）→ 方針の日付（手順4）→ 計算」にした。CSV の取込は形と日付の誤りを分けられないので、CSV の日付の誤り（2）は、年休の対応外（3）より先に返る。
5. **`leave` にも労働時間の対応外を当てる**: `work_system`、`worker_kind`、`MonthStartDayMismatch` などは、方針ファイルの決まりとして `leave` でも見る（年休の計算そのものは、これらで変わらない）。外すなら仕様に書いてほしい。
6. **方針ファイルの必須**: `[agreement36]` と `[[employee]]`（空の `employee = []` は可）は、どのコマンドでも必須。`[[employee]]` の `hire_date` `weekly_scheduled_days` `weekly_scheduled_minutes` `attendance` は、`report` でも必須（年休のケースと同じ）。`worker_kind` は `[[employee]]` だけに書ける（`[workplace]` では知らない項目）。`[workplace]` は JSON のケースと同じく `holiday_system` `holiday_swaps` `night_window` も受け付ける。`legal_holiday_weekday` の欠けは `LegalHolidayUnspecified`（3）。`seconds` と `rounding` の既定は `worker_favorable` と `none`。日付は TOML の文字列だけ（TOML の日付の型は型の誤り）。`kind = "none"` の `month_start_day` は任意で、書けば範囲（1〜28）だけ見る。協定月の起算日は、どの種類でも `workplace.month_start_day` を使う。先頭の UTF-8 の BOM は取り除く。
7. **`data_from`**（追加の修正で解決）: 打刻CSVの最初の日付が月曜なら、既定では最初の週が `IncompleteWeek` で拒否される。`check` と `explain` にも `--data-from` を足したので、3つのコマンドとも `--data-from` で避けられる（試験あり）。**新しい読み**: `check` の判定する最初の協定月は「打刻がそろっている最初の日（`--data-from`。既定は打刻CSVの最初の日付）以後で最初に始まる協定月」にした。既定のときは `cli.md` の文（打刻CSVの最初の日付以後）と同じ。違うのは `--data-from` を書いたときだけで、例えば打刻が 10/05 からしかなくても、`--data-from 2026-09-27`（その間は勤務なし）と伝えれば10月を判定する（文のとおりだと、最初の協定月が11月になり10月は判定できない）。`--data-from` を打刻CSVの最初の日付より後にしたときは、それより前の週の勤務が `IncompleteWeek` で拒否されるので、どちらの読みでも判定はしない。`cli.md` の「`check` の月」の文を「打刻がそろっている最初の日（`--data-from`）以後」に直すとよい。打刻が1件もないときは、`--data-from` があっても不正な入力のまま。`check` と `explain` の人が読む出力には、使った `data_from` の1行を足した（`explain` の JSON の項目は文書のとおりで、足していない）。
8. **曖昧さの拒否も打刻の全体で見る**: 打刻の異常は仕様が全体で見ると書く。曖昧さは、区分を打刻の全体で計算するので、前の月の境界の勤務でも、その人の対象の月が拒否される。
9. **`check` の範囲と終了コード**: 判定した協定月（最初の協定月から対象の月まで）のすべての判定を違反に数え、休憩の不足と曖昧さの記録もその範囲で出す。打刻の異常は違反でもあり拒否でもあるので、`--fail-on-violation` では1、フラグなしでは4。打刻が1件もないときと、対象の月が最初の協定月より前のときは、判定できないので不正な入力にした。
10. **履歴CSVと休暇CSVの従業員ID**: 打刻CSVと同じく、方針にないIDは不正な入力にした（行番号つき、IDは文に出さない）。
11. **`AttendanceUnconfirmed`**: `report.md` は付ける記録に挙げるが、年休の入力がない `report` には出さず、`leave` だけに出す。`leave_days` は Phase 1 なので作っていない（R2b と同じ）。
12. **`demo`**: `--out` のない一時フォルダは OS の一時フォルダ（Unix系では環境変数 `TMPDIR`）の下に、プロセス番号と通し番号の名前で作る。環境変数は場所にだけ使い、計算には使わない。既にある空のフォルダは、権限を変えずに使う。`case` はデモに入れていない（`getting-started.md` の並びに合わせた）。
13. **`refused` の形**: ケースの `{code, dates}` に `anomalies` を足し、曖昧さでも打刻の異常でも同じ形にした。
14. **text の表の揃え**: ASCII でない文字を幅2と数える。端末の字形によってはずれる。

## ⑥ やらなかったこと

- AC-10.1〜10.3（速さ）と `tools/mutations/`（R3m の担当）。自分のコードの変異は、作業用のスクリプトで確かめただけ。
- `docs/` の更新（読むだけのため）。④の形は、調整役が `docs/spec/cli-output.md` に取り込み済み。`cli.md` の「`check` の月」の文の直し（⑤の7）と、`docs/testing.md`・`docs/benchmarks.md` を作ることは、調整役に残す。
- `cargo deny check` の勧告の検査（このコピーでは動かない）。本体で流してほしい。
- `check` と `retention` の JSON（⑤の2）。`check` と `explain` の `--data-from` は、追加の修正で作った。

## ⑦ 次の周への申し送り

- **取り込み**: ①のファイルを本体へ。`tests/fixtures/synthetic/` の4つは `demo.rs` が `include_str!` で埋め込むので、パスを変えるときは両方を直す。打刻を作り直すときは、`$WS/.tmp/gen_punches.py` が作り方の記録になる（成果物ではない）。
- **本体で流すもの**: `cargo deny check` と `tools/check-links.py`。リンク検査は、`docs/testing.md` と `docs/benchmarks.md` を置けば、残りは調査記録の除外の問題だけになるはず。
- **差分試験**: 別実装と照合しているのは `case` だけで、コマンドライン（`report` など）は照合の外にある。`leave` は、1人分がケースと一致することを試験で確かめている。
- **R3m との重なり**: 私が変えたのは `kintai-cli` と `tests/fixtures/synthetic/` だけで、R3m の範囲（部品の `tests/mut_*.rs`、`examples/bench.rs`、`tools/`）とは重ならない。
- **合成データの期待の値**（試験に入れた手計算）: 10月の E001 は労働日21・労働時間9152分・延長270・深夜60、E002 は深夜6120、E003 は休日労働360・休憩の不足2日、E004 は延長3960・60時間超360・直近2か月の合計9660分（9600分を超える）、E005 は 2026-10-10 の勤務で拒否。年休は E001 の2024-10-01の付与が7半日失効し、残数44半日。

## L2への対応（07:55〜08:23。作業コピーは本体の最新に同期済み）

独立の読み取りレビュー L2 の指摘のうち、調整役に頼まれたコマンド側の8件（🟡3件・⚪5件）を直した。直さないと決まったもの（L2-14 は文書で扱う。L2-03・L2-05〜08・L2-15 は調整役）は触っていない。計算の答えは変えていない。

| ID | 直したこと | 確かめている試験 |
|---|---|---|
| L2-01 🟡 | 方針ファイルの行番号を、値ごとにファイルの先頭から数え直していた（従業員数の2乗）。行の始まりの位置を1回だけ表にし、二分探索で求める（`policy_file.rs` の `line_starts` と `Source::line_at`）。10万人の読み込みは、Apple M4 Pro のリリースのビルドで0.23秒（3回で0.229〜0.238秒。目標2秒以内）。最適化なしの試験のビルドでは2.23〜2.27秒で、2秒を少し超える | `policy_file.rs` の `l2_01_hundred_thousand_employees_parse_quickly`（10万人。上限は、遅い機械の最適化なしのビルドでも揺れないよう10秒にした。前の作りは値ごとに先頭から数え直すので、10万人では上限を大きく超える）と `line_table_agrees_with_counting`（表と先頭から数えた行番号が全位置で一致） |
| L2-02 🟡 | 打刻から組み立てた勤務が範囲の外（秒の切り上げで 2100-01-01T00:00）になるときの誤りの文から、従業員IDを外した。代わりに、その打刻（2099-12-31 23:59 台の `out` か `break_start`）の行番号を示す（`inputs.rs` の `Punches::out_of_range`） | `ac12_errors_cli.rs` の `l2_02_out_of_range_shift_error_hides_the_employee_id`（ID `Tanaka.Taro-1980` が `report` と `check` の文に出ず、「3行目の列 timestamp」が出る。AC-10.7） |
| L2-04 🟡 | 生き残った変異3件の場面の試験を足した。M11: 2025年10月の1,000件と2026年10月の1件は受け付け、2025年10月にもう1件で拒否。M19: `use_default` で、期間をまたぐ週の `{"date":"2026-09-27","code":"IncompleteWeek"}` が10月の `report` に出る。M21: 翌期間の初日（11/01）の休憩の不足を10月の `check` に入れない | M11 `kintai-core/tests/mut_l2_review.rs`、M19 `ac12_report_cli.rs` の `week_record_spanning_the_period_start_is_reported`、M21 `ac12_check_cli.rs` の `break_shortage_on_the_next_period_start_is_not_counted`。変異の定義 `tools/mutations/cli-review.json`（L2 の26件をそのまま）: 結果は下 |
| L2-09 ⚪ | 取込: LF で終わらない最後の行（見出しだけの行を含む）の CR は取り除かず、改行でない CR として不正にする（`kintai-import/src/lib.rs` の `strip_carriage_return` に「LF で終わる行か」を渡す） | `kintai-import/tests/ac01_line_endings.rs`（打刻・休暇・履歴の CSV、CRLF と末尾の空の行は今までどおり読めること） |
| L2-10 ⚪ | 取込: 途中の空の行の下の行に CR の誤りがあるとき、上の空の行の誤りを先に返す（空の行の判定を CR の検査より前に置いた） | 同じファイルの `l2_10_blank_line_error_comes_before_a_stray_cr_below` |
| L2-11 ⚪ | `retention` の先頭の3行を `statutory_current: 日付` `statutory_base: 日付` `product_default: 日付` だけにし、題と説明はその後の行へ移した | `ac12_errors_cli.rs` の `l2_11_retention_starts_with_three_date_lines` |
| L2-12 ⚪ | `demo` の最後の行を参考値の1行にした（終了コードの行はその前） | `ac12_demo_cli.rs` の `assert_demo_output`（`--out` と一時フォルダの両方） |
| L2-13 ⚪ | `explain` で、その日に始まった勤務がなく、前日に始まった勤務の続きがあるときは「この日に始まった勤務はありません（前日に始まった勤務の続きは、前日の労働日に数えています）」と出す（JSON の形は変えない） | `ac12_explain_cli.rs` の `continuation_from_the_previous_day_is_explained`（E002 の 10/02。勤務のない 10/10 は今までどおり「この日の実労働はありません」） |

**変えたファイル**（この対応の分。調整役が本体へ取り込む）

- `crates/kintai-cli/src/`: `policy_file.rs`（L2-01 と試験）、`inputs.rs`（L2-02）、`retention.rs`（L2-11）、`demo.rs`（L2-12）、`explain.rs`（L2-13）
- `crates/kintai-cli/tests/`: `ac12_errors_cli.rs`（L2-02・L2-11）、`ac12_report_cli.rs`（M19）、`ac12_check_cli.rs`（M21）、`ac12_explain_cli.rs`（L2-13）、`ac12_demo_cli.rs`（L2-12）
- 部品のソース（許可の範囲の最小の変更）: `crates/kintai-import/src/lib.rs`（L2-09・L2-10。`for_each_row`・`read_header`・`strip_carriage_return`）
- 部品の試験（新規）: `crates/kintai-import/tests/ac01_line_endings.rs`、`crates/kintai-core/tests/mut_l2_review.rs`
- 道具（新規）: `tools/mutations/cli-review.json`（`$WS/.tmp/l2-mutations.json` と同じ中身）
- 作業フォルダ: `r3-mutate-cli-review.log`、`r3-mutate-import-after-l2.log`、`r3-gates.txt`（流し直したもの）、`r3-progress.md`

**気をつけた点**: 変異の定義（L2 の26件と、R3m の `import.json` の30件）が探す文字列（`if let Some(blank) = first_blank {`、`if line.contains('\r') {`、行の長さの検査の `strip_suffix(b"\r")` など）は、1か所ずつ残した。行の長さの検査は今までどおり行末の CR を除いて数えるので、LF で終わらない最後の行が4096バイトちょうどで単独の CR が続くときは、行が長すぎる誤りでなく、改行でない CR の誤りになる（どちらも終了コード2）。

**変異の試験の結果**: どちらも生き残りはなかった。

- `python3 tools/mutate.py tools/mutations/cli-review.json`（L2 の26件）: 26件すべて検出、終了コード0（`r3-mutate-cli-review.log`）。足した試験は、M11 で `l2_m11_shift_count_is_per_year_and_month`、M19 で `week_record_spanning_the_period_start_is_reported`、M21 で `break_shortage_on_the_next_period_start_is_not_counted` が赤になった。
- 取込を変えたので、R3m の `tools/mutations/import.json`（30件）も流し直した: 30件すべて検出、終了コード0（`r3-mutate-import-after-l2.log`）。
- `tools/mutations/` の9つの定義（合計323件）が探す文字列は、どれも今のソースに定義どおりの数だけある（`holidays.json` の1件は `count: 2` の指定どおり2か所）。

**品質の基準**（08:20 に流し直した。全文は `r3-gates.txt`）

| 基準 | 結果 |
|---|---|
| `cargo fmt --all -- --check` | 通過 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通過 |
| `cargo test --workspace` | 438件合格、失敗0、無視0 |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | 通過 |
| `cargo deny --offline check bans licenses sources` | 通過（winnow の重複の警告は前と同じ） |
| `python3 tools/check-spec-trace.py` | 通過。参照の無い条件は4件（AC-03.14・AC-06.11・AC-05.11・AC-13.6。どれも種類が python の条件）。AC-10.1・10.2 は、同期で入った `bench_jp_labor.rs` から参照されるようになった |
| `python3 tools/check-links.py` | 失敗（4件）。調査記録の `downloads/`（2件）と `notes_B.md`、査読書 `review-l2.md:58` の `benchmarks.md`。どれも触ってよい範囲の外。前の7件のうち `docs/testing.md`・`docs/benchmarks.md` の分は、同期で解消した |
| `cargo +1.88 check --workspace --all-targets` | 通過 |
| `cargo deny check`（勧告の検査） | 流していない。勧告の一覧に git の情報がなく、内部で git を呼んで失敗するため。L2 の対応では依存を足しておらず、`Cargo.lock` と各 `Cargo.toml` は対応の前から変わっていない |

**作業上の逸脱**: 08:22 ごろ、要約の前の自分の会話の記録（$WS の外）を1回 grep しかけた。正規表現の誤りで何も読めていない。以後は読んでいない。
