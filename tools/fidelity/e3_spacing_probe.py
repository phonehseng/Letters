"""e3 probe: how does word turn a glyph advance plus w:spacing into a pen position?

each line repeats one character 30 times at 12 pt, one run per character, with a
fixed w:spacing per line from -4 to +12 twips. a perfect engine would advance by
font advance + spacing/20 pt every time. this prints what word's pdf shows:
the mean advance, its error against that ideal, and the distinct step sizes.

usage:  .venv\\Scripts\\python.exe e3_spacing_probe.py
output: out\\e3_probe\\ (the docx, word's pdf, probe.json)
"""

from __future__ import annotations

import json
import os
from collections import defaultdict

import numpy as np
from fontTools.ttLib import TTFont

import docx_writer as dw
import test_fonts
from pdf_analysis import extract_chars
from word_export import export_docx_to_pdf

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e3_probe")
SIZE_PT = 12.0
REPEATS = 30
PROBE_CHARS = ("n", "i", "m")
SPACINGS_TWIPS = list(range(-4, 13))


def probe_rows() -> list[tuple[str, int, bool]]:
    rows = [(char, spacing, True) for char in PROBE_CHARS for spacing in SPACINGS_TWIPS]
    # one run for the whole line, to see whether splitting into runs matters
    rows += [(char, 0, False) for char in PROBE_CHARS]
    return rows


def probe_body(family: str, rows) -> str:
    paragraphs = []
    for index, (char, spacing, per_char_runs) in enumerate(rows):
        if per_char_runs:
            runs = "".join(dw.run_xml(char, family, SIZE_PT, spacing_twips=spacing) for _ in range(REPEATS))
        else:
            runs = dw.run_xml(char * REPEATS, family, SIZE_PT)
        y_twips = 720 + index * 270
        paragraphs.append(dw.frame_paragraph_xml(runs, x_twips=1447, y_twips=y_twips, width_twips=9000, line_twips=240))
    return "".join(paragraphs)


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    font_path = test_fonts.truetype_font()
    font = TTFont(font_path)
    units_per_em = font["head"].unitsPerEm
    cmap = font.getBestCmap()
    rows = probe_rows()
    with open(font_path, "rb") as handle:
        embedded = dw.EmbeddedFont(family=test_fonts.TRUETYPE_FAMILY, font_bytes=handle.read())
    docx = dw.write_docx(
        os.path.join(OUT_DIR, "probe.docx"),
        probe_body(embedded.family, rows),
        family=embedded.family,
        default_size_pt=SIZE_PT,
        embedded=embedded,
    )
    pdf = os.path.join(OUT_DIR, "probe.word.pdf")
    info = export_docx_to_pdf(docx, pdf)

    lines = defaultdict(list)
    for char in extract_chars(pdf):
        lines[round(char.origin_y, 2)].append(char.origin_x)
    measured = [sorted(xs) for _, xs in sorted(lines.items(), key=lambda item: -item[0])]
    if len(measured) != len(rows):
        raise RuntimeError(f"expected {len(rows)} probe lines, word's pdf has {len(measured)}")

    results = []
    print(f"word {info['word_version']}, {SIZE_PT} pt, {REPEATS} repeats per line")
    print("char  spacing  runs   ideal adv   word mean adv   error/char   distinct steps (pt)")
    for (char, spacing, per_char_runs), xs in zip(rows, measured, strict=True):
        ideal = font["hmtx"][cmap[ord(char)]][0] * SIZE_PT / units_per_em + spacing / dw.TWIPS_PER_PT
        steps = np.diff(xs)
        mean_advance = float(steps.mean())
        distinct = sorted({round(float(s), 3) for s in steps})
        results.append(
            {
                "char": char,
                "spacing_twips": spacing,
                "per_char_runs": per_char_runs,
                "ideal_advance": ideal,
                "word_mean_advance": mean_advance,
                "error_per_char": mean_advance - ideal,
                "distinct_steps": distinct,
            }
        )
        print(
            f"  {char}   {spacing:+4d}    {'each' if per_char_runs else 'one '}   {ideal:9.4f}   {mean_advance:12.4f}"
            f"   {mean_advance - ideal:+9.4f}   {distinct[:6]}"
        )
    with open(os.path.join(OUT_DIR, "probe.json"), "w", encoding="utf-8") as handle:
        json.dump({"word": info, "rows": results}, handle, indent=2)


if __name__ == "__main__":
    main()
