# R3m 報告書: 速さの測定と変異試験（実装担当: Claude Opus）

## 速さは4項目とも目標の1割未満で、変異は7部品とも生き残り0になった

速さの4項目（区分、36協定の年間判定、打刻CSVの取込、起動）は、どれも目標の時間の1割未満で終わった。遅い所がないので、部品のソースは変えていない。測定用の例を2つと、まとめて流すシェルを置いた。

変異試験は、部品ごとの7ファイルに計264件の境界の誤りを入れた（どのファイルも30件以上）。1回目は10件が生き残った。1件は出力の変わらない等価な変異だったので定義から外し、残りの9件は試験を足して2回目で赤くした。最終の流し直しでは、全件を検出した（生き残り0、コンパイルできない変異0）。生き残った9件は、どれも「その日・その値を含む側」の境界か、60時間超の分の帰属で、それまでの試験が内側の値しか見ていなかった。

調整役に先に伝えたいことが1つある。R1 の `tools/mutations/r1-classify.json` の3件目（月の期間で、起算日当日を前の期間にする）を今のコードで流すと、第2周で足された `limits36::months_from_periods` のループが止まらず、メモリを使い切るおそれがある（⑤の1）。私は流していない。

## ① 作った・変えたファイル（部品のソースは変えていない）

本体へ取り込むのは次の14ファイル。`crates/*/src/**`、各 `Cargo.toml`、`Cargo.lock`、`docs/` は変えていない。依存も足していない。

- `crates/kintai-jp-labor/examples/bench.rs`（新規）: AC-10.1 区分、AC-10.2 36協定、起動（`kintai --version` を `std::process::Command` で200回）
- `crates/kintai-import/examples/bench.rs`（直した）: AC-10.3 取込。慣らしのあと複数回測り、最良と中央値を出す形にした。測る範囲を `read_limited` → `read_punches` の通しにした
- `tools/bench.sh`（新規）: 機械・OS・Rust の版・release のプロファイルを出してから、上の2つを流す
- `tools/mutations/` の7ファイル（新規）: `limits36.json` `leave.json` `holidays.json` `report.json` `punch.json` `import.json` `core.json`
- 変異で生き残った誤りを赤くする試験（新規）: `crates/kintai-jp-labor/tests/mut_limits36.rs` `mut_leave.rs` `mut_report.rs`、`crates/kintai-core/tests/mut_core.rs`

作業フォルダには、`r3m-progress.md`、この報告書、`r3m-gates.txt`（品質の基準の出力）、`r3m-bench.txt`（速さの測定の出力）、変異試験のログ（`r3m-mutate-<名前>-run1.log`・`-run2-survivors.log`・`-final.log`）を置いた。

## ② 速さの測定の結果: どれも目標の1割未満

`tools/bench.sh` を3回流した（各項目は慣らし1回のあと7回）。最良は3回の流しを通した最小（起動は600回、ほかは21回の中）、中央値は3回それぞれの中央値の範囲。判定は中央値で行う。出力の全文は `r3m-bench.txt`。

| 項目 | 目標 | 最良 | 中央値 | 測った範囲 |
|---|---|---|---|---|
| 区分の計算（AC-10.1） | 100 ms | 6.873 ms | 6.886〜7.378 ms | 1,000人×31日、勤務27,457件。勤務の検査 `ShiftSet::new` → `classify` |
| 参考: 区分だけ | 100 ms | 6.500 ms | 6.581〜6.804 ms | 検査済みの勤務 → `classify` |
| 36協定の年間判定（AC-10.2） | 50 ms | 0.933 ms | 0.935〜0.979 ms | 1,000人×12か月＋履歴5か月。`Agreement::validate` → `judge` |
| 打刻CSVの取込（AC-10.3） | 3,000 ms | 114.169 ms | 116.810〜122.174 ms | 100万行・33,250,027バイト（UTF-8、LF）。`read_limited` → `read_punches` |
| 参考: 打刻の対 | — | 52.004 ms | 56.976〜60.239 ms | 上の100万件を `pair`（`worker_favorable`）。1,000人 |
| 起動（`nfr.md` の表。番号なし） | 50 ms | 1.281 ms | 1.602〜1.937 ms | `kintai --version` を `std::process::Command` で200回 |

