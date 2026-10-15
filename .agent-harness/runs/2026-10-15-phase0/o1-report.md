---
tags: [kintai, phase0, oracle, difftest]
created: 2026-10-15
updated: 2026-10-15
---

# O1 別実装（オラクル）の報告書（仕様 第2版・最終）

## 結論

- 最後の回答（Q45・Q46）を入れて、オラクル・生成器・差分試験を仕上げた。Rust の本実装（`crates/`、`~/Projects/kintai`）は一度も見ていない。
- **selftest は 25/25。**
- **差分試験（本実装なし）は、種 1・2・3・7・42・99 の各2000件×3種類（各6000件）で、例外0件、検算の不一致0件、生成の意図との食い違い0件。**
- 疑問は Q1〜Q46 がすべて決着した。**仕様がまだ決めていない点は、もう無い。** 読みで答えが変わる probe も、生成器に残っていない。
- 作業コピーの docs/spec には、最後の回答の記述（44640分の上限）がまだ入っていなかった。本体には触れず、回答の文面どおりに実装した。

## ① 作ったファイル

| ファイル | 役割 | 最後の回答で変えたこと |
|---|---|---|
| `tools/oracle/common.py` | 日付と時点、JSON の読み込み（RFC 8259、`-0`、サロゲート、127段まで）、CLI と同じ結果を返す `run_bytes` | なし |
| `tools/oracle/classify_oracle.py` | 区分。手順1〜5を関数に分け、実労働を1分ずつ数える | なし |
| `tools/oracle/limits36_oracle.py` | 36協定。月ごとに数える | O と H は0以上で合計44640分以下、`history` の `total` は0〜44640分（手順2の範囲の検査） |
| `tools/oracle/leave_oracle.py` | 年休。半日ずつ古いロットから消化 | 印の整理だけ（Q45 は実装のまま） |
| `tools/oracle/gen_cases.py` | 無作為のケース（種・種類・番号で決まる） | 上限の境の場面（O・H・O＋H・`total` の44640と44641）を足した。Q45 の場面は意図を `InvalidInput` にして probe から外した |
| `tools/oracle/selftest.py` | vectors の全件を確かめる | なし |
| `tools/difftest.py` | 差分試験、`--stats`、オラクルの検算 | 上限の場面の集計と、44640分ちょうどの件数 |

検算は、オラクルとは別の素朴な数え方で毎回確かめる。区分の実労働と深夜の合計（区間の長さと交わりで数え直す）、`IncompleteWeek` の週の起算日、36協定の年の累計・発動月の数・`year_overtime_known`、年休の「消化の合計＝`as_of` までの取得の合計」と管理簿の並びなどを見る。オラクルの CLI と関数呼び出しは、最後の変更のあとも160件で一致した。

## ② selftest の出力

`python3 tools/oracle/selftest.py`:

```
一致   official-examples.json mhlw-check-roudou-ex1
一致   official-examples.json mhlw-check-roudou-ex2
一致   official-examples.json tokyo-aramashi-week-excess-1
一致   official-examples.json tokyo-aramashi-week-excess-2
一致   official-examples.json nagasaki-no28-case1
一致   official-examples.json nagasaki-no28-case2
一致   official-examples.json mhlw-60h-leaflet-31day-month
一致   official-examples.json derived-incomplete-week-complete
一致   official-examples.json derived-incomplete-week-reject
一致   official-examples.json derived-incomplete-week-use-default
一致   limits36-official.json mhlw-36-leaflet-p16-table1
一致   limits36-official.json mhlw-36-leaflet-p17-table2
一致   limits36-official.json mhlw-36-leaflet-p21-over45-seven-times
一致   limits36-official.json mhlw-36-leaflet-p21-june-100h
一致   limits36-official.json mhlw-36-leaflet-p21-avg80-three-months
一致   limits36-official.json mhlw-36-leaflet-p4-44plus56
一致   limits36-official.json derived-36-agreement-limit-30h
一致   limits36-official.json derived-36-special-count-at-45h
一致   limits36-official.json derived-36-annual-agreement-300h
一致   limits36-official.json derived-36-annual-statutory-360h
一致   leave-official.json mhlw-annual5-p8-case1
一致   leave-official.json mhlw-annual5-p9-case2-no-prorate
一致   leave-official.json derived-first-grant-dates
一致   leave-official.json derived-expiry-and-order
一致   leave-official.json derived-2023-08-29-corresponding-dates
selftest: 一致 25/25
```

