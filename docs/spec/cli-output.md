# 仕様: コマンドの出力の形（JSON・CSV・終了コード）

`report` `leave` `explain` の `--format json` と、`report` の `--format csv` の形を決めます。`check` と `retention` は、この版では人が読む形だけです。JSONは1行で、末尾に改行。日付は `YYYY-MM-DD`、時点は `YYYY-MM-DDTHH:MM`、分と半日は整数。エラーは標準エラーに `{"error": {"code": ..., "message": ...}}`。出力は決定的で、従業員はID順、日付は昇順。コマンドの意味は[コマンドライン](cli.md)。

## report（`kintai.report.v1`）

| 項目 | 中身 |
|---|---|
| `schema` `month` | `"kintai.report.v1"`、`--month` の値 |
| `period_start` `period_end` | 対象の月の期間の初日と最後の日（含む） |
| `data_from` | 使った値（`--data-from` か打刻CSV全体の最初の日付。打刻がなければ `null`） |
| `rounding` | `none` か `monthly_30min` |
| `employees[]` | 従業員ID順。`employee_id` `ledger` `refused` `ambiguities` `break_shortages` |
| `ledger[]` | 0か1件。`period_start` `work_days` `work_minutes` `overtime_minutes` `over60_minutes` `holiday_minutes` `night_minutes` `within_minutes` `extra_within_minutes` `rounded`（`{overtime, holiday, night}` は時間。`none` では `null`）。拒否のときは空 |
| `refused` | `null` か `{code, dates, anomalies}`。`code` は `AmbiguousBoundary` か `PunchAnomaly`、`dates` は重複なし昇順、`anomalies[]` は `{kind, date, line}`（打刻の異常のときだけ） |
| `ambiguities[]` | `{date, code}`（`use_default` で、期間に入る記録） |
| `break_shortages[]` | `{date, work_minutes, break_minutes, required_minutes, shortage_minutes}` |
| `notices` `table_checked_on` `review_status` | [月次の集計](report.md)の4つの注意、表の確認日、`"unreviewed"` |

**CSV**: UTF-8、LF、見出し行あり。列は `employee_id,period_start,work_days,work_minutes,overtime_minutes,over60_minutes,holiday_minutes,night_minutes,within_minutes,extra_within_minutes,table_checked_on,review_status,notices`（`notices` は4つのコードを `;` でつなぐ）。1人1行。拒否した人の行は出さず、終了コード4で分かる。値は ASCII の英数字と `-` `.` `_` `;` だけで、引用符を使わない。

## leave（`kintai.leavereport.v1`）

`schema` `as_of` `employees[]` `notices` `table_checked_on` `review_status`。`employees[]` は `employee_id` のあとに、[年休のケース](case-format-36-leave.md)の出力 `kintai.leave.v1` と同じ `lots` `balance_halves` `five_day` `register` `notices`（`AttendanceUnconfirmed`）を、同じ順で持つ。

## explain（`kintai.explain.v1`）

| 項目 | 中身 |
|---|---|
| `employee_id` `date` `weekday` | 従業員、労働日、曜日（`sun`〜`sat`） |
| `legal_holiday` | その曜日が方針の法定休日か |
| `national_holiday` | `null` か `{name, kind}`（`Statutory` `Substitute` `Citizens`） |
| `holiday_table` | `{status, notices, source}`（`Official` `Estimated` `OutOfRange`、`EquinoxUnverified`、出典の1行） |
| `day` | その日の区分の合計（`kintai.classify.v1` の `days` と同じ12項目）か `null` |
| `segments[]` | `{start, end, minutes, category, night, premium}`。`category` は `within` `extra_within` `overtime` `over60` `holiday`。`premium` は `{percent, basis[]}`（`percent` は2023-04-01より前の60時間超で `null`）、`basis[]` は `{law, article, revision_id, url, confirmed_on}` |
| `notes[]` | `{code, message}`（`ExtraWithinIsContractual` `Over60RateNotShown`、曖昧さのコード） |
| `refused` `notices` `table_checked_on` `review_status` | `report` と同じ |

## check と retention

人が読む形だけ。`check` は従業員ごとに、協定月の表（O・H・T、直近2〜6か月の合計、発動月の数、年の時間外、判定。下限値は「（下限）」）、判定の説明と根拠、休憩の不足、打刻の異常、曖昧さの記録、違反と警告の件数、注意を出す。`retention` は `statutory_current: 日付` `statutory_base: 日付` `product_default: 日付` の3行。人が読むどの出力も、末尾の1行は「この出力は参考値です（専門家の確認前。review_status: unreviewed）。」。

## 終了コード

`report` と `explain` は、拒否があれば4。`check` は、`--fail-on-violation` で違反か警告があれば1、そうでなく拒否があれば4。`leave` と `retention` は0か2・3。`demo` は書けたら0。

## 受け入れ条件

- AC-12.9 [rust] `report` `leave` `explain` のJSONと、`report` のCSVが、この文書の項目と並びのとおりに出る。
- AC-12.10 [rust] 人が読む出力に、注意、表の確認日、確認の状態、末尾の参考値の1行が付く。
