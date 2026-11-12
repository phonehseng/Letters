"""reading pdf font dictionaries into one plain description per font.

everything a later stage needs is resolved here once: how many bytes a char
code takes, each code's width in 1/1000 text space units, its unicode text and,
for simple fonts, its glyph name. the embedded font program is kept as bytes.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

import pikepdf
from fontTools.agl import toUnicode
from fontTools.encodings.StandardEncoding import StandardEncoding

STANDARD_14 = {
    "Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique",
    "Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic",
    "Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique",
    "Symbol", "ZapfDingbats",
}  # fmt: skip
SYMBOLIC_FLAG = 1 << 2


@dataclass
class PdfFont:
    key: tuple  # object id and generation, or a name for inline fonts
    base_name: str  # without a subset prefix such as ABCDEF+
    subset_tag: str
    subtype: str  # Type1, TrueType, Type0, Type3, MMType1
    code_bytes: int  # 1 for simple fonts, 2 for identity cid fonts
    widths: dict[int, float]
    default_width: float
    to_unicode: dict[int, str]
    glyph_names: dict[int, str]  # simple fonts only: code -> glyph name from the encoding
    program: bytes | None
    program_kind: str | None  # truetype, cff, opentype, type1
    cid_to_gid: bytes | None  # None means identity
    symbolic: bool
    ascent: float
    descent: float
    problems: list[str] = field(default_factory=list)

    @property
    def embedded(self) -> bool:
        return self.program is not None

    @property
    def is_cid(self) -> bool:
        return self.subtype == "Type0"

    def width(self, code: int) -> float:
        return self.widths.get(code, self.default_width)

    def text(self, code: int) -> str:
        """unicode for a code: tounicode first, then the encoding's glyph name."""
        if code in self.to_unicode:
            return self.to_unicode[code]
        name = self.glyph_names.get(code)
        return toUnicode(name) if name else ""

    def split_codes(self, data: bytes) -> list[int]:
        if self.code_bytes == 2:
            return [int.from_bytes(data[i : i + 2], "big") for i in range(0, len(data) - 1, 2)]
        return list(data)


def _name(value) -> str:
    return str(value)[1:] if str(value).startswith("/") else str(value)


def split_subset_prefix(base_font: str) -> tuple[str, str]:
    match = re.match(r"^([A-Z]{6})\+(.+)$", base_font)
    return (match.group(2), match.group(1)) if match else (base_font, "")


# ---------------------------------------------------------------- tounicode cmaps


def _hex_to_int(token: bytes) -> int:
    return int(token.strip(b"<>") or b"0", 16)


def _hex_to_text(token: bytes) -> str:
    raw = bytes.fromhex(token.strip(b"<>").decode("ascii"))
    return raw.decode("utf-16-be", errors="replace")


def parse_to_unicode(data: bytes) -> dict[int, str]:
    """bfchar and bfrange sections of a tounicode cmap."""
    mapping: dict[int, str] = {}
    for block in re.findall(rb"beginbfchar(.*?)endbfchar", data, re.S):
        tokens = re.findall(rb"<[0-9A-Fa-f]*>", block)
        for source, target in zip(tokens[0::2], tokens[1::2], strict=False):
            mapping[_hex_to_int(source)] = _hex_to_text(target)
    for block in re.findall(rb"beginbfrange(.*?)endbfrange", data, re.S):
        for low, high, target in re.findall(
            rb"(<[0-9A-Fa-f]*>)\s*(<[0-9A-Fa-f]*>)\s*(\[.*?\]|<[0-9A-Fa-f]*>)", block, re.S
        ):
            start, end = _hex_to_int(low), _hex_to_int(high)
            if target.startswith(b"["):
                for offset, item in enumerate(re.findall(rb"<[0-9A-Fa-f]*>", target)):
                    mapping[start + offset] = _hex_to_text(item)
            else:
                first = _hex_to_text(target)
                for offset in range(end - start + 1):
                    # only the last utf-16 unit increments across a range
                    mapping[start + offset] = first[:-1] + chr(ord(first[-1]) + offset) if first else ""
    return mapping


# ---------------------------------------------------------------- simple font encodings


def _base_encoding_names(encoding_name: str | None) -> dict[int, str]:
    if encoding_name == "WinAnsiEncoding":
        names = {}
        for code in range(256):
            try:
                char = bytes([code]).decode("cp1252")
            except UnicodeDecodeError:
                continue
            names[code] = _glyph_name_for(char)
        return names
    if encoding_name == "MacRomanEncoding":
        return {code: _glyph_name_for(bytes([code]).decode("mac_roman")) for code in range(256)}
    return {code: name for code, name in enumerate(StandardEncoding) if name != ".notdef"}


def _glyph_name_for(char: str) -> str:
    from fontTools.agl import UV2AGL

    return UV2AGL.get(ord(char), f"uni{ord(char):04X}")


