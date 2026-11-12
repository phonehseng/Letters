"""which real font is behind every embedded font of a corpus pdf, by glyph outlines.

word renames embedded fonts (___WRD_EMBED_SUB_nn), so names prove nothing; the
outlines are compared with every font file the corpus was built from.

usage:  .venv\\Scripts\\python.exe corpus_fonts_check.py corpus\\syllabus.pdf [...]
"""

from __future__ import annotations

import glob
import io
import os
import sys

import pikepdf
from fontTools.ttLib import TTFont

from pdf_analysis import _truetype_outline_keys

HERE = os.path.dirname(os.path.abspath(__file__))


def reference_fonts() -> dict[str, set]:
    keys = {}
    for path in glob.glob(os.path.join(HERE, "fonts", "*.ttf")) + glob.glob(
        os.path.join(HERE, "fonts", "build", "corpus", "*.ttf")
    ):
        if "VF" in path or "family" in path:
            continue
        try:
            keys[os.path.basename(path)] = _truetype_outline_keys(TTFont(path))
        except Exception:
            continue
    return keys


def embedded_programs(pdf_path: str):
    with pikepdf.open(pdf_path) as pdf:
        seen = set()
        for page in pdf.pages:
            for _name, font in page.Resources.get("/Font", {}).items():
                if font.objgen in seen:
                    continue
                seen.add(font.objgen)
                descriptor = (
                    font.DescendantFonts[0].get("/FontDescriptor")
                    if "/DescendantFonts" in font
                    else font.get("/FontDescriptor")
                )
                program = (
                    descriptor.FontFile2.read_bytes() if descriptor is not None and "/FontFile2" in descriptor else None
                )
                yield str(font.BaseFont), program


def main() -> None:
    references = reference_fonts()
    for pdf_path in sys.argv[1:]:
        print(pdf_path)
        for base, program in embedded_programs(pdf_path):
            if program is None:
                print(f"   {base}: not embedded")
                continue
            keys = _truetype_outline_keys(TTFont(io.BytesIO(program)))
            best = max(references.items(), key=lambda item: len(keys & item[1]))
            share = len(keys & best[1]) / max(1, len(keys))
            print(f"   {base}: {len(keys)} outlines, {100 * share:.0f}% match {best[0]}")


if __name__ == "__main__":
    main()
