"""fidelity harness: does word render a docx where the source pdf puts its glyphs?

usage:
    .venv\\Scripts\\python.exe harness.py source.pdf candidate.docx [--out DIR] [--dpi 300]

word exports the docx to DIR\\<name>.word.pdf, then both pdfs are compared:
pixels at the chosen dpi (difference, ssim, heatmap per page) and per-character
pen origins in points. a short table goes to stdout, everything to DIR\\<name>.report.json.
"""

from __future__ import annotations

import argparse
import json
import os
import time

from compare import TOLERANCE_PT, compare_glyphs, compare_rasters, per_char_deltas
from pdf_analysis import (
    embedded_font_programs,
    extract_chars,
    fonts_used,
    outline_match_fraction,
    page_sizes,
    render_pages_gray,
)
from word_export import export_docx_to_pdf


def run_harness(
    source_pdf: str,
    candidate_docx: str,
    out_dir: str,
    dpi: int = 300,
    keep_pairs: bool = False,
    reference_fonts: list[str] | None = None,
) -> dict:
    os.makedirs(out_dir, exist_ok=True)
    name = os.path.splitext(os.path.basename(candidate_docx))[0]
    exported_pdf = os.path.join(out_dir, f"{name}.word.pdf")
    started = time.perf_counter()

    word_info = export_docx_to_pdf(candidate_docx, exported_pdf)
    after_word = time.perf_counter()

    raster = compare_rasters(
        render_pages_gray(source_pdf, dpi),
        render_pages_gray(exported_pdf, dpi),
        os.path.join(out_dir, f"{name}.heatmap"),
    )
    after_raster = time.perf_counter()

    source_chars = extract_chars(source_pdf)
    candidate_chars = extract_chars(exported_pdf)
    glyphs = compare_glyphs(source_chars, candidate_chars)
    after_glyphs = time.perf_counter()

    report = {
        "source_pdf": os.path.abspath(source_pdf),
        "candidate_docx": os.path.abspath(candidate_docx),
        "word_pdf": os.path.abspath(exported_pdf),
        "word": word_info,
        "page_sizes": {"source": page_sizes(source_pdf), "word": page_sizes(exported_pdf)},
        "raster": raster,
        "glyphs": glyphs,
        "fonts_in_word_pdf": _fonts_with_outline_check(exported_pdf, reference_fonts or []),
        "fonts_in_source_pdf": [_font_row(f) for f in fonts_used(source_pdf)],
        "seconds": {
            "word_total": after_word - started,
            "raster": after_raster - after_word,
            "glyphs": after_glyphs - after_raster,
            "total": after_glyphs - started,
        },
    }
    if keep_pairs:
        report["pairs"] = per_char_deltas(source_chars, candidate_chars)
    report_path = os.path.join(out_dir, f"{name}.report.json")
    with open(report_path, "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2)
    report["report_path"] = report_path
    return report


def _fonts_with_outline_check(pdf_path: str, reference_fonts: list[str]) -> list[dict]:
    """font rows plus, for each reference font file, how many outlines match it."""
    programs = embedded_font_programs(pdf_path)
    rows = []
    for font in fonts_used(pdf_path):
        row = _font_row(font)
        row["outline_match"] = {
            os.path.basename(path): outline_match_fraction(programs.get(font.base_name, b""), path)
            for path in reference_fonts
        }
        rows.append(row)
    return rows


def _font_row(font) -> dict:
    return {
        "base_name": font.base_name,
        "family_name": font.family_name,
        "embedded": font.embedded,
        "program": font.font_program,
        "embedded_family": font.embedded_family,
        "chars": font.char_count,
        "pages": sorted(p + 1 for p in font.pages),
    }


def _fmt(value, digits: int = 4) -> str:
    return "-" if value is None else f"{value:.{digits}f}"


def print_summary(report: dict, label: str | None = None) -> None:
    glyphs = report["glyphs"]
    distance = glyphs["origin_distance_pt"]
    print(f"== {label or os.path.basename(report['candidate_docx'])}")
    print(
        f"   word {report['word']['word_version']}, pages source/word: "
        f"{len(report['page_sizes']['source'])}/{len(report['page_sizes']['word'])}"
    )
    print(
        f"   chars source {glyphs['source_chars']}  word {glyphs['candidate_chars']}  "
        f"matched {glyphs['matched']}  wrong page {glyphs['matched_on_wrong_page']}"
    )
    print(
        f"   origin delta pt   max {_fmt(distance['max'])}  mean {_fmt(distance['mean'])}  p99 {_fmt(distance['p99'])}"
    )
    print(
        f"   |dx| pt           max {_fmt(glyphs['abs_dx_pt']['max'])}  mean {_fmt(glyphs['abs_dx_pt']['mean'])}  "
        f"(signed mean {_fmt(glyphs['mean_dx_pt'])})"
    )
    print(
        f"   |dy| pt           max {_fmt(glyphs['abs_dy_pt']['max'])}  mean {_fmt(glyphs['abs_dy_pt']['mean'])}  "
        f"(signed mean {_fmt(glyphs['mean_dy_pt'])})"
    )
    within = glyphs["within_tolerance_fraction"]
    print(
        f"   within {TOLERANCE_PT} pt: {_fmt(None if within is None else 100 * within, 1)}%  "
        f"font size delta max {_fmt(glyphs['abs_font_size_delta_max'])}"
    )
    for page in report["raster"]["pages"]:
        if "ssim" in page:
            print(
                f"   page {page['page'] + 1}: ssim {page['ssim']:.4f}  mean|diff| {page['mean_abs_diff']:.3f}  "
                f"px>noise {page['pixels_over_noise']}/{page['ink_pixels']} ink"
            )
        else:
            print(f"   page {page['page'] + 1}: missing in {page['missing_in']}")
    for font in report["fonts_in_word_pdf"]:
        print(
            f"   word font: {font['base_name']}  embedded={font['embedded']}  program={font['program']}  "
            f"family-in-program='{font['embedded_family']}'  chars={font['chars']}"
        )
        for reference, fraction in font.get("outline_match", {}).items():
            shown = "n/a (not truetype)" if fraction is None else f"{100 * fraction:.1f}% of its outlines"
            print(f"      outlines found in {reference}: {shown}")
    seconds = report["seconds"]
    print(
        f"   seconds: word {seconds['word_total']:.1f}  raster {seconds['raster']:.1f}  "
        f"glyphs {seconds['glyphs']:.1f}  total {seconds['total']:.1f}"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("source_pdf")
    parser.add_argument("candidate_docx")
    parser.add_argument("--out", default="out\\harness")
    parser.add_argument("--dpi", type=int, default=300)
    parser.add_argument(
        "--font", action="append", default=[], help="font file to check word's embedded outlines against"
    )
    args = parser.parse_args()
    report = run_harness(args.source_pdf, args.candidate_docx, args.out, args.dpi, reference_fonts=args.font)
    print_summary(report)
    print(f"   report: {report['report_path']}")


if __name__ == "__main__":
    main()
