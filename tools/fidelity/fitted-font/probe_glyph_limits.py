"""experiment c2: how big may one pre-composed glyph be before word's pdf export fails?

c1 found that a 55-character composite made ExportAsFixedFormat fail while a
32-character one and 300-character ligatures with a two-component glyph
worked. this probe separates the suspects: the number of components of a
composite glyph, its width in font units, and a decomposed (simple) glyph
holding the same outlines. each case is its own document, so one failure
cannot hide another answer.
the second part (--em-width) followed a corpus failure: the form's export
failed on a 94-component glyph 45.7 em wide (650 units per em). it sets two
"l" glyphs a given number of ems apart, at 500 and 1000 units per em, and
separately a narrow glyph with a very wide advance, to see whether the limit
is in font units, in ems, or in the advance.

usage:  ..\\.venv\\Scripts\\python.exe probe_glyph_limits.py
"""

from __future__ import annotations

import fidelity_path

# isort: split

import json
import os
import sys

from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

import docx_writer as dw
import fitted_docx as fd
import fitted_font as ff
import font_prep
import probe_ligatures as c1
import test_fonts
from pdf_glyphs import drawn_glyphs
from word_session import WordSession

OUT = os.path.join(fidelity_path.OUT_DIR, "c2_glyph_limits")
COMPONENT_COUNTS = (32, 36, 40, 44, 48, 56, 64, 96, 128, 255, 256, 300)
NARROW_TEXT = "il"  # narrow letters, so a long composite stays well inside int16 offsets
WIDE_TEXT = "WM"


def text_of(pattern: str, count: int) -> str:
    return "Q" + "".join(pattern[i % len(pattern)] for i in range(count - 1))


def decomposed(font: TTFont, composite: ff.Composite, name: str) -> None:
    """replace a composite by one simple glyph with the same points."""
    glyph_set = font.getGlyphSet()
    pen = TTGlyphPen(None)
    for component in composite.components:
        recording = DecomposingRecordingPen(glyph_set)
        glyph_set[component.glyph].draw(recording)
        recording.replay(TransformPen(pen, (1, 0, 0, 1, component.dx, component.dy)))
    font["glyf"][name] = pen.glyph()
    font["glyf"][name].recalcBounds(font["glyf"])


def build_cases(font: TTFont) -> list[dict]:
    cases = []
    for count in COMPONENT_COUNTS:
        for label, pattern in (("narrow", NARROW_TEXT), ("wide", WIDE_TEXT)):
            text = text_of(pattern, count)
            composite = c1.natural_composite(font, f"lim_{label}_{count}", text, "liga")
            too_far = max(abs(c.dx) for c in composite.components) > ff.MAX_COMPONENT_OFFSET
            cases.append({"label": f"{label} composite {count}", "text": text, "composite": composite,
                          "simple": False, "too_far": too_far})  # fmt: skip
        text = text_of(NARROW_TEXT, count)
        composite = c1.natural_composite(font, f"lim_simple_{count}", text, "liga")
        cases.append({"label": f"narrow simple {count}", "text": text, "composite": composite, "simple": True,
                      "too_far": False})  # fmt: skip
    return [case for case in cases if not case["too_far"]]


def em_width_cases() -> list[dict]:
    cases = []
    for units_per_em in (500, 1000):
        font = base_font_at(units_per_em)
        hmtx, l_glyph = font["hmtx"], ff.glyph_name_for_char(font, "l")
        for ems in (20, 30, 32, 32.7, 32.8, 34, 40, 46, 60):
            reach = round(ems * units_per_em)
            if reach > ff.MAX_COMPONENT_OFFSET:
                continue
            components = [ff.Component(l_glyph, 0, 0), ff.Component(l_glyph, reach, 0)]
            composite = ff.Composite(f"em_{units_per_em}_{ems}", [l_glyph, l_glyph], components,
                                     min(ff.MAX_ADVANCE, reach + hmtx[l_glyph][0]))  # fmt: skip
            cases.append({"label": f"two l {ems} em apart, {units_per_em} upm", "text": "ll", "composite": composite,
                          "simple": False, "units_per_em": units_per_em})  # fmt: skip
        for ems in (32, 34, 46):
            composite = ff.Composite(f"adv_{units_per_em}_{ems}", [l_glyph, l_glyph], [ff.Component(l_glyph, 0, 0)],
                                     min(ff.MAX_ADVANCE, round(ems * units_per_em)))  # fmt: skip
            cases.append({"label": f"one l, advance {ems} em, {units_per_em} upm", "text": "ll",
                          "composite": composite, "simple": False, "units_per_em": units_per_em})  # fmt: skip
    return cases


def base_font_at(units_per_em: int) -> TTFont:
    if units_per_em == 1000:
        return c1.base_font()
    path = os.path.join(OUT, f"base_{units_per_em}.ttf")
    font_prep.make_renamed_font(test_fonts.font_path("CrimsonText-Regular.ttf"), path, c1.FAMILY,
                                units_per_em=units_per_em)  # fmt: skip
    font = TTFont(path)
    for table in ("GSUB", "GPOS", "kern", "GDEF"):
        if table in font:
            del font[table]
    return font


def run_case(session, case: dict, number: int) -> dict:
    font = base_font_at(case.get("units_per_em", 1000))
    try:
        ff.add_composites(font, [case["composite"]])
    except ValueError as error:
        return {"case": case["label"], "result": f"cannot be built: {error}"}
    if case["simple"]:
        decomposed(font, case["composite"], case["composite"].name)
    ff.add_rules(font, [case["composite"]])
    font_path = os.path.join(OUT, f"case_{number}.ttf")
    font.save(font_path)
    embedded = [dw.EmbeddedFont(c1.FAMILY, open(font_path, "rb").read())]
    run = fd.TextRun(case["text"], c1.FAMILY, c1.SIZE_HALF_POINTS, features=fd.RunFeatures("standard"))
    body = fd.frame_paragraph([run], 720, 720, c1.PAGE_W * 20 - 1440, c1.LINE_TWIPS)
    docx = fd.write_probe_docx(os.path.join(OUT, f"case_{number}.docx"), body, embedded, c1.PAGE_W, c1.PAGE_H)
    pdf = os.path.join(OUT, f"case_{number}.pdf")
    width = max(c.dx for c in case["composite"].components)
    row = {"case": case["label"], "components": len(case["composite"].components), "max_offset_units": width}
    try:
        session.export(docx, pdf)
    except Exception as error:
        row["result"] = "export failed: " + str(error)[:90]
        return row
    reference = TTFont(font_path)
    rule = ff.outline_signature(reference, case["composite"].name)
    drawn = drawn_glyphs(pdf)
    row["result"] = "rule glyph drawn" if any(g.signature == rule for g in drawn) else "not drawn"
    row["glyphs"] = len(drawn)
    return row


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    em_width = "--em-width" in sys.argv
    cases = em_width_cases() if em_width else build_cases(c1.base_font())
    rows = []
    with WordSession() as session:
        for number, case in enumerate(cases):
            rows.append(run_case(session, case, number + (1000 if em_width else 0)))
            print(rows[-1])
    name = "c2_em_width_results.json" if em_width else "c2_results.json"
    with open(os.path.join(OUT, name), "w", encoding="utf-8") as handle:
        json.dump(rows, handle, indent=1)


if __name__ == "__main__":
    main()
