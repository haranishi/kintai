# 調査Cのデータ

出典：内閣府「国民の祝日について」（https://www8.cao.go.jp/chosei/shukujitsu/gaiyou.html ）、公式CSV（https://www8.cao.go.jp/chosei/shukujitsu/syukujitsu.csv ）。2026-10-15取得。
利用条件：内閣府ホームページ利用規約（https://www.cao.go.jp/notice/rule.html ）が採用する公共データ利用規約第1.0版（PDL1.0）。規約対象コンテンツについてCC BY 4.0との互換性あり。詳細と出典範囲の注意はtrack-C.md Q7(a)参照。

本データは同CSVを調査CでUTF-8化し、日付をYYYY-MM-DDに変換したもの。原本そのものと変換版を区別し、政府が当変換版・生成版を作成したように表示しない。

- syukujitsu_raw.csv：取得した原バイト、Shift_JISとして厳密復号可能、1955〜2027年1,067件。
- syukujitsu_utf8.csv：UTF-8、BOMなし、LF、ヘッダー日付,名称、1,067件。名称は原CSVのまま。
- verify_holidays.py：調査用変換・検算器。researchで `python3 data/verify_holidays.py` を実行。ネットワークアクセス不要。日付はCSVを使わずに法の規則から生成する。
- holidays_generated_2020_2027.csv：独立生成した143件。公式CSVと日付・名称が全件一致。3条2項／3項の種別は検算器が生成した付加情報。
- equinox_verified.csv：暦要項p1から抽出した正式春分・秋分と近似式との16件照合。近似式を法的に確定した日付の正本として使わない。
- verification_result.json：全件照合の集計と差分。差分0。
- grant_table_comparison.csv：手元の労基法・施行規則と厚労省事業者Q&A Q1の付与日数35セル照合。

公式CSV HTTP Last-Modified：2026-02-02T00:30:17Z。ページ本文で示された更新日とは区別。
原CSV SHA-256：cec37a743c96995cdb9cb52b685c9003634682a9b0e1a640a6b9b96881fe964a。

2028〜2030年は収録外・暦要項未公表のため確定データなし。1955〜2019年の全件について現行規則からの再生成は行っていない。
