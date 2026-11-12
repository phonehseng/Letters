"""e15: how word turns w:spacing into whole pixels, negative values included.

round 2 fitted pen = floor(advances + cumulative twips * 5/12) on targets that
mostly sat above the plain advances. the journal's author line showed a glyph
one pixel right of that rule where the cumulative spacing was negative, so this
probe sweeps it: every line holds a few 'n' glyphs of a grid font (whole-pixel
advance a), each in its own text object (alternating w:lang w:eastAsia), so
word_writer_model decodes every pen to its exact layout pixel.
    single   "nn" with w:spacing s after the first glyph, s = -60..60 twips
    pairs    "nnn" with spacings (s1, s2), to see whether word converts the
             running sum or each value on its own
result (word 16.0.20527): each glyph's spacing becomes whole pixels on its own,
rounded half away from zero (grid_model.spacing_pixels); fractions are never
carried to the next glyph.

usage:  .venv\\Scripts\\python.exe e15_spacing_rule.py [--reuse]
"""

from __future__ import annotations

import json
import math
import os
import random
import sys

import docx_writer as dw
import font_prep
import grid_model as gm
import test_fonts
import word_writer_model as wm
from e12_exported_plan import grid_font, run_xml
from word_export import export_docx_to_pdf
from word_pdf_raw import read_text_objects

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e15")
SIZES = (12.0, 10.5)


def rules(spacings: tuple[int, ...]) -> dict[str, int]:
    """extra whole pixels after len(spacings) glyphs under each candidate rule."""
    cumulative = sum(spacings) * 5 / 12
    return {
        "floor of the sum": math.floor(cumulative + 1e-9),
        "round of the sum": math.floor(cumulative + 0.5),
        "each rounded half up": sum(math.floor(s * 5 / 12 + 0.5) for s in spacings),
        "each rounded half away from zero": sum(gm.spacing_pixels(s) for s in spacings),
    }


def build() -> list[dict]:
    rng = random.Random(15)
    table = wm.step_table("x", 5100)
    parts, plans, fonts = [], [], []
    y = 600
    for size in SIZES:
        family, path = grid_font(size)
        fonts.append(dw.EmbeddedFont(family, open(path, "rb").read()))
        advance = font_prep.advance_pixels(path, "n")[0]
        line_twips = 20 * round(size * 1.3)
        cases = [("single", (s,)) for s in range(-60, 61)]
        cases += [("pairs", (rng.randint(-40, 40), rng.randint(-40, 40))) for _ in range(80)]
        for kind, spacings in cases:
            text = "n" * (len(spacings) + 1)
            runs = "".join(run_xml(c, family, size, (spacings + (0,))[i], i) for i, c in enumerate(text))
            if y + line_twips > 15840 - 900:
                parts.append(dw.page_break_paragraph_xml())
                y = 600
            parts.append(dw.frame_paragraph_xml(runs, x_twips=1440, y_twips=y, width_twips=3000,
                                                line_twips=line_twips))  # fmt: skip
            plans.append({"size": size, "kind": kind, "spacings": spacings, "advance": advance})
            y += line_twips
    docx = dw.write_docx(os.path.join(OUT_DIR, "e15.docx"), "".join(parts), family=fonts[0].family,
                         default_size_pt=12.0, embedded=fonts)  # fmt: skip
    pdf = os.path.join(OUT_DIR, "e15.word.pdf")
    export_docx_to_pdf(docx, pdf)
    lines: dict[tuple, list] = {}
    for obj in read_text_objects(pdf):
        if obj.glyphs and obj.glyphs[0].code == ord("n"):
            lines.setdefault((obj.page, obj.tm_text[5]), []).append(wm.decode_x(obj.x, table))
    keys = sorted(lines, key=lambda k: (k[0], -float(k[1])))
    for plan, key in zip(plans, keys, strict=True):
        pens = sorted(lines[key])
        plan["offsets"] = [p - pens[0] for p in pens]
    with open(os.path.join(OUT_DIR, "e15.json"), "w", encoding="utf-8") as handle:
        json.dump(plans, handle)
    return plans


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    saved = os.path.join(OUT_DIR, "e15.json")
    if "--reuse" in sys.argv and os.path.exists(saved):
        plans = json.load(open(saved, encoding="utf-8"))
    else:
        plans = build()
    scores: dict[tuple, dict[str, int]] = {}
    for plan in plans:
        score = scores.setdefault((plan["size"], plan["kind"]), {"pens": 0})
        spacings = tuple(plan["spacings"])
        for index in range(1, len(spacings) + 1):
            extra = plan["offsets"][index] - index * plan["advance"]
            score["pens"] += 1
            for rule, value in rules(spacings[:index]).items():
                score[rule] = score.get(rule, 0) + (value == extra)
    for (size, kind), score in scores.items():
        pens = score.pop("pens")
        print(f"{size:>5} pt {kind:<7} {pens:4d} pens, rule holds: " + ", ".join(f"{r} {n}" for r, n in score.items()))


if __name__ == "__main__":
    main()
