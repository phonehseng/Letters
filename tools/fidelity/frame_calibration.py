"""choosing frame x/y twips by measurement instead of by formula.

word maps a frame's twips to pdf positions on a 1/600 inch grid, by a rule the
probes could not pin down (it is deterministic, but not a plain floor or round of
twips * 5/12). a twip is 0.4167 pixel, so a few neighbouring twip values always
include the pixel nearest to the target. the calibration document places a short
sample of every line at each candidate offset, one candidate per page, and the
best candidate per line is chosen independently for x and for y.
"""

from __future__ import annotations

import os
from collections import defaultdict
from dataclasses import dataclass

import numpy as np

import docx_writer as dw
from pdf_analysis import extract_chars
from test_pdf import PAGE_HEIGHT_PT
from word_export import export_docx_to_pdf

CANDIDATE_OFFSETS_TWIPS = range(-4, 5)


@dataclass
class FrameRequest:
    """one positioned line: where its first pen origin and baseline must land."""

    key: tuple
    x_pt: float
    baseline_from_top_pt: float
    size_pt: float
    family: str
    sample: str  # a few leading chars of the line, enough to find it again
    baseline_offset_guess_pt: float
    line_twips: int


@dataclass
class FrameChoice:
    x_twips: int
    y_twips: int
    measured_x_pt: float
    measured_baseline_from_top_pt: float


def base_twips(request: FrameRequest) -> tuple[int, int]:
    x = round(request.x_pt * dw.TWIPS_PER_PT)
    y = round((request.baseline_from_top_pt - request.baseline_offset_guess_pt) * dw.TWIPS_PER_PT)
    return x, y


def calibration_body(requests: list[FrameRequest]) -> str:
    parts = []
    for page, offset in enumerate(CANDIDATE_OFFSETS_TWIPS):
        if page > 0:
            parts.append(dw.page_break_paragraph_xml())
        for request in requests:
            x, y = base_twips(request)
            parts.append(
                dw.frame_paragraph_xml(
                    dw.run_xml(request.sample, request.family, request.size_pt),
                    x_twips=x + offset,
                    y_twips=y + offset,
                    width_twips=2000,
                    line_twips=request.line_twips,
                )
            )
    return "".join(parts)


def _line_starts_by_page(pdf_path: str) -> dict[int, list[tuple[float, float]]]:
    """(first pen x, baseline from top) of every line, page by page."""
    rows = defaultdict(list)
    for char in extract_chars(pdf_path):
        rows[(char.page, round(char.origin_y, 3))].append(char.origin_x)
    starts = defaultdict(list)
    for (page, baseline), xs in rows.items():
        xs = sorted(xs)
        # two lines can share a baseline, a gap wider than a sample splits them
        group_start = xs[0]
        for previous, current in zip(xs, xs[1:], strict=False):
            if current - previous > 40:
                starts[page].append((group_start, PAGE_HEIGHT_PT - baseline))
                group_start = current
        starts[page].append((group_start, PAGE_HEIGHT_PT - baseline))
    return starts


def calibrate_frames(requests: list[FrameRequest], out_dir: str, name: str, fonts: list[dw.EmbeddedFont]) -> dict:
    os.makedirs(out_dir, exist_ok=True)
    docx = dw.write_docx(
        os.path.join(out_dir, f"{name}.docx"),
        calibration_body(requests),
        family=fonts[0].family,
        default_size_pt=requests[0].size_pt,
        embedded=fonts,
    )
    pdf = os.path.join(out_dir, f"{name}.word.pdf")
    export_docx_to_pdf(docx, pdf)
    starts = _line_starts_by_page(pdf)
    choices = {}
    edge_hits = 0
    for request in requests:
        x0, y0 = base_twips(request)
        measured = []
        for page, offset in enumerate(CANDIDATE_OFFSETS_TWIPS):
            expected_x = (x0 + offset) / dw.TWIPS_PER_PT
            expected_baseline = (y0 + offset) / dw.TWIPS_PER_PT + request.baseline_offset_guess_pt
            found = min(
                starts[page], key=lambda s: abs(s[0] - expected_x) + abs(s[1] - expected_baseline), default=None
            )
            measured.append((offset, found))
        best_x = min(measured, key=lambda m: abs(m[1][0] - request.x_pt))
        best_y = min(measured, key=lambda m: abs(m[1][1] - request.baseline_from_top_pt))
        if best_y[0] in (CANDIDATE_OFFSETS_TWIPS[0], CANDIDATE_OFFSETS_TWIPS[-1]):
            edge_hits += 1
        choices[request.key] = FrameChoice(x0 + best_x[0], y0 + best_y[0], best_x[1][0], best_y[1][1])
    x_errors = np.array([c.measured_x_pt - r.x_pt for r, c in ((r, choices[r.key]) for r in requests)])
    y_errors = np.array(
        [c.measured_baseline_from_top_pt - r.baseline_from_top_pt for r, c in ((r, choices[r.key]) for r in requests)]
    )
    return {
        "choices": choices,
        "x_error_max": float(np.abs(x_errors).max()),
        "y_error_max": float(np.abs(y_errors).max()),
        "edge_hits": edge_hits,
    }
