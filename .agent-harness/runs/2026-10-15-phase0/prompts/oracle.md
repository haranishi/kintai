# 実装指示書 O1: 別実装（Python、1分ずつ数える素朴な参照実装）

あなたは**別実装の担当**です。リポジトリ `~/Projects/kintai`（日本の労働基準法に基づく勤怠のOSS）の仕様から、Rustの本実装とは独立に、**素朴な参照実装（オラクル）**をPythonで書きます。本実装と無作為の入力で出力を突き合わせ、仕様の読み違いと実装の誤りを見つけるためのものです。

## 最重要: 独立性

- **`crates/` の下を絶対に読まない。** Rust のコードを探さない、開いない、検索しない。本実装の中身を知ると、読み違いが移ります。読んでよいのは、下の「読むもの」だけです。
- **わざと遅く、素朴に書く。** 区分の計算は、勤務の実労働を**1分ずつ**数えるループにする（時点の分を1つずつ進め、その分がどの区分かを決める）。区間の演算や最適化は使わない。速さは求めない。読みやすさと、仕様との対応のしやすさを優先する。
- 標準ライブラリだけを使う（`datetime` `json` `argparse` `random` `csv` `sys` など）。追加の依存は入れない。
- 仕様に曖昧さを見つけたら、**推測で決めず**、報告書の「疑問」に書き、そのとき最も素直な読みで実装して、コメントに `# 疑問Q1` のように印を付ける。

## 守ること

1. gitの操作をしない。ネットワークを使わない。
2. 書き込んでよい場所は、`tools/oracle/`、`tools/difftest.py`、`.agent-harness/runs/2026-10-15-phase0/` だけ。
3. 実データ・実在の人名を使わない。試験データは合成。
4. 他の勤怠・人事・給与ソフトのコードを探さない。

## 読むもの

- `docs/spec/classify.md`、`docs/spec/over60.md`、`docs/spec/unsupported.md`、`docs/spec/validation.md`（検査の順序と曖昧さの定義。**必ず従う**）、`docs/spec/case-format.md`（区分）
- `docs/spec/limits36.md`、`docs/spec/leave.md`、`docs/spec/case-format-36-leave.md`（36協定と年次有給休暇）
- `docs/spec/core.md` の「規則」（暦と範囲。日付は2000-01-01〜2099-12-31、月の期間の起算日は1〜28）
- `docs/spec/vectors/*.json`（公的な数値例と期待値。**自分の実装の自己検査に使う**）
- 参考: `.agent-harness/runs/2026-10-15-phase0/research/track-A.md`、`track-B.md`、`track-C.md`（根拠の引用）

## 作るもの

1. `tools/oracle/classify_oracle.py FILE`: 区分のケース（`kintai.case.v1`）を読み、`kintai.classify.v1` のJSONを標準出力へ。入力の検査（開始 < 終了、重なり、休憩が勤務の内側、必須項目、値の範囲）に失敗したら、標準エラーに `{"error": {"code": "InvalidInput", "message": ...}}` を出して終了コード2。対応外（`work_system` が `standard` 以外、`legal_holiday_weekday` の欠け、週44時間の確認なし、など）は、そのコード（`unsupported.md` の表）で終了コード3。曖昧さの拒否は、`refused` を入れた出力を標準出力に出して終了コード4（`days` と `periods` は空）。成功は終了コード0。
2. `tools/oracle/limits36_oracle.py FILE`: 36協定のケース（`kintai.case36.v1`）→ `kintai.limits36.v1`。エラーの扱いは上と同じ。
3. `tools/oracle/leave_oracle.py FILE`: 年休のケース（`kintai.caseleave.v1`）→ `kintai.leave.v1`。`hours` の取得単位は `LeaveHourly` で終了コード3。
4. `tools/oracle/gen_cases.py`: 乱数の種を与えると、上の3種類のケースを無作為に作る。**境界を狙う**生成を含める（例: 勤務が日をまたぐ、22:00・5:00・24:00ちょうどに接する、480分ちょうどと481分、週の上限ちょうど、法定休日の暦日にまたがる、月の起算日の前後にまたがる、月の時間外が3600分に近い、36協定のTが6000分に近い・直近n月の合計が4800×nに近い、履歴が足りない、年休の付与日が応当日なしの月、取得が失効日の前日・当日）。有効なケースが大半で、不正なケースと対応外のケースも1〜2割混ぜる。
5. `tools/difftest.py --cases N --seed S [--bin PATH]`: `gen_cases.py` で3種類のケースをN件ずつ作り、各ケースについて、Pythonのオラクルと、Rustのコマンド `PATH case FILE`（既定は `.cargo-target/release/kintai`、無ければ `.cargo-target/debug/kintai`）を動かして比べる。比べるのは、①終了コード、②終了コード0または4のとき、標準出力のJSONを読み込んだものの全フィールドの一致、③終了コード2または3のとき、`error.code`（`message` は比べない）。不一致があれば、ケースのファイルを `tools/difftest-out/`（`.gitignore` に無ければ作らず、`.cargo-target/difftest-out/` を使う）に残し、先頭の数件を詳しく表示して、終了コード1。`--bin` が無く本実装がまだ無いときは、「本実装なし」と表示して、オラクルだけを全ケースで実行して終了コード0（オラクルが例外を出さないことの確認）。
6. `tools/oracle/selftest.py`: `docs/spec/vectors/` の全件（`official-examples.json` の7件、`limits36-official.json` の6件、`leave-official.json`）に対し、自分のオラクルが期待値を再現するかを確かめる。期待値の書式は各JSONのとおり（一部の項目は、期待値に書かれたものだけを比べる。`findings_by_month` `sums_at_last` など）。`derived-` で始まるIDは導出の例。**全部一致するまで、オラクルを直す。** 期待値のほうが誤りと考える場合は、直さずに、報告書の「疑問」に根拠つきで書く。
7. 差分試験が本実装を呼ぶ形式は、`docs/spec/case-format*.md` の「エラーの形」「終了コード」のとおり。

## 完了の条件

- `python3 tools/oracle/selftest.py` が全件一致で終了コード0。
- `python3 tools/difftest.py --cases 2000 --seed 1`（本実装なし）が、例外なく終了コード0。
- 生成したケースの分布（有効・不正・対応外・曖昧さの拒否・36協定の違反あり・年休の5日義務 Violated など）の件数を、`--stats` オプションで表示できる。境界を狙う生成が、実際に境界を踏んでいることを、この統計で示す。

## 報告

作業中は `.agent-harness/runs/2026-10-15-phase0/o1-progress.md` に、段階ごとに1〜3行追記する。最後に `.agent-harness/runs/2026-10-15-phase0/o1-report.md` に次を書く: ①作ったファイル ②selftestの出力 ③統計の出力 ④**仕様への疑問**（どの文書のどの記述が曖昧だったか、どう読んで実装したか、`# 疑問Qn` の印の場所）⑤やらなかったこと。
