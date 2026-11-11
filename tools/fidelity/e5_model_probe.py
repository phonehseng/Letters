"""e5 probe: does word follow grid_model's floor rule for whole-pixel advances plus w:spacing?

for each size, a grid font (every advance a whole 1/600 inch pixel) sets lines of
mixed text in frames. each line asks for random whole-pixel pen offsets, the
spacing comes from grid_model.spacing_for_pixel_targets, and word's pdf is read
back. if the model holds, every glyph lands on its target pixel, apart from the
pdf writer's own rounding inside a line (1/1000 em, about 0.1 pixel).

usage:  .venv\\Scripts\\python.exe e5_model_probe.py
output: out\\e5_probe\\ and a table per size and mode
"""

from __future__ import annotations

import os
import random
from collections import defaultdict

import numpy as np

import docx_writer as dw
import font_prep
import grid_model as gm
import test_fonts
from pdf_analysis import extract_chars
from test_pdf import sample_text
from word_export import export_docx_to_pdf

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e5_probe")
SIZES = (9.0, 10.5, 12.0)
LINES_PER_MODE = 6
MODES = ("plain", "jitter")


def grid_font_for(size_pt: float) -> tuple[str, str]:
    family = f"Fidelity Grid {size_pt:g}".replace(".", "h")
    test_fonts.assert_not_installed(family)
    path = test_fonts.build_path(f"FidelityGrid-{size_pt:g}".replace(".", "h") + ".ttf")
    font_prep.make_grid_font(test_fonts.truetype_font(), path, family, size_pt)
    return path, family


def plan_line(text: str, advances: list[int], mode: str, rng: random.Random) -> tuple[list[int], list[int]]:
    """target pixel offsets and the spacing that should produce them."""
    targets = [0]
    for advance in advances[:-1]:
        extra = 0 if mode == "plain" else rng.randint(-2, 3)
        targets.append(targets[-1] + advance + extra)
    return targets, gm.spacing_for_pixel_targets(advances, targets)


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    rng = random.Random(5)
    for size in SIZES:
        font_path, family = grid_font_for(size)
        with open(font_path, "rb") as handle:
            embedded = dw.EmbeddedFont(family=family, font_bytes=handle.read())
        paragraphs, plans = [], []
        for row in range(LINES_PER_MODE * len(MODES)):
            mode = MODES[row // LINES_PER_MODE]
            text = sample_text(700 + row, 6)
            advances = font_prep.advance_pixels(font_path, text)
            targets, spacing = plan_line(text, advances, mode, rng)
            runs = "".join(
                dw.run_xml(char, family, size, spacing_twips=twips) for char, twips in zip(text, spacing, strict=True)
            )
            y_twips = 720 + row * 400
            paragraphs.append(
                dw.frame_paragraph_xml(runs, x_twips=1440, y_twips=y_twips, width_twips=10000, line_twips=300)
            )
            plans.append((mode, text, targets))
        name = f"model_{size:g}pt".replace(".", "h")
        docx = dw.write_docx(
            os.path.join(OUT_DIR, f"{name}.docx"),
            "".join(paragraphs),
            family=family,
            default_size_pt=size,
            embedded=embedded,
        )
        pdf = os.path.join(OUT_DIR, f"{name}.word.pdf")
        export_docx_to_pdf(docx, pdf)
        report(pdf, plans, size)


def report(pdf: str, plans, size: float) -> None:
    rows = defaultdict(list)
    for char in extract_chars(pdf):
        rows[round(char.origin_y, 3)].append(char.origin_x)
    measured = [np.sort(xs) for _, xs in sorted(rows.items(), key=lambda item: -item[0])]
    tf_sizes = set()
    by_mode = defaultdict(list)
    for (mode, text, targets), xs in zip(plans, measured, strict=True):
        wanted = np.array([t for t, c in zip(targets, text, strict=True) if not c.isspace()], dtype=float)
        got = (xs - xs[0]) / gm.PT_PER_PIXEL
        by_mode[mode].extend(np.abs(got - wanted))
    for mode, errors in by_mode.items():
        errors = np.array(errors)
        print(
            f"{size:>5} pt {mode:<7} glyphs {errors.size}: |pen - target| max {errors.max():.3f} px "
            f"({errors.max() * gm.PT_PER_PIXEL:.4f} pt) mean {errors.mean():.3f} px, "
            f"exact (<0.15 px) {100 * (errors < 0.15).mean():.1f}%, off by >=0.5 px {100 * (errors >= 0.5).mean():.1f}%"
        )
    del tf_sizes


if __name__ == "__main__":
    main()
