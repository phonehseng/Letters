"""e5: place every glyph on word's 1/600 inch grid by prediction, not correction.

recipe per line:
    1. a grid font per font size: hmtx rewritten so every advance is a whole
       1/600 inch pixel at that size (font_prep.make_grid_font)
    2. frame x/y twips chosen by a calibration document (frame_calibration.py),
       because word's twips-to-pixel rule is not a plain round or floor
    3. each glyph's target is the pixel nearest its source x, counted from the
       line start word really produced, and per-glyph w:spacing comes from
       grid_model.spacing_for_pixel_targets
variants measured on both test sets:
    calibrated_plain   steps 2 only, original font, no spacing (what calibration alone gives)
    grid_snap          steps 1-3
the "floor" line is pure arithmetic: distance from every source glyph to the
nearest point of the grid the line start sits on, i.e. the best any grid-bound
method could do.

usage:  .venv\\Scripts\\python.exe e5_grid_snap.py
output: out\\e5\\ (calibration and test docx, word pdfs, reports, e5_summary.json)
"""

from __future__ import annotations

import json
import os

import numpy as np

import docx_writer as dw
import e2_positioning as e2
import e3_advance_widths as e3
import font_prep
import grid_model as gm
import test_fonts
from compare import match_chars
from frame_calibration import FrameRequest, calibrate_frames
from harness import print_summary, run_harness
from pdf_analysis import extract_chars
from test_pdf import PAGE_HEIGHT_PT, TextLine, natural_advances, register_font, write_verified_pdf

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e5")
PDF_FONT_NAME = "FidelityProbeSerif"
LINE_TWIPS = 272
BASELINE_OFFSET_GUESS_PT = 10.91  # e2's frame calibration for a 13.6 pt exact line


def target_advances(line: TextLine) -> list[float]:
    return line.advances if line.advances is not None else natural_advances(PDF_FONT_NAME, line)


def target_origins(line: TextLine) -> list[float]:
    """pen x of every char, spaces included, as the source pdf places them."""
    origins = [line.x]
    for advance in target_advances(line)[:-1]:
        origins.append(origins[-1] + advance)
    return origins


def word_size(line_or_size) -> float:
    """the size the docx stores for a pdf size: the nearest half point."""
    size = line_or_size.size if isinstance(line_or_size, TextLine) else line_or_size
    return gm.nearest_half_point(size)


def grid_font(pdf_size_pt: float) -> tuple[str, str, dict]:
    """one grid font per pdf size: whole-pixel advances at the stored size and,
    for sizes a docx cannot store, outlines scaled to the pdf size."""
    tag = f"{pdf_size_pt:g}".replace(".", "h")
    family = f"Fidelity Grid {tag}"
    test_fonts.assert_not_installed(family)
    path = test_fonts.build_path(f"FidelityGrid-{tag}.ttf")
    info = font_prep.make_grid_font(test_fonts.truetype_font(), path, family, word_size(pdf_size_pt), pdf_size_pt)
    return path, family, info


def position_key(line: TextLine) -> tuple:
    return (round(line.x, 3), round(line.baseline_y, 3), line.size)


def frame_requests(lines: list[TextLine], family_of_size: dict[float, str]) -> list[FrameRequest]:
    """one request per distinct position and size, pages repeat the same positions."""
    requests = {}
    for line in lines:
        key = position_key(line)
        if key not in requests:
            requests[key] = FrameRequest(
                key=key,
                x_pt=line.x,
                baseline_from_top_pt=PAGE_HEIGHT_PT - line.baseline_y,
                size_pt=word_size(line),
                family=family_of_size[line.size],
                sample=line.text.split(" ")[0][:6],
                baseline_offset_guess_pt=BASELINE_OFFSET_GUESS_PT,
                line_twips=LINE_TWIPS,
            )
    return list(requests.values())


def grid_runs(line: TextLine, family: str, font_path: str, start_x_pt: float) -> str:
    advances = font_prep.advance_pixels(font_path, line.text)
    targets = [round(gm.pt_to_pixels(x - start_x_pt)) for x in target_origins(line)]
    targets[0] = 0  # the first glyph is the line start itself
    spacing = gm.spacing_for_pixel_targets(advances, targets)
    return "".join(
        dw.run_xml(char, family, word_size(line), spacing_twips=twips)
        for char, twips in zip(line.text, spacing, strict=True)
    )


