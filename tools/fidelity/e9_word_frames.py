"""e9: one frame per word, the fallback if per-line grid snapping did not hold.

every word of the e2 test set (3 pages, 300 lines) gets its own frame at the
word's source x (twips rounded, no calibration) on its line's calibrated frame y,
so drift can only build up inside a word. compared with e5's per-line grid
snapping on the same set: accuracy, docx size, word's open and export time, and
how word sees the text (paragraph and frame counts, text order, phrase search).

usage:  .venv\\Scripts\\python.exe e9_word_frames.py
output: out\\e9\\
"""

from __future__ import annotations

import json
import os

import docx_writer as dw
import e2_positioning as e2
import e5_grid_snap as e5
import test_fonts
from frame_calibration import calibrate_frames
from harness import print_summary, run_harness
from test_pdf import TextLine, register_font, write_verified_pdf

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e9")
PHRASE_WORDS = 3


def word_starts(line: TextLine) -> list[tuple[str, float]]:
    """each word of the line with the pen x of its first char."""
    origins = e5.target_origins(line)
    words, start = [], None
    for index, char in enumerate(line.text + " "):
        if char != " " and start is None:
            start = index
        elif char == " " and start is not None:
            words.append((line.text[start:index], origins[start]))
            start = None
    return words


def word_frames_body(lines: list[TextLine], choices: dict, family: str) -> str:
    parts = []
    for page in sorted({line.page for line in lines}):
        if page > 0:
            parts.append(dw.page_break_paragraph_xml())
        for line in (line for line in lines if line.page == page):
            y_twips = choices[e5.position_key(line)].y_twips
            for word, x in word_starts(line):
                parts.append(
                    dw.frame_paragraph_xml(
                        dw.run_xml(word, family, line.size),
                        x_twips=round(x * dw.TWIPS_PER_PT),
                        y_twips=y_twips,
                        width_twips=round((len(word) * line.size * 0.7 + 20) * dw.TWIPS_PER_PT),
                        line_twips=e5.LINE_TWIPS,
                    )
                )
    return "".join(parts)


def text_view(phrase: str):
    """what word thinks the document is: read-only queries on the open document."""

    def prepare(word, document):
        finder = document.Content.Duplicate.Find
        finder.ClearFormatting()
        found = bool(finder.Execute(FindText=phrase, MatchCase=True, Forward=True, Wrap=0))
        return {
            "paragraphs": document.Paragraphs.Count,
            "frames": document.Frames.Count,
            "text_start": document.Content.Text[:120],
            "phrase": phrase,
            "phrase_found": found,
        }

    return prepare


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    register_font(test_fonts.truetype_font(), e5.PDF_FONT_NAME)
    lines = e2.test_lines()
    source = write_verified_pdf(os.path.join(OUT_DIR, "e9_source.pdf"), e5.PDF_FONT_NAME, lines)
    base_path = test_fonts.truetype_font()
    base = dw.EmbeddedFont(test_fonts.TRUETYPE_FAMILY, open(base_path, "rb").read())
    calibration = calibrate_frames(
        e5.frame_requests(lines, {size: base.family for size in {line.size for line in lines}}),
        OUT_DIR,
        "e9_calibration",
        [base],
    )
    phrase = " ".join(lines[0].text.split(" ")[1 : 1 + PHRASE_WORDS])
    docx = dw.write_docx(
        os.path.join(OUT_DIR, "e9_word_frames.docx"),
        word_frames_body(lines, calibration["choices"], base.family),
        family=base.family,
        default_size_pt=lines[0].size,
        embedded=base,
    )
    report = run_harness(source, docx, OUT_DIR, keep_pairs=True)
    print_summary(report, label="e9 word frames")
    stats = e5.glyph_summary(report)
    e5.print_glyph_summary("all glyphs", stats)

    # the per-line e5 document is opened again only for word's view of its text
    from word_export import export_docx_to_pdf

    views = {}
    e5_docx = os.path.join(e5.OUT_DIR, "e2set", "e2set_grid_snap.docx")
    for name, path in (("word_frames", docx), ("e5_line_frames", e5_docx)):
        if not os.path.exists(path):
            print(f"{path} missing, run e5_grid_snap.py first")
            continue
        info = export_docx_to_pdf(path, os.path.join(OUT_DIR, f"{name}.view.pdf"), prepare=text_view(phrase))
        views[name] = {"docx_bytes": os.path.getsize(path), "timings": info["timings"], **info["notes"]}
        print(f"{name}: {views[name]}")
    with open(os.path.join(OUT_DIR, "e9_summary.json"), "w", encoding="utf-8") as handle:
        json.dump({"glyphs": stats, "views": views, "seconds": report["seconds"]}, handle, indent=2)


if __name__ == "__main__":
    main()