- 機械: Apple M4 Pro（14コア、48 GiB）、macOS（Darwin 27.0.0、arm64）。2026-10-15 07:22 に測った。
- Rust: rustc 1.99.0（2026-09-28）。プロファイルは release（ワークスペースに `[profile.release]` がないので Cargo の既定: opt-level=3、lto なし、codegen-units=16、debug なし）。
- 測定中も、この機械ではほかの作業（ブラウザを使う試験など）が動いていた。負荷の平均（uptime の1分値）は約5。
- 合成データは決まった式で作る（乱数を使わない）。区分のデータは、日勤・遅番（深夜を少し含む）・夜勤（暦日・週・月をまたぐ）の3通りで、日曜（法定休日）の労働、週40時間の超過、月60時間の超過、曖昧さ（`use_default` で記録して計算を続ける）、`data_from` による `IncompleteWeek` を含む。36協定は、特別条項つき・一般条項・協定なしを3分の1ずつにし、発動月（12,000か月のうち3,376）、T が100時間以上の月（150）、平均80時間超（月とnの組で402）、履歴の歯抜け（143人）を含む。
- 測るたびに答えの指紋（分の合計や指摘の数）を比べ、回によって答えが変われば「測れなかった」として失敗にする（AC-10.4 の簡単な確かめを兼ねる）。
- 起動の値は、`kintai` を新しく起こす時間を含む。今の `kintai --version` は R1 の版で、R3 の取り込みのあとは取り直しが要る（⑦）。

## ③ 変異試験の結果: 1回目の生き残り10件のうち9件を試験で赤くし（1件は等価）、最終は264件すべて検出

| 部品（ファイル） | 件数 | 試験のコマンド（`cargo test -q ... --no-fail-fast`） | 1回目 検出／生き残り | 足した試験 | 最終 検出／生き残り |
|---|---|---|---|---|---|
| 36協定（`limits36.json`） | 44 | `-p kintai-jp-labor` | 41／3 | `mut_limits36.rs`（2件） | 44／0 |
| 年休（`leave.json`） | 44 | `-p kintai-jp-labor` | 41／3 | `mut_leave.rs`（4件） | 44／0 |
| 祝日（`holidays.json`） | 30 | `-p kintai-jp-labor` | 30／0 | なし | 30／0 |
| 月次・保存期限・explain（`report.json`） | 41 | `-p kintai-core -p kintai-jp-labor` | 39／3（1件は等価で外した。⑤の7） | `mut_report.rs`（2件） | 41／0 |
| 打刻と休憩の不足（`punch.json`） | 38 | 3つの部品 | 38／0 | なし | 38／0 |
| 取込（`import.json`） | 30 | `-p kintai-import` | 30／0 | なし | 30／0 |
| core（`core.json`） | 37 | `-p kintai-core` | 35／1（1回目は36件を3つの部品で） | `mut_core.rs`（1件） | 37／0 |
| 計 | 264 | | 254／10 | 9件 | 264／0 |

コンパイルできない変異と、適用できない変異（`find` が見つからない）は、どの回も0件だった。定義を書く道具で、各 `find` がソースに期待の数だけあることを先に照合した。出発点に使った前の周の変異は、R2a の57件（CLI の6件を除く）、R2b の39件、R1 の core の7件。残りの161件をこの周で足した。指示書の例の境界（`4800 * n`、`T < 6000`、発動月の `> 2700`、協定年の起算月の替わり、履歴の月数、`UnknownHistory`、`year_overtime_known`、付与日の次の初日、失効の当日、半日、古いロットから、5日の判定、期間の重なり、`below`、`as_of` より後、振替休日、国民の休日、春分・秋分の定数、`OutOfRange`、30分丸め、遅い方の起算日、ID、`night` の扱い、秒の丸め、遷移の抜け、24時間、同時刻、休憩の360・480分、文字コード、BOM、見出し、列の過不足、入力の上限、日付の加減、うるう年、月の期間の初日）は、すべて入れた。

生き残った9件と、足した試験（試験の名前かコメントに AC の番号を付けた）:

