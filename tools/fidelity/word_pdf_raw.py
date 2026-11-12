"""the raw text operators word writes into its pdfs, kept as printed.

the export model (word_writer_model.py) is fitted on the exact digits word
prints, so this reader keeps every Tm, Tf and TJ operand as its original
string next to its value, and resolves each glyph's width from the font's
/Widths the way a viewer would. only what word writes is handled: one Tm per
text object, TJ arrays, simple (one byte) fonts and Identity-H fonts.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import pikepdf


@dataclass
class RawGlyph:
    code: int
    width: float  # /Widths entry, 1/1000 em
    adjust_before: float  # TJ number written just before this glyph (0 if none), 1/1000 em


@dataclass
class TextObject:
    page: int
    font: str
    size_text: str
    size: float
    tm_text: tuple[str, ...]
    tm: tuple[float, ...]
    char_spacing: float
    glyphs: list[RawGlyph] = field(default_factory=list)

    @property
    def x(self) -> float:
        return self.tm[4]

    @property
    def y(self) -> float:
        return self.tm[5]

    def pens(self) -> list[float]:
        """pen x of every glyph in text space units (points when Tm has no scale)."""
        pens = []
        pen = 0.0
        for glyph in self.glyphs:
            pen -= glyph.adjust_before / 1000 * self.size
            pens.append(pen)
            pen += glyph.width / 1000 * self.size + self.char_spacing
        return pens


class _Widths:
    def __init__(self, font):
        self.two_byte = str(font.get("/Subtype")) == "/Type0"
        self.table: dict[int, float] = {}
        self.default = 0.0
        if self.two_byte:
            descendant = font.DescendantFonts[0]
            self.default = float(descendant.get("/DW", 1000))
            w = list(descendant.get("/W", []))
            i = 0
            while i < len(w):
                first = int(w[i])
                if isinstance(w[i + 1], pikepdf.Array):
                    for offset, value in enumerate(w[i + 1]):
                        self.table[first + offset] = float(value)
                    i += 2
                else:
                    last, value = int(w[i + 1]), float(w[i + 2])
                    for code in range(first, last + 1):
                        self.table[code] = value
                    i += 3
        else:
            first = int(font.get("/FirstChar", 0))
            for offset, value in enumerate(font.get("/Widths", [])):
                self.table[first + offset] = float(value)

    def codes(self, data: bytes) -> list[int]:
        if self.two_byte:
            return [data[i] << 8 | data[i + 1] for i in range(0, len(data) - 1, 2)]
        return list(data)

    def width(self, code: int) -> float:
        return self.table.get(code, self.default)


def read_text_objects(pdf_path: str) -> list[TextObject]:
    objects: list[TextObject] = []
    with pikepdf.open(pdf_path) as pdf:
        for page_index, page in enumerate(pdf.pages):
            resources = page.obj.get("/Resources")
            fonts = resources.get("/Font") if resources is not None else None
            widths_cache: dict[str, _Widths] = {}
            font_name, size_text, size, char_spacing = "", "0", 0.0, 0.0
            current: TextObject | None = None
            for operands, operator in pikepdf.parse_content_stream(page):
                op = str(operator)
                if op == "Tf":
                    font_name = str(operands[0])
                    size_text = pikepdf.unparse_content_stream([([operands[1]], pikepdf.Operator("n"))]).decode()[:-2]
                    size = float(operands[1])
                    if font_name not in widths_cache and fonts is not None:
                        widths_cache[font_name] = _Widths(fonts[font_name])
                elif op == "Tc":
                    char_spacing = float(operands[0])
                elif op == "Tm":
                    texts = tuple(
                        pikepdf.unparse_content_stream([([v], pikepdf.Operator("n"))]).decode()[:-2] for v in operands
                    )
                    current = TextObject(page_index, font_name, size_text, size, texts,
                                         tuple(float(v) for v in operands), char_spacing)  # fmt: skip
                    objects.append(current)
                elif op in ("TJ", "Tj") and current is not None:
                    widths = widths_cache[font_name]
                    items = operands[0] if op == "TJ" else [operands[0]]
                    pending = 0.0
                    for item in items:
                        if isinstance(item, pikepdf.String):
                            for code in widths.codes(bytes(item)):
                                current.glyphs.append(RawGlyph(code, widths.width(code), pending))
                                pending = 0.0
                        else:
                            pending += float(item)
                elif op == "ET":
                    current = None
    return objects
