"""e7: pdf font sizes a docx cannot store (w:sz is in half points).

lines at 10.3, 11.27 and 7.9 pt, plus 10.5 pt as a storable control, all in frames
placed by calibration (see e5_grid_snap.py). two variants:
    calibrated_plain  the nearest half-point size with the plain font: glyphs
                      too large or too small, advances drift
    grid_snap         the nearest half-point size with a grid font whose outlines
                      and metrics are scaled by pdf size / drawn size, every advance
                      a whole 1/600 inch pixel, spacing from grid_model
glyph size is measured from ink box heights, positions as everywhere else.

usage:  .venv\\Scripts\\python.exe e7_odd_sizes.py
output: out\\e5\\e7set\\ (shares e5's machinery and folder layout)
"""

from __future__ import annotations

import json
import os

import e5_grid_snap as e5
import test_fonts
from test_pdf import TextLine, register_font, sample_text

SIZES = (10.3, 11.27, 7.9, 10.5)
LINES_PER_SIZE = 10


def odd_size_lines() -> list[TextLine]:
    lines = []
    baseline = 740.41
    for index in range(LINES_PER_SIZE * len(SIZES)):
        size = SIZES[index % len(SIZES)]
        x = 72.37 + (index % 7) * 2.13
        lines.append(TextLine(0, x, round(baseline, 4), size, sample_text(300 + index, 6)))
        baseline -= 16.13
    return lines


def main() -> None:
    register_font(test_fonts.truetype_font(), e5.PDF_FONT_NAME)
    summary = e5.run_set("e7set", odd_size_lines(), None)
    with open(os.path.join(e5.OUT_DIR, "e7_summary.json"), "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)


if __name__ == "__main__":
    main()
