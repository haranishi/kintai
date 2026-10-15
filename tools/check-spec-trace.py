#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""受け入れ条件（AC-xx.y）ごとに、試験からの参照があるかを一覧にする。

docs/spec/*.md の「- AC-xx.y [種類]」の行を集め、crates/**/*.rs と tools/**/*.py に
「AC-xx.y」の文字列があるかを数える。種類が manual の条件は対象外。

    python3 tools/check-spec-trace.py           # 表を出す（終了コードは常に0）
    python3 tools/check-spec-trace.py --strict  # rust・python の条件に参照が無ければ終了コード1

標準ライブラリだけを使う。
"""

from __future__ import annotations

import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONDITION = re.compile(r"^\s*-\s+(AC-\d{2}\.\d+)\s+\[(\w+)\]")
REFERENCE = re.compile(r"AC-\d{2}\.\d+(?!\d)")
SOURCES = [("crates", "*.rs"), ("tools", "*.py")]
CHECKED_KINDS = {"rust", "python"}
SKIP_DIRS = {"node_modules", "target", ".git"}


def conditions() -> list[tuple[str, str]]:
    found = []
    for story in sorted((ROOT / "docs" / "spec").glob("*.md")):
        for line in story.read_text("utf-8").splitlines():
            match = CONDITION.match(line)
            if match:
                found.append((match.group(1), match.group(2)))
    return found


def references() -> dict[str, list[str]]:
    places: dict[str, list[str]] = defaultdict(list)
    for folder, pattern in SOURCES:
        base = ROOT / folder
        if not base.is_dir():
            continue
        for path in sorted(base.rglob(pattern)):
            if SKIP_DIRS.intersection(path.relative_to(ROOT).parts):
                continue
            text = path.read_text("utf-8", errors="replace")
            for number, line in enumerate(text.splitlines(), start=1):
                for ac in sorted(set(REFERENCE.findall(line))):
                    places[ac].append(f"{path.relative_to(ROOT)}:{number}")
    return places


def main(argv: list[str]) -> int:
    strict = "--strict" in argv
    found = conditions()
    places = references()
    rows = [(ac, kind, places.get(ac, [])) for ac, kind in found if kind != "manual"]
    width = max((len(ac) for ac, _, _ in rows), default=8)
    # 種類と参照の場所が合っているか（python の条件が tools/ から参照されているか）を見られるよう、内訳も出す。
    print(f"{'条件':<{width}}  種類  参照  内訳（crates/tools）  例")
    missing = []
    for ac, kind, where in rows:
        example = where[0] if where else "（なし）"
        split = "/".join(str(sum(1 for w in where if w.startswith(folder + "/"))) for folder, _ in SOURCES)
        print(f"{ac:<{width}}  {kind:<4}  {len(where):>4}  {split:<20}  {example}")
        if not where and kind in CHECKED_KINDS:
            missing.append(ac)
    known = {ac for ac, _ in found}
    unknown = sorted(set(places) - known)
    manual = sum(1 for _, kind in found if kind == "manual")
    print(f"\n条件 {len(found)}件（manual {manual}件は対象外）。参照の無い条件: {len(missing)}件")
    if missing:
        print("  " + " ".join(missing))
    if unknown:
        print("仕様に無い番号への参照: " + " ".join(unknown))
    return 1 if strict and missing else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
