"""e14: the form class (standard 14 fonts, not embedded) with open stand-ins and with windows fonts.

converts corpus\\form.pdf twice, once per stand-in set (font_rebuild.substitute_file,
LETTERS_STANDINS), exports each through word and compares the text regions
with the source page as pdfium renders it at 300 dpi: one crop per source line
(pen box of the line, ascender to descender), structural similarity per crop.
positions come from the harness as usual. pdfium itself draws the source's
non-embedded fonts with its own stand-ins, so this measures closeness to what
a viewer shows, not to the original Helvetica or Times.

usage:  .venv\\Scripts\\python.exe e14_standin_ssim.py
"""

from __future__ import annotations

import os

import numpy as np
from skimage.metrics import structural_similarity

from corpus_run import glyph_stats
from harness import run_harness
from pdf_analysis import render_pages_gray
from prototype.convert import convert
from prototype.lines import group_lines
from prototype.pdf_content import read_document

HERE = os.path.dirname(os.path.abspath(__file__))
OUT_DIR = os.path.join(HERE, "out", "e14")
SOURCE = os.path.join(HERE, "corpus", "form.pdf")
DPI = 300


def line_crops(pdf_path: str) -> list[tuple[int, tuple[int, int, int, int]]]:
    scale = DPI / 72
    pages = read_document(pdf_path)
    crops = []
    for line in group_lines([g for p in pages for g in p.glyphs if g.render_mode not in (3, 7)]):
        height = pages[line.page].height
        left, right = line.x - 1, line.x + line.width() + 1
        top, bottom = line.y + 0.85 * line.max_size, line.y - 0.25 * line.max_size
        box = (
            int(left * scale),
            int((height - top) * scale),
            int(right * scale) + 1,
            int((height - bottom) * scale) + 1,
        )
        if box[2] - box[0] >= 7 and box[3] - box[1] >= 7:
            crops.append((line.page, box))
    return crops


def text_ssim(source_pdf: str, word_pdf: str) -> dict:
    source_pages, word_pages = render_pages_gray(source_pdf, DPI), render_pages_gray(word_pdf, DPI)
    values = []
    for page, (x0, y0, x1, y1) in line_crops(source_pdf):
        a = source_pages[page][y0:y1, x0:x1]
        b = word_pages[page][y0:y1, x0:x1]
        if a.shape == b.shape and min(a.shape) >= 7:
            values.append(structural_similarity(a, b, data_range=255))
    values = np.array(values)
    return {"lines": int(values.size), "mean": float(values.mean()), "min": float(values.min())}


def main() -> None:
    for standins in ("ofl", "windows"):
        os.environ["LETTERS_STANDINS"] = standins
        out = os.path.join(OUT_DIR, standins)
        os.makedirs(out, exist_ok=True)
        docx = os.path.join(out, "form.docx")
        conversion = convert(SOURCE, docx, os.path.join(out, "work"))
        report = run_harness(SOURCE, docx, out, keep_pairs=True, exact_positions=True)
        stats = glyph_stats(report["pairs"])
        ssim = text_ssim(SOURCE, report["word_pdf"])
        faces = ", ".join(
            f"{f['pdf_font']} -> {f['substitute'].split(' -> ')[1].split(' (')[0]} (fsType {f['fs_type']})"
            for f in conversion["fonts"]
            if f.get("substitute")
        )
        print(f"{standins:<8} glyph max {stats['max']:.3f} mean {stats['mean']:.4f} | text-line ssim mean "
              f"{ssim['mean']:.4f} min {ssim['min']:.4f} over {ssim['lines']} lines | {faces}")  # fmt: skip
    os.environ.pop("LETTERS_STANDINS", None)


if __name__ == "__main__":
    main()
