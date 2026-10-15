# R3m 進捗（速さの測定と変異試験。実装担当: Claude Opus）

- 06:12 指示書 impl-r3m.md、nfr.md、tools/mutate.py、r1-classify.json、R1・R2a・R2b の報告書、r2a-self-mutate.py（63件）、r2b-mutations.json（39件）、既存の kintai-import/examples/bench.rs を読んだ。一時ファイルは $WS/.tmp に置く（mutate.py の一時フォルダは TMPDIR=$WS/.tmp/target。名前が target なので、mutate.py の写しの除外に当たり、JSON に "exclude" を書かずに済む）。
- 06:25 基準の試験は全件合格（rustc 1.99.0、Apple M4 Pro）。変異の定義7ファイルを tools/mutations/ に作った（36協定44・年休44・祝日30・月次とexplain 42・打刻と休憩38・取込30・core 36件。find の数は道具で照合済み）。
- 06:25 危険を1件見つけた: R1 の r1-classify.json の「月の期間で、起算日当日を前の期間にする」は、第2周で足された limits36::months_from_periods の while ループを止まらなくし、MonthHours を積み続けてメモリを使い切る（ac05_1_months_from_periods_fill_gaps_with_zero が呼ぶ）。core.json には入れず、安全な境界（contains の両端、初日の位置）に替えた。報告書に書く。
- 06:26 変異試験の1回目を4本並行で開始（limits36・leave・holidays・punch）。一時フォルダは $WS/.tmp/target。
- 06:29 速さの測定を作った: crates/kintai-jp-labor/examples/bench.rs（AC-10.1 区分・AC-10.2 36協定・起動を Command で200回）、crates/kintai-import/examples/bench.rs（AC-10.3。慣らしのあと複数回で最良と中央値）、tools/bench.sh（環境の表示と通し）。試しの測定（変異試験と並行のため参考値）: 区分 約7ms、36協定 約1ms、起動 約1.4ms、取込 約120ms。どれも目標の1割未満で、ソースの最適化は要らない。
- 06:29 気づき: 2つの例がどちらも名前 bench なので、cargo test --workspace などで一度にビルドすると「output filename collision」の警告が2つ出る（失敗ではない）。直すには crates/*/Cargo.toml に [[example]] name を足す必要があり、触ってよい範囲の外なので報告書に書く。tools/bench.sh はパッケージごとに別々に動かす。
- 06:36 変異試験の1回目の結果（その1。月次・取込・core の3本は 06:31 に追加で開始）: 祝日 30件すべて検出（613秒）、取込 30件すべて検出（254秒）。生き残り0。explain の「同じ曖昧さの注意を重ねて付ける」は、classify の結果では同じ日に同じコードの記録が2件にならないので等価な変異と判断して、report.json から外した（1回目の流れには入っている）。
- 06:48 変異試験の1回目の結果（その2）: 打刻と休憩 38件すべて検出（1427秒）。36協定 44件中3件、年休 44件中3件、月次とexplain 42件中3件（うち1件は外した等価な変異）、core（途中）で1件が生き残った。どれも「その日・その値を含む側」の境界か、60時間超の帰属で、既存の試験が内側の値しか見ていなかった。
- 06:49 生き残りを赤くする試験を足した: jp-labor の tests/mut_limits36.rs（60時間超は O の内数で H に入れない、履歴の日のずれは前でも誤り）、mut_leave.rs（5日の期間の最後の日、as_of 当日の取得と残数の検査、as_of 当日の明示の付与）、mut_report.rs（IncompleteWeek の記録が月の集計と explain の注意に出る）、core の tests/mut_core.rs（時点の型の両端が作れる）。元のコードではすべて合格。
- 06:55 2回目（生き残った9件だけ）: 足した試験で9件すべて検出。うち2件（5日の期間の最後の日、as_of 当日の取得）は、性質の試験 ac06_matches_a_naive_reading_of_the_spec も赤くした。性質の試験は無作為なので、境界を踏むかは回によって変わる（1回目は踏まなかった）。決まった試験で必ず赤くなるようにした。
- 06:56 core.json を kintai-core の試験だけで流す形に変え、R1 の「起算日当日を前の期間にする」を警告つきのラベルで戻した（kintai-core の試験は months_from_periods を呼ばないので止まらない。1回目の検出もすべて kintai-core の試験を含んでいた）。7ファイルの最終の流しを並行で開始。
- 06:57 品質の基準: fmt・clippy・test（337件合格・失敗0）・doc・spec-trace（AC-10.1〜10.3 は参照あり）・cargo +1.88 check は終了コード0。cargo deny check は勧告の一覧に .git がなく1（R2a・R2b と同じ。--offline の bans・licenses・sources は0。Cargo.lock は変えていない）。check-links.py は既存の7件で1（docs/testing.md・docs/benchmarks.md がない等。私のファイルではない）。出力は r3m-gates.txt。
- 06:58 記録の時刻を、ファイルの更新時刻に合わせて直した（06:29 以降の行は、最初は見込みの時刻で書いていた）。
- 07:21 最終の流し（7ファイル並行、06:56〜07:21の約25分）: 36協定44・年休44・祝日30・月次とexplain 41・打刻と休憩38・取込30・core 37、計264件をすべて検出。生き残り0、コンパイルできない変異0、適用できない変異0。ログは r3m-mutate-<名前>-final.log。
- 07:22 速さの測定（tools/bench.sh を3回）: 区分（検査から）中央値 6.9〜7.4ms、36協定 0.94〜0.98ms、取込 117〜122ms、起動 1.6〜1.9ms。どれも目標の1割未満（r3m-bench.txt）。品質の基準を最終の形で流し直した（r3m-gates.txt。結果は 06:57 と同じ）。
- 07:24 報告書 r3m-report.md を書いた。natural-japanese の lint は $WS の外のスクリプトを動かすことになるので流さず、文体の要点（結論から、見出しは結論、太字は最重要の1か所、長い文を分ける）を手で当てた。
- 07:26 仕上げ: jp-labor の bench.rs の使い方の説明に --startup-runs を足し、fmt・clippy・doc を流し直して0。報告書の数字（1回目の生き残り10件のうち1件は等価、36協定の合成データの境の件数）を確かめて直した。作業は完了。
