# 仕様: ケースの形式（36協定と年次有給休暇）

[区分のケース](case-format.md)と同じ流儀です。`kintai case <ファイル>` が、`schema` の値で処理を選びます。別実装（Python）との差分試験では、同じファイルを両方に渡して出力を比べます。だから、この形式が両者の唯一の約束です。数値は整数。日付は `YYYY-MM-DD`。期間は半開です。

## 36協定 `kintai.case36.v1`

```json
{
  "schema": "kintai.case36.v1",
  "agreement": {"kind": "special", "year_start_month": 4,
    "monthly_limit_minutes": 2700, "annual_limit_minutes": 21600,
    "special_monthly_total_limit_minutes": 5940,
    "special_annual_limit_minutes": 43200, "special_months_max": 6},
  "months": [{"start": "2021-04-01", "overtime": 4800, "holiday": 0}],
  "history": [{"start": "2021-03-01", "total": 600}]
}
```

- `kind` は `none` `general` `special`。限度の項目は、`none` では不要、`general` は月・年の限度が必須、`special` はさらに特別条項の3項目（`special_monthly_total_limit_minutes` `special_annual_limit_minutes` `special_months_max`）も必須。`year_start_month`（1〜12）は `none` でも必須。限度は0以上で、法定の最大（2700・21600・5999・43200・6回）を超えたら不正な入力。その `kind` で使わない限度の項目があっても、範囲だけ検査して使わない。
- トップレベルの必須は `schema` `agreement` `months` `history`（`null` は型の誤り）。
- `months` は協定月を日付順に隙間なく並べる。空でもよい（出力の `months` も空。そのとき `history` の `start` は、日が1〜28であることだけを見る）。`start` は協定月の初日で、日は全部同じ（1〜28）。`overtime`（O）と `holiday`（H）は分で、0以上の整数。合わせて44640分（31日分）以下で、超えたら不正な入力。
- `history` は必須の配列（空でもよい）で、`months` より前の協定月の T（O＋H。`total` は0〜44640分）。歯抜けでよい。並びは問わない。協定月の初日でない日付、`months` の最初の月以後の月、同じ月の2回は不正な入力。使うのは、直近5か月ぶんまで。

出力 `kintai.limits36.v1`:

```json
{"schema": "kintai.limits36.v1",
 "months": [{"start": "2021-04-01", "total": 4800,
   "sums": {"2": 5400, "3": null, "4": null, "5": null, "6": null},
   "special_count": 1, "year_overtime": 4800, "year_overtime_known": true,
   "findings": [{"code": "UnknownHistory", "n": 3}, {"code": "UnknownHistory", "n": 4},
                {"code": "UnknownHistory", "n": 5}, {"code": "UnknownHistory", "n": 6}]}],
 "notices": ["OwnEmployerOnly"]}
```

- `sums` は、その月までの直近 n か月（2〜6）の T の合計。月も履歴も足りなければ `null`。
- `special_count` は協定年の始めからその月までの、O が2700分を超えた月数。`year_overtime` は協定年の始めからの ΣO。協定年の最初の月が `months` に含まれないときは、`months` の中の最初の月からの合計で、`year_overtime_known` が偽になる。偽のとき、年と回数の判定は、すでに超えている確実な違反だけを出す（出ないことは適合を意味しない）。
- `findings` のコードは次のとおり。`NoAgreement` `MonthlyOvertimeExceeded` `GeneralMonthlyLimitExceeded` `AnnualOvertimeExceeded` `GeneralAnnualLimitExceeded` `SpecialCountExceeded` `SpecialMonthlyTotalExceeded` `SpecialAnnualExceeded` `MonthlyTotalReached100h` `MultiMonthSumExceeded`（`n` つき）`UnknownHistory`（`n` つき）。順序は、この順、同じコードなら `n` の昇順。意味は[36協定の仕様](limits36.md)。
- `notices` は常に `OwnEmployerOnly`（自社のTだけを判定している）を含む。

## 年次有給休暇 `kintai.caseleave.v1`

