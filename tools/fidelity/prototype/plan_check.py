"""did word put each glyph on the layout pixel the plan asked for? compares a conversion's line plans with word's pdf.

usage:  .venv\\Scripts\\python.exe -m prototype.plan_check out\\prototype\\NAME [--list N]

every glyph of the converter's docx starts its own text object, so its printed
Tm decodes to the exact layout pixel (word_writer_model). for every planned
horizontal line this compares
    planned   the targets the plan asked for
    rule      the pens grid_model.pen_pixels predicts from the advances and spacings
    word      the decoded layout pixels
a glyph where rule and word differ is a glyph whose layout advance is not the
grid font's advance: the per-glyph pairs are listed so the cause can be found.
"""

from __future__ import annotations

import json
import os
import sys
from collections import Counter

import pikepdf

import grid_model as gm
import word_writer_model as wm
from word_pdf_raw import read_text_objects


def check(folder: str) -> dict:
    name = os.path.basename(folder.rstrip("\\/"))
    plans = json.load(open(os.path.join(folder, f"{name}.convert.json"), encoding="utf-8"))["line_plans"]
    pdf_path = os.path.join(folder, f"{name}.word.pdf")
    with pikepdf.open(pdf_path) as pdf:
        widths = [wm.page_pixels(float(p.mediabox[2]) - float(p.mediabox[0])) for p in pdf.pages]
    from prototype.calibration import ModelCache
    from prototype.convert import word_build

    cache = ModelCache(word_build())
    by_page: dict[int, list] = {}
    for obj in read_text_objects(pdf_path):
        if obj.glyphs and abs(obj.tm[1]) < 1e-6:
            by_page.setdefault(obj.page, []).append(obj)
    result = {"lines": 0, "glyphs": 0, "planned_misses": 0, "rule_misses": 0, "pairs": Counter(), "unmatched_lines": 0}
    for plan in plans:
        if plan.get("rotated"):
            continue
        table = cache.table("x", widths[plan["page"]])
        # the line's text objects: those on its baseline, from its start pixel on
        start_pt = wm.exported_x(plan["start_pixel"], table)
        row = sorted((o for o in by_page.get(plan["page"], []) if abs(o.y - plan["word_y"]) < 0.01
                      and o.x >= start_pt - 0.01), key=lambda o: o.x)  # fmt: skip
        text = plan["text"]
        if len(row) < len(text):
            result["unmatched_lines"] += 1
            continue
        row = row[: len(text)]
        if any(len(o.glyphs) != 1 for o in row) or "".join(chr(o.glyphs[0].code) if o.glyphs[0].code < 128 else "?"
                                                           for o in row) == "":  # fmt: skip
            result["unmatched_lines"] += 1
            continue
        word = [wm.decode_x(o.x, table) - plan["start_pixel"] for o in row]
        rule = gm.pen_pixels(plan["advances_px"], plan["spacing_twips"])
        result["lines"] += 1
        for i in range(len(text)):
            result["glyphs"] += 1
            result["planned_misses"] += word[i] != plan["targets_px"][i]
            if i and (word[i] - word[i - 1]) != (rule[i] - rule[i - 1]):
                result["rule_misses"] += 1
                result["pairs"][(text[i - 1], text[i], word[i] - word[i - 1] - (rule[i] - rule[i - 1]))] += 1
    return result


def main() -> None:
    result = check(sys.argv[1])
    print(f"{result['lines']} lines ({result['unmatched_lines']} not matched), {result['glyphs']} glyphs: "
          f"{result['planned_misses']} off the planned pixel, {result['rule_misses']} steps off the spacing rule")  # fmt: skip
    count = int(sys.argv[sys.argv.index("--list") + 1]) if "--list" in sys.argv else 12
    for (first, second, delta), n in result["pairs"].most_common(count):
        print(f"   {first!r} -> {second!r}: {delta:+d} px, {n} times")


if __name__ == "__main__":
    main()
