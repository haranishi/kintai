# 仕様: コマンドライン `kintai`

Phase 0 の入口です。入出力のJSONは[区分のケース](case-format.md)と[36協定・年休のケース](case-format-36-leave.md)、集計の項目は[月次の集計](report.md)、打刻は[打刻の仕様](punch.md)にあります。ここではコマンドと、方針ファイルを決めます。

## コマンド

| コマンド | 働き |
|---|---|
| `kintai report --policy P --punches C --month YYYY-MM [--data-from D] [--employee ID] [--format text\|json\|csv]` | 月の期間ごとの台帳項目、警告、曖昧さの記録。`--data-from` は打刻がそろっている最初の日（既定は打刻CSV全体の最初の日付）。月の最初の週の起算日より後だと、その週は `IncompleteWeek` で拒否される |
| `kintai check --policy P --punches C --month YYYY-MM [--data-from D] [--history H] [--fail-on-violation]` | 36協定の判定、休憩の不足、打刻の異常。違反があれば、`--fail-on-violation` で終了コード1 |
| `kintai leave --policy P --leaves L --as-of YYYY-MM-DD [--employee ID] [--format text\|json]` | 年休のロット、残数、5日取得義務、管理簿 |
| `kintai explain --policy P --punches C --employee ID --date YYYY-MM-DD [--data-from D] [--format text\|json]` | その日の区間ごとの区分と、根拠の条文・割増の下限・表の確認日 |
| `kintai retention --kind K --completed D --wage-due D` | 保存の期限の目安（[月次の集計](report.md)） |
| `kintai case FILE` | ケースのJSONを計算して、結果のJSONを出す。別実装との差分試験に使う |
| `kintai demo [--out DIR]` | 合成データの方針と打刻を使い、上の各出力を一通り出す。実データには触れない |
| `kintai --version` / `--help` | 版（git の短いハッシュを含む）/ 使い方 |

出力は標準出力。エラーは標準エラーにJSON（`{"error": {"code": ..., "message": ...}}`）。時刻・日付は日本時間。終了コードは[区分のケース](case-format.md)の表のとおり。`--format` の既定は `text`（人が読む表）。JSON・CSVの形は[出力の形](cli-output.md)。

## コマンドの意味

- **対象の月**: `--month YYYY-MM` は、その月の `month_start_day` に始まる月の期間（起算日が16なら 10月16日〜11月15日）。`--month` は 2000-01〜2099-12。起算日が2以上のとき、2000年1月の起算日より前の労働日は、どの月でも報告できない（端の制約）。
- **使う打刻**: 打刻CSVの全体を使う（対象の月の外の日も、週の数えに要る）。`--data-from`（`report` `check` `explain`）の既定は、CSV全体の最初の日付。CSVが週の途中から始まると、その週は `IncompleteWeek` で拒否されるので、対象の月の最初の週の起算日（既定は日曜）から打刻を含めるか、`--data-from` でその日からそろっていることを伝える。
- **従業員**: 計算の対象は、方針の `[[employee]]` の全員（打刻のない人は0の行）。`--employee` で1人に絞る。打刻・履歴・休暇のCSVのIDは、すべて方針にあること（なければ不正な入力。エラーにIDは出さず行番号を示す）。出力はID順。
- **方針の必須**: `[agreement36]` と `[[employee]]`（空でもよい）は、どのコマンドでも必須。`[[employee]]` の `hire_date` `weekly_scheduled_days` `weekly_scheduled_minutes` `attendance` も必須。`worker_kind` は `[[employee]]` だけに書く。`legal_holiday_weekday` の欠けは `LegalHolidayUnspecified`。`seconds` と `rounding` の既定は `worker_favorable` と `none`。日付は TOML の文字列だけ（TOML の日付の型は型の誤り）。先頭の UTF-8 の BOM は取り除く。`leave` にも、労働時間の対応外の判定（`work_system` `worker_kind` `MonthStartDayMismatch` など）を当てる。
- **拒否**: `worker_kind` などの対応外が1人でもあれば、コマンド全体を終了コード3で止め、エラーにIDを含める（`--employee` で他の人を計算できる）。曖昧さの拒否は従業員ごとで、その人の値は出さず `refused` を付け、終了コードは4。打刻の異常（[打刻](punch.md)）も同じ扱いで、`refused.code` は `PunchAnomaly`、異常の一覧（種類と日付）を付ける。どちらも、打刻の全体で見る。
- **`check` の月**: 協定月は、`data_from`（既定は打刻CSVの最初の日付）以後で最初に始まる協定月から、対象の月まで（実労働のない月はOもHも0）。それより前は `--history` のT。違反は、36協定の判定（`UnknownHistory` を含む）、休憩の不足、打刻の異常。どれかあれば、`--fail-on-violation` で終了コード1（打刻の異常は違反でもあり拒否でもあるので、フラグがなければ4）。打刻が1件もないとき、対象の月が最初の協定月より前のときは、判定できないので不正な入力。
- **`leave`**: 従業員の属性は方針の `[[employee]]`（`grants` も同じ）。休暇CSVの取得は `taken`。
- **`explain`**: その労働日を、区分と深夜の属性が同じ連続した区間に分け（所定外法定内は、日の法定内の分のうち時刻の後ろの分）、区間ごとに区分、割増の下限（%）、根拠の条文、表の確認日を出す。
- **`demo`**: `--out` を省くと新しい一時フォルダに書き、そのパスを示す。`--out` は、存在しないか空のフォルダだけ。そこにだけ書く。

