"""pdf -> docx with a fitted document font: the prototype's docx, its text re-set in pre-composed glyphs.

1. the round 4 prototype converts the pdf (frames, pictures, shapes, rotated
   boxes, grid fonts), with its checking export switched off: the fitted
   pass does its own measuring.
2. every line's characters are grouped into runs and pieces (fitted_pieces);
   every piece becomes one composite glyph and one gsub rule (fitted_plan).
3. measure: word exports the docx, every piece's glyph is found in the pdf and
   its printed origin read (fitted_measure).
4. bake: the composites are rebuilt with offsets measured from that origin.
   only outlines change, never an advance, so word's layout cannot move.
5. word exports again and every source glyph's ink position is checked.

usage:  ..\\.venv\\Scripts\\python.exe fitted_convert.py input.pdf out_dir [--mode line|word]
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import argparse
import contextlib
import json
import os
import time
from collections import defaultdict

from fontTools.ttLib import TTFont

import fitted_font as ff
import fitted_measure as fm
import fitted_pieces as fp
import fitted_plan as fl
from fitted_package import PrototypePackage
from pdf_glyphs import drawn_glyphs
from prototype import calibration, lines, rotated, verify_pass
from prototype import convert as prototype_convert
from prototype.pdf_content import read_document
from word_session import WordSession

MEASURE_PASSES = 2  # one export to find where word puts each glyph, one with the residual baked in


@contextlib.contextmanager
def exports_through(session: WordSession):
    """the prototype's own word exports go through our session, so we know every word pid."""

    def export(docx: str, pdf: str, **_):
        return {"timings": {"export_s": session.export(docx, pdf)}, "word_version": session.version}

    modules = (calibration, rotated, verify_pass)
    saved = [module.export_docx_to_pdf for module in modules]
    for module in modules:
        module.export_docx_to_pdf = export
    try:
        yield
    finally:
        for module, original in zip(modules, saved, strict=True):
            module.export_docx_to_pdf = original


def source_runs(pdf_path: str, work_dir: str, mode: str) -> tuple[list, list, int]:
    """(horizontal lines' runs, rotated lines' runs, visible source glyphs) in the prototype's order."""
    pages = read_document(pdf_path)
    glyphs = [g for page in pages for g in page.glyphs]
    tag = os.path.splitext(os.path.basename(pdf_path))[0]
    # the same tag gives the same family names as the prototype's run (font_rebuild.unique_family)
    plan = prototype_convert.FontPlan(glyphs, work_dir, tag, {"fonts": []})
    horizontal, turned = [], []
    for line in lines.group_lines(glyphs):
        runs = fp.split_runs(line, fp.line_items(line, plan))
        for run in runs:
            fp.make_pieces(run, mode)
        target = turned if abs(line.angle) > lines.ANGLE_TOLERANCE_DEG else horizontal
        target.append(runs)
    visible = sum(1 for g in glyphs if g.text.strip() and g.render_mode not in (3, 7))
    return horizontal, turned, visible


def families_of(all_runs: list[fp.Run]) -> list[fl.FittedFamily]:
    families: dict[str, fl.FittedFamily] = {}
    for run in all_runs:
        first = run.items[0]
        families.setdefault(first.family, fl.FittedFamily(first.family, first.font_path)).runs.append(run)
    return list(families.values())


def check_texts(package: PrototypePackage, horizontal: list, turned: list) -> None:
    frames, boxes = package.line_paragraphs()
    for paragraphs, line_runs, kind in ((frames, horizontal, "frames"), (boxes, turned, "rotated boxes")):
        if len(paragraphs) != len(line_runs):
            raise RuntimeError(f"the prototype wrote {len(paragraphs)} {kind}, we found {len(line_runs)} lines")
        for paragraph, runs in zip(paragraphs, line_runs, strict=True):
            ours = "".join(run.text for run in runs)
            theirs = PrototypePackage.paragraph_text(paragraph)
            if ours != theirs:
                raise RuntimeError(f"line text differs from the prototype's: {theirs!r} vs {ours!r}")


def write_pass(package, families, horizontal, turned, out_dir: str, name: str) -> tuple[str, dict]:
    """fonts and runs for one pass; returns the docx path and the face font files by family name."""
    face_paths = {}
    for family in families:
        for face_family, path in fl.build_family_fonts(family, os.path.join(out_dir, "fonts"), name):
            with open(path, "rb") as handle:
                package.set_font(face_family, handle.read())
            face_paths[face_family] = path
    family_of = {id(run): family for family in families for run in family.runs}

    def family_name(run: fp.Run) -> str:
        return family_of[id(run)].face_family(run.face)

    frames, boxes = package.line_paragraphs()
    for paragraph, runs in zip(frames + boxes, horizontal + turned, strict=True):
        package.replace_runs(paragraph, runs, family_name)
    return package.save(os.path.join(out_dir, f"{name}.docx")), face_paths


