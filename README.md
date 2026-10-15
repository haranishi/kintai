# kintai（勤怠）

[![ci](https://github.com/haranishi/kintai/actions/workflows/ci.yml/badge.svg)](https://github.com/haranishi/kintai/actions/workflows/ci.yml)

打刻のCSVから、日本の労働基準法にそって「法定内・時間外・法定休日・深夜・月60時間超」を分け、36協定の上限と年次有給休暇まで計算する、Rust製のOSSです。計算は手元で完結し、外へ通信しません。

> **参考値です。** 条文と通達の読みに基づく計算で、社会保険労務士の確認は受けていません。出力には毎回 `review_status: "unreviewed"` が付きます。労働基準法を満たしていることの証明ではありません。→ [免責](DISCLAIMER.md)、[確認してほしい点](docs/legal/professional-review.md)

状態: Phase 0（計算の部品とコマンドライン）。画面とサーバーはまだありません（[ロードマップ](docs/roadmap.md)）。

## できること

- **時間の区分**: 日8時間・週40時間（特例44時間）、法定休日、深夜（22時〜5時）、月60時間超を、分単位で出す。
- **月次の集計**: 賃金台帳に書く勤怠の項目（労働日数、延長時間数、休日労働時間数、深夜時間数）。
- **36協定の上限**: 月45時間・年360時間、特別条項、月100時間未満、2〜6か月平均80時間。
- **年次有給休暇**: 付与、2年で失効、残数、年5日の取得義務、管理簿。
- **保存期限の目安**、**国民の祝日**（2020〜2027年は内閣府の表、以後は推計）。
- **速さ**: 1,000人×1か月の区分は約7ミリ秒、100万行の打刻CSVの取込は約0.12秒（Apple M4 Pro、合成データ、ディスクの読み込みを除く。[測定](docs/benchmarks.md)）。

## 試す

```sh
cargo build --release
./target/release/kintai demo --out /tmp/kintai-demo   # 合成データで、全コマンドの出力を一通り出す
./target/release/kintai --help
```

使い方は[はじめかた](docs/getting-started.md)、機能の一覧は[機能](docs/features.md)。

## 守っていること

- **対応外の働き方は、計算せず拒否する。** 変形労働時間制、フレックス、裁量労働、管理監督者など25種類は、通常制として計算すると違反が隠れるため、終了コード3で止める。
- **曖昧なときは、確定集計を拒否する。** 勤務が法定休日・週・月の境界をまたぐ、週の途中から打刻がそろっていない、といった場合です。公的な資料が帰属を直接定めていない所は、黙って推測しない。
- **根拠を出力に付ける。** どの条文で、割増の下限が何%か、表を確認した日はいつか。
- **別実装で突き合わせる。** 本体とは別に、1分ずつ数えるPythonの実装を仕様だけから書き、無作為の入力で答えを比べる（[試験](docs/testing.md)）。

## しないこと

賃金の金額、社会保険、36協定の締結・届出、健康確保措置は扱いません。副業・兼業先の時間の通算は、呼び出し側が渡した値を判定するだけです。理由は[よくある質問](docs/faq.md)にあります。

## つながり

会計OSS [kamoku](https://github.com/haranishi/kamoku) へ、給与の仕訳を渡す構想があります。部品どうしはデータベースを共有せず、版つきのファイルで受け渡します（[全体の構想](docs/suite.md)）。

## 出典・ライセンス・貢献

- コードは MIT OR Apache-2.0、文書は CC BY 4.0、法令の写しは CC0。[ライセンスの方針](docs/legal/license-policy.md)
- 数値例は厚生労働省などの公開資料から引用・加工した（[公共データ利用規約（第1.0版）](https://www.digital.go.jp/resources/open_data/public_data_license_v1.0)に準拠。出典は各試験データの `source`）。この結果は国が作成したものではありません。
- 祝日は、内閣府「国民の祝日について」の[CSV](https://www8.cao.go.jp/chosei/shukujitsu/gaiyou.html)（公共データ利用規約。UTF-8に変換して利用。2026-10-15取得）。春分・秋分の日付は、国立天文台（暦要項と予想表。[利用規程](https://www.nao.ac.jp/terms/copyright.html)。クレジット: 国立天文台）。
- 貢献は[CONTRIBUTING.md](CONTRIBUTING.md)。最初の一歩は[着手しやすい課題](docs/good-first-issues.md)。権利の確認は[記録](docs/legal/rights-check.md)。
- 脆弱性は[SECURITY.md](SECURITY.md)へ。

*English: kintai calculates Japanese labor-law overtime categories, "36 agreement" caps, and paid-leave obligations from punch CSVs. Pre-release, unreviewed by professionals; for reference only.*