- 36協定: 区分の結果から O を作るとき60時間超を引く（`months_from_periods`）、H に60時間超を足す（`from_period`）。それまでの試験の月には60時間超がなかった。→ `ac05_1_over60_minutes_stay_in_overtime_and_out_of_holiday`（10月の平日22日に時間外240分ずつ、O＝5280・うち60時間超1680、H＝480）。
- 36協定: 履歴の日が協定月の初日の日より前でも許す（`!=` を `>` に）。それまでの試験は、日が後ろにずれた例だけだった。→ `history_day_must_equal_the_agreement_month_start_day`（起算日16日の協定月に、1日と15日の履歴）。
- 年休: 5日の期間の最後の日の取得を期間に入れない、`as_of` 当日の取得を数えない、`as_of` 当日の明示の付与を出さない。→ `ac06_6_taking_on_the_last_day_of_the_window_counts`、`ac06_10_taking_on_as_of_is_counted`、`ac06_10_balance_is_checked_for_a_record_on_as_of`、`ac06_10_explicit_grant_on_as_of_is_shown`。
- 月次: 週の曖昧さの記録（`IncompleteWeek`）を、週の起算日が期間にあるときだけ出す。explain: 週の記録を注意に入れない。→ `ac03_15_incomplete_week_record_is_in_the_report_of_each_month_it_touches`（10月の集計に、9月27日に始まる週の記録が出る）、`ac03_15_incomplete_week_note_is_attached_to_the_days_of_that_week`。
- core: 時点の型の最後の分（2200-12-31T23:59）を範囲外にする。それまでの試験は、端の1分外が作れないことだけを見ていた。→ `ac02_6_instant_type_edges_are_inside_the_range`。

2回目は、生き残った9件だけを、試験を足した状態で流し、9件とも検出した（`r3m-mutate-*-run2-survivors.log`）。最終は7ファイルの全件を流し直した（`r3m-mutate-*-final.log`。並行で約25分）。各ファイルの末尾は次のとおり。

```
$ python3 tools/mutate.py tools/mutations/limits36.json   → == 生き残り == なし  [終了コード 0、1206秒]
$ python3 tools/mutate.py tools/mutations/leave.json      → == 生き残り == なし  [終了コード 0、1202秒]
$ python3 tools/mutate.py tools/mutations/holidays.json   → == 生き残り == なし  [終了コード 0、727秒]
$ python3 tools/mutate.py tools/mutations/report.json     → == 生き残り == なし  [終了コード 0、1186秒]
$ python3 tools/mutate.py tools/mutations/punch.json      → == 生き残り == なし  [終了コード 0、1550秒]
$ python3 tools/mutate.py tools/mutations/import.json     → == 生き残り == なし  [終了コード 0、257秒]
$ python3 tools/mutate.py tools/mutations/core.json       → == 生き残り == なし  [終了コード 0、293秒]
（どれも [基準] 変更なし: 終了コード=0。秒は7本を並行に流したときの値）
```

## ④ 品質の基準のコマンドの出力: cargo deny の勧告の検査だけが、環境の理由で通らない

全文は `r3m-gates.txt`（2026-10-15 07:22、最終の形で流したもの）。各コマンドの末尾は次のとおり。

```
$ cargo fmt --all -- --check                                   → 出力なし [終了コード 0]
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.32s            [終了コード 0]
$ cargo test --workspace
    all doctests ran in 0.43s; merged doctests compilation took 0.19s
    （試験の結果の行49個の集計: 合格 337 件、失敗 0 件）                            [終了コード 0]
    ※ 例の名前の重なりで「warning: output filename collision」が2つ出る（⑤の5）
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    Generated .../.cargo-target/doc/kintai/index.html and 3 other files            [終了コード 0]
$ cargo deny check
    [ERROR] failed to fetch advisory database https://github.com/RustSec/advisory-db ...
    fatal: not a git repository (or any of the parent directories): .git           [終了コード 1]
$ cargo deny --offline check bans licenses sources
    bans ok, licenses ok, sources ok                                               [終了コード 0]
$ python3 tools/check-spec-trace.py
    条件 114件（manual 3件は対象外）。参照の無い条件: 13件
    AC-03.14 AC-12.1〜12.8 AC-06.11 AC-05.11 AC-10.8 AC-13.6                         [終了コード 0]
    （AC-10.1・10.2 は crates/kintai-jp-labor/examples/bench.rs、AC-10.3 は crates/kintai-import/examples/bench.rs が参照）
$ cargo +1.88 check --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.48s            [終了コード 0]
$ python3 tools/mutate.py tools/mutations/<名前>.json  → 7ファイルとも生き残り0（③）
```

