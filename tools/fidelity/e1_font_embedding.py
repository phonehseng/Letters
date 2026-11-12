"""e1: does word render with a font embedded in the docx, or quietly substitute?

cases, each a docx with the same few lines placed in frames:
    full_ttf        renamed crimson text, whole truetype font, obfuscated .odttf
    subset_ttf      the same font cut down to the used characters by fontTools
    cff_otf         renamed source serif 4, cff-flavoured opentype, obfuscated the same way
    not_embedded    control: the docx names the truetype family but carries no font

usage:  .venv\\Scripts\\python.exe e1_font_embedding.py
output: out\\e1\\  (source pdf, docx files, word pdfs, heatmaps, reports, e1_summary.json)
"""

from __future__ import annotations

import json
import os

from reportlab.pdfbase import pdfmetrics

import docx_writer as dw
import font_prep
import placement
import test_fonts
from harness import print_summary, run_harness
from test_pdf import TextLine, register_font, sample_text, write_verified_pdf

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e1")
SIZE_PT = 14.0
CALIBRI = r"C:\Windows\Fonts\calibri.ttf"


def e1_lines() -> list[TextLine]:
    first = [TextLine(0, 72.37, 700.0 - 20.0 * i, SIZE_PT, sample_text(i, 6)) for i in range(6)]
    pangram = TextLine(0, 72.37, 560.0, SIZE_PT, "Sphinx of black quartz, judge my vow. ffi fl 0123456789")
    return first + [pangram]


def used_text(lines: list[TextLine]) -> str:
    return "".join(sorted({c for line in lines for c in line.text}))


def write_case_docx(
    path: str,
    family: str,
    font_file: str | None,
    lines: list[TextLine],
    width_font: str,
    embed_flag: bool | None = None,
) -> str:
    embedded = None
    if font_file is not None:
        with open(font_file, "rb") as handle:
            embedded = dw.EmbeddedFont(family=family, font_bytes=handle.read())

    def width_of(line: TextLine) -> float:
        return pdfmetrics.stringWidth(line.text, width_font, line.size)

    body = placement.frames_body(lines, placement.plain_runs(family), width_of, baseline_offsets={})
    return dw.write_docx(path, body, family=family, default_size_pt=SIZE_PT, embedded=embedded, embed_flag=embed_flag)


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    lines = e1_lines()
    truetype = test_fonts.truetype_font()
    cff = test_fonts.cff_font()
    test_fonts.assert_not_installed(test_fonts.SUBSET_FAMILY)
    subset = font_prep.make_subset_font(
        truetype, test_fonts.build_path("FidelityProbeSubset.ttf"), used_text(lines), test_fonts.SUBSET_FAMILY
    )

    pdf_font = register_font(truetype, "FidelityProbeSerif")
    source_pdf = write_verified_pdf(os.path.join(OUT_DIR, "e1_source.pdf"), pdf_font, lines)

    cases = {
        # name: (family in the docx, font file to embed, w:embedTrueTypeFonts or None for default)
        "full_ttf": (test_fonts.TRUETYPE_FAMILY, truetype, None),
        "full_ttf_without_settings_flag": (test_fonts.TRUETYPE_FAMILY, truetype, False),
        "subset_ttf": (test_fonts.SUBSET_FAMILY, subset, None),
        "cff_otf": (test_fonts.CFF_FAMILY, cff, None),
        "not_embedded": (test_fonts.TRUETYPE_FAMILY, None, None),
    }
    # calibri is the usual fallback, so it serves as the negative outline check
    reference_fonts = [truetype, CALIBRI] if os.path.exists(CALIBRI) else [truetype]
    summary = {}
    for case, (family, font_file, embed_flag) in cases.items():
        embedded_name = os.path.basename(font_file) if font_file else "none"
        docx = write_case_docx(os.path.join(OUT_DIR, f"e1_{case}.docx"), family, font_file, lines, pdf_font, embed_flag)
        report = run_harness(source_pdf, docx, OUT_DIR, reference_fonts=reference_fonts)
        print_summary(
            report,
            label=f"e1 {case} (family '{family}', embedded file: {embedded_name})",
        )
        summary[case] = {
            "family_in_docx": family,
            "embedded_file": font_file,
            "embedded_bytes": os.path.getsize(font_file) if font_file else 0,
            "fonts_in_word_pdf": report["fonts_in_word_pdf"],
            "glyphs": report["glyphs"],
            "ssim": [p.get("ssim") for p in report["raster"]["pages"]],
            "seconds": report["seconds"],
        }
    with open(os.path.join(OUT_DIR, "e1_summary.json"), "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)
    print(
        "note: the cff case is compared against the truetype source pdf, its glyph deltas only show"
        " which font was used, not positioning quality"
    )


if __name__ == "__main__":
    main()
