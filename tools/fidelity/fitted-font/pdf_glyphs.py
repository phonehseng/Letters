"""every glyph word drew in its pdf export: printed origin, size, and which outline it is.

word subsets and renumbers the fonts it embeds, so a glyph id says nothing on
its own. each drawn glyph is resolved to the outline in word's embedded
program (code -> gid through the font's CIDToGIDMap or cmap) and described by
fitted_font.outline_signature, which a fitted font's own glyph also has. the
pen positions come from word_pdf_raw, which keeps word's printed digits.
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import io
import math
from dataclasses import dataclass

import pikepdf
from fontTools.ttLib import TTFont

import fitted_font
from prototype.pdf_fonts import parse_to_unicode
from word_pdf_raw import read_text_objects


@dataclass
class DrawnGlyph:
    page: int
    x: float  # pen origin, pdf points
    y: float
    size: float  # Tf, the size word draws at
    font_resource: str
    code: int
    signature: tuple | None
    text: str  # from the font's ToUnicode, may be several chars for a ligature glyph
    starts_object: bool  # placed by Tm alone (on the print lattice), not by TJ arithmetic
    direction: tuple[float, float]  # unit vector of the text matrix's x axis (rotated text)


class _FontInfo:
    def __init__(self, font_dict):
        self.two_byte = str(font_dict.get("/Subtype")) == "/Type0"
        descriptor_owner = font_dict.DescendantFonts[0] if self.two_byte else font_dict
        self.cid_to_gid: bytes | None = None
        if self.two_byte:
            mapping = descriptor_owner.get("/CIDToGIDMap")
            if isinstance(mapping, pikepdf.Stream):
                self.cid_to_gid = mapping.read_bytes()
        descriptor = descriptor_owner.get("/FontDescriptor")
        program = descriptor.get("/FontFile2") if descriptor is not None else None
        self.font = TTFont(io.BytesIO(program.read_bytes())) if program is not None else None
        to_unicode = font_dict.get("/ToUnicode")
        self.to_unicode = parse_to_unicode(to_unicode.read_bytes()) if to_unicode is not None else {}
        self._signatures: dict[int, tuple | None] = {}

    def gid(self, code: int) -> int | None:
        if self.two_byte:
            if self.cid_to_gid is None:
                return code
            index = 2 * code
            return self.cid_to_gid[index] << 8 | self.cid_to_gid[index + 1]
        if self.font is None:
            return None
        # a simple truetype font from word: the (3,0) or (1,0) cmap maps the byte code
        for table in self.font["cmap"].tables:
            for candidate in (code, 0xF000 + code):
                if candidate in table.cmap:
                    return self.font.getGlyphID(table.cmap[candidate])
        return None

    def signature(self, code: int) -> tuple | None:
        if code not in self._signatures:
            gid = self.gid(code)
            value = None
            if self.font is not None and gid is not None and gid < len(self.font.getGlyphOrder()):
                value = fitted_font.outline_signature(self.font, self.font.getGlyphName(gid))
            self._signatures[code] = value
        return self._signatures[code]


def _font_infos(pdf_path: str) -> dict[tuple[int, str], _FontInfo]:
    infos = {}
    with pikepdf.open(pdf_path) as pdf:
        for page_index, page in enumerate(pdf.pages):
            resources = page.obj.get("/Resources")
            fonts = resources.get("/Font") if resources is not None else None
            if fonts is None:
                continue
            for name, font_dict in fonts.items():
                infos[(page_index, str(name))] = _FontInfo(font_dict)
    return infos


def drawn_glyphs(pdf_path: str) -> list[DrawnGlyph]:
    infos = _font_infos(pdf_path)
    glyphs = []
    for text_object in read_text_objects(pdf_path):
        # pens run along the text matrix's x axis, which is rotated for rotated text
        a, b, _, _, e, f = text_object.tm
        info = infos.get((text_object.page, text_object.font))
        scale = math.hypot(a, b)
        for index, (raw, pen) in enumerate(zip(text_object.glyphs, text_object.pens(), strict=True)):
            glyphs.append(
                DrawnGlyph(
                    page=text_object.page,
                    x=e + a * pen,
                    y=f + b * pen,
                    size=text_object.size * scale,
                    font_resource=text_object.font,
                    code=raw.code,
                    signature=info.signature(raw.code) if info else None,
                    text=info.to_unicode.get(raw.code, "") if info else "",
                    starts_object=index == 0,
                    direction=(a / scale, b / scale),
                )
            )
    return glyphs