def _simple_encoding(font_dict) -> dict[int, str]:
    encoding = font_dict.get("/Encoding")
    if encoding is None:
        return _base_encoding_names(None)
    if isinstance(encoding, pikepdf.Name):
        return _base_encoding_names(_name(encoding))
    names = _base_encoding_names(_name(encoding.get("/BaseEncoding", "/StandardEncoding")))
    code = 0
    for item in encoding.get("/Differences", []):
        if isinstance(item, pikepdf.Name):
            names[code] = _name(item)
            code += 1
        else:
            code = int(item)
    return names


# ---------------------------------------------------------------- widths


def _simple_widths(font_dict) -> dict[int, float]:
    first = int(font_dict.get("/FirstChar", 0))
    return {first + i: float(w) for i, w in enumerate(font_dict.get("/Widths", []))}


def _cid_widths(cid_font) -> dict[int, float]:
    widths = {}
    array = list(cid_font.get("/W", []))
    i = 0
    while i < len(array):
        first = int(array[i])
        following = array[i + 1]
        if isinstance(following, pikepdf.Array):
            for offset, width in enumerate(following):
                widths[first + offset] = float(width)
            i += 2
        else:
            for cid in range(first, int(following) + 1):
                widths[cid] = float(array[i + 2])
            i += 3
    return widths


# ---------------------------------------------------------------- font programs


def _font_program(descriptor) -> tuple[bytes | None, str | None]:
    if descriptor is None:
        return None, None
    if "/FontFile2" in descriptor:
        return descriptor.FontFile2.read_bytes(), "truetype"
    if "/FontFile3" in descriptor:
        stream = descriptor.FontFile3
        subtype = _name(stream.get("/Subtype", ""))
        return stream.read_bytes(), "opentype" if subtype == "OpenType" else "cff"
    if "/FontFile" in descriptor:
        return descriptor.FontFile.read_bytes(), "type1"
    return None, None


def read_font(font_dict) -> PdfFont:
    subtype = _name(font_dict.get("/Subtype", ""))
    base, tag = split_subset_prefix(_name(font_dict.get("/BaseFont", "unnamed")))
    problems = []
    to_unicode = {}
    if "/ToUnicode" in font_dict and isinstance(font_dict.ToUnicode, pikepdf.Stream):
        to_unicode = parse_to_unicode(font_dict.ToUnicode.read_bytes())
    key = font_dict.objgen if font_dict.is_indirect else ("direct", base)

    if subtype == "Type0":
        cid_font = font_dict.DescendantFonts[0]
        descriptor = cid_font.get("/FontDescriptor")
        encoding = _name(font_dict.get("/Encoding", ""))
        if not encoding.startswith("Identity"):
            problems.append(f"cmap {encoding} read as 2-byte identity")
        cid_to_gid = None
        mapping = cid_font.get("/CIDToGIDMap")
        if isinstance(mapping, pikepdf.Stream):
            cid_to_gid = mapping.read_bytes()
        program, kind = _font_program(descriptor)
        return PdfFont(
            key, base, tag, subtype, 2, _cid_widths(cid_font), float(cid_font.get("/DW", 1000)), to_unicode, {},
            program, kind, cid_to_gid, True, *_vertical(descriptor), problems,
        )  # fmt: skip

    descriptor = font_dict.get("/FontDescriptor")
    program, kind = _font_program(descriptor)
    if subtype == "Type3":
        problems.append("type3 font: glyphs are drawn by procedures and cannot be rebuilt")
    flags = int(descriptor.get("/Flags", 0)) if descriptor is not None else 0
    default_width = float(descriptor.get("/MissingWidth", 0)) if descriptor is not None else 0.0
    widths = _simple_widths(font_dict)
    if not widths and program is None and base in STANDARD_14:
        widths = standard_14_widths(base)
    return PdfFont(
        key, base, tag, subtype, 1, widths, default_width, to_unicode, _simple_encoding(font_dict),
        program, kind, None, bool(flags & SYMBOLIC_FLAG), *_vertical(descriptor), problems,
    )  # fmt: skip


def _vertical(descriptor) -> tuple[float, float]:
    if descriptor is None:
        return 750.0, -250.0
    return float(descriptor.get("/Ascent", 750)), float(descriptor.get("/Descent", -250))


def standard_14_widths(base: str) -> dict[int, float]:
    """widths of a standard font that the pdf did not list, from reportlab's afm data."""
    from reportlab.pdfbase import pdfmetrics

    face = pdfmetrics.getFont(base).face
    names = _base_encoding_names("WinAnsiEncoding")
    return {code: float(face.glyphWidths.get(name, 0)) for code, name in names.items() if name in face.glyphWidths}


def fonts_of_resources(resources) -> dict[str, PdfFont]:
    found = {}
    fonts = resources.get("/Font") if resources is not None else None
    if fonts is None:
        return found
    for name, font_dict in fonts.items():
        found[name] = read_font(font_dict)
    return found
