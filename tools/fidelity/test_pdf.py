"""source pdfs with text at exactly known positions, drawn with reportlab.

every placed line is described by a TextLine, so the same list can drive both
the pdf and each docx variant.
"""

from __future__ import annotations

import os
from dataclasses import dataclass

from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont as ReportlabTTFont
from reportlab.pdfgen import canvas

PAGE_WIDTH_PT = 612.0
PAGE_HEIGHT_PT = 792.0


@dataclass
class TextLine:
    page: int
    x: float  # pen start, points from the left edge
    baseline_y: float  # points from the bottom edge, pdf style
    size: float
    text: str
    char_space: float = 0.0  # extra pt after every char, like the pdf Tc operator
    advances: list[float] | None = None  # explicit pen advance per char, overrides the font


def register_font(font_path: str, name: str) -> str:
    if name not in pdfmetrics.getRegisteredFontNames():
        pdfmetrics.registerFont(ReportlabTTFont(name, font_path))
    return name


def _draw_line(pdf: canvas.Canvas, font_name: str, line: TextLine) -> None:
    if line.advances is not None:
        # one text object per glyph, the way many real pdfs place text
        x = line.x
        for char, advance in zip(line.text, line.advances, strict=True):
            pdf.setFont(font_name, line.size)
            pdf.drawString(x, line.baseline_y, char)
            x += advance
        return
    text_object = pdf.beginText(line.x, line.baseline_y)
    text_object.setFont(font_name, line.size)
    text_object.setCharSpace(line.char_space)
    text_object.textOut(line.text)
    pdf.drawText(text_object)


def write_pdf(path: str, font_name: str, lines: list[TextLine]) -> str:
    pdf = canvas.Canvas(path, pagesize=(PAGE_WIDTH_PT, PAGE_HEIGHT_PT), pageCompression=1)
    pdf.setTitle("fidelity test")
    page_count = max(line.page for line in lines) + 1
    for page in range(page_count):
        for line in (line for line in lines if line.page == page):
            _draw_line(pdf, font_name, line)
        pdf.showPage()
    pdf.save()
    return path


def expected_origins(font_name: str, line: TextLine) -> list[float]:
    """pen x of every non-space char, as the pdf is meant to place it."""
    advances = line.advances if line.advances is not None else natural_advances(font_name, line)
    origins, x = [], line.x
    for char, advance in zip(line.text, advances, strict=True):
        if not char.isspace():
            origins.append(x)
        x += advance
    return origins


def verify_source_pdf(pdf_path: str, font_name: str, lines: list[TextLine]) -> float:
    """largest gap between where we meant glyphs to go and where pdfium reads them.

    every number the harness reports is relative to pdfium's reading of the
    source, so this must be ~0 or the source itself is not what we think."""
    from collections import defaultdict

    from pdf_analysis import extract_chars

    expected = defaultdict(list)
    for line in lines:
        key = (line.page, round(line.baseline_y, 3))
        expected[key] += [(x, line.baseline_y) for x in expected_origins(font_name, line)]
    found = defaultdict(list)
    for char in extract_chars(pdf_path):
        found[(char.page, round(char.origin_y, 3))].append((char.origin_x, char.origin_y))
    if expected.keys() != found.keys():
        raise RuntimeError("source pdf rows do not match the line definitions")
    worst = 0.0
    for key, wanted in expected.items():
        for (wx, wy), (fx, fy) in zip(sorted(wanted), sorted(found[key]), strict=True):
            worst = max(worst, abs(wx - fx), abs(wy - fy))
    return worst


SOURCE_CHECK_LIMIT_PT = 0.001


def write_verified_pdf(path: str, font_name: str, lines: list[TextLine]) -> str:
    write_pdf(path, font_name, lines)
    worst = verify_source_pdf(path, font_name, lines)
    print(f"source check {os.path.basename(path)}: pdfium reads glyph origins within {worst:.6f} pt of the definitions")
    if worst > SOURCE_CHECK_LIMIT_PT:
        raise RuntimeError(f"source pdf glyphs are {worst:.4f} pt from where they were meant to be")
    return path


def natural_advances(font_name: str, line: TextLine) -> list[float]:
    """pen advances reportlab used for a plain or char-spaced line."""
    return [pdfmetrics.stringWidth(c, font_name, line.size) + line.char_space for c in line.text]


SAMPLE_WORDS = (
    "fidelity quartz jumping lynx vexed wizard glyph baseline kerning offset hamburgefonstiv "
    "minimum ligature office affine waffle typography exact frame anchor margin column twips "
    "emu quantise drift sphinx black quartz judge vow 0123456789 (pdf) [docx] {word} 72.37 13.61"
).split()


def sample_text(index: int, word_count: int = 5) -> str:
    """deterministic short text, different for each line so matching cannot slip."""
    words = [SAMPLE_WORDS[(index * 7 + k * 3) % len(SAMPLE_WORDS)] for k in range(word_count)]
    return f"{index:03d} " + " ".join(words)
