#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""文書の相対リンク切れと、100行の上限を検査する。標準ライブラリだけで動く。

使い方:  python3 tools/check-links.py
- `[文字](相対パス)` と `[文字](相対パス#見出し)` の、相対パスのファイルの存在を確かめる。
- 開発用の文書(README.md、CONTRIBUTING.md、docs/ の下)は、1ファイル100行以内であることを確かめる。
  実行記録(.agent-harness/)は、行数の検査から外す。
- ファイル名に `#` `|` `[` `]` `^` を含むものを見つけたら失敗する(Obsidianなどでリンクが壊れる)。
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LINK = re.compile(r"\]\(([^)\s]+)\)")
SKIP_LINES: set[str] = set()
BAD_NAME = set("#|[]^")


SKIP_DIRS = {".git", ".cargo-home", ".cargo-target", "target", "node_modules"}


def tracked_or_new() -> list[Path]:
    """git があれば追跡中と新規のファイル。gitのリポジトリでなければ(初期化前)、ツリーを歩く。"""
    run = subprocess.run(
        ["git", "ls-files", "-o", "-c", "--exclude-standard"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if run.returncode == 0:
        return [ROOT / p for p in run.stdout.splitlines() if (ROOT / p).is_file()]
    return [
        p for p in sorted(ROOT.rglob("*"))
        if p.is_file() and not (set(p.relative_to(ROOT).parts) & SKIP_DIRS) and not is_private_note(p)
    ]


def is_git_ignored(path: Path) -> bool:
    """git の管理下で、.gitignore に除外されているファイルか。手元にあってもリポジトリには入らないので、リンク先にできない。"""
    run = subprocess.run(["git", "check-ignore", "-q", str(path)], cwd=ROOT, capture_output=True)
    return run.returncode == 0


def is_private_note(path: Path) -> bool:
    """公開しない作業メモ(.gitignore の `research/out-*.md` と `research/notes_*.md`)。git が無い間も、検査から外す。"""
    return path.parent.name == "research" and (path.name.startswith("out-") or path.name.startswith("notes_"))


def main() -> int:
    problems: list[str] = []
    files = tracked_or_new()
    for path in files:
        rel = path.relative_to(ROOT).as_posix()
        if any(ch in path.name for ch in BAD_NAME):
            problems.append(f"{rel}: ファイル名に使えない文字があります")
    for path in files:
        rel = path.relative_to(ROOT).as_posix()
        if path.suffix != ".md":
            continue
        text = path.read_text(encoding="utf-8")
        in_fence = False
        for no, line in enumerate(text.splitlines(), 1):
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
            if in_fence:
                continue
            for target in LINK.findall(line):
                if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("#"):
                    continue
                dest = (path.parent / target.split("#")[0]).resolve()
                if not dest.exists():
                    problems.append(f"{rel}:{no}: リンク切れ {target}")
                elif is_git_ignored(dest):
                    problems.append(f"{rel}:{no}: 公開されないファイルへのリンク {target}")
        is_dev_doc = (
            rel in {"README.md", "CONTRIBUTING.md"} or rel.startswith("docs/")
        ) and not rel.startswith("docs/images/")
        if is_dev_doc and rel not in SKIP_LINES:
            lines = len(text.splitlines())
            if lines > 100:
                problems.append(f"{rel}: {lines}行あります(上限100行)")
    for p in problems:
        print(p)
    print(f"検査したMarkdown: {sum(1 for f in files if f.suffix == '.md')}件、問題: {len(problems)}件")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
