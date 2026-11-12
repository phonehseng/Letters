"""e11: which run property makes word's pdf writer start a new text object for every glyph?

word merges neighbouring runs with the same formatting into one text object
and then places the glyphs inside it with Tc and TJ, which carries its own
rounding (see e10). a glyph that starts its own text object is placed by its
Tm alone, which word_writer_model predicts exactly. so each variant here sets
a 10.5 pt line (grid font, no spacing) with a property alternating from char
to char and counts how many glyphs start their own text object.

usage:  .venv\\Scripts\\python.exe e11_run_split.py
"""

from __future__ import annotations

import os
import shutil
import zipfile

import docx_writer as dw
import font_prep
import test_fonts
from test_pdf import sample_text
from word_export import export_docx_to_pdf
from word_pdf_raw import read_text_objects

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e11")
SIZE = 10.5


def alternate(xml_a: str, xml_b: str):
    def properties(index: int) -> str:
        return xml_a if index % 2 == 0 else xml_b

    return properties


VARIANTS = {
    "same formatting": alternate("", ""),
    "lang en-US / en-GB": alternate('<w:lang w:val="en-US"/>', '<w:lang w:val="en-GB"/>'),
    "eastAsia lang en-US / ja-JP": alternate('<w:lang w:eastAsia="en-US"/>', '<w:lang w:eastAsia="ja-JP"/>'),
    "bidi lang ar-SA / he-IL": alternate('<w:lang w:bidi="ar-SA"/>', '<w:lang w:bidi="he-IL"/>'),
    "noProof on / off": alternate("<w:noProof/>", ""),
    "colour 000000 / 000001": alternate('<w:color w:val="000000"/>', '<w:color w:val="000001"/>'),
    "spacing 0 / 1 twip": alternate("", '<w:spacing w:val="1"/>'),
    "rStyle none / style": alternate("", '<w:rStyle w:val="Split"/>'),
}


def run_xml(char: str, family: str, extra: str) -> str:
    half_points = round(SIZE * 2)
    return (
        "<w:r><w:rPr>"
        f'<w:rFonts w:ascii="{family}" w:hAnsi="{family}" w:cs="{family}" w:eastAsia="{family}"/>'
        f'{extra}<w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/>'
        '<w14:ligatures w14:val="none"/>'
        f'</w:rPr><w:t xml:space="preserve">{char}</w:t></w:r>'
    )


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    tag = f"{SIZE:g}".replace(".", "h")
    family = f"Fidelity Grid {tag}"
    path = test_fonts.build_path(f"FidelityGrid-{tag}.ttf")
    if not os.path.exists(path):
        font_prep.make_grid_font(test_fonts.truetype_font(), path, family, SIZE)
    font = dw.EmbeddedFont(family, open(path, "rb").read())
    parts, rows = [], []
    for row, (name, properties) in enumerate(VARIANTS.items()):
        text = sample_text(400 + row, 8)
        runs = "".join(run_xml(char, family, properties(i)) for i, char in enumerate(text))
        y = 1000 + row * 600
        parts.append(dw.frame_paragraph_xml(runs, x_twips=1440, y_twips=y, width_twips=10000, line_twips=300))
        rows.append((name, text))
    docx = dw.write_docx(os.path.join(OUT_DIR, "e11.docx"), "".join(parts), family=family, default_size_pt=SIZE,
                         embedded=font)  # fmt: skip
    _add_style(docx)
    pdf = os.path.join(OUT_DIR, "e11.word.pdf")
    export_docx_to_pdf(docx, pdf)
    by_baseline: dict[float, list] = {}
    for obj in read_text_objects(pdf):
        by_baseline.setdefault(round(792 - obj.y, 1), []).append(obj)
    # rows are 30 pt apart top to bottom, so baselines sorted from the top match rows in order
    for (name, text), baseline in zip(rows, sorted(by_baseline), strict=True):
        objects = by_baseline[baseline]
        visible = [o for o in objects if any(g.code != 32 for g in o.glyphs)]
        glyphs = sum(len(o.glyphs) for o in visible)
        print(f"{name:<28} chars {len(text):3d}  glyphs {glyphs:3d}  text objects {len(visible):3d}")


def _add_style(docx_path: str) -> None:
    """add the character style the rStyle variant refers to (no formatting of its own)."""
    temp = docx_path + ".tmp"
    with zipfile.ZipFile(docx_path) as source, zipfile.ZipFile(temp, "w", zipfile.ZIP_DEFLATED) as target:
        for item in source.infolist():
            data = source.read(item.filename)
            if item.filename == "word/styles.xml":
                data = data.replace(
                    b"</w:styles>",
                    b'<w:style w:type="character" w:styleId="Split"><w:name w:val="Split"/></w:style></w:styles>',
                )
            target.writestr(item, data)
    shutil.move(temp, docx_path)


if __name__ == "__main__":
    main()
