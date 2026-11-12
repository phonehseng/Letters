"""experiment b0 sources: the smallest pdfs the fitted font must match (one line, one font).

reportlab writes crimson text (sil ofl), rescaled to 1000 units per em so its
/Widths are whole numbers and pdfium renders the source where the pdf spec
puts it (raster_check depends on that), at positions none of which sit on
word's 1/600 in grid: one plain line, one with character spacing and word
spacing (so every glyph is off its natural advance),
a three-line page with a repeated line (the duplicate case), and 25 repeats
(more duplicates than one font copy has slots).

usage:  ..\\.venv\\Scripts\\python.exe make_one_line.py
"""

from __future__ import annotations

import fidelity_path

# isort: split

import os

from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont as ReportlabFont
from reportlab.pdfgen import canvas

import test_fonts

OUT = os.path.join(fidelity_path.OUT_DIR, "b0_sources")
FONT_NAME = "CrimsonProbe"
SENTENCE = "Fitted fonts place every glyph where the source has it, quickly."


def _canvas(name: str) -> canvas.Canvas:
    pdfmetrics.registerFont(ReportlabFont(FONT_NAME, test_fonts.truetype_font()))
    return canvas.Canvas(os.path.join(OUT, name), pagesize=(612, 792))


def plain_line() -> None:
    page = _canvas("one_line.pdf")
    page.setFont(FONT_NAME, 11)
    page.drawString(72.37, 700.13, SENTENCE)
    page.save()


def spaced_line() -> None:
    page = _canvas("one_line_spaced.pdf")
    text = page.beginText(81.613, 655.457)
    text.setFont(FONT_NAME, 10.3)
    text.setCharSpace(0.137)
    text.setWordSpace(0.611)
    text.textLine(SENTENCE)
    page.drawText(text)
    page.save()


def repeated_lines() -> None:
    page = _canvas("repeated_lines.pdf")
    page.setFont(FONT_NAME, 12)
    for y, x in ((700.07, 72.0), (683.61, 72.043), (667.19, 90.517)):
        page.drawString(x, y, "Repeated line, same words.")
    page.save()


def many_repeats() -> None:
    """25 identical lines at different x: more than one font copy's 21 slots."""
    page = _canvas("many_repeats.pdf")
    page.setFont(FONT_NAME, 10)
    for row in range(25):
        page.drawString(72.0 + row * 0.537, 740.0 - row * 26.013, "Repeated line, same words.")
    page.save()


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    plain_line()
    spaced_line()
    repeated_lines()
    many_repeats()


if __name__ == "__main__":
    main()
