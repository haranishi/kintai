# 仕様: 入力の検査の順序と、対応外の指定、曖昧さの厳密な定義

本実装と別実装が、同じ入力に同じ答えを返すための取り決めです。[区分のケース](case-format.md)と[対応外の仕様](unsupported.md)を補います。ここが曖昧だと、両者が別々に推測して食い違います。

## 検査の順序（最初に当たったものを返す）

1. JSONとして読めない（RFC 8259 の JSON だけ。`NaN` `Infinity`、孤立したサロゲート（`\ud800` など）、最も外側の値を1段目と数え、配列かオブジェクトが128段目に入るものは読めない扱い。127段まで読める。`1e400` のように倍精度に収まらない数も読めない扱い）、同じ名前の項目が2回ある、`schema` が違う、型が違う（整数の欄は、小数点も指数もない JSON の整数だけ。`-0` `1.0` `1e2` `true` は型の誤り。整数の欄でない所（`holiday_swaps` の要素など、中身を検査しない所）では、`-0` は読める）、**知らない項目がある**、必須の項目がない → `InvalidInput`（終了コード2）。任意の項目の `null` は、項目がないのと同じ。必須の項目の `null` は型の誤り（`legal_holiday_weekday` の欠けと `null` だけは手順3）。日時・日付の欄は「文字列であること」だけを見る。
2. 列挙の値が定義にない、数値が範囲外 → `InvalidInput`。範囲は、`scheduled_minutes_per_day` が0〜480、`month_start_day` が1〜28、`weekly_limit_minutes` が2400か2640、`ambiguity` が `reject` か `use_default`、`week_start` と `legal_holiday_weekday` が `sun`〜`sat`。
3. 対応外の指定（下の表）→ そのコードで終了コード3。表の上から順に見て、最初に当たったものを返す。
4. 勤務と日付の検査 → `InvalidInput`。次のどれかに当たる:開始が終了以上、勤務どうしが重なる（接するのはよい。接する2つの勤務は、つながず別の勤務として数える）、休憩が勤務の内側にない（両端に接するのも不可）、休憩の開始が終了以上、休憩どうしが重なる（接するのはよい）、勤務が1440分以上、日時が2000-01-01T00:00〜2099-12-31T23:59の外（終了が2100-01-01T00:00の勤務も不可）、**日時・日付の文字列の書式の誤り**（2月30日、25時、24:00、区切りの誤りなど。`data_from` を含む）。
5. 計算。曖昧さがあれば、`reject` で終了コード4（出力に `refused`）。

日時の書式の誤りは手順1ではなく手順4なので、手順3の対応外が先に返る。方針の項目の型の誤り（数値の欄に文字列など）は手順1。

36協定と年休のケース（[ケースの形式](case-format-36-leave.md)）も同じ流儀で、形と型 → 列挙と範囲 → 対応外 → 日付の書式と整合 → 計算の順に検査する。年休の対応外が複数あるときの順は[年休の仕様](leave.md)。

## 対応外の指定

表の順に見る。値が表にも、列挙の許す値にも無ければ手順2で `InvalidInput`。

| 項目 | 値 | コード |
|---|---|---|
| `policy.work_system` | `variable_monthly` `variable_yearly` `variable_weekly_irregular` `flex` `discretionary` `offsite_deemed` `managerial` `agriculture_exempt` `surveillance_intermittent` `night_duty_permit` `highly_professional` | `VariableMonthly` `VariableYearly` `VariableWeeklyIrregular` `Flex` `DiscretionaryDeemed` `OffsiteDeemed` `Managerial` `AgricultureExempt` `SurveillanceIntermittent` `NightDutyPermit` `HighlyProfessional` |
| `policy.worker_kind`（任意、既定 `standard`） | `minor` `pregnant_or_postpartum` `research_development` `driver` `physician` `disaster_recovery_construction` `hazardous_work` | `Minor` `PregnantOrPostpartum` `ResearchDevelopment` `AutomobileDriver` `Physician` `DisasterRecoveryConstruction` `HazardousWork` |
| `policy.holiday_system`（任意、既定 `weekly`） | `four_weeks` `continuous_rotation` | `FourWeekHolidays` `ContinuousRotation` |
| `policy.holiday_swaps`（任意、配列、既定は空） | 空でない配列（要素の中身は検査しない） | `HolidaySwap` |
| `policy.night_window`（任意、既定 `22-05`） | `23-06` | `NightWindowOverride` |
| `policy.legal_holiday_weekday` | 項目がない、または `null` | `LegalHolidayUnspecified` |
| `policy.weekly_limit_minutes` | `2640` かつ `special_44h_confirmed` が真でない | `Weekly44WithoutConfirmation` |

`MonthStartDayMismatch` は、コマンドラインの方針ファイル（TOML）だけで判定する（[コマンドラインの仕様](cli.md)）。

## 曖昧さの厳密な定義

勤務の**実労働の分**が属する暦日の集合を D とする（休憩の分は除く）。`|D| ≥ 2` の勤務を「またぐ勤務」とする。24:00ちょうどに終わる勤務は、実労働が1つの暦日だけなのでまたがない。労働日 L は、勤務の開始の暦日。

- `HolidayBoundary`: D のどれかの暦日の曜日が `legal_holiday_weekday`。
- `WeekBoundary`: D の暦日が、2つ以上の週（`week_start` による）にまたがる。
- `MonthBoundary`: D の暦日が、2つ以上の月の期間（`month_start_day` による）にまたがる。
- `IncompleteWeek`: 入力に `data_from` があり、実労働のある労働日を含む週で、週の起算日が `data_from` より前。打刻がそろっていない日が週にあるので、週の上限を超えた分を少なく数えるおそれがある。記録の日付は、その週の起算日。

記録は（日付、コード）の組ごとに1件。日付は、前の3つは労働日 L、`IncompleteWeek` は週の起算日。並びは日付の昇順、同じ日付では上の表の順。`reject` のとき `refused.dates` は、記録のある日付の重複のない昇順。`use_default` のときは、上の手順をそのまま適用する。`data_from` がないときは、入力の勤務がすべてだとみなし、`IncompleteWeek` は付かない。

## 受け入れ条件

- AC-13.1 [rust] 検査の順序が上のとおり（1→5）で、同じ入力に複数の誤りがあっても最初のものを返す。
- AC-13.2 [rust] 対応外の指定の表の全行が、対応するコードを返す。表の順に見て最初に当たる。
- AC-13.3 [rust] 曖昧さの4つのコードが、定義どおりの勤務で付く。24:00ちょうどに終わる勤務は付かない。
- AC-13.4 [rust] 記録と `refused.dates` の並びが定義どおり。
- AC-13.5 [rust] 勤務と日付の検査の各条件が `InvalidInput` になる。接する勤務はよい。
- AC-13.6 [python] 無作為の入力で、別実装とエラーのコードと終了コードが全件一致する。
