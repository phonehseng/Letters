"""e2: how exactly can word place lines of text at awkward fractional positions?

three ways of placing the same lines (see placement.py):
    frames      w:framePr, page-relative x/y in twips
    textboxes   drawingml wps text boxes anchored to the page, offsets in emu
    flow        ordinary paragraphs: indent, tab stops and spacing before, in twips

word puts the top of a line box where we ask, the pdf puts the baseline, so a
calibration pass first measures the top-to-baseline distance per method and size
on a separate one-page pdf with different positions. the test run then uses it.

usage:  .venv\\Scripts\\python.exe e2_positioning.py
output: out\\e2\\  (calibration.json, e2_summary.json, reports and heatmaps per method)
"""

from __future__ import annotations

import json
import os
from collections import defaultdict

import numpy as np
from reportlab.pdfbase import pdfmetrics

import docx_writer as dw
import placement
import test_fonts
from harness import print_summary, run_harness
from test_pdf import TextLine, register_font, sample_text, write_verified_pdf

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e2")
# 9 pt is a whole number of 1/600 inch units, 10.5 pt is not; both are legal word sizes
SIZES = (9.0, 10.5)
PDF_FONT_NAME = "FidelityProbeSerif"


def test_lines() -> list[TextLine]:
    """three pages, two columns, rows 13.61 pt apart starting at an awkward baseline."""
    lines = []
    index = 0
    for page in range(3):
        row = 0
        baseline = 731.47
        while baseline > 60:
            size = SIZES[row % 2]
            left_x = 72.37 + (row % 5) * 3.17
            right_x = 318.91 + (row % 3) * 1.33
            lines.append(TextLine(page, left_x, round(baseline, 4), size, sample_text(index, 4)))
            lines.append(TextLine(page, right_x, round(baseline, 4), size, sample_text(index + 1, 4)))
            index += 2
            row += 1
            baseline -= 13.61
    return lines


def calibration_lines() -> list[TextLine]:
    """one page at different positions, so calibration is not fitted to the test itself."""
    lines = []
    for row in range(24):
        baseline = round(700.13 - 17.29 * row, 4)
        size = SIZES[row % 2]
        lines.append(TextLine(0, 100.13, baseline, size, sample_text(500 + row, 3)))
        lines.append(TextLine(0, 350.77, baseline, size, sample_text(600 + row, 3)))
    return lines


def make_docx(path: str, method: str, lines: list[TextLine], font_file: str, offsets: dict[float, float]) -> str:
    family = test_fonts.TRUETYPE_FAMILY
    with open(font_file, "rb") as handle:
        embedded = dw.EmbeddedFont(family=family, font_bytes=handle.read())

    def width_of(line: TextLine) -> float:
        return pdfmetrics.stringWidth(line.text, PDF_FONT_NAME, line.size)

    body = placement.build_body(method, lines, placement.plain_runs(family), width_of, offsets)
    return dw.write_docx(path, body, family=family, default_size_pt=SIZES[0], embedded=embedded)


def _line_starts(lines: list[TextLine]) -> set[tuple]:
    return {(line.page, round(line.x, 3), round(line.baseline_y, 3)) for line in lines}


def analyse_pairs(pairs: list[dict], lines: list[TextLine]) -> dict:
    """split the error into line placement and drift along each line, and look for trends."""
    starts = _line_starts(lines)
    same_page = [p for p in pairs if p["page"] == p["candidate_page"]]
    first_chars = [p for p in same_page if (p["page"], round(p["source_x"], 3), round(p["source_y"], 3)) in starts]
    line_x0 = {}
    for p in first_chars:
        line_x0[(p["page"], round(p["source_y"], 3), round(p["source_x"], 3))] = p["dx"]

    # drift along a line: dx of each char minus dx of the first char of its line
    by_line = defaultdict(list)
    for p in same_page:
        by_line[(p["page"], round(p["source_y"], 3), p["source_x"] >= 300)].append(p)
    along_line = []
    for chars in by_line.values():
        chars.sort(key=lambda p: p["source_x"])
        along_line.extend(abs(p["dx"] - chars[0]["dx"]) for p in chars)
    along_line = np.array(along_line)

    first_dx = np.array([p["dx"] for p in first_chars])
    first_dy = np.array([p["dy"] for p in first_chars])
    per_page = {}
    for page in sorted({p["page"] for p in first_chars}):
        on_page = [p for p in first_chars if p["page"] == page]
        y = np.array([p["source_y"] for p in on_page])
        dy = np.array([p["dy"] for p in on_page])
        slope = float(np.polyfit(792 - y, dy, 1)[0]) if len(on_page) > 2 else None
        per_page[page + 1] = {
            "line_starts": len(on_page),
            "mean_dy": float(dy.mean()),
            "max_abs_dy": float(np.abs(dy).max()),
            "dy_slope_pt_per_100pt_down": None if slope is None else slope * 100,
        }
    return {
        "line_start_count": len(first_chars),
        "line_start_abs_dx": {"max": float(np.abs(first_dx).max()), "mean": float(np.abs(first_dx).mean())},
        "line_start_abs_dy": {"max": float(np.abs(first_dy).max()), "mean": float(np.abs(first_dy).mean())},
        "drift_along_line_abs": {
            "max": float(along_line.max()),
            "mean": float(along_line.mean()),
            "p99": float(np.percentile(along_line, 99)),
        },
        "per_page": per_page,
        "dy_by_size": _dy_by_size(first_chars, lines),
    }


