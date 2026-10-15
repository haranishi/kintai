# はじめかた: 15分でデモの出力を読む

kintai（勤怠）を手元で動かし、合成データの出力を読むまでの手順です。コードを直したい人は[開発の手引き](development.md)を見てください。実データは使わず、デモの合成データだけで試せます。

## 用意するもの

- Rust 1.88以上（[rustup](https://rustup.rs/)）
- Windows、macOS、Linuxのどれか。手元で動かして確かめたのはmacOSだけです。CIの設定にはLinuxも入れてあります（まだ流していません）。LinuxとWindowsで試した結果は、Issueで教えてください

## 1. 組み立てる

```sh
git clone https://github.com/haranishi/kintai
cd kintai
cargo build --release
```

実行ファイルは `target/release/kintai` にできます。

## 2. デモを動かす

```sh
./target/release/kintai demo --out /tmp/kintai-demo
```

`/tmp/kintai-demo` に、合成の方針ファイル（`policy.toml`）、打刻（`punches.csv`）、年休の取得（`leaves.csv`）、36協定の履歴（`history.csv`）ができ、そのあとで `report` `check` `leave` `explain` `retention` の出力が並びます。デモは、そのフォルダにだけ書きます。

## 3. 出力を読む

自分で1つずつ動かすなら、次のようにします。

```sh
cd /tmp/kintai-demo
kintai report --policy policy.toml --punches punches.csv --month 2026-10
kintai check --policy policy.toml --punches punches.csv --month 2026-10 --history history.csv
kintai leave --policy policy.toml --leaves leaves.csv --as-of 2026-10-31
kintai explain --policy policy.toml --punches punches.csv --employee E001 --date 2026-10-08
```

- `report`: 従業員と月の期間ごとに、労働日数、延長時間数（法定外の時間外）、休日労働時間数、深夜時間数、60時間超。単位は分です。`--format json` と `--format csv` も選べます。
- `check`: 36協定の上限（月45時間・年360時間、月100時間未満、2〜6か月平均80時間）、休憩の不足、打刻の異常。`--fail-on-violation` を付けると、違反や警告があるとき終了コード1で終わります。
- `leave`: 年休のロット（付与日、日数、失効日）、残数、年5日の取得義務、管理簿。
- `explain`: その日を区間に分け、区分、割増の下限（%）、根拠の条文を示します。

## 4. 結果の読み方で注意すること

- 出力には `review_status: "unreviewed"` と、表を確認した日（`table_checked_on`）が付きます。専門家の確認前の**参考値**です（[免責](../DISCLAIMER.md)）。
- `notices` には、考慮していないこと（休日の振替、他社の時間の通算、災害等の臨時の労働）が並びます。
- 終了コードは、0が成功、1が `check --fail-on-violation` の違反か警告、2が不正な入力、3が対応外の働き方、4が曖昧さか打刻の異常による拒否です。3と4は、数字を出さずに止まった印です。理由は[よくある質問](faq.md)にあります。

## 5. 自分のデータで試すとき

- 打刻CSVの列は `employee_id,timestamp,kind`（`kind` は `in` `out` `break_start` `break_end`）。UTF-8かShift_JIS、ヘッダ行が要ります。従業員IDは、先頭が英数字で、英数字と `._-` の64文字までです。
- 打刻CSVは、対象の月の最初の週の起算日（既定は日曜）から含めてください。含まれていないと、週の上限を超えた分（時間外）を少なく数えるおそれがあるので、その週は拒否されます（終了コード4）。その日からそろっていると分かっているときは、`--data-from` で伝えられます。
- 方針ファイルの書き方は[コマンドの仕様](spec/cli.md)の例のとおりです。法定休日の曜日と月の起算日は、必ず書きます。
- 勤怠は個人情報です。kintaiは外へ通信しませんが、ファイルの置き場所とアクセス権は、使う人が管理してください。
- 変形労働時間制やフレックスなど、対応していない働き方が含まれると、計算せずに止まります（[対応外の一覧](spec/unsupported.md)）。