## 方針ファイル（TOML）

```toml
schema = "kintai.policy.v1"

[workplace]
work_system = "standard"
scheduled_minutes_per_day = 420
week_start = "sun"
weekly_limit_minutes = 2400
special_44h_confirmed = false
legal_holiday_weekday = "sun"
month_start_day = 1
ambiguity = "reject"
seconds = "worker_favorable"
rounding = "none"

[agreement36]
kind = "general"            # none | general | special
year_start_month = 4
month_start_day = 1         # 36協定の月の起算日。workplace.month_start_day と同じにする
monthly_limit_minutes = 2700
annual_limit_minutes = 21600

[[employee]]
id = "E001"
worker_kind = "standard"
hire_date = "2024-04-01"
weekly_scheduled_days = 5
weekly_scheduled_minutes = 2400
attendance = ["meets", "meets"]
```

- `[workplace]` の値の意味と範囲は[区分のケース](case-format.md)と[打刻の仕様](punch.md)。`rounding` は `none` か `monthly_30min` だけ。
- `[agreement36]` は[36協定のケース](case-format-36-leave.md)の `agreement` と同じ（`year_start_month` は `kind` によらず必須）。`kind` が `none` でないとき、`month_start_day` は必須で、`workplace.month_start_day` と違うと `MonthStartDayMismatch`（対応外）で拒否する。就業規則などに起算日の定めがなければ、賃金計算期間の初日を入れる（平成21年基発0529001号）。
- `worker_kind` は `standard` だけが計算できる。ほかの値は[対応外](unsupported.md)で拒否。
- 知らない項目、必須項目の欠け、範囲外の値は、不正な入力にする（黙って既定値にしない）。
- `--history H` のCSV列は `employee_id,month_start,total_minutes`（36協定の履歴）。年休の記録CSVの列は `employee_id,date,unit`。

## 受け入れ条件

- AC-12.1 [rust] 方針ファイルの検査（必須、未知の項目、範囲、対応外）が、不正な入力か対応外のエラーになる。
- AC-12.2 [rust] `report` が、打刻CSV・方針から、月ごと・従業員ごとの台帳項目をJSONとCSVで出す。
- AC-12.3 [rust] `check --fail-on-violation` が、違反と警告のとき1で終わる。
- AC-12.4 [rust] `leave` が、ロット・残数・5日取得義務・管理簿を出す。
- AC-12.5 [rust] `explain` が、区間ごとに区分・割増の下限・根拠の条文・確認日を出す。
- AC-12.6 [rust] `case` が、3種類のケースを処理し、`schema` が未知なら不正な入力にする。
- AC-12.7 [rust] `demo` が、一時フォルダだけに書き、すべてのコマンドの出力が揃う。
- AC-12.8 [rust] エラーは標準エラーにJSONで出て、終了コードが決まりどおりになる。
