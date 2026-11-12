"""experiment c4: do gsub rules work on private-use characters from U+F0000 (the prototype's ligature glyphs)?

the prototype types a source ligature glyph (Skia's "ff", "ffi") as a
supplementary private-use character, because word draws U+E000..U+F8FF from
another font. in the corpus, every fitted run that held such a character
stayed ordinary glyphs. one marker font maps U+F0000 and U+F0001 and holds:
    a ligature that contains U+F0000,
    a single substitution of U+F0000 alone,
    a ligature right after U+F0000 in the same run ("does it split the run?"),
    the same text with the real letters ("ff") where the font has them.

usage:  ..\\.venv\\Scripts\\python.exe probe_private_use.py
"""

from __future__ import annotations

import fidelity_path

# isort: split

import json
import os

from fontTools.ttLib import TTFont

import docx_writer as dw
import fitted_docx as fd
import fitted_font as ff
import probe_ligatures as c1
from pdf_glyphs import drawn_glyphs
from word_session import WordSession

OUT = os.path.join(fidelity_path.OUT_DIR, "c4_private_use")
PRIVATE = "\U000f0000"
PRIVATE_TWO = "\U000f0001"
CASES = [
    ("ligature containing U+F0000", "a" + PRIVATE + "ects", "a" + PRIVATE + "ects", ""),
    ("U+F0000 alone, single substitution", PRIVATE, PRIVATE, ""),
    ("ligature right after U+F0000 in the same run", "x" + PRIVATE_TWO + "ects now", "ects now", ""),
    ("ligature before U+F0000 in the same run", "the a" + PRIVATE_TWO + "x", "the a", ""),
    ("real letters 'affects'", "affects", "affects", ""),
]


def build_font() -> TTFont:
    font = c1.base_font()
    # two private-use glyphs: copies of "f" offset upwards, so they are recognisable
    f_glyph = ff.glyph_name_for_char(font, "f")
    for name, raise_units in (("pua0", 200), ("pua1", 300)):
        ff.add_composites(font, [ff.Composite(name, [f_glyph], [ff.Component(f_glyph, 0, raise_units)],
                                              font["hmtx"][f_glyph][0])])  # fmt: skip
    for table in font["cmap"].tables:
        if table.format == 12 or (table.platformID == 3 and table.platEncID == 10):
            table.cmap[ord(PRIVATE)] = "pua0"
            table.cmap[ord(PRIVATE_TWO)] = "pua1"
    if not any(t.format == 12 for t in font["cmap"].tables):
        from fontTools.ttLib.tables._c_m_a_p import cmap_format_12

        table = cmap_format_12(12)
        table.platformID, table.platEncID, table.language = 3, 10, 0
        table.cmap = dict(font.getBestCmap())
        table.cmap[ord(PRIVATE)] = "pua0"
        table.cmap[ord(PRIVATE_TWO)] = "pua1"
        font["cmap"].tables.append(table)
    return font


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    font = build_font()
    composites = []
    for index, (_, _, rule_text, _) in enumerate(CASES):
        composites.append(c1.natural_composite(font, f"rule{index}", rule_text, "liga", marker=True))
    ff.add_composites(font, composites)
    ff.add_rules(font, composites)
    path = os.path.join(OUT, "pua.ttf")
    font.save(path)
    font = TTFont(path)
    embedded = [dw.EmbeddedFont(c1.FAMILY, open(path, "rb").read())]
    paragraphs = []
    for index, (_, text, _, _) in enumerate(CASES):
        run = fd.TextRun(text, c1.FAMILY, c1.SIZE_HALF_POINTS, features=fd.RunFeatures("standard"))
        paragraphs.append(fd.frame_paragraph([run], 720, 720 + index * 480, 8000, c1.LINE_TWIPS))
    docx = fd.write_probe_docx(os.path.join(OUT, "pua.docx"), "".join(paragraphs), embedded)
    pdf = os.path.join(OUT, "pua.pdf")
    with WordSession() as session:
        session.export(docx, pdf)
        pid = session.pid
    names = {ff.outline_signature(font, n): n for n in reversed(font.getGlyphOrder())}
    drawn = drawn_glyphs(pdf)
    rows = []
    for index, (label, _, _, _) in enumerate(CASES):
        baseline = 792 - (720 + index * 480 + 0.8 * c1.LINE_TWIPS) / 20
        line = [g for g in drawn if abs(g.y - baseline) < 6]
        glyph_names = [names.get(g.signature, "?") for g in line]
        rows.append({"case": label, "applied": f"rule{index}" in glyph_names, "drawn": glyph_names,
                     "fonts": sorted({g.font_resource for g in line})})  # fmt: skip
        print(rows[-1])
    with open(os.path.join(OUT, "c4_results.json"), "w", encoding="utf-8") as handle:
        json.dump({"word_pid": pid, "rows": rows}, handle, indent=1)


if __name__ == "__main__":
    main()
