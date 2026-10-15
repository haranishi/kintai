# 仕様: ケースの形式（区分の計算の入出力JSON）

区分の計算は、1人分の方針と勤務を渡すと、日ごとの分を返します。公的な数値例の試験、別実装（Python）との差分試験、`kintai classify --case <ファイル>` が、この形式を共有します。数値はすべて整数の分、日時は `YYYY-MM-DDTHH:MM`（秒なし、日本時間。区切りは `T` か空白）です。

## 入力 `kintai.case.v1`

```json
{
  "schema": "kintai.case.v1",
  "data_from": "2026-10-04",
  "policy": {
    "work_system": "standard",
    "scheduled_minutes_per_day": 420,
    "week_start": "sun",
    "weekly_limit_minutes": 2400,
    "special_44h_confirmed": false,
    "legal_holiday_weekday": "sun",
    "month_start_day": 1,
    "ambiguity": "use_default"
  },
  "shifts": [
    {"start": "2026-10-07T09:00", "end": "2026-10-08T05:00",
     "breaks": [["2026-10-07T12:00", "2026-10-07T13:00"]]}
  ]
}
```

| 項目 | 規則 |
|---|---|
| `work_system` | 必須。`standard` 以外は対応外のエラー（[対応外の仕様](unsupported.md)） |
| `scheduled_minutes_per_day` | 必須。0〜480 |
| `week_start` | `sun` `mon` … `sat`。既定は `sun` |
| `weekly_limit_minutes` | 2400（既定）または2640。2640は `special_44h_confirmed` が真のときだけ許す |
| `legal_holiday_weekday` | 必須。`sun` … `sat` |
| `month_start_day` | 必須。1〜28 |
| `ambiguity` | `reject`（既定）または `use_default`（[区分の仕様](classify.md)） |
| `worker_kind` `holiday_system` `night_window` | 任意。対応外の指定にだけ使う（[検査の順序](validation.md)の表）。既定は `standard` `weekly` `22-05` |
| `holiday_swaps` | 任意の配列。空でなければ `HolidaySwap` で拒否する。要素の中身は検査しない。`null` は項目なし、配列でなければ型の誤り |
| `data_from`（トップレベル、任意） | 打刻がそろっている最初の日（`YYYY-MM-DD`）。省略すると、入力がすべてだとみなす。週の途中から打刻がそろっていない入力を見つける（[検査の順序](validation.md)の `IncompleteWeek`） |
| `shifts` | 勤務。開始 < 終了。互いに重ならない（接してよいが、つなげない）。`breaks`（任意）は勤務の内側（両端を含まない）にあり、互いに重ならない（接してよい） |

検査の順序と、対応外の指定、曖昧さの定義は[検査の順序](validation.md)にある。知らない項目は不正な入力。曖昧な読みで直さない。

## 出力 `kintai.classify.v1`

```json
{
  "schema": "kintai.classify.v1",
  "days": [
    {"date": "2026-10-07", "total": 1140, "within": 480, "extra_within": 60,
     "overtime": 660, "over60": 0, "holiday": 0,
     "night": 420, "night_within": 0, "night_overtime": 420,
     "night_over60": 0, "night_holiday": 0}
  ],
  "periods": [
    {"start": "2026-10-01", "total": 1140, "within": 480, "overtime": 660,
     "over60": 0, "holiday": 0, "night": 420}
  ],
  "ambiguities": [{"date": "2026-10-31", "code": "MonthBoundary"}],
  "refused": null
}
```

- `days`: 労働日ごと。実労働が0の日は出さない。日付順。`date` は労働日（始業の暦日）。
- `within` は法定内、`extra_within` はその内数の所定外法定内、`overtime` は時間外（`over60` はその内数）、`holiday` は法定休日。`total` は `within + overtime + holiday`。
- `night` は深夜。`night_within + night_overtime + night_holiday` に等しく、`night_over60` は `night_overtime` の内数。
- `periods`: 月の期間ごとの合計。実労働のある期間だけ。`start` は期間の初日。日付順。起算日が1以外のとき、2000年の初めの期間の初日は1999年になりうる（`IncompleteWeek` の週の起算日も同じ）。出力の日付は、入力の範囲（2000〜2099年）の外になってよい。
- `ambiguities`: 曖昧さの記録。コードは `HolidayBoundary` `WeekBoundary` `MonthBoundary` `IncompleteWeek`。`use_default` のときだけ出る（`reject` では空の配列）。日付と並びは[検査の順序](validation.md)。
- `refused`: 既定の `reject` で曖昧な勤務があったとき `{"code": "AmbiguousBoundary", "dates": [...]}` を返し、`days` と `periods` は空にする。

## エラーの形

標準エラーに `{"error": {"code": ..., "message": ...}}`。`code` は、不正な入力なら `InvalidInput`、対応外なら[対応外の仕様](unsupported.md)のコード。曖昧さによる拒否は、エラーでなく出力の `refused` で返し、終了コードだけ4にする。差分試験は `code` と終了コードだけを比べ、`message` は比べない。

## 終了コード（`kintai` コマンド共通）

0: 成功。1: `--fail-on-violation` で違反または警告が見つかった。2: 不正な入力。3: 対応外。4: 曖昧さによる確定集計の拒否。

## 受け入れ条件

- AC-09.1 [rust] 入力の検査（開始 < 終了、重なり、休憩が勤務の内側、必須項目、値の範囲）に失敗したら、不正な入力のエラーにする。
- AC-09.2 [rust] 出力の合計の関係（total、night）が常に成り立つ。
- AC-09.3 [rust] 同じ入力は、毎回まったく同じ出力（順序・空白を含む）になる。
- AC-09.4 [rust] 終了コードが上の表のとおりになる。
