# MCPから月次の結果を読む

`kintai mcp` は、起動時に指定した従業員と月の集計を読み取るstdioサーバーです。
ファイルの書き換え、打刻、承認、給与計算は行いません。
Rustの既存の計算を使い、新しい本番用の依存は追加していません。

## 起動する

```sh
cargo build -p kintai-cli --release --locked
./target/release/kintai mcp \
  --policy tests/fixtures/synthetic/policy.toml \
  --punches tests/fixtures/synthetic/punches.csv \
  --month 2026-10 --employee E001
```

標準入力を待つので、人が端末で結果を読むコマンドではありません。
MCPクライアントの起動設定に、この実行ファイルと引数を渡します。
クライアントによって設定の保存先が異なります。設定例のパスは置き換えてください。

```json
{
  "mcpServers": {
    "kintai": {
      "command": "/absolute/path/to/kintai",
      "args": ["mcp", "--policy", "/absolute/path/to/policy.toml",
        "--punches", "/absolute/path/to/punches.csv", "--month", "2026-10",
        "--employee", "E001"]
    }
  }
}
```

相対パスを使う場合は、クライアントが起動する作業フォルダにも注意してください。
`--data-from` はCLIのreportと同じ意味です（[はじめかた](getting-started.md)）。

## 呼べるツール

| ツール | 引数 | 結果 |
|---|---|---|
| `kintai_employees` | 空のオブジェクト | 対象のID、集計不可・要確認の状態、表の確認日 |
| `kintai_monthly_report` | `employee_id` は任意 | `report` に元のJSON、`incomplete` に集計不可の有無 |

両方とも `content` のJSONテキストと、同じ内容の `structuredContent` を返します。
ツールは、起動時に指定した月とファイルだけを扱います。
`--employee E001` で起動したサーバーは、E005のデータを返しません。
引数の誤りは `isError: true`、集計不可は理由つきの正常な結果です。
集計不可を0時間や承認済みの給与として扱わないでください。

起動後にCSVを変更しても、現在のサーバーの控えは変わりません。更新は再起動します。
標準出力にはJSON-RPCだけを出し、EOFで終了します。1要求は改行を含め64KiBまでです。
対応プロトコルと境界の試験は[仕様](spec/review-mcp.md)にあります。

## 他のOSSとの接続

この版は勤怠の時間数を返します。kamokuの会計仕訳とは別のデータです。
給与の金額と `journal.v1` への変換は今後の段階です（[全体の構想](suite.md)）。
接続先のAIクライアントは、結果をモデルの提供元に送る場合があります。
手元での計算と、接続先でのデータの扱いは別です。必要な範囲だけを指定してください。