```json
{"schema": "kintai.caseleave.v1", "as_of": "2026-03-15",
 "employee": {"hire_date": "2024-04-01", "weekly_scheduled_days": 5,
   "weekly_scheduled_minutes": 2400, "annual_scheduled_days": null,
   "attendance": ["meets", "meets"], "grants": null},
 "taken": [{"date": "2025-05-01", "unit": "full"}, {"date": "2025-06-02", "unit": "half"}]}
```

- トップレベルの必須は `schema` `as_of` `employee` `taken`（`taken` は空でもよい。`null` は型の誤り）。`employee` の必須は `hire_date` `weekly_scheduled_days`（0〜7）`weekly_scheduled_minutes`（0〜10080）`attendance`（空でもよい）。`annual_scheduled_days`（0〜366）と `grants` は、項目がなくても `null` でも、指定なし。
- `attendance` は付与の順（6か月、1年6か月…）の `meets` `below` `unknown`。足りない付与は `unknown`。付与の数より長いとき、余りは使わない（誤りにしない）。
- `grants` は、前倒しの付与を明示するとき `[{"date": "2024-04-01", "halves": 20}]` の形にする（`halves` は1〜200の整数）。`null` なら法定の付与日。明示したときは、付与を事実として扱い、`attendance` は計算に使わない（値の型と列挙の検査はする）。
- `taken` の `unit` は `full`（2半日）か `half`（1半日）。`hours` は `LeaveHourly` で拒否。それ以外は不正な入力。`as_of` より後の記録は数えない。

出力 `kintai.leave.v1`:

```json
{"schema": "kintai.leave.v1",
 "lots": [{"grant_date": "2024-10-01", "granted_halves": 20, "expires": "2026-10-01",
   "used_halves": 3, "remaining_halves": 17, "expired_halves": 0},
  {"grant_date": "2025-10-01", "granted_halves": 22, "expires": "2027-10-01",
   "used_halves": 0, "remaining_halves": 22, "expired_halves": 0}],
 "balance_halves": 39,
 "five_day": [{"base_date": "2024-10-01", "window_end": "2025-09-30", "taken_halves": 3,
   "status": "NotMet", "needed_halves": 7},
  {"base_date": "2025-10-01", "window_end": "2026-09-30", "taken_halves": 0,
   "status": "InProgress", "needed_halves": 10}],
 "register": [{"base_date": "2024-10-01", "granted_halves": 20,
   "taken": [{"date": "2025-05-01", "unit": "full"}, {"date": "2025-06-02", "unit": "half"}],
   "taken_halves": 3},
  {"base_date": "2025-10-01", "granted_halves": 22, "taken": [], "taken_halves": 0}],
 "notices": []}
```

- 日数はすべて「半日」を1とする整数（`*_halves`）。人が読む出力だけが日に直す。
- `remaining_halves` は、`as_of` の時点で失効していないロットの未使用分。失効済みなら0で、未使用分は `expired_halves` に出す（失効の当日から失効済み）。`balance_halves` は `remaining_halves` の合計。
- `status` は `Met` `InProgress` `NotMet` `NotApplicable`。`needed_halves` は 10 − `taken_halves`（0未満にならない）で、`Met` と `NotApplicable` では0。`window_end` は期間の最後の日（含む）。`NotMet` は期間が過ぎて5日に満たないことを示すだけで、法違反とは言わない（休業などで取得が不可能だった場合は違反でない。通達 基発1228第15号の答6）。
- 付与日、失効日、`window_end` の日付の決め方は[年休の仕様](leave.md)の「日付の規則」。
- `lots` と `five_day` と `register` は、付与日の昇順。`register[].taken` は、基準日から5日の期間（1年間）に入る取得で、日付の昇順（同じ日は入力の順）。そのロットから消化した分とは限らない。
- `notices` は `AttendanceUnconfirmed` だけ。`attendance` が `unknown` の付与があるとき（`grants` を明示していないとき）に、1回だけ付く。

## 受け入れ条件

- AC-09.5 [rust] 36協定のケースが、上の形で読まれ、範囲外の限度は不正な入力になる。
- AC-09.6 [rust] 年休のケースが、上の形で読まれ、`hours` は拒否される。
- AC-09.7 [rust] 出力のコードと順序が、上の規則のとおりに決まる。
