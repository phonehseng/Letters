"""recover word's layout pixels from a pdf word exported.

a word-made pdf prints every text object's Tm on the writer lattice, so the
layout pixel behind it is recovered exactly by inverting word_writer_model
(decode_x, decode_y). glyphs after the first in a text object are placed by Tc
and TJ, which word rounds to 1/1000 em and only corrects past about 3/1000 em;
their layout pixel is estimated as the nearest whole pixel from the Tm, with
the leftover reported as writer error.

the converter does not need these pixels to plan (it aims at the printed
source positions, which is what is measured), but they show how much of a
word-made source is exact lattice and how much is writer noise.

usage:  .venv\\Scripts\\python.exe -m prototype.source_pixels corpus\\NAME.pdf
"""

from __future__ import annotations

import sys
from dataclasses import dataclass

import numpy as np
import pikepdf

import word_writer_model as wm
from word_pdf_raw import read_text_objects


@dataclass
class SourcePixel:
    page: int
    pixel_x: int | None  # exact for a text object's first glyph, estimated inside one
    baseline_pixel: int | None
    first_in_object: bool
    writer_error_px: float  # printed minus layout pixel, inside a text object (0 for first glyphs)


def recover(pdf_path: str, tables: dict[str, wm.StepTable]) -> list[SourcePixel]:
    with pikepdf.open(pdf_path) as pdf:
        boxes = [[float(v) for v in page.mediabox] for page in pdf.pages]
    rows = []
    for obj in read_text_objects(pdf_path):
        if abs(obj.tm[1]) > 1e-6 or abs(obj.tm[2]) > 1e-6:
            continue  # rotated text objects are not on the lattice
        box = boxes[obj.page]
        width_px, height_px = wm.page_pixels(box[2] - box[0]), wm.page_pixels(box[3] - box[1])
        x_table, y_table = tables.get(f"x:{width_px}"), tables.get(f"y:{height_px}")
        if x_table is None or y_table is None:
            continue
        start = wm.decode_x(obj.x, x_table)
        baseline = wm.decode_y(obj.y, y_table, height_px)
        for index, pen in enumerate(obj.pens()):
            if index == 0:
                rows.append(SourcePixel(obj.page, start, baseline, True, 0.0))
                continue
            offset_px = pen / wm.PT_PER_PIXEL
            estimate = round(offset_px)
            rows.append(SourcePixel(obj.page, None if start is None else start + estimate, baseline, False,
                                    offset_px - estimate))  # fmt: skip
    return rows


def main() -> None:
    from prototype.calibration import ModelCache
    from prototype.convert import word_build

    cache = ModelCache(word_build())
    tables = {key: wm.StepTable(tuple(steps)) for key, steps in cache.steps.items()}
    rows = recover(sys.argv[1], tables)
    firsts = [r for r in rows if r.first_in_object]
    inner = np.array([abs(r.writer_error_px) for r in rows if not r.first_in_object])
    decoded = sum(1 for r in firsts if r.pixel_x is not None and r.baseline_pixel is not None)
    print(
        f"{len(rows)} glyphs, {len(firsts)} start a text object: {decoded} decode exactly to a layout pixel (x and y)"
    )
    if inner.size:
        print(f"{inner.size} glyphs inside text objects: writer error max {inner.max():.3f} px, mean {inner.mean():.3f} px,"
              f" over 0.5 px (pixel not recoverable by rounding): {(inner > 0.5).sum()}")  # fmt: skip


if __name__ == "__main__":
    main()