def frames_document(path: str, lines: list[TextLine], choices: dict, run_for, fonts: list[dw.EmbeddedFont]) -> str:
    parts = []
    for page in sorted({line.page for line in lines}):
        if page > 0:
            parts.append(dw.page_break_paragraph_xml())
        for line in (line for line in lines if line.page == page):
            choice = choices[position_key(line)]
            parts.append(
                dw.frame_paragraph_xml(
                    run_for(line, choice),
                    x_twips=choice.x_twips,
                    y_twips=choice.y_twips,
                    width_twips=round((sum(target_advances(line)) + 40) * dw.TWIPS_PER_PT),
                    line_twips=LINE_TWIPS,
                )
            )
    return dw.write_docx(
        path, "".join(parts), family=fonts[0].family, default_size_pt=word_size(lines[0]), embedded=fonts
    )


def grid_floor(lines: list[TextLine], choices: dict) -> dict:
    """the error left if every glyph sat exactly on the nearest grid point of its line."""
    distances = []
    for line in lines:
        start = choices[position_key(line)].measured_x_pt
        for index, (char, x) in enumerate(zip(line.text, target_origins(line), strict=True)):
            if char.isspace():
                continue
            # the first glyph cannot move off the line start, the rest snap to its grid
            reachable = start if index == 0 else start + round(gm.pt_to_pixels(x - start)) * gm.PT_PER_PIXEL
            distances.append(abs(reachable - x))
    distances = np.array(distances)
    return {"max": float(distances.max()), "mean": float(distances.mean()), "p99": float(np.percentile(distances, 99))}


def glyph_summary(report: dict) -> dict:
    pairs = report["pairs"]
    distance = np.array([np.hypot(p["dx"], p["dy"]) for p in pairs if p["page"] == p["candidate_page"]])
    dx = np.array([abs(p["dx"]) for p in pairs])
    dy = np.array([abs(p["dy"]) for p in pairs])
    return {
        "max": float(distance.max()),
        "mean": float(distance.mean()),
        "p99": float(np.percentile(distance, 99)),
        "within_0.025": float((distance <= 0.025).mean()),
        "within_0.06": float((distance <= 0.06).mean()),
        "dx_max": float(dx.max()),
        "dx_mean": float(dx.mean()),
        "dy_max": float(dy.max()),
        "dy_mean": float(dy.mean()),
    }


def glyph_scale_by_size(source_pdf: str, word_pdf: str) -> dict:
    """how large word draws the glyphs, from ink box heights of matched glyphs.

    pdfium's font size is the Tf operand and says nothing about scaled
    outlines, the ink boxes do. the median of word/source height per source size
    gives the drawn size."""
    pairs = match_chars(extract_chars(source_pdf), extract_chars(word_pdf))
    ratios = {}
    for source, word in pairs:
        source_height = source.box[3] - source.box[2]
        if source_height > 0.5 * source.font_size:
            ratios.setdefault(round(source.font_size, 2), []).append((word.box[3] - word.box[2]) / source_height)
    result = {}
    for size, values in sorted(ratios.items()):
        drawn = float(np.median(values)) * size
        result[f"{size:g}"] = {"drawn_pt": drawn, "error_pt": drawn - size, "glyphs": len(values)}
    return result


def print_glyph_summary(label: str, stats: dict) -> None:
    print(
        f"   {label}: max {stats['max']:.4f}  mean {stats['mean']:.4f}  p99 {stats['p99']:.4f}  "
        f"<=0.025 {100 * stats['within_0.025']:.1f}%  <=0.06 {100 * stats['within_0.06']:.1f}%  "
        f"|dx| max {stats['dx_max']:.4f} mean {stats['dx_mean']:.4f}  "
        f"|dy| max {stats['dy_max']:.4f} mean {stats['dy_mean']:.4f}"
    )


