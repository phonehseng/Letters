"""markdown tables for REPORT.md from the corpus results of both modes and the round 4 prototype.

usage:  ..\\.venv\\Scripts\\python.exe report_tables.py   (writes out\\tables.md and prints it)
"""

from __future__ import annotations

import fidelity_path

# isort: split

import json
import os

from corpus_fitted import RASTER_INVALID

PROTOTYPE_REPORT = os.path.join(fidelity_path.FIDELITY_DIR, "corpus_report.json")


def _load(mode: str) -> dict:
    path = os.path.join(fidelity_path.OUT_DIR, f"corpus_{mode}", "results.json")
    if not os.path.exists(path):
        return {}
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def _prototype() -> dict:
    with open(PROTOTYPE_REPORT, encoding="utf-8") as handle:
        return {r["name"]: r for r in json.load(handle)["results"]}


def _errors(report: dict) -> dict:
    return report["passes"][-1]["errors"] if report and "passes" in report else {}


def _fmt(value, digits: int = 4) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, bool):
        return "yes" if value else "no"
    if isinstance(value, float):
        return f"{value:.{digits}f}"
    return str(value)


def positions_table(line: dict, word: dict, prototype: dict) -> list[str]:
    rows = [
        "| class | glyphs measured | round 4 max / mean | fitted line: unbaked max | fitted line max / mean / p99 "
        "| <=0.025 | fitted word max / mean | raster check max / mean (pt) | fonts in word pdf ours |",
        "|---|---|---|---|---|---|---|---|---|",
    ]
    for name, proto in prototype.items():
        report, word_report = line.get(name, {}), word.get(name, {})
        errors, word_errors = _errors(report), _errors(word_report)
        if not errors:
            rows.append(f"| {proto['class']} | failed: {report.get('error', 'not run')} |||||||")
            continue
        unbaked = report["passes"][0]["errors"]
        raster = report["raster"]
        raster_text = f"{_fmt(raster.get('max_blob_shift_pt'), 3)} / {_fmt(raster.get('mean_blob_shift_pt'))}"
        if name in RASTER_INVALID:
            raster_text += " (not valid)"
        fonts = report["fonts_in_word_pdf"]
        ours = f"{sum(f['ours'] for f in fonts)} / {sum(f['drawn'] for f in fonts)}"
        word_text = f"{_fmt(word_errors.get('max'))} / {_fmt(word_errors.get('mean'))}" if word_errors else "not run"
        rows.append(
            f"| {proto['class']} | {errors['glyphs_measured']} / {errors['source_glyphs']} "
            f"| {proto['glyphs']['max']:.3f} / {proto['glyphs']['mean']:.4f} | {_fmt(unbaked['max'], 3)} "
            f"| {_fmt(errors['max'])} / {_fmt(errors['mean'])} / {_fmt(errors['p99'])} "
            f"| {100 * errors['within_0.025']:.1f}% | {word_text} | {raster_text} | {ours} |"
        )
    return rows


def text_table(line: dict, prototype: dict) -> list[str]:
    rows = [
        "| class | docx chars present | word pdf chars present | find phrase docx / pdf | find sentence docx / pdf "
        "| pieces | font copies |",
        "|---|---|---|---|---|---|---|",
    ]
    for name, proto in prototype.items():
        report = line.get(name, {})
        if "text" not in report:
            continue
        text = report["text"]
        rows.append(
            f"| {proto['class']} | {_fmt(text['docx_chars_present'], 3)} | {_fmt(text['word_pdf_chars_present'], 3)} "
            f"| {_fmt(text['find_phrase_in_docx'])} / {_fmt(text['find_phrase_in_word_pdf'])} "
            f"| {_fmt(text['find_sentence_in_docx'])} / {_fmt(text['find_sentence_in_word_pdf'])} "
            f"| {report['pieces']} | {sum(report['faces'].values())} for {len(report['faces'])} grid fonts |"
        )
    return rows


def edit_table(results: dict, prototype: dict) -> list[str]:
    rows = [
        "| class | replaced | other glyphs unchanged | edited line glyphs kept | before the edit: fallback max "
        "| after the edit: shift / spacing error max / mean | saved, reopened identical |",
        "|---|---|---|---|---|---|---|",
    ]
    for name, proto in prototype.items():
        edit = results.get(name, {}).get("edit")
        if not edit:
            continue
        if not edit.get("edited"):
            rows.append(f"| {proto['class']} | not edited: {edit.get('reason')} ||||||")
            continue
        after = "n/a (line wrapped or not matched)"
        if edit.get("after_edit_chars"):
            after = (f"{_fmt(edit['after_edit_shift_pt'], 2)} / {_fmt(edit['after_edit_spacing_error_max_pt'], 3)}"
                     f" / {_fmt(edit['after_edit_spacing_error_mean_pt'], 3)}")  # fmt: skip
        before = edit.get("before_edit_fallback_max_pt")
        before_text = _fmt(before, 3) if before is not None else "exact (pieces kept)"
        if edit["edited_line_chars_before_edit"] == 0:
            before_text = "n/a (first word)"
        rows.append(
            f"| {proto['class']} | {edit['replaced']} -> {edit['with']} "
            f"| {edit['other_glyphs_unchanged']} / {edit['other_glyphs']} (+{edit['other_glyphs_new_or_moved']}) "
            f"| {edit['edited_line_glyphs_kept']} / {edit['edited_line_glyphs_before']} | {before_text} | {after} "
            f"| {_fmt(edit['saved_and_reopened_identical'])} |"
        )
    return rows


def main() -> None:
    line, word, prototype = _load("line"), _load("word"), _prototype()
    parts = ["## positions", *positions_table(line, word, prototype), "", "## text", *text_table(line, prototype),
             "", "## edit, line mode", *edit_table(line, prototype), "", "## edit, word mode",
             *edit_table(word, prototype)]  # fmt: skip
    text = "\n".join(parts) + "\n"
    with open(os.path.join(fidelity_path.OUT_DIR, "tables.md"), "w", encoding="utf-8") as handle:
        handle.write(text)
    print(text)


if __name__ == "__main__":
    main()
