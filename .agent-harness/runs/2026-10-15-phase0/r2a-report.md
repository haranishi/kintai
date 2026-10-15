# R2a 報告書: 36協定の上限・年次有給休暇・国民の祝日（実装担当: Claude Opus）

## 第2周の前半は完了し、品質の基準は cargo deny の脆弱性の検査を除いて通った

36協定の上限（`limits36`）、年次有給休暇（`leave`）、国民の祝日（`holidays`）の3つの部品と、`kintai case` の36協定・年休のケースを、03:33 の版の仕様どおりに作った。数値例（36協定は公的な6件と導出の4件、年休は公的な2件と導出の2件、ほかに最初の付与日の6件）は、ライブラリでも実際のバイナリでも期待の全項目と一致した。試験はこの周で102件を足し、全体の231件が合格、失敗は0件だった。

品質の基準のうち `cargo deny check` だけが、advisories（脆弱性の検査）で失敗した。この作業コピーの脆弱性データベースには `.git` がなく、ネットワークも使えないため、cargo-deny がデータベースを読めない。git の操作は禁止なので、こちらでは直していない。bans・licenses・sources は `--offline` で通った。依存は1つも足しておらず（`Cargo.lock` は 01:59 のまま）、本体で流せば R1 と同じ結果になるはずだ。本体に取り込んだ後で、`cargo deny check` を流して確かめてほしい。

試験の強さは、境界の誤りをわざと入れる変異で確かめた。最終のコードに63件を入れ、すべて検出した。

別実装との差分試験の前に、そろえるべき読みが4点ある（④の1〜4）。最も影響が大きいのは、年休の出力の例が管理簿を1行しか示していない点だ。私は規則の文（付与ごとに1行）に従い、取得のない付与にも行を出している。

## ① 作った・変えたファイル（R1 のコードは変えていない）

本体へ取り込むのは次の17ファイル。`lib.rs` `main.rs` `case.rs` `dto.rs` `failure.rs`、各 `Cargo.toml`、`docs/`、`tools/` は変えていない。

- `crates/kintai-jp-labor/src/`: `limits36.rs` `leave.rs` `holidays.rs`
- `crates/kintai-jp-labor/tests/`: `ac05_limits36_rules.rs` `ac05_limits36_vectors.rs` `ac05_limits36_properties.rs` `ac06_leave_dates.rs` `ac06_leave_table.rs` `ac06_leave_rules.rs` `ac06_leave_properties.rs` `ac07_holidays.rs`
- `crates/kintai-cli/src/`: `case36.rs` `case_leave.rs`
- `crates/kintai-cli/tests/`: `ac09_36_case.rs` `ac09_36_properties.rs` `ac09_leave_case.rs` `ac09_leave_properties.rs`

作業フォルダには `r2a-progress.md`、この報告書、`r2a-gates.txt`（品質の基準の全出力）、`r2a-self-mutate.py`（変異の確認の道具。作業コピーの中で書き換えて戻す）、`r2a-self-mutate.log`（最終の63件）、`r2a-self-mutate-first.log`（初回の60件）を置いた。

## ② 受け入れ条件と試験の対応