- `cargo deny check` の勧告の検査は、R2a・R2b と同じ理由で失敗する。この作業コピーの勧告の一覧（`.cargo-home/advisory-dbs/`）に `.git` がなく、cargo-deny が中で `git reset` と `git fetch` を試みて、すぐに失敗する（`.git` がないので通信の前に止まる）。依存は足しておらず、`Cargo.lock` は 06:10 の写しのまま。本体で流し直してほしい。
- 参照の無い13件は、R3 の範囲（AC-12、AC-10.8）と別実装の側（`[python]` の4件）で、この周の私の範囲（AC-10.1〜10.3）は参照がある。
- 07:22 の流しのあとに直したのは、`crates/kintai-jp-labor/examples/bench.rs` の使い方の説明の1行（`--startup-runs` を足した）だけで、書式・clippy・文書を流し直して終了コード0を確かめた。
- 必須ではないが `python3 tools/check-links.py` も流した。終了コード1で、問題の7件はどれも前からあるもの（`docs/testing.md` と `docs/benchmarks.md` がない、調査の記録の `downloads/` など）。私のファイルにリンク切れはない。

## ⑤ 仕様への疑問・曖昧な点

1. **r1-classify.json の1件が、メモリを使い切るおそれがある**（最重要）。3件目の「core: 月の期間で、起算日当日を前の期間にする」（`period.rs` の `date.day() >= start_day` を `>` に）を入れると、`MonthPeriod::containing(次の期間の初日)` が今の期間を返す。第2周の `limits36::months_from_periods` は、`while current <= last` のループの中で `current = MonthPeriod::containing(next, start_day)` として次の期間へ進む。この変異の下では `current` が同じ期間に戻るので、ループが止まらず、`MonthHours` を積み続ける。この関数は `ac05_1_months_from_periods_fill_gaps_with_zero` が呼ぶ。`r1-classify.json` は `--workspace` で流すので、mutate.py の時間切れ（900秒）より先に、機械のメモリが尽きうる。コードを読んで確かめ、危ないので流していない。対処は2つある。(a) その1件を `r1-classify.json` から外す（同じ変異は、kintai-core の試験だけで流す `core.json` に入れた。kintai-core の試験は `months_from_periods` を呼ばない）。(b) `months_from_periods` を、年と月を1つずつ進める書き方にし、前に進まなければ誤りを返すようにする。(b) はソースの変更なので、私はしていない。
2. 起動の目標に、受け入れ条件の番号がない。`nfr.md` の速さの表は4項目だが、受け入れ条件は AC-10.1〜10.3 の3つで、起動（`kintai --version` が50ミリ秒以内）には番号がない。測って出しているが、`check-spec-trace.py` では追えない。番号を足すか、表から外すかを決めてほしい。
3. 測る範囲の読み。`docs/benchmarks.md` に書くとき、次の読みを明記してほしい。(a) 区分の「入出力を除く計算だけ」に、勤務の検査（`ShiftSet::new`）を含めるか。両方を出し、判定は含む方（厳しい方）で行った。(b)「1,000人×1か月（約3万日）」は、1,000人×31暦日で、休みの日を除いた勤務は27,457件。(c) 36協定の「年間判定」は、協定月ごとの O と H（12か月）と履歴5か月を渡して判定するところだけで、区分の計算は含めない。(d) 取込は、メモリの中の100万行（33,250,027バイト）を読む時間で、ディスクの読み込みは含めない（環境で大きく変わるため）。
4. `docs/benchmarks.md` がない。`nfr.md` は「測定の結果と方法は `docs/benchmarks.md` に書きます」と書き、`faq.md` もそこへリンクしている（`check-links.py` の既存の失敗の1つ）。`docs/` は読むだけなので作っていない。②の表をそのまま使える。
5. 2つの例の名前が同じ（`bench`）。`cargo test --workspace` のように一度にビルドすると、cargo が「output filename collision」の警告を2つ出す（失敗ではないが、cargo は将来エラーにしうると書いている）。指示書と `nfr.md` がどちらも `examples/bench.rs` と決めているので、名前は変えていない。直すなら、片方の `Cargo.toml` に `[[example]]` を足して名前を分ける（例: `name = "bench-labor"`、`path = "examples/bench.rs"`）。`tools/bench.sh` は、パッケージごとに別々に `cargo run` するので、この警告に関係なく正しい方が動く。
6. 性質の試験は無作為なので、変異試験の結果が回によって変わりうる。1回目で生き残った年休の2件（5日の期間の最後の日、`as_of` 当日の取得）は、2回目には性質の試験 `ac06_matches_a_naive_reading_of_the_spec` も赤くした。乱数が境界を踏むかどうかで変わる。この2件は、決まった試験を足したので毎回赤くなる。
7. 等価な変異を1件外した。explain の「同じ曖昧さの注意を重ねて付ける」（`!notes.contains(&note)` を外す）は、`classify` の結果では、同じ労働日に同じコードの記録が2件にならない（労働日の記録はコードが別々、週の記録は1つの日に1つ）ので、出力が変わらない。手で重複のある区分の結果を作れば違いが出るが、`explain_day` の前提（同じ方針で区分した結果）から外れるので、変異の定義から外した（1回目のログには生き残りとして残っている）。

