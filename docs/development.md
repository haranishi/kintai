# 開発の手引き: 手元で動かして、直して、確かめる

コードを直したい人向けの案内です。動かすだけなら[はじめかた](getting-started.md)を見てください。

## 必要なもの

- Rust 1.88以上
- Python 3（別実装と、その差分試験だけ。標準ライブラリだけで動きます。本体の組み立てと、ふだんの点検には要りません）
- Node 20以上（画面を変える場合のPlaywright試験のみ。計算と利用には不要）
- あると良いもの: `cargo-deny`（依存の検査）、`uv`（ライセンス表示の検査 `reuse`）

## どこに何があるか

| 場所 | 役割 |
|---|---|
| `crates/kintai-core` | 日付、分、区間。外部の依存を持たず、入出力もしない |
| `crates/kintai-jp-labor` | 労働基準法にそった計算（区分、36協定、年休、祝日、月次、説明）。入出力を持たない |
| `crates/kintai-import` | 打刻・休暇・履歴のCSVを読む（文字コード、列の対応、入力の上限） |
| `crates/kintai-cli` | コマンド `kintai`。JSONとTOML、HTML月次画面、MCP stdio、終了コード |
| `tools/oracle/` | 別実装（Python）。仕様だけから、1分ずつ数えて書いた |
| `tools/` | 点検の道具（差分試験、変異試験、リンク、仕様と試験の対応） |
| `docs/spec/` | 仕様。受け入れ条件（`AC-nn.n`）が試験との対応 |
| `.agent-harness/` | 作業の記録（調査の根拠、査読の結果と対応、実行の証拠） |

計算の部品（core・jp-labor・import）は、ファイルも通信も持ちません。入出力は `kintai-cli` に閉じ込めてあります（[設計](architecture.md)）。

## 動かす

```sh
cargo run -p kintai-cli -- demo --out /tmp/kintai-demo
cargo run -p kintai-cli -- case tests/fixtures/cases/classify-basic.json
cargo run -p kintai-cli -- report --policy tests/fixtures/synthetic/policy.toml \
  --punches tests/fixtures/synthetic/punches.csv --month 2026-10
```

## 試験を通す

PRの前に、次を流してください。

```sh
cargo xtask check-all        # 書式、clippy、試験、文書、依存、仕様の対応、差分試験、ライセンス表示
```

`cargo xtask check-all --quick` は、依存の検査と最小のRustの確認を省きます。個別に流すなら、`cargo test -p kintai-jp-labor` のように対象を指定します。どの層が何を確かめているかは、[試験の地図](testing.md)にあります。

画面を変更したら[ブラウザ試験](review.md)を、要望を試験にするときは[要件データの手順](spec/decision-regressions.md)を使います。PRでは仕様対応、範囲制限、変異の検査も動きます。

## 仕様を先に直す

挙動を変えるときは、先に `docs/spec/` の該当する仕様と受け入れ条件を直し、そのあとで実装と試験を直してください。別実装（`tools/oracle/`）も、同じ仕様から直します。本体のコードを見ながら別実装を直さないでください。独立して書いてあることが、突き合わせの価値だからです。

法令の読みが分かれる所は、仕様に「読み」として書き、[確認シート](legal/professional-review.md)に行を足します。

## 試験が「門番」として働くかを確かめる

試験が通っていても、守りたい規則が試験に守られているとは限りません。`cargo xtask mutate` は、ソースにわざと誤りを入れて、試験が赤くなるかを確かめます。誤りの一覧は `tools/mutations/*.json` にあり、新しい守りを足したら、その守りを外す誤りも足してください。生き残った誤りは、試験の穴です。

## 変更を出すとき

[参加の手引き](../CONTRIBUTING.md)に従ってください。特に、次の3つを守ってください。

- 新しい機能、依存、公開するAPIの変更は、実装の前にIssueで相談する。
- 文書は1ファイル100行以内。`cargo xtask check-links` が、リンク切れと行数を検査します。
- 他の勤怠・人事・給与ソフトのコードや、特定の製品の画面・文言を写さない。GPLやAGPLのものは、見ることも避けてください（[権利の確認](legal/rights-check.md)）。
