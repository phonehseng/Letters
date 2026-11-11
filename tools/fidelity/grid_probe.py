"""a one-page probe that tells which coordinate grid word used for a document.

the page holds three kinds of text, all in frames:
    48 short lines whose frame x grows by 1 twip and frame y by 251 twips each,
        so line starts and baselines sweep through every sub-grid offset
    lines of one character repeated with per-character w:spacing chosen so the
        ideal advance is not a multiple of any candidate grid
word's pdf is then tested against candidate grids. a grid "fits" when every
value lands within a small tolerance of one common lattice; the coarsest fitting
grid is reported. pdf writing adds its own noise (5 significant digits for
coordinates, 1/1000 em steps inside a line), which sets the tolerances.
"""

from __future__ import annotations

import os
from collections import defaultdict
from dataclasses import dataclass

import numpy as np

import docx_writer as dw
import test_fonts
from pdf_analysis import extract_chars
from word_export import export_docx_to_pdf
from word_pdf_grid import text_matrices_and_sizes

CANDIDATE_GRIDS_PT = (0.12, 0.06, 0.05, 0.024, 0.012)
SWEEP_COUNT = 48
SWEEP_X0_TWIPS = 1440
SWEEP_Y0_TWIPS = 720
SWEEP_Y_STEP_TWIPS = 251
ADVANCE_LINE_X_TWIPS = 6000
ADVANCE_LINES = (("n", 1), ("n", 3), ("i", 0), ("i", 7))  # (char, w:spacing twips)
ADVANCE_REPEATS = 20
LINE_TWIPS = 240
# line starts and baselines come straight from the text matrix: print rounding only
POSITION_TOLERANCE_PT = 0.006
# pen positions inside a line also carry 1/1000 em rounding from the pdf writer
PEN_TOLERANCE_PT = 0.016


@dataclass
class ProbeLayout:
    """what the probe asked word for, kept to compare with what came out."""

    sweep_x_pt: list[float]
    sweep_top_pt: list[float]
    advance_rows_top_pt: list[float]


def probe_body(family: str, size_pt: float) -> tuple[str, ProbeLayout]:
    paragraphs, sweep_x, sweep_top, advance_top = [], [], [], []
    for k in range(SWEEP_COUNT):
        x_twips = SWEEP_X0_TWIPS + k
        y_twips = SWEEP_Y0_TWIPS + k * SWEEP_Y_STEP_TWIPS
        paragraphs.append(
            dw.frame_paragraph_xml(
                dw.run_xml("nn", family, size_pt),
                x_twips=x_twips,
                y_twips=y_twips,
                width_twips=1440,
                line_twips=LINE_TWIPS,
            )
        )
        sweep_x.append(x_twips / dw.TWIPS_PER_PT)
        sweep_top.append(y_twips / dw.TWIPS_PER_PT)
    for row, (char, spacing) in enumerate(ADVANCE_LINES):
        y_twips = SWEEP_Y0_TWIPS + row * 600
        runs = "".join(dw.run_xml(char, family, size_pt, spacing_twips=spacing) for _ in range(ADVANCE_REPEATS))
        paragraphs.append(
            dw.frame_paragraph_xml(
                runs, x_twips=ADVANCE_LINE_X_TWIPS, y_twips=y_twips, width_twips=5000, line_twips=LINE_TWIPS
            )
        )
        advance_top.append(y_twips / dw.TWIPS_PER_PT)
    return "".join(paragraphs), ProbeLayout(sweep_x, sweep_top, advance_top)


def coarsest_fitting_grid(values: np.ndarray, tolerance: float) -> float | None:
    """the largest candidate grid that all values sit on, allowing one common offset."""
    for grid in CANDIDATE_GRIDS_PT:
        phases = np.mod(values, grid) / grid * 2 * np.pi
        # circular mean, so an offset near 0 and one near grid count as the same
        centre = np.angle(np.mean(np.exp(1j * phases)))
        distance = np.abs(np.angle(np.exp(1j * (phases - centre)))) / (2 * np.pi) * grid
        if distance.max() <= tolerance:
            return grid
    return None


