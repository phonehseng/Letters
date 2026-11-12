"""experiment c3: what makes word's export fail on a ligature that spans spaces?

c1: "Zed and a zoo" (3 spaces) as one ligature exported fine, a 55-character
sentence with 10 spaces made ExportAsFixedFormat fail, and c2 showed the
glyph itself (components, width, nested composites) is not the cause. here
the glyph is either the natural composite or a two-component marker, and the
text varies in length and in how many spaces it holds.

usage:  ..\\.venv\\Scripts\\python.exe probe_space_ligatures.py
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

OUT = os.path.join(fidelity_path.OUT_DIR, "c3_space_ligatures")
SENTENCE = "Yes, this is a much longer sentence with spaces in it."


def spaced(length: int, every: int) -> str:
    letters = "abcdefghijklmnopqrstuvwxyz"
    chars = []
    for index in range(length):
        chars.append(" " if every and index % every == every - 1 else letters[index % 26])
    return "Q" + "".join(chars[1:])


CASES = [
    ("sentence, natural glyph", SENTENCE, False),
    ("sentence, marker glyph", SENTENCE, True),
    ("sentence without spaces, natural", SENTENCE.replace(" ", ""), False),
    ("sentence without punctuation, natural", SENTENCE.replace(",", "").replace(".", ""), False),
    ("sentence first 30 chars, natural", SENTENCE[:30], False),
    ("sentence first 40 chars, natural", SENTENCE[:40], False),
    ("sentence first 47 chars, natural", SENTENCE[:47], False),
] + [
    (f"{length} chars, space every {every}, marker", spaced(length, every), True)
    for length in (16, 24, 32, 33, 40, 48, 64, 100)
    for every in (4, 8)
]


def run_case(session, number: int, label: str, text: str, marker: bool) -> dict:
    font = c1.base_font()
    composite = c1.natural_composite(font, "rule", text, "liga", marker=marker)
    ff.add_composites(font, [composite])
    ff.add_rules(font, [composite])
    font_path = os.path.join(OUT, f"case_{number}.ttf")
    font.save(font_path)
    embedded = [dw.EmbeddedFont(c1.FAMILY, open(font_path, "rb").read())]
    run = fd.TextRun(text, c1.FAMILY, c1.SIZE_HALF_POINTS, features=fd.RunFeatures("standard"))
    body = fd.frame_paragraph([run], 720, 720, c1.PAGE_W * 20 - 1440, c1.LINE_TWIPS)
    docx = fd.write_probe_docx(os.path.join(OUT, f"case_{number}.docx"), body, embedded, c1.PAGE_W, c1.PAGE_H)
    pdf = os.path.join(OUT, f"case_{number}.pdf")
    row = {"case": label, "chars": len(text), "spaces": text.count(" ")}
    try:
        session.export(docx, pdf)
    except Exception as error:
        row["result"] = "export failed: " + str(error)[60:120]
        return row
    rule = ff.outline_signature(TTFont(font_path), "rule")
    drawn = drawn_glyphs(pdf)
    hits = [g for g in drawn if g.signature == rule]
    row["result"] = "rule glyph drawn" if hits else "not drawn"
    row["to_unicode"] = hits[0].text if hits else None
    return row


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    rows = []
    with WordSession() as session:
        for number, (label, text, marker) in enumerate(CASES):
            rows.append(run_case(session, number, label, text, marker))
            print({k: v for k, v in rows[-1].items() if k != "to_unicode"}, rows[-1].get("to_unicode") == text)
    with open(os.path.join(OUT, "c3_results.json"), "w", encoding="utf-8") as handle:
        json.dump(rows, handle, indent=1)


if __name__ == "__main__":
    main()