## ③ 統計の出力

### 6つの種のまとめ

| 種 | 件数・時間・例外 | 意図との食い違いが0の種類 | 不正と対応外の割合（区分/36協定/年休） |
|---|---|---|---|
| 1 | 6000件 3.1秒 例外0 | 3/3 | 17%/17%/19% |
| 2 | 6000件 3.1秒 例外0 | 3/3 | 20%/19%/17% |
| 3 | 6000件 3.5秒 例外0 | 3/3 | 18%/17%/16% |
| 7 | 6000件 3.8秒 例外0 | 3/3 | 18%/17%/17% |
| 42 | 6000件 3.8秒 例外0 | 3/3 | 18%/15%/16% |
| 99 | 6000件 3.7秒 例外0 | 3/3 | 19%/17%/15% |

どの種も、検算の不一致は0件（あれば例外と同じく数えて終了コード1になる）。

### 種1の出力（`python3 tools/difftest.py --cases 2000 --seed 1 --stats`）

```
本実装なし（.cargo-target/release/kintai も .cargo-target/debug/kintai も無い）。オラクルだけを全ケースで動かします。

== 区分（kintai.case.v1）  2000 件（種 1）
  生成の意図: valid 1083, any 429, InvalidInput 198, refused 155, HolidaySwap 27, LegalHolidayUnspecified 24, Weekly44WithoutConfirmation 14, DisasterRecoveryConstruction 7, ContinuousRotation 6, Managerial 6, PregnantOrPostpartum 5, VariableWeeklyIrregular 5, ResearchDevelopment 4, Physician 4, AgricultureExempt 4, NightDutyPermit 3, Minor 3, DiscretionaryDeemed 3, OffsiteDeemed 3, FourWeekHolidays 3, VariableMonthly 3, AutomobileDriver 2, HazardousWork 2, SurveillanceIntermittent 2, VariableYearly 2, Flex 1, HighlyProfessional 1, NightWindowOverride 1
  オラクルの終了コード: 0: 1384, 2: 198, 3: 135, 4: 283
  有効 1384（69%）・曖昧さの拒否 283・不正 198・対応外 135（不正と対応外で 17%、0か4で 1667）
  エラーのコード: InvalidInput 198, HolidaySwap 27, LegalHolidayUnspecified 24, Weekly44WithoutConfirmation 14, DisasterRecoveryConstruction 7, ContinuousRotation 6, Managerial 6, PregnantOrPostpartum 5, VariableWeeklyIrregular 5, ResearchDevelopment 4, Physician 4, AgricultureExempt 4, NightDutyPermit 3, Minor 3, DiscretionaryDeemed 3, OffsiteDeemed 3, FourWeekHolidays 3, VariableMonthly 3, AutomobileDriver 2, HazardousWork 2, SurveillanceIntermittent 2, VariableYearly 2, Flex 1, HighlyProfessional 1, NightWindowOverride 1
  生成の意図とオラクルの結果の食い違い（probe を除く）: 0 件
  JSON の読み方の場面（タグ: オラクルの結果）: json-1e400-in-swaps InvalidInput 2, json-Infinity-in-swaps InvalidInput 1, json-NaN InvalidInput 1, json-NaN-in-swaps InvalidInput 2, json-bool-in-integer-field InvalidInput 3, json-depth-126 HolidaySwap 1, json-depth-127 HolidaySwap 4, json-depth-128 InvalidInput 2, json-depth-129 InvalidInput 2, json-depth-200 InvalidInput 1, json-integer-field--0 InvalidInput 1, json-integer-field--0.0 InvalidInput 1, json-integer-field-1E2 InvalidInput 1, json-integer-field-2.4e3 InvalidInput 1, json-lone-surrogate-before-unsupported InvalidInput 1, json-lone-surrogate-in-swaps InvalidInput 1, json-number--0.0-in-swaps HolidaySwap 1, json-number-1.5-in-swaps HolidaySwap 1, json-number-1e2-in-swaps HolidaySwap 1, json-surrogate-pair-in-swaps HolidaySwap 5
  検査の順序の場面（タグ: オラクルの結果）: order-data-from-type-before-unsupported InvalidInput 4, order-holiday-before-44h LegalHolidayUnspecified 9, order-holiday-system-before-swaps ContinuousRotation 2/FourWeekHolidays 2, order-range-before-unsupported InvalidInput 6, order-shift-check-before-ambiguity InvalidInput 3, order-swaps-before-night-window HolidaySwap 2, order-two-unsupported DiscretionaryDeemed 1/NightDutyPermit 1, order-type-before-unsupported InvalidInput 2, order-unknown-field-before-unsupported InvalidInput 2, order-unsupported-before-data-from-format LegalHolidayUnspecified 1/Managerial 2, order-unsupported-before-datetime-format DisasterRecoveryConstruction 1/VariableWeeklyIrregular 1, order-unsupported-before-shift-check AutomobileDriver 1
  境界を踏んだ件数（末尾の括弧は数えた単位）:
    1439分の勤務（ケース）: 34
    1日の平日型がちょうど479分（ケース）: 77
    1日の平日型がちょうど480分（ケース）: 481
    1日の平日型がちょうど481分（ケース）: 101
    1日の法定休日の労働が480分超（ケース）: 236
    60時間超の深夜 night_over60>0（ケース）: 35
    IncompleteWeek の週の起算日が2000年より前（ケース）: 6
    data_from あり（ケース）: 588
    data_from が最初の週の起算日ちょうど（ケース）: 334
    勤務が0:00に始まる（ケース）: 99
    勤務が24:00ちょうど（翌0:00）に終わる（ケース）: 286
    勤務が暦日をまたぐ（ケース）: 740
    勤務どうしが接する（ケース）: 202
    勤務の端が22:00ちょうど（ケース）: 328
    勤務の端が5:00ちょうど（ケース）: 192
    同じケースに月の期間が2つ以上（ケース）: 390
    所定外法定内 extra_within>0（ケース）: 941
    日の中で60時間超に入る（0<over60<overtime、ケース）: 67
    曖昧さの拒否 refused（ケース）: 283
    曖昧さの拒否のうち IncompleteWeek を含む（ケース）: 38
    曖昧さの記録 HolidayBoundary（use_default、件）: 327
    曖昧さの記録 IncompleteWeek（use_default、件）: 59
    曖昧さの記録 MonthBoundary（use_default、件）: 73
    曖昧さの記録 WeekBoundary（use_default、件）: 195
    月の期間に60時間超がある（期間）: 67
    月の期間の時間外が3500〜3700分（期間）: 124
    月の期間の時間外がちょうど3599分（期間）: 14
    月の期間の時間外がちょうど3600分（期間）: 41
    月の期間の時間外がちょうど3601分（期間）: 33
    月の起算日が1以外（ケース）: 1007
    空白の区切りの日時（ケース）: 187
    週44時間（ケース）: 252
    週の法定内が上限ちょうどで、時間外もある（ケース）: 252
    週の法定内が上限ちょうど（ケース）: 283

== 36協定（kintai.case36.v1）  2000 件（種 1）
  生成の意図: valid 1663, InvalidInput 337
  オラクルの終了コード: 0: 1663, 2: 337
  有効 1663（83%）・曖昧さの拒否 0・不正 337・対応外 0（不正と対応外で 17%、0か4で 1663）
  エラーのコード: InvalidInput 337
  生成の意図とオラクルの結果の食い違い（probe を除く）: 0 件
  JSON の読み方の場面（タグ: オラクルの結果）: json-integer-field--0 InvalidInput 4, json-integer-field--0.0 InvalidInput 1, json-integer-field-1e2 InvalidInput 4, json-integer-field-60.0 InvalidInput 2, json-lone-surrogate InvalidInput 8, json-year-start-month--0 InvalidInput 1, json-year-start-month-4.0 InvalidInput 3, json-year-start-month-4e0 InvalidInput 3
  上限44640分の場面（タグ: オラクルの結果）: cap-history-44640 終了0 6, cap-history-44641 InvalidInput 8, cap-holiday-44640 終了0 12, cap-holiday-44641 InvalidInput 5, cap-overtime-44640 終了0 4, cap-overtime-44641 InvalidInput 4, cap-sum-44640 終了0 9, cap-sum-44641 InvalidInput 11
  境界を踏んだ件数（末尾の括弧は数えた単位）:
    O がちょうど2700分（発動月でない、ケース）: 360
    O がちょうど2701分（発動月、ケース）: 384
    O が協定の月限度+1（ケース）: 137
    O が協定の月限度ちょうど（ケース）: 198
    O＋H がちょうど44640分（上限、ケース）: 25
    findings AnnualOvertimeExceeded（件）: 1397
    findings GeneralAnnualLimitExceeded（件）: 597
    findings GeneralMonthlyLimitExceeded（件）: 2079
    findings MonthlyOvertimeExceeded（件）: 2316
    findings MonthlyTotalReached100h（件）: 1079
    findings MultiMonthSumExceeded（件）: 7703
    findings NoAgreement（件）: 2526
    findings SpecialAnnualExceeded（件）: 731
    findings SpecialCountExceeded（件）: 1022
    findings SpecialMonthlyTotalExceeded（件）: 833
    findings UnknownHistory（件）: 7331
    history の total がちょうど44640分（上限、ケース）: 6
    months が空（ケース）: 41
    year_overtime_known が偽の月がある（協定年の途中から、ケース）: 168
    協定の月限度が2700未満で、限度超え2700以下の月（ケース）: 546
    協定の種類 general（ケース）: 590
    協定の種類 none（ケース）: 328
    協定の種類 special（ケース）: 745
    協定年の ΣO が協定の年限度+1（ケース）: 26
    協定年の ΣO が協定の年限度ちょうど（ケース）: 33
    協定年の ΣO が法定の21600分+1（ケース）: 41
    協定年の ΣO が法定の21600分ちょうど（ケース）: 36
    協定年をまたぐ（ケース）: 683
    回数を超えたあとに発動しない月がある（ケース）: 133
    履歴が足りず Unknown あり（ケース）: 623
    月の T がちょうど5999分（ケース）: 37
    月の T がちょうど6000分（ケース）: 53
    月の T がちょうど6001分（ケース）: 23
    発動月の数が上限+1（ケース）: 266
    発動月の数が上限ちょうど（ケース）: 331
    直近n月の合計が4800×n+1（ケース）: 70
    直近n月の合計が4800×n−1（ケース）: 66
    直近n月の合計が4800×nちょうど（ケース）: 79
    違反あり（UnknownHistory を除く、ケース）: 1400

== 年休（kintai.caseleave.v1）  2000 件（種 1）
  生成の意図: valid 1625, InvalidInput 217, LeaveSplitGrant 70, LeaveHourly 52, LeaveOutOfTable 36
  オラクルの終了コード: 0: 1625, 2: 217, 3: 158
  有効 1625（81%）・曖昧さの拒否 0・不正 217・対応外 158（不正と対応外で 19%、0か4で 1625）
  エラーのコード: InvalidInput 217, LeaveSplitGrant 70, LeaveHourly 52, LeaveOutOfTable 36
  生成の意図とオラクルの結果の食い違い（probe を除く）: 0 件
  JSON の読み方の場面（タグ: オラクルの結果）: json-grant-halves-20.0 InvalidInput 1, json-grant-halves-2e1 InvalidInput 2, json-integer-field--0 InvalidInput 2, json-integer-field-2.4e3 InvalidInput 2, json-integer-field-5.0 InvalidInput 2, json-integer-field-5e0 InvalidInput 3, json-lone-surrogate InvalidInput 3
  検査の順序の場面（タグ: オラクルの結果）: order-hourly-before-out-of-table LeaveHourly 8, order-hourly-before-split LeaveHourly 8, order-split-before-out-of-table LeaveSplitGrant 8, order-unsupported-before-date-format LeaveHourly 4
  as_of で対応外の判定から除く・除かない場面（タグ: オラクルの結果）: asof-bad-as-of-hourly-future LeaveHourly 2, asof-bad-as-of-split LeaveSplitGrant 10, asof-hourly-after-as-of-ignored 終了0 9, asof-hourly-bad-date LeaveHourly 6
  境界を踏んだ件数（末尾の括弧は数えた単位）:
    2月29日の付与（ケース）: 3
    5日の期間の終わりが2月29日（ケース）: 48
    5日義務 InProgress（ケース）: 1063
    5日義務 Met（ケース）: 320
    5日義務 NotApplicable（ケース）: 385
    5日義務 NotMet（ケース）: 1021
    5日義務の取得が9か11半日（ケース）: 205
    5日義務の取得がちょうど10半日（ケース）: 124
    as_of が失効日の当日（ケース）: 87
    as_of が期間の最後の日の翌日（ケース）: 91
    as_of が期間の最後の日（ケース）: 114
    as_of の当日の付与（ケース）: 100
    as_of より後の取得の記録（数えない、ケース）: 174
    使用が奇数半日のロットがある（半日ずつの消化、ケース）: 847
    出勤率 below を含む（ケース）: 370
    初回の付与日に同じ日付がなく翌月1日（ケース）: 185
    前倒しの付与を明示（ケース）: 154
    半日の取得（ケース）: 1228
    失効した未使用分 expired_halves>0（ケース）: 1037
    失効日の前日に取得（ケース）: 222
    失効日の当日に取得（別のロットから消化、ケース）: 150
    明示の付与に as_of より後のもの（出さない、ケース）: 44
    比例付与（10日未満の付与あり、ケース）: 385
    注意 AttendanceUnconfirmed（ケース）: 621

本実装なし: オラクルだけを 6000 件で実行（3.1秒）。例外 0 件。
```

