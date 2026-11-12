"""does word use a given font file when it is embedded in a docx? a quick yes/no.

usage:  .venv\\Scripts\\python.exe -m prototype.font_check font.ttf [more.ttf ...]

each font sets a short line in a frame; word's pdf is then checked for a font
whose outlines match the file (word renames embedded fonts, so names prove nothing).
"""

from __future__ import annotations

import io
import os
import sys
import tempfile

from fontTools.ttLib import TTFont

import docx_writer as dw
from pdf_analysis import embedded_font_programs, outline_match_fraction
from word_export import export_docx_to_pdf


def sample_text(font: TTFont) -> str:
    chars = [chr(c) for c in sorted(font.getBestCmap()) if 0x21 <= c < 0x7F]
    return "".join(chars[:30]) or "".join(chr(c) for c in sorted(font.getBestCmap())[:30])


def check(paths: list[str]) -> dict[str, bool]:
    work = tempfile.mkdtemp(prefix="font_check_")
    parts, fonts = [], []
    for index, path in enumerate(paths):
        font = TTFont(path)
        family = font["name"].getDebugName(1)
        fonts.append(dw.EmbeddedFont(family, open(path, "rb").read()))
        run = dw.run_xml(sample_text(font), family, 12)
        parts.append(
            dw.frame_paragraph_xml(run, x_twips=1440, y_twips=1440 + index * 400, width_twips=9000, line_twips=300)
        )
    docx = dw.write_docx(
        os.path.join(work, "check.docx"), "".join(parts), family=fonts[0].family, default_size_pt=12, embedded=fonts
    )
    pdf = os.path.join(work, "check.pdf")
    export_docx_to_pdf(docx, pdf)
    programs = embedded_font_programs(pdf)
    result = {}
    for path in paths:
        result[path] = any((outline_match_fraction(data, path) or 0) > 0.9 for data in programs.values())
    result["fonts_in_pdf"] = list(programs)
    return result


if __name__ == "__main__":
    for key, value in check(sys.argv[1:]).items():
        print(f"{value}  {key}")
    del io