## ⑥ やらなかったこと

- 部品のソースの変更。速さはどれも目標の1割未満で、最適化は要らなかった。⑤の1の `months_from_periods` も直していない。
- `crates/*/Cargo.toml` の変更（⑤の5の例の名前）と、`docs/benchmarks.md` の作成（⑤の4）。触ってよい範囲の外。
- `kintai-cli` の変異。R3 が並行で作り直している範囲なので、どの変異の定義の試験のコマンドにも `kintai-cli` を入れていない（`r1-classify.json` の CLI の5件と、R2a の CLI の6件は、それぞれ `r1-classify.json` と `r2a-self-mutate.py` に残っている）。
- `cargo deny check` の勧告（advisories）の検査。この作業コピーの勧告の一覧に `.git` がなく、通信もできないため、R2a・R2b と同じく失敗する（④）。依存は足しておらず、`Cargo.lock` も変えていない。
- ディスクからCSVを読む時間の測定（⑤の3の(d)）。

## ⑦ 次の周への申し送り

- 変異試験の流し方: `python3 tools/mutate.py tools/mutations/<名前>.json`。7ファイルを並行で流すと、この機械（14コア）で全体が約25分だった（1つずつ流すと約1時間の見込み）。作業コピーの中に一時フォルダを置くときは、`TMPDIR` のフォルダ名を `target` にすると、mutate.py の写しの除外（`target`）に当たって、写しが写しを含まない（例: `TMPDIR=$WS/.tmp/target`）。JSON に `"exclude"` を書くと既定の除外の一覧を丸ごと置き換えるので、使わなかった。
- 試験のコマンドは部品ごと: `core.json` は kintai-core、`import.json` は kintai-import、`limits36`・`leave`・`holidays` は kintai-jp-labor、`report` は kintai-core と kintai-jp-labor（従業員IDの検査が core にあるため）、`punch` は3つの部品。`core.json` を `--workspace` に広げないこと（⑤の1。ラベルにも書いた）。
- R3 の取り込みのあと: 起動の測定を取り直す（`--version` の経路が変わりうる）。`check` が `months_from_periods` を使うなら、⑤の1の(b)を先に直すとよい。打刻の異常や方針ファイルの読み込みが入ったら、`kintai-cli` の変異の定義（`cli.json` など）を作り、CLI の試験で赤くなるかを見る。
- 速さの測り直し: `tools/bench.sh`（回数は引数。既定7）。release のプロファイルはワークスペースに定義がなく、Cargo の既定。
