"""experiment d2: are drawingml shapes, pictures and text boxes placed off word's 1/600 in text grid?

25 rows; row k is anchored at x = 100 + k * 0.01 pt and holds
    a custom-geometry shape (a:custGeom, a small L whose corner sits at a
    fractional emu inside the shape),
    a picture (a 4 x 4 pixel png),
    a text box with one letter in an ordinary embedded font,
each with its anchor (wp:posOffset) in emu. word's pdf is read back: the
first path point, the picture's cm origin and the letter's Tm origin. if a
kind follows the 0.01 pt steps it is placed off the grid; if it moves in 0.12
or 0.024 pt steps it is snapped.

usage:  ..\\.venv\\Scripts\\python.exe probe_drawingml.py
"""

from __future__ import annotations

import fidelity_path

# isort: split

import io
import json
import os

import pikepdf
from PIL import Image

import docx_writer as dw
import fitted_docx as fd
import probe_ligatures as c1
from pdf_glyphs import drawn_glyphs
from prototype import docx_out
from word_session import WordSession

OUT = os.path.join(fidelity_path.OUT_DIR, "d2_drawingml")
ROWS = 25
STEP_PT = 0.01
BASE_X_PT = 100.0
ROW_PITCH_PT = 24.0
SHAPE_SIZE_PT = 6.0
INNER_CORNER_EMU = 12345  # the l's corner inside the shape, in shape path units = emu here


def custom_shape(shape_id: int, left: float, top: float) -> str:
    size = docx_out.emu(SHAPE_SIZE_PT)
    path = (
        f'<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/>'
        f'<a:pathLst><a:path w="{size}" h="{size}"><a:moveTo><a:pt x="{INNER_CORNER_EMU}" y="0"/></a:moveTo>'
        f'<a:lnTo><a:pt x="{INNER_CORNER_EMU}" y="{INNER_CORNER_EMU}"/></a:lnTo>'
        f'<a:lnTo><a:pt x="{size}" y="{INNER_CORNER_EMU}"/></a:lnTo></a:path></a:pathLst></a:custGeom>'
    )
    sp_pr = (f"{docx_out._xfrm(SHAPE_SIZE_PT, SHAPE_SIZE_PT)}{path}<a:noFill/>"
             '<a:ln w="3175"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>')  # fmt: skip
    return docx_out._wps(shape_id, left, top, SHAPE_SIZE_PT, SHAPE_SIZE_PT, sp_pr)


def text_box(shape_id: int, left: float, top: float) -> str:
    run = fd.TextRun("H", c1.FAMILY, 20, features=fd.RunFeatures(ligatures="none"))
    paragraph = f"<w:p><w:pPr>{docx_out.line_paragraph_props(240)}</w:pPr>{fd.run_xml(run)}</w:p>"
    body = ('<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="0" tIns="0" rIns="0" bIns="0" anchor="t" '
            'upright="0"><a:noAutofit/></wps:bodyPr>')  # fmt: skip
    sp_pr = (
        f'{docx_out._xfrm(20, 12)}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln>'
    )
    text = f"<wps:txbx><w:txbxContent>{paragraph}</w:txbxContent></wps:txbx>"
    return docx_out._wps(shape_id, left, top, 20, 12, sp_pr, body, text, behind=False)


def build() -> str:
    font = c1.base_font()
    font_path = os.path.join(OUT, "probe.ttf")
    font.save(font_path)
    package = docx_out.Package()
    package.fonts.append(dw.EmbeddedFont(c1.FAMILY, open(font_path, "rb").read()))
    png = io.BytesIO()
    Image.new("L", (4, 4), 0).save(png, "PNG")
    relationship = package.add_media("png", png.getvalue())
    drawings = []
    for row in range(ROWS):
        left = BASE_X_PT + row * STEP_PT
        top = 50 + row * ROW_PITCH_PT
        drawings.append(custom_shape(package.shape_id(), left, top))
        drawings.append(docx_out.picture_drawing(package.shape_id(), relationship, left + 20, top, 4, 4))
        drawings.append(text_box(package.shape_id(), left + 40, top))
    package.body = [docx_out.anchor_paragraph("".join(drawings))]
    package.final_section = docx_out.section_properties(612, 792)
    return package.save(os.path.join(OUT, "drawingml.docx"))


def _multiply(m, n):
    a, b, c, d, e, f = m
    p, q, r, s, t, u = n
    return (a * p + b * r, a * q + b * s, c * p + d * r, c * q + d * s, e * p + f * r + t, e * q + f * s + u)


def read_back(pdf_path: str) -> dict:
    """first point of every path (page space) and the origin of every picture drawn."""
    path_starts, picture_origins = [], []
    with pikepdf.open(pdf_path) as pdf:
        page = pdf.pages[0]
        stack, ctm = [], (1, 0, 0, 1, 0, 0)
        for operands, operator in pikepdf.parse_content_stream(page):
            op = str(operator)
            if op == "q":
                stack.append(ctm)
            elif op == "Q":
                ctm = stack.pop()
            elif op == "cm":
                ctm = _multiply(tuple(float(v) for v in operands), ctm)
            elif op == "m":
                x, y = float(operands[0]), float(operands[1])
                a, b, c, d, e, f = ctm
                path_starts.append((a * x + c * y + e, b * x + d * y + f))
            elif op == "Do":
                picture_origins.append((ctm[4], ctm[5]))
    # each box also draws its paragraph mark (an empty glyph after the letter), and word
    # writes no ToUnicode here, so the letter is the leftmost glyph of each box's baseline
    first_on_baseline: dict[float, tuple[float, float]] = {}
    for glyph in drawn_glyphs(pdf_path):
        key = round(glyph.y, 2)
        if glyph.x > 100 and (key not in first_on_baseline or glyph.x < first_on_baseline[key][0]):
            first_on_baseline[key] = (glyph.x, glyph.y)
    letters = list(first_on_baseline.values())
    return {"path_starts": path_starts, "picture_origins": picture_origins, "text_origins": letters}


def steps(values: list[float]) -> list[float]:
    return [round(b - a, 4) for a, b in zip(values, values[1:], strict=False)]


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    docx = build()
    pdf = os.path.join(OUT, "drawingml.pdf")
    with WordSession() as session:
        session.export(docx, pdf)
        pid = session.pid
    found = read_back(pdf)
    # rows run down the page, so sort each kind by y (pdf y grows upwards)
    result = {"word_pid": pid}
    for kind, points in found.items():
        ordered = sorted(points, key=lambda p: -p[1])
        xs = [p[0] for p in ordered]
        intended = [BASE_X_PT + k * STEP_PT for k in range(len(xs))]
        offsets = [round(x - i, 4) for x, i in zip(xs, intended, strict=False)]
        result[kind] = {"count": len(xs), "x": [round(x, 4) for x in xs], "x_steps": steps(xs),
                        "x_minus_row_offset": offsets}  # fmt: skip
        print(kind, result[kind]["count"], "steps", result[kind]["x_steps"])
    with open(os.path.join(OUT, "d2_results.json"), "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=1)


if __name__ == "__main__":
    main()