def run_set(name: str, lines: list[TextLine], group_of: dict | None) -> dict:
    out_dir = os.path.join(OUT_DIR, name)
    os.makedirs(out_dir, exist_ok=True)
    source = write_verified_pdf(os.path.join(out_dir, f"{name}_source.pdf"), PDF_FONT_NAME, lines)
    sizes = sorted({line.size for line in lines})
    grid_fonts = {size: grid_font(size) for size in sizes}
    base_path = test_fonts.truetype_font()
    base = dw.EmbeddedFont(test_fonts.TRUETYPE_FAMILY, open(base_path, "rb").read())
    embedded_grid = [dw.EmbeddedFont(family, open(path, "rb").read()) for path, family, _ in grid_fonts.values()]
    summary = {
        "grid_font_bytes": {f"{size:g}": os.path.getsize(path) for size, (path, _, _) in grid_fonts.items()},
        "grid_font_size_error_pt": {f"{size:g}": info["size_error_pt"] for size, (_, _, info) in grid_fonts.items()},
        "base_font_bytes": os.path.getsize(base_path),
    }

    plain_calibration = calibrate_frames(
        frame_requests(lines, {size: base.family for size in sizes}), out_dir, f"{name}_calibration_plain", [base]
    )
    grid_calibration = calibrate_frames(
        frame_requests(lines, {size: grid_fonts[size][1] for size in sizes}),
        out_dir,
        f"{name}_calibration_grid",
        embedded_grid,
    )
    for label, calibration in (("plain", plain_calibration), ("grid", grid_calibration)):
        print(
            f"{name} calibration {label}: best line start |x error| max {calibration['x_error_max']:.4f} pt, "
            f"best baseline |y error| max {calibration['y_error_max']:.4f} pt, "
            f"candidates at the edge {calibration['edge_hits']}"
        )
        summary[f"calibration_{label}"] = {k: v for k, v in calibration.items() if k != "choices"}

    floor = grid_floor(lines, grid_calibration["choices"])
    print(
        f"{name} arithmetic floor (nearest grid point per glyph): "
        f"max {floor['max']:.4f} mean {floor['mean']:.4f} p99 {floor['p99']:.4f}"
    )
    summary["floor"] = floor

    variants = {
        "calibrated_plain": (
            plain_calibration["choices"],
            lambda line, choice: dw.run_xml(line.text, base.family, word_size(line)),
            [base],
        ),
        "grid_snap": (
            grid_calibration["choices"],
            lambda line, choice: grid_runs(
                line, grid_fonts[line.size][1], grid_fonts[line.size][0], choice.measured_x_pt
            ),
            embedded_grid,
        ),
    }
    for variant, (choices, run_for, fonts) in variants.items():
        docx = frames_document(os.path.join(out_dir, f"{name}_{variant}.docx"), lines, choices, run_for, fonts)
        report = run_harness(source, docx, out_dir, keep_pairs=True)
        print_summary(report, label=f"e5 {name} {variant}")
        stats = glyph_summary(report)
        print_glyph_summary("all glyphs", stats)
        stats["glyph_scale"] = glyph_scale_by_size(source, report["word_pdf"])
        for size, scale in stats["glyph_scale"].items():
            print(f"   glyph size at {size} pt: drawn {scale['drawn_pt']:.3f} pt, error {scale['error_pt']:+.4f} pt")
        entry = {"glyphs": stats, "docx_bytes": os.path.getsize(docx), "seconds": report["seconds"]}
        if group_of is not None:
            groups = e3.group_stats(report["pairs"], lines, group_of)
            e3.print_group_table(variant, groups)
            entry["groups"] = groups
        else:
            analysis = e2.analyse_pairs(report["pairs"], lines)
            drift = analysis["drift_along_line_abs"]
            print(f"   drift along a line: max {drift['max']:.4f} mean {drift['mean']:.4f}")
            entry["analysis"] = analysis
        summary[variant] = entry
    return summary


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    register_font(test_fonts.truetype_font(), PDF_FONT_NAME)
    e3_lines, group_of = e3.build_lines()
    summary = {
        "e3_set": run_set("e3set", e3_lines, group_of),
        "e2_set": run_set("e2set", e2.test_lines(), None),
    }
    with open(os.path.join(OUT_DIR, "e5_summary.json"), "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)


if __name__ == "__main__":
    main()
