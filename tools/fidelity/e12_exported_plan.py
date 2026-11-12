"""e12: plan glyphs in exported coordinates and check word prints exactly the planned numbers.

each line has random fractional source positions. the plan (word_writer_model):
    - line start: the frame twips whose exported x is nearest the first source x
      (twips -> pixel rule from e10, no calibration export)
    - every glyph: the layout pixel whose exported x is nearest its source x
    - w:spacing from grid_model's floor rule, runs split by alternating
      w:lang w:eastAsia so every glyph gets its own text object
then word's pdf is read raw: a glyph is "exact" when its printed Tm x equals the
model's printed value for its planned pixel. sizes cover drawn = stored (9, 12,
36) and drawn != stored (10.5, 11, 14, 20), the e10 line probe's trouble cases.

usage:  .venv\\Scripts\\python.exe e12_exported_plan.py
"""

from __future__ import annotations

import os
import random

import numpy as np

import docx_writer as dw
import font_prep
import grid_model as gm
import test_fonts
import word_writer_model as wm
from word_export import export_docx_to_pdf
from word_pdf_raw import read_text_objects

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e12")
SIZES = (9.0, 10.5, 11.0, 12.0, 14.0, 20.0, 36.0)
LINES_PER_SIZE = 6
SPLIT_LANGUAGES = ("en-US", "en-GB")
TEXT = "Hamburgefonstiv quartz judge 0123456789 sphinx of black wavy jumps"


def grid_font(size_pt: float) -> tuple[str, str]:
    tag = f"{size_pt:g}".replace(".", "h")
    family = f"Fidelity Grid {tag}"
    test_fonts.assert_not_installed(family)
    path = test_fonts.build_path(f"FidelityGrid-{tag}.ttf")
    if not os.path.exists(path):
        font_prep.make_grid_font(test_fonts.truetype_font(), path, family, size_pt)
    return family, path


def twips_pixel(twips: int) -> int:
    pixel = (5 * twips + 1) // 12
    if (5 * twips + 1) % 12 == 0 and twips % 60 in (19, 55):
        pixel -= 1
    return pixel


def run_xml(char: str, family: str, size: float, spacing: int, index: int) -> str:
    half_points = round(size * 2)
    spacing_xml = f'<w:spacing w:val="{spacing}"/>' if spacing else ""
    return (
        f'<w:r><w:rPr><w:rFonts w:ascii="{family}" w:hAnsi="{family}" w:cs="{family}" w:eastAsia="{family}"/>'
        f'{spacing_xml}<w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/>'
        f'<w:lang w:eastAsia="{SPLIT_LANGUAGES[index % 2]}"/><w14:ligatures w14:val="none"/></w:rPr>'
        f'<w:t xml:space="preserve">{char}</w:t></w:r>'
    )


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    rng = random.Random(12)
    table = wm.step_table("x", 5100)
    parts, plans, fonts = [], [], []
    y = 700
    for size in SIZES:
        family, path = grid_font(size)
        fonts.append(dw.EmbeddedFont(family, open(path, "rb").read()))
        line_twips = 20 * max(2, round(size * 1.3))
        for _ in range(LINES_PER_SIZE):
            text = TEXT[: int(380 / (0.5 * size))]
            advances = font_prep.advance_pixels(path, text)
            start_pt = rng.uniform(40, 120)
            # sources: the font's own advances at the pdf size, plus random jitter, in points
            sources, pen = [], start_pt
            for advance in advances:
                sources.append(pen)
                pen += advance * gm.PT_PER_PIXEL + rng.uniform(-0.3, 0.6)
            twips = min(range(round(start_pt * 20) - 4, round(start_pt * 20) + 5),
                        key=lambda t: abs(wm.exported_x(twips_pixel(t), table) - start_pt))  # fmt: skip
            start_pixel = twips_pixel(twips)
            pixels = [start_pixel] + [wm.nearest_exported_pixel(s, table) for s in sources[1:]]
            spacing = gm.spacing_for_pixel_targets(advances, [p - start_pixel for p in pixels])
            runs = "".join(run_xml(c, family, size, s, i) for i, (c, s) in enumerate(zip(text, spacing, strict=True)))
            if y + line_twips > 15840 - 900:
                parts.append(dw.page_break_paragraph_xml())
                y = 700
            parts.append(dw.frame_paragraph_xml(runs, x_twips=twips, y_twips=y, width_twips=11000 - twips,
                                                line_twips=line_twips))  # fmt: skip
            plans.append({"size": size, "y": y, "line_twips": line_twips, "text": text, "sources": sources,
                          "pixels": pixels, "page": sum(1 for p in parts if "pageBreakBefore" in p)})  # fmt: skip
            y += line_twips + 60
    docx = dw.write_docx(os.path.join(OUT_DIR, "e12.docx"), "".join(parts), family=fonts[0].family,
                         default_size_pt=12.0, embedded=fonts)  # fmt: skip
    pdf = os.path.join(OUT_DIR, "e12.word.pdf")
    export_docx_to_pdf(docx, pdf)
    report(pdf, plans, table)


def report(pdf: str, plans: list[dict], table: wm.StepTable) -> None:
    objects = [o for o in read_text_objects(pdf) if o.glyphs and o.glyphs[0].code != 32]
    by_line: dict[tuple, list] = {}
    for obj in objects:
        by_line.setdefault((obj.page, obj.tm_text[5]), []).append(obj)
    keys = sorted(by_line, key=lambda k: (k[0], -float(k[1])))
    rows = {}
    for plan, key in zip(plans, keys, strict=True):
        line_objects = sorted(by_line[key], key=lambda o: o.x)
        visible = [(c, s, p) for c, s, p in zip(plan["text"], plan["sources"], plan["pixels"], strict=True) if c != " "]
        if len(visible) != len(line_objects):
            print(f"   {plan['size']} pt: {len(line_objects)} text objects for {len(visible)} glyphs, line skipped")
            continue
        stats = rows.setdefault(plan["size"], {"exact": 0, "glyphs": 0, "errors": [], "multi": 0})
        for (_, source, pixel), obj in zip(visible, line_objects, strict=True):
            stats["glyphs"] += 1
            stats["multi"] += len(obj.glyphs) > 1
            stats["exact"] += obj.tm_text[4] == wm.printed_text(wm.exported_x(pixel, table))
            stats["errors"].append(abs(obj.x - source))
    print("size  glyphs  own text object  printed as planned  |x - source| max / mean pt")
    for size, stats in rows.items():
        errors = np.array(stats["errors"])
        print(f"{size:>5} {stats['glyphs']:6d}  {stats['glyphs'] - stats['multi']:6d}  {stats['exact']:6d}"
              f"       {errors.max():.4f} / {errors.mean():.4f}")  # fmt: skip


if __name__ == "__main__":
    main()
