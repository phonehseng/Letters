"""the best any plan can do through word: each source glyph's distance to the nearest exported position.

word prints a glyph that starts a text object on a lattice (word_writer_model):
x = printed(0.024 * (5 p + k(p))) for a layout pixel p, and the same for y from
the top edge. a converter can choose p freely, so the distance from a source
position to the nearest lattice point is the floor of the error for that
glyph, whatever the converter does. a class whose achieved numbers sit on its
floor has no error left that is the converter's.

usage:  .venv\\Scripts\\python.exe -m prototype.floor corpus\\NAME.pdf
"""

from __future__ import annotations

import math
import sys

import numpy as np

import word_writer_model as wm
from prototype.lines import ANGLE_TOLERANCE_DEG
from prototype.pdf_content import read_document


def nearest_exported_y(pdf_y: float, table: wm.StepTable, page_height_pixels: int) -> float:
    from_top = page_height_pixels * wm.PT_PER_PIXEL - pdf_y
    guess = round(from_top / wm.PT_PER_PIXEL)
    return min((wm.exported_y(p, table, page_height_pixels) for p in range(guess - 2, guess + 3)),
               key=lambda y: abs(y - pdf_y))  # fmt: skip


def glyph_floors(pdf_path: str, tables: dict[str, wm.StepTable]) -> list[dict]:
    """per visible horizontal glyph: x, y and combined floor in points.

    tables maps "x:<px>" and "y:<px>" to the writer's step tables for the page sizes."""
    rows = []
    for page in read_document(pdf_path):
        width_px, height_px = wm.page_pixels(page.width), wm.page_pixels(page.height)
        x_table, y_table = tables[f"x:{width_px}"], tables[f"y:{height_px}"]
        for glyph in page.glyphs:
            if not glyph.text.strip() or abs(glyph.angle) > ANGLE_TOLERANCE_DEG or glyph.render_mode in (3, 7):
                continue
            dx = wm.lattice_error(glyph.x, x_table)
            dy = abs(nearest_exported_y(glyph.y, y_table, height_px) - glyph.y)
            rows.append({"page": page.index, "x": glyph.x, "y": glyph.y, "dx": dx, "dy": dy, "d": math.hypot(dx, dy)})
    return rows


def summary(rows: list[dict]) -> dict:
    if not rows:
        return {}
    d = np.array([r["d"] for r in rows])
    dx = np.array([r["dx"] for r in rows])
    dy = np.array([r["dy"] for r in rows])
    return {"glyphs": int(d.size), "max": float(d.max()), "mean": float(d.mean()), "x_max": float(dx.max()),
            "x_mean": float(dx.mean()), "y_max": float(dy.max()), "y_mean": float(dy.mean())}  # fmt: skip


def main() -> None:
    from prototype.calibration import ModelCache
    from prototype.convert import word_build

    cache = ModelCache(word_build())
    tables = {key: wm.StepTable(tuple(steps)) for key, steps in cache.steps.items()}
    print(summary(glyph_floors(sys.argv[1], tables)))


if __name__ == "__main__":
    main()
