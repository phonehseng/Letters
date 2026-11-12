"""an independent check of ink positions: render both pdfs and compare where each blob of ink sits.

fitted_measure computes ink positions from the outlines we wrote; this does
not trust that. each source line's box is rendered from both pdfs by pdfium
at RENDER_DPI; every connected blob of ink (usually one glyph) in the source
crop is paired with the blob in word's crop whose centroid is nearest, and
the shift between their intensity-weighted centroids is the ink error.
whole-line phase correlation was tried first and rejected: on a one-line
probe it reported 0.17 pt where every glyph's ink edge agreed within one
1200 dpi pixel (0.06 pt), so it is not used.
blobs whose areas differ by more than AREA_TOLERANCE are skipped (touching
glyphs that merged in one rendering and not the other). both pdfs are first
copied with every path and picture removed (text_only_copy): table rules in
a line's box are shapes word places on its own grid, not glyphs, and in the
first syllabus run they were the only blobs off by more than 0.02 pt.
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import math
import os

import numpy as np
import pikepdf
import pypdfium2 as pdfium
from scipy import ndimage

from prototype import lines
from prototype.pdf_content import read_document

RENDER_DPI = 1200  # one pixel is 0.06 pt; a weighted centroid resolves a small fraction of that
PT_PER_PIXEL = 72 / RENDER_DPI
MARGIN_PT = 3.0
INK_THRESHOLD = 16  # on 0..255 ink: includes anti-aliased edges, so centroids use the whole glyph
AREA_TOLERANCE = 0.1
PAIR_RADIUS_PIXELS = 6  # 0.36 pt: further apart than that is a different blob
# pdf 1.7 tables 59 and 60: path construction and painting, and clipping that consumes the path
PATH_OPERATORS = {"m", "l", "c", "v", "y", "h", "re", "S", "s", "f", "F", "f*", "B", "B*", "b", "b*", "n", "W", "W*",
                  "sh"}  # fmt: skip


def text_only_copy(pdf_path: str, out_path: str) -> str:
    """the same pdf with paths, shadings and pictures removed from every page's content."""
    with pikepdf.open(pdf_path) as pdf:
        for page in pdf.pages:
            images = set()
            xobjects = page.obj.get("/Resources", {}).get("/XObject", {})
            for name, xobject in xobjects.items():
                if xobject.get("/Subtype") == "/Image":
                    images.add(str(name))
            kept = []
            for operands, operator in pikepdf.parse_content_stream(page):
                op = str(operator)
                if op in PATH_OPERATORS or op == "INLINE IMAGE":
                    continue
                if op == "Do" and str(operands[0]) in images:
                    continue
                kept.append((operands, operator))
            page.obj.Contents = pdf.make_stream(pikepdf.unparse_content_stream(kept))
        pdf.save(out_path)
    return out_path


def _render_box(document, page_index: int, box: tuple[float, float, float, float]) -> np.ndarray:
    page = document[page_index]
    width, height = page.get_size()
    left, bottom, right, top = box
    # pdfium crops are distances from each page edge
    crop = (max(0.0, left), max(0.0, bottom), max(0.0, width - right), max(0.0, height - top))
    bitmap = page.render(scale=RENDER_DPI / 72, crop=crop, grayscale=True)
    image = np.array(bitmap.to_pil().convert("L"), dtype=np.float64)
    page.close()
    return 255.0 - image


def line_boxes(pdf_path: str) -> list[tuple[int, tuple[float, float, float, float], str]]:
    glyphs = [g for page in read_document(pdf_path) for g in page.glyphs if g.render_mode not in (3, 7)]
    boxes = []
    for line in lines.group_lines(glyphs):
        text = "".join(g.text for g in line.glyphs)
        if abs(line.angle) > lines.ANGLE_TOLERANCE_DEG or not text.strip():
            continue
        size = line.max_size
        box = (line.x - MARGIN_PT, line.y - 0.35 * size - MARGIN_PT,
               line.x + line.width() + MARGIN_PT, line.y + 1.0 * size + MARGIN_PT)  # fmt: skip
        boxes.append((line.page, box, text))
    return boxes


def _blobs(image: np.ndarray) -> list[tuple[float, float, float]]:
    """(row, column, area) of every connected blob of ink, centroids weighted by ink."""
    labels, count = ndimage.label(image > INK_THRESHOLD)
    if count == 0:
        return []
    index = np.arange(1, count + 1)
    centroids = ndimage.center_of_mass(image, labels, index)
    areas = ndimage.sum(np.ones_like(image), labels, index)
    return [(r, c, a) for (r, c), a in zip(centroids, areas, strict=True)]


def blob_shifts(a: np.ndarray, b: np.ndarray) -> list[tuple[float, float]]:
    """(dx, dy) in pt from each source blob to its word blob; positive dy is up the page."""
    theirs = _blobs(b)
    shifts = []
    for row, column, area in _blobs(a):
        best = min(theirs, key=lambda t: math.hypot(t[0] - row, t[1] - column), default=None)
        if best is None or math.hypot(best[0] - row, best[1] - column) > PAIR_RADIUS_PIXELS:
            continue
        if abs(best[2] - area) > AREA_TOLERANCE * area:
            continue
        shifts.append(((best[1] - column) * PT_PER_PIXEL, (row - best[0]) * PT_PER_PIXEL))
    return shifts


def line_shifts(source_pdf: str, word_pdf: str) -> list[dict]:
    """per source line: blob count and the largest and mean ink shift, text only (paths removed)."""
    folder = os.path.dirname(os.path.abspath(word_pdf))
    source_text = text_only_copy(source_pdf, os.path.join(folder, "source.text_only.pdf"))
    word_text = text_only_copy(word_pdf, os.path.splitext(word_pdf)[0] + ".text_only.pdf")
    source, word = pdfium.PdfDocument(source_text), pdfium.PdfDocument(word_text)
    rows = []
    try:
        for page_index, box, text in line_boxes(source_pdf):
            if page_index >= len(word):
                continue
            a = _render_box(source, page_index, box)
            b = _render_box(word, page_index, box)
            height, width = min(a.shape[0], b.shape[0]), min(a.shape[1], b.shape[1])
            shifts = blob_shifts(a[:height, :width], b[:height, :width])
            if not shifts:
                continue
            distances = [math.hypot(dx, dy) for dx, dy in shifts]
            rows.append({"page": page_index, "text": text[:40], "blobs": len(shifts),
                         "max_pt": max(distances), "mean_pt": sum(distances) / len(distances),
                         "mean_dx": sum(s[0] for s in shifts) / len(shifts),
                         "mean_dy": sum(s[1] for s in shifts) / len(shifts)})  # fmt: skip
    finally:
        source.close()
        word.close()
    return rows


def summary(rows: list[dict]) -> dict:
    if not rows:
        return {"lines": 0}
    blobs = sum(r["blobs"] for r in rows)
    return {
        "lines": len(rows),
        "blobs": blobs,
        "max_blob_shift_pt": max(r["max_pt"] for r in rows),
        "mean_blob_shift_pt": sum(r["mean_pt"] * r["blobs"] for r in rows) / blobs,
    }