def piece_signatures(families, face_paths: dict) -> dict[int, tuple]:
    signatures = {}
    fonts = {}
    for family in families:
        for run in family.runs:
            path = face_paths[family.face_family(run.face)]
            if path not in fonts:
                fonts[path] = TTFont(path)
            for piece in run.pieces:
                signatures[id(piece)] = ff.outline_signature(fonts[path], piece.name)
    return signatures


def convert_fitted(pdf_path: str, out_dir: str, mode: str, session: WordSession) -> tuple[dict, list]:
    """returns the report and the horizontal lines' runs (the edit check picks a line from them)."""
    started = time.perf_counter()
    os.makedirs(os.path.join(out_dir, "fonts"), exist_ok=True)
    name = os.path.splitext(os.path.basename(pdf_path))[0]
    prototype_docx = os.path.join(out_dir, f"{name}.prototype.docx")
    with exports_through(session):
        prototype_report = prototype_convert.convert(pdf_path, prototype_docx, os.path.join(out_dir, "prototype_work"),
                                                     check_passes=False)  # fmt: skip
    horizontal, turned, visible = source_runs(pdf_path, os.path.join(out_dir, "work"), mode)
    package = PrototypePackage(prototype_docx)
    check_texts(package, horizontal, turned)
    all_runs = [run for runs in horizontal + turned for run in runs]
    families = families_of(all_runs)
    fl.name_pieces(families)
    for family in families:
        fl.assign_slots(family)
    report = {"source": pdf_path, "mode": mode, "passes": [], "visible_source_glyphs": visible,
              "runs": len(all_runs), "pieces": sum(len(r.pieces) for r in all_runs),
              "faces": {f.base_family: f.faces for f in families},
              "prototype_seconds": prototype_report["seconds"]}  # fmt: skip
    previous_origins = {}
    for number in range(MEASURE_PASSES):
        docx, face_paths = write_pass(package, families, horizontal, turned, out_dir, f"{name}.pass{number}")
        pdf = os.path.join(out_dir, f"{name}.pass{number}.word.pdf")
        export_seconds = session.export(docx, pdf)
        drawn = drawn_glyphs(pdf)
        signatures = piece_signatures(families, face_paths)
        located = fm.locate_pieces(all_runs, drawn, lambda run, piece, table=signatures: table[id(piece)])
        errors = fm.glyph_errors(all_runs, drawn)
        moved = _origin_moves(all_runs, previous_origins)
        previous_origins = {id(p): p.measured_origin for r in all_runs for p in r.pieces if p.measured_origin}
        report["passes"].append({"docx": docx, "pdf": pdf, "export_seconds": export_seconds, **located,
                                 "origin_moves_since_last_pass": moved,
                                 "errors": fm.summary(errors, visible)})  # fmt: skip
        report["final_errors"] = [e.__dict__ for e in errors]
    report["docx"], report["word_pdf"] = docx, pdf
    report["face_fonts"] = face_paths
    report["seconds"] = time.perf_counter() - started
    return report, horizontal


def _origin_moves(runs: list[fp.Run], previous: dict) -> dict:
    """how far each piece's printed origin moved since the last pass (outlines must not move layout)."""
    moves = defaultdict(int)
    largest = 0.0
    for run in runs:
        for piece in run.pieces:
            before = previous.get(id(piece))
            if before is None or piece.measured_origin is None:
                continue
            shift = max(abs(piece.measured_origin[0] - before[0]), abs(piece.measured_origin[1] - before[1]))
            largest = max(largest, shift)
            moves["moved" if shift > 1e-6 else "same"] += 1
    return {**moves, "largest_pt": largest}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("pdf")
    parser.add_argument("out_dir")
    parser.add_argument("--mode", choices=("line", "word"), default="line")
    args = parser.parse_args()
    with WordSession() as session:
        report, _ = convert_fitted(os.path.abspath(args.pdf), os.path.abspath(args.out_dir), args.mode, session)
        report["word"], report["word_pid"] = session.version, session.pid
    with open(os.path.join(args.out_dir, "fitted_report.json"), "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=1, default=str)
    for number, entry in enumerate(report["passes"]):
        print(f"pass {number}: {json.dumps({k: v for k, v in entry.items() if k not in ('docx', 'pdf')})}")


if __name__ == "__main__":
    main()
