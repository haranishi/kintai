#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# 手元で、PRの前に通す検査を、まとめて実行する。
#   tools/check-all.sh          全部
#   tools/check-all.sh --quick  依存の検査と最小のRustの確認を省く
# 失敗した検査の名前を最後に一覧する。
set -u
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
quick=0
[ "${1:-}" = "--quick" ] && quick=1
failed=()

step() {
  local name="$1"; shift
  printf '\n== %s\n' "$name"
  if "$@"; then echo "   -> 通過"; else echo "   -> 失敗"; failed+=("$name"); fi
}

step "Rust: 書式" cargo fmt --all -- --check
step "Rust: clippy" cargo clippy --workspace --all-targets --all-features -- -D warnings
step "Rust: 試験" cargo test --workspace
step "Rust: 文書" env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
if [ "$quick" = 0 ]; then
  if rustup toolchain list | grep -q '^1.88'; then
    step "Rust: 最小の1.88" cargo +1.88 check --workspace
  else
    echo "(最小の1.88が未導入: rustup toolchain install 1.88 --profile minimal)"
  fi
  if command -v cargo-deny >/dev/null; then step "依存: cargo deny" cargo deny check
  else echo "(cargo-denyが未導入: cargo install cargo-deny)"; fi
fi
step "文書: リンクと行数" python3 tools/check-links.py
[ -f tools/check-spec-trace.py ] && step "仕様: 条件と試験の対応" python3 tools/check-spec-trace.py
if [ -f tools/difftest.py ]; then
  step "実行ファイルの組み立て(差分試験用)" cargo build -q -p kintai-cli
  target_dir="$(cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
  step "別実装との差分試験(少数)" python3 tools/difftest.py --cases 300 --seed 1 --bin "$target_dir/debug/kintai"
fi
if command -v uvx >/dev/null; then
  step "ライセンス表示(REUSE)" uvx --from "reuse[charset-normalizer]" reuse lint
fi

printf '\n== まとめ\n'
if [ "${#failed[@]}" = 0 ]; then echo "すべて通過しました。"; exit 0; fi
printf '失敗: %s\n' "${failed[@]}"; exit 1