| 条件 | 確かめている試験（ファイル: 要点） |
|---|---|
| AC-05.1 | `ac05_limits36_rules.rs`: 区分の結果から O と H を取り出す（法定休日の12時間は H だけ、土曜の法定外休日は8時間を超えた分だけが O）。`months_from_periods` は実労働のない月を0で埋める |
| AC-05.2〜05.5、05.10、05.12〜05.15 | 同じファイル: 協定なし、協定の限度ちょうどと1分超、21600分ちょうどと1分超で別のコード、6回目と7回目、5999分と6000分、限度の範囲（5項目×3種類）、発動月は2700分超、累計の違反は O が0の月にも付く、年の途中から始まる入力の下限値 |
| AC-05.6〜05.8 | 同じファイル: n＝2〜6のそれぞれで 4800×n ちょうどと1分超、履歴なしと歯抜け、協定年の替わり。`ac05_limits36_properties.rs`: T の分け方で合計が変わらない、協定年の初めで戻る |
| AC-05.9 | `ac05_limits36_vectors.rs`: 10件の `expect` の全項目（知らない項目があれば試験を失敗にする）。CLI の `ac09_5_vectors_through_the_binary` |
| AC-05 全体 | `ac05_matches_a_naive_reading_of_the_spec`: 協定年の初めを後ろ向きに探す素朴な実装と、無作為の300件で全項目が一致 |
| AC-06.1、06.12 | `ac06_leave_table.rs`: 条文の写しの漢数字を読み、通常7行・比例28セル・年間の区分・30時間・4日・216日と照合。優先順位①→②→③と、各区分の両端 |
| AC-06.2 | `ac06_leave_dates.rs`: 2023-08-29 の例、`derived-first-grant-dates`、2000〜2099年の全日付×n＝1・6・12・24・36 を、1日ずつ進める読みと照合 |
| AC-06.3〜06.7、06.9、06.10 | `ac06_leave_rules.rs`: below の年は付与せず勤続年数は進む（平成6年基発第1号の例）、注意は1回、付与日の当日から使え失効日の当日から使えない、古いロットから消化し1日は2つのロットにまたがる、半日、10半日ちょうどで `Met`、繰越分と重なる期間、対応外の順と `as_of` より後の扱い、管理簿の並び、残数の不足 |
| AC-06.8 | `ac06_8_official_vectors`（4件の `expect` の全項目）と CLI の `ac09_6_vectors_through_the_binary` |
| AC-06 全体 | `ac06_leave_properties.rs`: 半日ずつ消化する素朴な実装と、結果も誤りも一致（400件）。残数＝付与−使用−失効。取得を足しても期間の合計は減らない |
| AC-07.1〜07.6 | `ac07_holidays.rs`: 埋め込みの143行と公式CSV、規則で143件、春分・秋分の16件と62件、2020〜2099年の全年を式と照合、年の状態、3条の非再帰、2条の表と生成の一致、R1 の区分で祝日が法定休日にならない |
| AC-09.5〜09.7 | `ac09_36_case.rs`（9件）と `ac09_leave_case.rs`（8件）: 出力の1行の完全一致、指摘と対応外の順、検査の順（手順2→3→4）、型・`null`・知らない項目・同じ名前の項目・範囲外。`ac09_*_properties.rs`（各3件）: 壊した入力でもパニックせず、決まった終了コードで終わる |

## ③ 品質の基準のコマンドの出力

全文は `r2a-gates.txt`。末尾は次のとおり。

```
$ cargo fmt --all -- --check                                  → 出力なし [終了コード 0]
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.18s       [終了コード 0]
$ cargo test --workspace
    test result: ok. 4 passed; 0 failed; ... （文書の例）
    （試験バイナリ32個と文書の例3つの集計: 合格 231 件、失敗 0 件）              [終了コード 0]
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    Generated .../.cargo-target/doc/kintai/index.html and 3 other files        [終了コード 0]
$ cargo deny check
    [ERROR] failed to fetch advisory database ...: fatal: not a git repository [終了コード 1]
$ cargo deny --offline check bans licenses sources
    bans ok, licenses ok, sources ok                                          [終了コード 0]
$ python3 tools/check-spec-trace.py
    参照の無い条件: 35件（AC-05・06・07・09.5〜09.7 の rust 条件はすべて参照あり）  [終了コード 0]
$ cargo +1.88 check --workspace --all-targets                                 [終了コード 0]
```

参照の無い35件は、別実装の側の条件（AC-03.14・05.11・06.11・13.6）と、R2b・R3 の範囲（AC-01・08・10・12）である。

変異の確認は、初回の60件で56件を検出した。生き残った4件から、2つの弱点が分かった。一つは、年休の消化で失効の判定が2か所にあり、片方だけを壊しても答えが変わらなかったこと。判定を `Draft::usable_on` の1か所にまとめ、付与日の当日の試験を足した。もう一つは、2088年の春分である。近似式の値が 20.000052 で整数の境に最も近く、2050年までの照合では定数の小さな誤りが見えなかった。2020〜2099年の全年を仕様の式と照合する試験を足した。秋分の定数を0.0001だけ変える変異は、2020〜2099年のどの日も変えない等価な変異だったので、日が変わる大きさの2件に置き換えた。

## ④ 仕様への疑問と、読みで決めた点

差分試験に直結する4点:

1. **年休の出力の例の管理簿**: 例は `lots` と `five_day` が2行なのに、`register` が1行しかない。規則は「付与ごとに」なので、取得のない付与にも `"taken":[]` の行を出した（試験 `ac09_7_output_is_one_deterministic_line`）。例の省略か、取得のない付与を出さない意図かを決め、例か規則を直してほしい。
2. **特別条項の必須の項目**: 「`special` は特別条項の4項目も必須」とあるが、特別条項の項目は3つしかない。判定3・4の「既定2700分」「既定21600分」も、ケースの形式の「必須」と合わない。私は `general` は月・年の2項目、`special` はさらに特別条項の3項目を必須とし、既定値では補っていない。
3. **手順3の「書式の誤り」**: 検査の順序は、範囲の誤りと書式の誤りを別に並べている。そこで、書式が正しく範囲の外の日付（`2100-01-01` など）は `as_of` と比べ、分割付与の並べ替えにも入れた。例えば `as_of` が2026-01-01のとき、`2100-01-01` の時間単位の取得は判定に入らず、手順4の不正な入力になる。0000年は Python の日付型が扱えないので、書式の誤りとした。範囲の誤りも書式の誤りに含める読みなら、この例は `LeaveHourly` になる。
4. **`grants` を明示したときの `attendance`**: 使わない項目だが、値は手順2で検査している（定義にない値は不正な入力）。

影響が小さいもの:

5. 同じ付与日の `grants` は禁止されていないので受け付け、入力の順に2つのロットにした。`NotApplicable` の付与の `taken_halves` は、0でなく期間の取得の合計を出している。
6. `as_of` が雇入れ日より前の入力は受け付ける（ロットは空）。最初の付与日より前の取得は、使えるロットがないので残数の不足（不正な入力）になる。
7. 祝日: 2051〜2099年の注意には、仕様にコードの名前がないので `EquinoxUnverified` を付けた。振替休日と国民の休日の両方に当たる日は振替休日にした（2020〜2099年では起きないことを試験で確かめた）。浮動小数点で近似式を計算する実装は、2088年の春分で丸めの誤差に注意が要る。
8. 根拠の条文の定数（36条、39条、115条、労基則24条の3・5・7、祝日法と法令の版）は、`provenance.rs` を変えられないので各モジュールに置いた。`provenance::ALL` には入っていないが、条文の写しとの照合は各試験で行っている。
9. `dto.rs` の JSON の部品は private なので、同じ流儀の部品を `case36.rs` に置いて `case_leave.rs` と共用した。年休の対応外のコードは R1 の `UnsupportedCode` にないので、`Failure` を直接作って終了コード3にしている。`kintai --help` の文は区分のケースだけを説明したままである（`main.rs` は R1 の担当範囲）。
10. 作業の初めに見つけた vector の誤り（`derived-2023-08-29` の `window_end`）は、03:24 に直してもらい、その後は全件一致している。

## ⑤ やらなかったこと

別実装の側の条件（AC-05.11・06.11）、`tools/mutations/` の変異の定義（R3 の担当で、`tools/` は読むだけのため）、コマンドの `check` `leave` `explain` と方針ファイル、速さの測定、仕様の文書の更新。

## ⑥ 次の周への申し送り

- **36協定**: `Agreement::validate()` → `judge(&ValidAgreement, &[MonthHours], &[HistoryMonth])`。区分の結果からは `months_from_periods(&periods, first, last)` が、実労働のない月を0で埋めた協定月を返す（`check` の「実労働のない月はOもHも0」に使える）。日付を読む前に分の範囲を見るなら `check_month_minutes`・`check_history_minutes`。
- **年休**: `compute(&LeaveInput)` が範囲→対応外→計算を順に行う。日付の文字列を読む前に対応外を見るなら、`case_leave.rs` と同じく `WorkingTerms::validate` → `GRANT_HALVES_RANGE` → `check_unsupported`（日付は `CalendarDate::parse`）→ `Date::parse` → `compute` の順に呼ぶ。人が読む出力では半日を2で割る。
- **祝日**: `holiday_on(date)` が名前・種別・表の状態・注意を返す。出典の1行は `SOURCE_NOTE`。法定休日の判定には使わない。
- **R3 の変異試験**: `r2a-self-mutate.py` の63件は `tools/mutate.py` と同じ `find`・`replace` の形なので、`tools/mutations/{limits36,leave,holidays}.json` に写せる。秋分の定数は0.0001程度ではどの日も変わらない（2074年の0.015、2045年の0.0086以上を動かす）。年休の消化の `first_active` の飛ばしは高速化のためで、消しても答えは変わらない。どちらも等価な変異になるので避ける。
- **仕上げ**: `dto.rs` の部品を `pub(crate)` にして `case36.rs` の写しを消す。根拠の条文の定数を `provenance.rs` に集めて `ALL` に入れる。
