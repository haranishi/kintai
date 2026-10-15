#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# 速さの測定（docs/spec/nfr.md の「速さ」。AC-10.1〜10.3 と起動）を、まとめて流す。
#   tools/bench.sh            各項目を慣らし1回のあと7回
#   tools/bench.sh 11         回数を変える（3以上）
# 機械・OS・Rust の版・release のプロファイルを先に出す。どれかが目標を超えると終了コード1。
# 測定の例は、パッケージごとに crates/*/examples/bench_*.rs にある。
set -u
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
runs="${1:-7}"

echo "== 測定の環境"
echo "日時: $(date '+%Y-%m-%d %H:%M %Z')"
echo "OS: $(uname -srm)"
if [ "$(uname -s)" = "Darwin" ]; then
  echo "機械: $(sysctl -n machdep.cpu.brand_string) / $(sysctl -n hw.ncpu) コア / $(( $(sysctl -n hw.memsize) / 1073741824 )) GiB"
elif [ -r /proc/cpuinfo ]; then
  echo "機械: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //') / $(nproc) コア"
fi
echo "Rust: $(rustc --version)"
echo "プロファイル: release（ワークスペースに [profile.release] がないので Cargo の既定。opt-level=3、lto=false、codegen-units=16、debug=false）"

failed=0
echo
echo "== 起動を測るための kintai の release ビルド"
cargo build --release -q -p kintai-cli || failed=1
echo
echo "== 区分の計算・36協定の年間判定・起動（AC-10.1・10.2）"
cargo run --release -q -p kintai-jp-labor --example bench_jp_labor -- --runs "$runs" || failed=1
echo
echo "== 打刻CSVの取込（AC-10.3）"
cargo run --release -q -p kintai-import --example bench_import -- --runs "$runs" || failed=1

echo
if [ "$failed" = 0 ]; then echo "すべて目標以内でした。"; else echo "目標を超えたか、測れなかった項目があります。"; fi
exit "$failed"