読み方: 境界の件数は、生成の意図ではなく、ケースとオラクルの出力から数え直している。指示書に挙げられた境界と、その後の回答で足した境界（`data_from`、入れ子の127/128段、`as_of` の前後、分割付与の境、上限44640分のちょうどと1分超など）は、どれも踏んでいる。上限の場面は、44640分ちょうどが有効、44641分が `InvalidInput` になっている。

## ④ 仕様への疑問

### 決着した疑問

**第1版の Q1〜Q34**（第2版で決着）。実装を変えたのは8件（Q9・Q10・Q11・Q20・Q21・Q24・Q27・Q31）で、ほかの26件は第1版の読みのとおりに決まった。

**第2版の Q35〜Q44**（第3回の回答で決着）。

| 番号 | 決まったこと | 実装の変化 |
|---|---|---|
| Q35 | `holiday_swaps` は入力の表にある任意の配列 | なし（読みどおり） |
| Q36 | `LeaveOutOfTable` は、規則2の①に当たらず、②で48日未満か③で週0日のときだけ。`grants` を明示しても同じ | なし（読みどおり） |
| Q37 | 対応外の判定から、`as_of` と日付が正しい書式で `as_of` より後の取得と付与を除く。書式の誤りは入れたまま判定し、手順4で不正。`as_of` が誤りなら何も除かない | 除く処理を足した |
| Q38 | 付与日の昇順に安定に並べ、同じ日は入力の順 | なし（読みどおり） |
| Q39 | 下限値に同じ規則を当てる（21600分以下なら `GeneralAnnualLimitExceeded`、超えれば `AnnualOvertimeExceeded`） | なし（読みどおり） |
| Q40 | `months` が空なら、`history` は日が1〜28であることだけを見る | なし（読みどおり） |
| Q41 | `halves` は1〜200の整数 | 0以上 → 1〜200 |
| Q42 | トップレベルの必須は、年休が `schema` `as_of` `employee` `taken`、36協定が `schema` `agreement` `months` `history` | なし（読みどおり） |
| Q43 | 最も外側を1段目と数え、127段まで読める。128段目に配列かオブジェクトがあれば読めない | 128段まで → 127段まで |
| Q44 | `-0` は整数の欄でなければ読める（小数として読み、整数の欄で型の誤り）。`1e400` はどこでも読めない | `-0` を読むときに拒否 → 整数の欄で拒否 |

