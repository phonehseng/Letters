"""did word put each glyph where the plan said? compares a conversion's line plans with word's pdf.

usage:  .venv\\Scripts\\python.exe -m prototype.plan_check out\\prototype\\NAME
for every planned line: pixel misses (word's pen offset != planned target) and
the pdf writer's residue (distance of word's pen offset from a whole pixel).
"""

from __future__ import annotations

import json
import os
import sys
from collections import defaultdict

import numpy as np

from prototype.lines import group_lines
from prototype.pdf_content import read_document

PT_PER_PIXEL = 0.12


def main() -> None:
    folder = sys.argv[1]
    name = os.path.basename(folder.rstrip("\\/"))
    plans = json.load(open(os.path.join(folder, f"{name}.convert.json"), encoding="utf-8"))["line_plans"]
    pages = read_document(os.path.join(folder, f"{name}.word.pdf"))
    produced = group_lines([g for p in pages for g in p.glyphs])
    by_text = defaultdict(list)
    for line in produced:
        by_text[(line.page, "".join(g.text for g in line.glyphs).replace(" ", ""))].append(line)
    misses, residues, checked, worst = 0, [], 0, []
    for plan in plans:
        key = (plan["page"], plan["text"].replace(" ", ""))
        candidates = by_text.get(key)
        if not candidates:
            continue
        line = min(candidates, key=lambda candidate: abs(candidate.y - plan["y"]) + abs(candidate.x - plan["x"]))
        visible = [t for t, c in zip(plan["targets_px"], plan["text"], strict=True) if c != " "]
        glyphs = [g for g in line.glyphs if g.text != " "]
        if len(visible) != len(glyphs):
            continue
        offsets = np.array([(g.x - glyphs[0].x) / PT_PER_PIXEL for g in glyphs])
        wanted = np.array(visible) - visible[0]
        miss = np.abs(np.round(offsets) - wanted)
        misses += int((miss >= 1).sum())
        residues.extend(np.abs(offsets - np.round(offsets)))
        checked += len(glyphs)
        worst.append((float(miss.max()), plan["text"][:40], plan["y"]))
    residues = np.array(residues)
    print(f"{checked} glyphs checked, {misses} land on a different pixel than planned")
    print(f"writer residue: max {residues.max():.3f} px, mean {residues.mean():.3f} px")
    for miss, text, y in sorted(worst, reverse=True)[:6]:
        print(f"   worst miss {miss:.0f} px at y={y:.2f}: {text!r}")


if __name__ == "__main__":
    main()