def _rows_by_baseline(pdf_path: str) -> dict[tuple[float, bool], list]:
    """chars grouped per line; the side of the page keeps sweep and advance lines apart."""
    rows = defaultdict(list)
    for char in extract_chars(pdf_path):
        rows[(round(char.origin_y, 3), char.origin_x > ADVANCE_LINE_X_TWIPS / 20 - 1)].append(char)
    return rows


def analyse_probe(pdf_path: str, layout: ProbeLayout, page_height_pt: float = 792.0) -> dict:
    rows = _rows_by_baseline(pdf_path)
    sweep_rows, advance_rows = [], []
    for (baseline, is_advance_line), chars in rows.items():
        (advance_rows if is_advance_line else sweep_rows).append((baseline, chars))
    sweep_rows.sort(key=lambda row: -row[0])
    starts = np.array([min(c.origin_x for c in chars) for _, chars in sweep_rows])
    baselines_from_top = np.array([page_height_pt - baseline for baseline, _ in sweep_rows])
    requested_x = np.array(layout.sweep_x_pt[: len(starts)])
    requested_top = np.array(layout.sweep_top_pt[: len(starts)])
    x_error = starts - requested_x
    baseline_offset = baselines_from_top - requested_top

    pen_relative = []  # steps between neighbouring glyphs, each carries one rounding of the writer
    distinct_steps = {}
    for _, chars in sorted(advance_rows, key=lambda row: -row[0]):
        xs = np.sort([c.origin_x for c in chars])
        pen_relative.extend(np.diff(xs))
        distinct_steps[chars[0].char + f"#{len(distinct_steps)}"] = sorted({round(float(s), 3) for s in np.diff(xs)})
    pen_relative = np.array(pen_relative)
    return {
        "sweep_lines_found": int(len(starts)),
        "line_start_grid_pt": coarsest_fitting_grid(starts, POSITION_TOLERANCE_PT),
        "line_start_error_pt": {"min": float(x_error.min()), "max": float(x_error.max())},
        "baseline_grid_pt": coarsest_fitting_grid(baselines_from_top, POSITION_TOLERANCE_PT),
        "baseline_offset_pt": {"min": float(baseline_offset.min()), "max": float(baseline_offset.max())},
        "pen_grid_pt": coarsest_fitting_grid(pen_relative, PEN_TOLERANCE_PT) if pen_relative.size else None,
        "distinct_steps_pt": distinct_steps,
    }


def run_grid_probe(
    out_dir: str,
    name: str,
    *,
    size_pt: float = 12.0,
    font_path: str | None = None,
    family: str | None = None,
    docx_options: dict | None = None,
    export_options: dict | None = None,
) -> dict:
    """write the probe docx with the given settings, export it, and analyse word's pdf."""
    os.makedirs(out_dir, exist_ok=True)
    font_path = font_path or test_fonts.truetype_font()
    family = family or test_fonts.TRUETYPE_FAMILY
    with open(font_path, "rb") as handle:
        embedded = dw.EmbeddedFont(family=family, font_bytes=handle.read())
    body, layout = probe_body(family, size_pt)
    docx = dw.write_docx(
        os.path.join(out_dir, f"{name}.docx"),
        body,
        family=family,
        default_size_pt=size_pt,
        embedded=embedded,
        **(docx_options or {}),
    )
    pdf = os.path.join(out_dir, f"{name}.word.pdf")
    info = export_docx_to_pdf(docx, pdf, **(export_options or {}))
    result = analyse_probe(pdf, layout)
    result["font_sizes_written"] = text_matrices_and_sizes(pdf)[1]
    result["word"] = info
    return result


def describe(result: dict) -> str:
    def grid(value):
        return "none" if value is None else f"{value:g}"

    steps = "; ".join(f"{k}: {v[:4]}" for k, v in result["distinct_steps_pt"].items())
    return (
        f"line start grid {grid(result['line_start_grid_pt'])} pt "
        f"(x error {result['line_start_error_pt']['min']:+.3f}..{result['line_start_error_pt']['max']:+.3f}), "
        f"baseline grid {grid(result['baseline_grid_pt'])} pt "
        f"(top-to-baseline {result['baseline_offset_pt']['min']:.3f}..{result['baseline_offset_pt']['max']:.3f}), "
        f"pen grid {grid(result['pen_grid_pt'])} pt | Tf {result.get('font_sizes_written')} | steps {steps}"
    )