**第3回の Q45・Q46**（最後の回答で決着）。

| 番号 | 決まったこと | 実装の変化 |
|---|---|---|
| Q45 | `grants` に書式の誤りのある付与日があるときは、分割付与の判定をしない（手順4で `InvalidInput`） | なし（読みどおり）。生成器の場面を probe から `InvalidInput` に上げた |
| Q46 | 36協定の O と H は0以上の整数で、合わせて44640分以下。`history` の `total` は0〜44640分。超えたら手順2で `InvalidInput` | 上限の検査を足した |

### 新しい疑問

なし。仕様がまだ決めていない点は残っていない。

## ⑤ やらなかったこと

- 本実装との突き合わせ。調整役が本体で行う（区分は、本実装と6つの種×2000件で不一致0件だったと連絡を受けた）。
- 仕様の文書と vectors の修正。作業コピーの docs/spec は、最後の回答の記述が入っていない状態のまま残っている。本体から同期すると本体に触れることになるので、同期していない。
- 指示書の「読むもの」と差分の外にある仕様は、計算に使っていない（区分の入力が分になってからの計算だけを扱う）。`MonthStartDayMismatch` は TOML だけの判定なので、ケースでは扱わない。
- 不一致の既定の書き出し先は `.cargo-target/difftest-out/`（`.gitignore` に `tools/difftest-out/` が無いため）。今回は本実装を流していないので作っていない。

作業の一時ファイル（統計の控え、CLI 照合用のケース）は、作業コピーの `.tmp/` に置き、終了前に消した。
