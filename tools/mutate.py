#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""変異試験の道具: わざと誤りを入れて、試験が赤くなるかを確かめる。

使い方:  python3 tools/mutate.py tools/mutations/<名前>.json [--only 文字列] [--keep]

仕様のJSON:
  {
    "test_cmd": ["cargo", "test", "-q", "-p", "kintai-jp-labor", "--no-fail-fast"],
    "timeout_sec": 900,
    "mutations": [
      {"label": "按分: 四捨五入を切捨てに", "file": "crates/kintai-jp-labor/src/compose.rs",
       "find": "+ 50) / 100", "replace": "/ 100"}
    ]
  }

リポジトリ全体をコピーした作業用の場所で試すので、本物のファイルは変わらない。
ファイルを戻すときは更新時刻を引き継がない(shutil.copy)。引き継ぐと、Cargoが「変更なし」と
判断して前の変異を含むビルドを使い回し、結果が嘘になる。生き残り(試験が赤くならない変異)が
あれば終了コード1。基準(変異なし)が赤い場合は、変異の結果を信用できないので終了コード2。
コンパイルエラーは「検出」に数えず、直すべき変異として一覧に出す。
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# 作業用の場所へ写さないもの。ほかの担当が同時に書き換える生成物や、Rustの試験に要らない
# 画面と通し試験の場所を除く。画面のファイルを変異させる仕様は、"exclude" で上書きする。
EXCLUDES = (
    "target", ".git", "node_modules", ".cargo-home", ".cargo-target", "screens-out",
    "test-results", "playwright-report", "e2e", "ui",
)


def copy_tree(dst: Path, excludes=EXCLUDES) -> None:
    dst.mkdir(parents=True, exist_ok=True)
    args = ["rsync", "-a", "--delete"]
    for name in excludes:
        args += ["--exclude", name]
    subprocess.run(args + [f"{ROOT}/", f"{dst}/"], check=True)


def run_tests(work: Path, cmd: list[str], timeout: int, env: dict) -> tuple[int, str, str]:
    """(終了コード, 種類, 赤くなった試験の名前)。種類は ok / test / compile / timeout。"""
    try:
        p = subprocess.run(cmd, cwd=work, env=env, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return 124, "timeout", "時間切れ"
    out = p.stdout + p.stderr
    if p.returncode == 0:
        return 0, "ok", ""
    # 試験が赤いときも cargo は「error: test failed」と出すので、コンパイルの失敗は
    # 「could not compile」だけで見分ける。
    if "could not compile" in out:
        return p.returncode, "compile", ""
    names: list[str] = []
    for line in out.splitlines():
        m = re.match(r"^test (\S+) \.\.\. FAILED$", line.strip())
        if m:
            names.append(m.group(1))
    in_list = False
    for line in out.splitlines():
        if line.strip() == "failures:":
            in_list = True
            continue
        if in_list:
            m = re.match(r"^    ([A-Za-z_][A-Za-z0-9_:]*)$", line)
            if m:  # 試験の名前だけ(proptestが出す入力の表示は除く)
                names.append(m.group(1))
            elif not line.strip() and names:
                in_list = False
    uniq = list(dict.fromkeys(names))
    return p.returncode, "test", ", ".join(uniq[:4])


def run(base: Path) -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    spec = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    keep = "--keep" in sys.argv
    work = base / "repo"
    env = dict(os.environ, CARGO_TARGET_DIR=str(base / "target"))
    env["PATH"] = f"{Path.home()}/.cargo/bin:" + env.get("PATH", "")
    copy_tree(work, tuple(spec.get("exclude", EXCLUDES)))
    timeout = int(spec.get("timeout_sec", 900))
    cmd = spec["test_cmd"]
    code, _, _ = run_tests(work, cmd, timeout, env)
    print(f"[基準] 変更なし: 終了コード={code}（0であること）")
    if code != 0:
        print("基準が赤いので、変異の結果は信用できません。")
        return 2
    survivors = []
    for m in spec["mutations"]:
        if only and only not in m["label"]:
            continue
        target = work / m["file"]
        original = ROOT / m["file"]
        text = original.read_text(encoding="utf-8")
        n = text.count(m["find"])
        want = int(m.get("count", 1))
        if n != want:
            print(f"[{m['label']}] 適用できない: 見つかった数={n}, 期待={want}")
            survivors.append(m["label"] + "（適用できない）")
            continue
        target.write_text(text.replace(m["find"], m["replace"]), encoding="utf-8")
        time.sleep(1.1)  # 更新時刻の差を確実にする
        code, kind, who = run_tests(work, cmd, timeout, env)
        shutil.copy(original, target)  # 更新時刻を引き継がない
        time.sleep(1.1)
        if kind == "ok":
            print(f"[{m['label']}] 生き残り ← 試験が赤くならなかった")
            survivors.append(m["label"])
        elif kind == "compile":
            print(f"[{m['label']}] 変異がコンパイルできない ← 検出に数えない。変異の書き方を直すこと")
            survivors.append(m["label"] + "（コンパイルできない）")
        else:
            print(f"[{m['label']}] 検出 ← 赤になった試験: {who or '(名前を取れなかった)'}")
    print("\n== 生き残り ==")
    print("\n".join(survivors) if survivors else "なし")
    return 1 if survivors else 0



def main() -> int:
    base = Path(tempfile.mkdtemp(prefix="kintai-mut-"))
    keep = "--keep" in sys.argv
    try:
        return run(base)
    finally:
        # 途中で失敗しても、1GBを超える作業場所を残さない(--keep のときだけ残す)。
        if not keep:
            shutil.rmtree(base, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
