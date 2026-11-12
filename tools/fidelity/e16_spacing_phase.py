"""e16: does the frame's position inside its pixel change how w:spacing becomes pixels?

e15 put every frame at 1440 twips and found each glyph's spacing rounded on its
own. converted documents still showed about 1.7% of steps one pixel off that
rule, with frames at other twip values. here each line is 12 'n' glyphs with
random spacings, its frame at 1440 + r twips for r = 0..59 (every phase of the
twip grid against the pixel grid), every glyph in its own text object, and the
pens are decoded exactly. the raw data goes to out\\e16\\e16.json.

usage:  .venv\\Scripts\\python.exe e16_spacing_phase.py
"""

from __future__ import annotations

import json
import os
import random

import docx_writer as dw
import font_prep
import grid_model as gm
import test_fonts
import word_writer_model as wm
from e12_exported_plan import grid_font, run_xml
from word_export import export_docx_to_pdf
from word_pdf_raw import read_text_objects

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e16")
SIZE = 12.0
GLYPHS = 12


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    rng = random.Random(16)
    table = wm.step_table("x", 5100)
    family, path = grid_font(SIZE)
    font = dw.EmbeddedFont(family, open(path, "rb").read())
    advance = font_prep.advance_pixels(path, "n")[0]
    parts, plans = [], []
    y = 600
    for repeat in range(6):
        for phase in range(wm.TWIPS_PERIOD):
            if repeat < 3:
                kind = "random spacings"
                spacings = [rng.randint(-30, 30) for _ in range(GLYPHS - 1)] + [0]
            else:
                # random pixel steps, spacing chosen by the planner that keeps the exact sum near the pixels
                kind = "planned spacings"
                targets = [0]
                for _ in range(GLYPHS - 1):
                    targets.append(targets[-1] + advance + rng.randint(-12, 12))
                spacings = gm.spacing_for_pixel_steps([advance] * GLYPHS, targets)
            x_twips = 1440 + phase + 1200 * (repeat % 3)
            runs = "".join(run_xml("n", family, SIZE, s, i) for i, s in enumerate(spacings))
            if y + 280 > 15840 - 900:
                parts.append(dw.page_break_paragraph_xml())
                y = 600
            parts.append(dw.frame_paragraph_xml(runs, x_twips=x_twips, y_twips=y, width_twips=6000, line_twips=280))
            plans.append({"x_twips": x_twips, "spacings": spacings, "advance": advance, "kind": kind})
            y += 280
    docx = dw.write_docx(os.path.join(OUT_DIR, "e16.docx"), "".join(parts), family=family, default_size_pt=SIZE,
                         embedded=font)  # fmt: skip
    pdf = os.path.join(OUT_DIR, "e16.word.pdf")
    export_docx_to_pdf(docx, pdf)
    lines: dict[tuple, list] = {}
    for obj in read_text_objects(pdf):
        if obj.glyphs and obj.glyphs[0].code == ord("n"):
            lines.setdefault((obj.page, obj.tm_text[5]), []).append(wm.decode_x(obj.x, table))
    keys = sorted(lines, key=lambda k: (k[0], -float(k[1])))
    counts: dict[str, list[int]] = {}
    for plan, key in zip(plans, keys, strict=True):
        pens = sorted(lines[key])
        plan["start_pixel"] = pens[0]
        plan["offsets"] = [p - pens[0] for p in pens]
        rule = gm.pen_pixels([advance] * GLYPHS, plan["spacings"])
        count = counts.setdefault(plan["kind"], [0, 0, 0])
        count[0] += 1
        for i in range(1, GLYPHS):
            count[1] += 1
            count[2] += (plan["offsets"][i] - plan["offsets"][i - 1]) != (rule[i] - rule[i - 1])
    with open(os.path.join(OUT_DIR, "e16.json"), "w", encoding="utf-8") as handle:
        json.dump(plans, handle)
    for kind, (lines_count, steps, misses) in counts.items():
        print(f"{kind}: {lines_count} lines, {steps} steps, {misses} off the per-glyph rule")


if __name__ == "__main__":
    main()
