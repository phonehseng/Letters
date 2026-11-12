"""the fitted font on the 13-document corpus: positions, fonts, text, and one edit per document.

each document gets its own private word instance (word_session), which is
quit before the next starts. results go to out\\corpus_<mode>\\results.json
and a markdown table to out\\corpus_<mode>\\results.md; the round 4
prototype's numbers (tools/fidelity/corpus_report.json) sit beside them.

usage:  ..\\.venv\\Scripts\\python.exe corpus_fitted.py [--mode line|word] [--only name ...]
"""

from __future__ import annotations

import fidelity_path

# isort: split

import argparse
import json
import os
import time

import corpus_checks as cc
import edit_check
import raster_check
from fitted_convert import convert_fitted
from word_session import WordSession

MANIFESTS = (os.path.join(fidelity_path.FIDELITY_DIR, "corpus", "manifest.json"),
             os.path.join(fidelity_path.FIDELITY_DIR, "corpus", "extra", "manifest.json"))  # fmt: skip
PROTOTYPE_REPORT = os.path.join(fidelity_path.FIDELITY_DIR, "corpus_report.json")
# pdfium positions glyphs with whole-number /Widths, so it renders sources with
# fractional widths and no per-glyph positioning away from where the pdf spec
# puts them (README "measurement pitfalls"); and the form's fonts are not
# embedded, so pdfium draws them with shapes of its own. the raster check is
# not valid for these sources
RASTER_INVALID = {"journal", "slides", "tj_kerning", "restricted_font", "form"}


def manifest() -> list[dict]:
    entries = []
    for path in MANIFESTS:
        with open(path, encoding="utf-8") as handle:
            entries.extend(json.load(handle))
    return entries


def run_document(entry: dict, mode: str, root: str) -> dict:
    name = entry["name"]
    source = os.path.join(fidelity_path.CORPUS_DIR, entry["pdf"].replace("\\", os.sep))
    out_dir = os.path.join(root, name)
    started = time.perf_counter()
    with WordSession() as session:
        report, runs_by_line = convert_fitted(source, out_dir, mode, session)
        report["word"], report["word_pid"] = session.version, session.pid
        report["text"] = cc.text_checks(session, report["docx"], report["word_pdf"], source,
                                        entry.get("known_phrase", ""), entry.get("known_sentence", ""))  # fmt: skip
        report["edit"] = edit_check.edit_check(session, report["docx"], report["word_pdf"], runs_by_line, out_dir)
    report["fonts_in_word_pdf"] = cc.font_check(report["word_pdf"], report["face_fonts"])
    report["raster"] = raster_check.summary(raster_check.line_shifts(source, report["word_pdf"]))
    report["raster"]["valid"] = name not in RASTER_INVALID
    report["total_seconds"] = time.perf_counter() - started
    with open(os.path.join(out_dir, "fitted_report.json"), "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=1, default=str)
    final = report["passes"][-1]["errors"]
    edit = report["edit"]
    print(f"{name:16} max {final.get('max', float('nan')):.4f} mean {final.get('mean', float('nan')):.4f} "
          f"measured {final['glyphs_measured']}/{final['source_glyphs']} "
          f"edit {edit.get('other_glyphs_unchanged')}/{edit.get('other_glyphs')}", flush=True)  # fmt: skip
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mode", choices=("line", "word"), default="line")
    parser.add_argument("--only", nargs="*")
    args = parser.parse_args()
    root = os.path.join(fidelity_path.OUT_DIR, f"corpus_{args.mode}")
    os.makedirs(root, exist_ok=True)
    results = {}
    for entry in manifest():
        if args.only and entry["name"] not in args.only:
            continue
        try:
            results[entry["name"]] = run_document(entry, args.mode, root)
        except Exception as error:  # one broken document must not hide the others' results
            results[entry["name"]] = {"error": f"{type(error).__name__}: {error}"}
            print(entry["name"], "failed:", results[entry["name"]]["error"], flush=True)
    summary_path = os.path.join(root, "results.json")
    previous = {}
    if os.path.exists(summary_path) and args.only:
        with open(summary_path, encoding="utf-8") as handle:
            previous = json.load(handle)
    previous.update({name: _short(report) for name, report in results.items()})
    with open(summary_path, "w", encoding="utf-8") as handle:
        json.dump(previous, handle, indent=1, default=str)


def _short(report: dict) -> dict:
    """everything but the per-glyph error list, which stays in the document's own report."""
    return {key: value for key, value in report.items() if key != "final_errors"}


if __name__ == "__main__":
    main()
