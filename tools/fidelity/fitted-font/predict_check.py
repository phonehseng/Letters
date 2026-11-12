"""can the measuring export be skipped? predicted piece origins against the ones word printed.

with every piece in its own run (so its own pdf text object) and every piece
advance a whole 1/600 in pixel, the round 4 model should predict where word
prints each piece: the frame's start pixel plus the advances before it, put
through the writer's step table (word_writer_model), and the frame's
baseline. this reads the unbaked export (pass 0) of a corpus run and compares.
no word is started.

usage:  ..\\.venv\\Scripts\\python.exe predict_check.py [--mode line|word]
"""

from __future__ import annotations

import fidelity_path

# isort: split

import argparse
import json
import math
import os

from fontTools.ttLib import TTFont

import fitted_convert as fc
import fitted_measure as fm
import fitted_plan as fl
import grid_model as gm
import word_writer_model as wm
from corpus_fitted import manifest
from pdf_glyphs import drawn_glyphs
from prototype import calibration
from prototype import convert as prototype_convert
from prototype.pdf_content import read_document


def predicted_origins(runs: list, cache, page) -> list[tuple[object, float, float]]:
    """(piece, x, y) for every piece of one horizontal line, from the model alone."""
    line = runs[0].line
    line_twips = prototype_convert.line_twips_for(line.max_size)
    placement = calibration.place(cache, line.x, page.height - line.y, line_twips, page.width, page.height)
    table = cache.table("x", wm.page_pixels(page.width))
    y = page.height - placement.baseline_from_top_pt
    pixel = placement.x_pixel
    out = []
    for run in runs:
        font = TTFont(run.items[0].font_path) if run.pieces else None
        for index, piece in enumerate(run.pieces):
            out.append((piece, wm.exported_x(pixel, table), y))
            pixel += fl.piece_advance_units(run, index, font) // gm.UNITS_PER_PIXEL
        if not run.pieces:
            return out  # a hidden run's width is not ours to know; stop predicting this line
    return out


def check(entry: dict, mode: str) -> dict:
    out_dir = os.path.join(fidelity_path.OUT_DIR, f"corpus_{mode}", entry["name"])
    source = os.path.join(fidelity_path.CORPUS_DIR, entry["pdf"].replace("\\", os.sep))
    with open(os.path.join(out_dir, "fitted_report.json"), encoding="utf-8") as handle:
        report = json.load(handle)
    horizontal, turned, _ = fc.source_runs(source, os.path.join(out_dir, "work"), mode)
    all_runs = [run for runs in horizontal + turned for run in runs]
    families = fc.families_of(all_runs)
    fl.name_pieces(families)
    for family in families:
        fl.assign_slots(family)
    pass0 = report["passes"][0]
    face_paths = {f.face_family(face): os.path.join(out_dir, "fonts", f"{f.face_family(face).replace(' ', '_')}"
                  f".{entry['name']}.pass0.ttf") for f in families for face in range(f.faces)}  # fmt: skip
    signatures = fc.piece_signatures(families, face_paths)
    fm.locate_pieces(all_runs, drawn_glyphs(pass0["pdf"]), lambda run, piece: signatures[id(piece)])
    cache = calibration.ModelCache(prototype_convert.word_build())
    pages = read_document(source)
    differences = []
    for runs in horizontal:
        for piece, x, y in predicted_origins(runs, cache, pages[runs[0].line.page]):
            if piece.measured_origin is not None:
                differences.append(math.hypot(piece.measured_origin[0] - x, piece.measured_origin[1] - y))
    exact = sum(d < 0.0005 for d in differences)
    return {"pieces_compared": len(differences), "predicted_exactly": exact,
            "largest_difference_pt": max(differences, default=None)}  # fmt: skip


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mode", choices=("line", "word"), default="line")
    args = parser.parse_args()
    results = {}
    for entry in manifest():
        results[entry["name"]] = check(entry, args.mode)
        print(entry["name"], results[entry["name"]], flush=True)
    with open(os.path.join(fidelity_path.OUT_DIR, f"predict_{args.mode}.json"), "w", encoding="utf-8") as handle:
        json.dump(results, handle, indent=1)


if __name__ == "__main__":
    main()