def _dy_by_size(first_chars: list[dict], lines: list[TextLine]) -> dict:
    size_of = {(line.page, round(line.x, 3), round(line.baseline_y, 3)): line.size for line in lines}
    groups = defaultdict(list)
    for p in first_chars:
        groups[size_of[(p["page"], round(p["source_x"], 3), round(p["source_y"], 3))]].append(p["dy"])
    return {str(size): {"mean": float(np.mean(v)), "spread": float(np.ptp(v))} for size, v in groups.items()}


def calibrate(font_file: str) -> dict:
    lines = calibration_lines()
    source = write_verified_pdf(os.path.join(OUT_DIR, "calibration_source.pdf"), PDF_FONT_NAME, lines)
    calibration = {}
    for method in placement.METHODS:
        docx = make_docx(os.path.join(OUT_DIR, f"calibration_{method}.docx"), method, lines, font_file, {})
        report = run_harness(source, docx, OUT_DIR, keep_pairs=True)
        print_summary(report, label=f"e2 calibration {method} (no baseline offset)")
        by_size = analyse_pairs(report["pairs"], lines)["dy_by_size"]
        # with no offset word puts the line top at the pdf baseline, so the
        # baseline lands lower by exactly the offset we are after
        calibration[method] = {size: -stats["mean"] for size, stats in by_size.items()}
        print(
            f"   measured top-to-baseline offsets: {calibration[method]}  (spread per size: "
            f"{ {s: round(v['spread'], 4) for s, v in by_size.items()} })"
        )
    return calibration


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    font_file = test_fonts.truetype_font()
    register_font(font_file, PDF_FONT_NAME)
    calibration = calibrate(font_file)
    with open(os.path.join(OUT_DIR, "calibration.json"), "w", encoding="utf-8") as handle:
        json.dump(calibration, handle, indent=2)

    lines = test_lines()
    source = write_verified_pdf(os.path.join(OUT_DIR, "e2_source.pdf"), PDF_FONT_NAME, lines)
    print(f"e2 test pdf: {len(lines)} lines on 3 pages, {sum(len(line.text) for line in lines)} characters")
    summary = {"calibration": calibration}
    for method in placement.METHODS:
        offsets = {float(size): value for size, value in calibration[method].items()}
        docx = make_docx(os.path.join(OUT_DIR, f"e2_{method}.docx"), method, lines, font_file, offsets)
        report = run_harness(source, docx, OUT_DIR, keep_pairs=True)
        print_summary(report, label=f"e2 {method} (calibrated)")
        analysis = analyse_pairs(report["pairs"], lines)
        start_dx, start_dy = analysis["line_start_abs_dx"], analysis["line_start_abs_dy"]
        print(
            f"   line starts: |dx| max {start_dx['max']:.4f} mean {start_dx['mean']:.4f}"
            f"   |dy| max {start_dy['max']:.4f} mean {start_dy['mean']:.4f}"
        )
        drift = analysis["drift_along_line_abs"]
        print(
            f"   drift along a line (dx minus line start dx): "
            f"max {drift['max']:.4f} mean {drift['mean']:.4f} p99 {drift['p99']:.4f}"
        )
        for page, stats in analysis["per_page"].items():
            print(
                f"   page {page}: mean dy {stats['mean_dy']:+.4f}  max|dy| {stats['max_abs_dy']:.4f}  "
                f"dy trend {stats['dy_slope_pt_per_100pt_down']:+.5f} pt per 100 pt down the page"
            )
        summary[method] = {
            "glyphs": report["glyphs"],
            "analysis": analysis,
            "ssim": [p.get("ssim") for p in report["raster"]["pages"]],
            "word_pages": report["word"]["word_pages"],
            "seconds": report["seconds"],
        }
    with open(os.path.join(OUT_DIR, "e2_summary.json"), "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)


if __name__ == "__main__":
    main()
