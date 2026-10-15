# 点検の道具

標準ライブラリだけで動くPythonと、シェルです。PRの前は `tools/check-all.sh` を流してください。

| 道具 | 何をするか | 使い方 |
|---|---|---|
| `check-all.sh` | 書式、clippy、全試験、文書、依存、仕様の対応、少数の差分試験、ライセンス表示を、まとめて流す | `tools/check-all.sh`（`--quick` で重い所を省く） |
| `check-links.py` | 文書の相対リンク切れと、1ファイル100行の上限、ファイル名の禁止文字（`#` `\|` `[` `]` `^`）を検査する | `python3 tools/check-links.py` |
| `check-spec-trace.py` | 受け入れ条件（`AC-nn.n`）ごとに、試験からの参照があるかを一覧する | `python3 tools/check-spec-trace.py`（`--strict` で参照のない条件があれば失敗） |
| `difftest.py` | 無作為のケースを、別実装（`oracle/`）と本実装の両方に通して、終了コードと出力を比べる | `python3 tools/difftest.py --cases 2000 --seed 1 --bin target/release/kintai` |
| `oracle/` | 別実装。Rustの本実装を見ず、仕様だけから、1分ずつ数えて書いた | `python3 tools/oracle/selftest.py`（公的な数値例を再現するか） |
| `mutate.py` | ソースにわざと誤りを入れて、試験が赤くなるか（誤りを見つけるか）を確かめる | `python3 tools/mutate.py tools/mutations/<名前>.json` |
| `mutations/` | 変異の定義。壊れると結果が狂う境界の誤りを、部品ごとに並べた | 新しい守りを足したら、外す誤りも足す |

## 別実装を直すときの決まり

別実装は、本実装と独立に書かれていることに価値があります。直すときは、`docs/spec/` の仕様だけを見て直し、Rustのコードを見ながら直さないでください。食い違いが出たら、どちらが正しいかを仕様で決め、仕様が決めていなければ、先に仕様を直します（[開発の手引き](../docs/development.md)）。

## 差分試験の見方

`--stats` を付けると、生成したケースの分布と、境界を踏んだ件数が出ます。不一致があると、ケースを `tools/difftest-out/`（ない場合は `.cargo-target/difftest-out/`）に残し、先頭の数件を詳しく表示して、終了コード1で終わります。比べるのは、終了コード、終了コードが0か4のときの出力のJSON、2か3のときの `error.code` です（`message` は比べません）。
