# R2b 進捗（打刻の対・CSV取込・休憩の不足・月次の集計・explain）

- 03:20 指示書・仕様（punch / report / nfr / validation / classify / core / cli）・R1 の報告書と公開API・R2a と R3 の指示書（境目の確認のため）を読んだ。オラクルの指示書は読んでいない。
- 03:25 手を入れる前の試験は全件合格。設計を決めた: punch は秒つきの時点・従業員ID・異常を punch.rs に置く。秒の丸めで隣の勤務と1分重なるときは前の勤務の終わりを切る（R1 の ShiftSet が受け付ける形を保つ）。
- 03:30 取込は自前の厳密なCSVの読み方にする（csv 部品は空行を黙って飛ばし、壊れた引用符を誤りにしないため、行番号が正確に出せない）。kintai-import の Cargo.toml から csv を外す予定（報告書に理由を書く）。
- 03:32 調整役から仕様更新の連絡（03:26）。punch・report・nfr・cli・core・classify は未変更（更新時刻で確認）。validation.md の変更は JSON の読み方だけで、CSV の取込には関係しない。
- 03:40 kintai-core::punch（punch.rs）と試験 ac01_punch.rs（23件）・ac01_punch_properties.rs（性質4件）を作り、全件合格。
- 03:44 kintai-import の本体（lib.rs。自前のCSVの読み方、UTF-8/BOM/Shift_JIS、上限、行番号と列名だけのエラー）を書き、Cargo.toml から csv を外した。ここで利用の上限で停止。
- 05:22 再開（調整役の連絡: cli.md に打刻の異常を refused（PunchAnomaly・終了コード4）と同じ扱いにする記述が加わったが、R3 の担当）。取込の試験から続ける。
- 05:45 取込の試験を作った: ac01_import.rs（13件。BOM・CRLF・Shift_JIS・見出し・引用符・値の誤りの行と列・AC-08.7・AC-10.7）、ac10_limits.rs（5件。256MiB・4096バイト・10万人・日付の範囲）、ac10_properties.rs（性質5件）。合成CSVは tests/fixtures/synthetic/。read_limited を上限＋1バイトの確かめ方に直した（512MiB まで膨らまない）。
- 05:50 examples/bench.rs: 100万行の取込 114〜140ms（目標 3000ms）、打刻の対 約65ms（Apple M4 Pro、rustc 1.99、release）。
- 06:05 kintai-jp-labor の breaks.rs（休憩の不足）・report.rs（台帳の項目・丸め・注意・MonthlyReport・保存期限）・explain.rs（区間ごとの区分・割増の下限・根拠）を書いた。R1 の根拠の表にない条文（34条1項、33条1項、38条1項、108条、109条、附則143条1項、労基則20条1項・2項、54条1項・2項、56条、24条の7、附則71条、安衛則52条の7の3第2項）は各ファイルに定数で置き、試験で手元の条文と照合する予定。clippy は通過。
- 06:30 jp-labor の試験を作った: ac01_breaks.rs（10件）、ac08_report.rs（15件。公的な60時間超の例、民法の期間の端、根拠14件の照合）、ac08_properties.rs（2件）、explain_day.rs（12件。厚労省の例1・例2・60時間超の例を正解に）、explain_properties.rs（1件）。書式と clippy（全体）は通過。
- 05:57 品質の基準を流した（fmt・clippy・test 224件・doc・deny の bans/licenses/sources・spec-trace・Rust 1.88 はすべて0。deny の advisories だけは、勧告の一覧に git の情報がなく動かない → 版と勧告のファイルを手で突き合わせ、該当なし）。/tmp に一時ファイルを2つ書いてしまい、すぐ消した（報告書に記載）。
- 05:57 変異試験（r2b-mutations.json、39件）の1回目: 検出37、生き残り2（従業員IDの途中の「=」、256MiB ちょうど）。試験を足し（途中の記号の例、大きさの検査の切り出しと単体試験）、流し直し中。
- 06:08 変異試験の流し直し: 39件すべて検出、生き残り0（r2b-mutate.log）。品質の基準は最終の形で r2b-gates.txt に記録（試験 226 件合格）。報告書 r2b-report.md を書き上げた。
