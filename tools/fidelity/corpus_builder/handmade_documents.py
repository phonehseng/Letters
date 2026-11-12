"""corpus pages built operator by operator with pikepdf, for font paths no producer on this machine writes.

word only writes truetype; reportlab only truetype and the standard 14; the
browser (skia) writes truetype as cid fonts and turns cff into type 3. so a
bare cff program (FontFile3 /Type1C), a type 1 program (FontFile), a font
that forbids embedding (fsType 2) and justified text with heavy TJ kerning are
written here directly. every page holds real text with a ToUnicode-free
WinAnsi encoding, the way simple fonts usually are.

fonts (all sil ofl 1.1): Source Serif 4 (cff, embedded unmodified), Crimson Text
(converted to type 1 and renamed "Letters One Serif", since a modified ofl font
must not keep its name), Lato (embedded as is, and a renamed copy "Letters
Locked Sans" whose fsType says no embedding).
"""

from __future__ import annotations

import io
import os
import random

import pikepdf
from fontTools.misc import eexec
from fontTools.misc.psCharStrings import T1CharString
from fontTools.pens.basePen import BasePen
from fontTools.ttLib import TTFont

from corpus_builder import fonts
from corpus_builder.text import paragraph_text

LETTER = (612.0, 792.0)
HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WIN_ANSI = {
    code: bytes([code]).decode("cp1252") for code in range(32, 256) if code not in (127, 129, 141, 143, 144, 157)
}


# ---------------------------------------------------------------- font dictionaries


def _glyph_for_char(font: TTFont, char: str) -> str | None:
    return font.getBestCmap().get(ord(char))


def _widths(font: TTFont, scale: float) -> list[float]:
    """/Widths for codes 32..255 in 1/1000 em (0 for codes the font cannot draw)."""
    hmtx = font["hmtx"]
    widths = []
    for code in range(32, 256):
        glyph = _glyph_for_char(font, WIN_ANSI[code]) if code in WIN_ANSI else None
        widths.append(round(hmtx[glyph][0] * scale, 3) if glyph else 0)
    return widths


def _descriptor(pdf: pikepdf.Pdf, font: TTFont, name: str, scale: float, file_key: str, stream) -> pikepdf.Dictionary:
    head, hhea = font["head"], font["hhea"]
    return pikepdf.Dictionary(
        Type=pikepdf.Name.FontDescriptor, FontName=pikepdf.Name("/" + name), Flags=32,
        FontBBox=[round(v * scale) for v in (head.xMin, head.yMin, head.xMax, head.yMax)], ItalicAngle=0,
        Ascent=round(hhea.ascent * scale), Descent=round(hhea.descent * scale), CapHeight=round(hhea.ascent * scale * 0.9),
        StemV=80, **{file_key.lstrip("/"): stream},
    )  # fmt: skip


def truetype_font(pdf: pikepdf.Pdf, path: str, name: str, fs_type: int | None = None) -> tuple[pikepdf.Object, TTFont]:
    """a simple truetype font, WinAnsi, the whole program embedded (fsType optionally rewritten)."""
    font = TTFont(path)
    if fs_type is not None:
        font["OS/2"].fsType = fs_type
    buffer = io.BytesIO()
    font.save(buffer)
    data = buffer.getvalue()
    scale = 1000 / font["head"].unitsPerEm
    stream = pdf.make_stream(data)
    stream.Length1 = len(data)
    font_dict = pikepdf.Dictionary(
        Type=pikepdf.Name.Font, Subtype=pikepdf.Name.TrueType, BaseFont=pikepdf.Name("/" + name), FirstChar=32,
        LastChar=255, Widths=_widths(font, scale), Encoding=pikepdf.Name.WinAnsiEncoding,
        FontDescriptor=pdf.make_indirect(_descriptor(pdf, font, name, scale, "/FontFile2", stream)),
    )  # fmt: skip
    return pdf.make_indirect(font_dict), font


def cff_font(pdf: pikepdf.Pdf, otf_path: str, name: str) -> tuple[pikepdf.Object, TTFont]:
    """a simple Type1 font whose program is the otf's bare cff table (FontFile3 /Type1C)."""
    font = TTFont(otf_path)
    cff_bytes = font.reader["CFF "]
    scale = 1000 / font["head"].unitsPerEm
    stream = pdf.make_stream(cff_bytes)
    stream.Subtype = pikepdf.Name("/Type1C")
    font_dict = pikepdf.Dictionary(
        Type=pikepdf.Name.Font, Subtype=pikepdf.Name.Type1, BaseFont=pikepdf.Name("/" + name), FirstChar=32,
        LastChar=255, Widths=_widths(font, scale), Encoding=pikepdf.Name.WinAnsiEncoding,
        FontDescriptor=pdf.make_indirect(_descriptor(pdf, font, name, scale, "/FontFile3", stream)),
    )  # fmt: skip
    return pdf.make_indirect(font_dict), font


class _Type1Pen(BasePen):
    """truetype outlines to type 1 charstring operators (quadratics raised to cubics exactly)."""

    def __init__(self, glyph_set, scale: float):
        super().__init__(glyph_set)
        self.scale = scale
        self.program: list = []
        self.position = (0, 0)

    def _round(self, point) -> tuple[int, int]:
        return round(point[0] * self.scale), round(point[1] * self.scale)

    def _relative(self, point) -> list[int]:
        x, y = self._round(point)
        dx, dy = x - self.position[0], y - self.position[1]
        self.position = (x, y)
        return [dx, dy]

    def _moveTo(self, point):  # noqa: N802 - pen protocol names
        self.program += self._relative(point) + ["rmoveto"]

    def _lineTo(self, point):  # noqa: N802
        self.program += self._relative(point) + ["rlineto"]

    def _curveToOne(self, p1, p2, p3):  # noqa: N802
        self.program += self._relative(p1) + self._relative(p2) + self._relative(p3) + ["rrcurveto"]

    def _qCurveToOne(self, p1, p2):  # noqa: N802
        p0 = self._getCurrentPoint()
        c1 = (p0[0] + 2 / 3 * (p1[0] - p0[0]), p0[1] + 2 / 3 * (p1[1] - p0[1]))
        c2 = (p2[0] + 2 / 3 * (p1[0] - p2[0]), p2[1] + 2 / 3 * (p1[1] - p2[1]))
        self._curveToOne(c1, c2, p2)

    def _closePath(self):  # noqa: N802
        self.program.append("closepath")


def _type1_program(ttf_path: str, name: str, chars: str) -> tuple[bytes, bytes, bytes]:
    """a type 1 font (pfa layout: clear text, eexec hex, 512 zeros) with the glyphs for chars."""
    font = TTFont(ttf_path)
    glyph_set = font.getGlyphSet()
    cmap = font.getBestCmap()
    scale = 1000 / font["head"].unitsPerEm
    charstrings = {}
    for char in sorted(set(chars) | {" "}):
        glyph = cmap.get(ord(char))
        if glyph is None:
            continue
        pen = _Type1Pen(glyph_set, scale)
        glyph_set[glyph].draw(pen)
        advance = round(font["hmtx"][glyph][0] * scale)
        program = [0, advance, "hsbw"] + pen.program + ["endchar"]
        compiled = T1CharString(program=program)
        compiled.compile()
        charstrings[_ps_name(glyph, char)] = eexec.encrypt(b"\0\0\0\0" + compiled.bytecode, 4330)[0]
    private = [b"dup /Private 8 dict dup begin", b"/RD{string currentfile exch readstring pop}executeonly def",
               b"/ND{noaccess def}executeonly def", b"/NP{noaccess put}executeonly def", b"/lenIV 4 def",
               b"/BlueValues [] def", b"/MinFeature{16 16} def", b"/password 5839 def", b"/Subrs 0 array ND",
               f"2 index /CharStrings {len(charstrings)} dict dup begin".encode()]  # fmt: skip
    body = b"\n".join(private) + b"\n"
    for glyph_name, data in charstrings.items():
        body += f"/{glyph_name} {len(data)} RD ".encode() + data + b" ND\n"
    body += b"end\nend\nreadonly put\nnoaccess put\ndup /FontName get exch definefont pop\nmark currentfile closefile\n"
    encrypted, _ = eexec.encrypt(b"\0\0\0\0" + body, 55665)
    clear = (
        f"%!PS-AdobeFont-1.0: {name} 001.000\n11 dict begin\n/FontInfo 2 dict dup begin /FamilyName "
        f"({name}) readonly def end readonly def\n/FontName /{name} def\n/Encoding StandardEncoding def\n"
        f"/PaintType 0 def\n/FontType 1 def\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n"
        f"/FontBBox {{-200 -300 1200 1000}} readonly def\ncurrentdict end\ncurrentfile eexec\n"
    ).encode()  # fmt: skip
    hex_lines = encrypted.hex()
    hex_part = "\n".join(hex_lines[i : i + 64] for i in range(0, len(hex_lines), 64)).encode() + b"\n"
    trailer = ("0" * 64 + "\n") * 8 + "cleartomark\n"
    return clear, hex_part, trailer.encode()


def _ps_name(glyph: str, char: str) -> str:
    """the glyph name the WinAnsi encoding expects for char (the pdf maps codes to names)."""
    from fontTools.agl import UV2AGL

    return UV2AGL.get(ord(char), glyph)


def type1_font(pdf: pikepdf.Pdf, ttf_path: str, name: str, chars: str) -> tuple[pikepdf.Object, TTFont]:
    font = TTFont(ttf_path)
    clear, hex_part, trailer = _type1_program(ttf_path, name, chars)
    stream = pdf.make_stream(clear + hex_part + trailer)
    stream.Length1, stream.Length2, stream.Length3 = len(clear), len(hex_part), len(trailer)
    scale = 1000 / font["head"].unitsPerEm
    present = set(chars) | {" "}
    widths = [w if code in WIN_ANSI and WIN_ANSI[code] in present else 0
              for code, w in zip(range(32, 256), _widths(font, scale), strict=True)]  # fmt: skip
    font_dict = pikepdf.Dictionary(
        Type=pikepdf.Name.Font, Subtype=pikepdf.Name.Type1, BaseFont=pikepdf.Name("/" + name), FirstChar=32,
        LastChar=255, Widths=[round(w) for w in widths], Encoding=pikepdf.Name.WinAnsiEncoding,
        FontDescriptor=pdf.make_indirect(_descriptor(pdf, font, name, scale, "/FontFile", stream)),
    )  # fmt: skip
    return pdf.make_indirect(font_dict), font


# ---------------------------------------------------------------- text


def _encode(text: str) -> bytes:
    return text.encode("cp1252", errors="replace")


def _width(text: str, widths: list[float], size: float) -> float:
    return sum(widths[b - 32] for b in _encode(text)) / 1000 * size


def wrap(text: str, widths: list[float], size: float, width: float) -> list[str]:
    lines, current = [], ""
    for word in text.split():
        candidate = f"{current} {word}".strip()
        if _width(candidate, widths, size) > width and current:
            lines.append(current)
            current = word
        else:
            current = candidate
    return lines + ([current] if current else [])


def _pdf_string(data: bytes) -> str:
    return "(" + "".join(f"\\{b:03o}" if b < 32 or b > 126 or b in b"()\\" else chr(b) for b in data) + ")"


def plain_line(font: str, size: float, x: float, y: float, text: str, rgb=(0, 0, 0)) -> str:
    return f"BT {rgb[0]} {rgb[1]} {rgb[2]} rg /{font} {size} Tf 1 0 0 1 {x} {y} Tm {_pdf_string(_encode(text))} Tj ET\n"


def justified_line(font: str, size: float, x: float, y: float, text: str, widths: list[float], width: float,
                   rng: random.Random) -> str:  # fmt: skip
    """TJ with a kerning number between most glyph pairs, then Tw stretches the spaces to the full width."""
    data = _encode(text)
    parts, kerning_total = [], 0.0
    for index, byte in enumerate(data):
        parts.append(_pdf_string(bytes([byte])))
        if index + 1 < len(data) and byte != 32 and data[index + 1] != 32:
            kern = rng.randint(-45, 25)  # heavy: well beyond a font's own kerning
            parts.append(str(kern))
            kerning_total += kern
    natural = _width(text, widths, size) - kerning_total / 1000 * size
    spaces = data.count(32)
    word_spacing = (width - natural) / spaces if spaces else 0.0
    return f"BT /{font} {size} Tf {word_spacing:.4f} Tw 1 0 0 1 {x} {y} Tm [{' '.join(parts)}] TJ ET\n"


def _save(pdf: pikepdf.Pdf, page_content: str, fonts_by_name: dict, path: str) -> None:
    page = pdf.add_blank_page(page_size=LETTER)
    page.Resources = pikepdf.Dictionary(Font=pikepdf.Dictionary({f"/{k}": v for k, v in fonts_by_name.items()}))
    page.Contents = pdf.make_stream(page_content.encode("latin-1"))
    pdf.docinfo["/Producer"] = "Letters fidelity corpus (pikepdf, hand-built operators)"
    pdf.save(path)


# ---------------------------------------------------------------- documents


def tj_kerning(out_dir: str) -> tuple[str, dict]:
    path = os.path.join(out_dir, "tj_kerning.pdf")
    pdf = pikepdf.new()
    font, _ = truetype_font(pdf, fonts.font_file("sans"), "Lato-Regular")
    widths = [float(w) for w in font.Widths]
    rng = random.Random(41)
    content = plain_line("F1", 16, 72, 720, "Justified text with heavy TJ kerning")
    y = 690
    for paragraph in range(4):
        lines = wrap(paragraph_text(300 + paragraph, 5), widths, 10.5, 468)
        for index, line in enumerate(lines):
            last = index == len(lines) - 1
            content += (
                plain_line("F1", 10.5, 72, y, line)
                if last
                else justified_line("F1", 10.5, 72, y, line, widths, 468, rng)
            )
            y -= 14
        y -= 8
    _save(pdf, content, {"F1": font}, path)
    return path, {"class": "justified TJ kerning", "producer": "hand-built with pikepdf (simple TrueType, Lato)",
                  "features": ["justified with Tw", "TJ kerning number between most glyph pairs (-45..25 /1000 em)"],
                  "known_phrase": "heavy TJ kerning", "known_sentence": ""}  # fmt: skip


def cff(out_dir: str) -> tuple[str, dict]:
    path = os.path.join(out_dir, "cff_font.pdf")
    pdf = pikepdf.new()
    font, _ = cff_font(pdf, os.path.join(HERE, "fonts", "SourceSerif4-Regular.otf"), "SourceSerif4-Regular")
    widths = [float(w) for w in font.Widths]
    content = plain_line("F1", 18, 72, 720, "A bare CFF program (FontFile3 Type1C)")
    y = 690
    for paragraph in range(5):
        for line in wrap(paragraph_text(400 + paragraph, 4), widths, 11, 468):
            content += plain_line("F1", 11, 72, y, line)
            y -= 15
        y -= 8
    _save(pdf, content, {"F1": font}, path)
    return path, {"class": "CFF font", "producer": "hand-built with pikepdf (Source Serif 4 CFF as FontFile3/Type1C)",
                  "features": ["simple Type1 font with a bare CFF program", "WinAnsi encoding by glyph name"],
                  "known_phrase": "bare CFF program", "known_sentence": ""}  # fmt: skip


def type1(out_dir: str) -> tuple[str, dict]:
    path = os.path.join(out_dir, "type1_font.pdf")
    pdf = pikepdf.new()
    texts = [paragraph_text(500 + p, 4) for p in range(5)]
    title = "A Type 1 program (FontFile)"
    font, _ = type1_font(pdf, fonts.font_file("serif"), "LettersOneSerif", title + "".join(texts))
    widths = [float(w) for w in font.Widths]
    content = plain_line("F1", 18, 72, 720, title)
    y = 690
    for text in texts:
        for line in wrap(text, widths, 11, 468):
            content += plain_line("F1", 11, 72, y, line)
            y -= 15
        y -= 8
    _save(pdf, content, {"F1": font}, path)
    return path, {"class": "Type 1 font", "producer": "hand-built with pikepdf (Crimson Text outlines as a Type 1 program)",
                  "features": ["simple Type1 font with FontFile (eexec, lenIV 4)", "WinAnsi encoding by glyph name"],
                  "known_phrase": "Type 1 program", "known_sentence": ""}  # fmt: skip


def restricted(out_dir: str) -> tuple[str, dict]:
    path = os.path.join(out_dir, "restricted_font.pdf")
    renamed = os.path.join(HERE, "fonts", "build", "corpus", "LettersLockedSans.ttf")
    if not os.path.exists(renamed):
        from font_prep import rename_family

        font = TTFont(fonts.font_file("sans"))
        rename_family(font, "Letters Locked Sans")
        font.save(renamed)
    pdf = pikepdf.new()
    font, _ = truetype_font(pdf, renamed, "LettersLockedSans-Regular", fs_type=0x0002)
    widths = [float(w) for w in font.Widths]
    content = plain_line("F1", 18, 72, 720, "A font whose fsType forbids embedding")
    y = 690
    for paragraph in range(3):
        for line in wrap(paragraph_text(600 + paragraph, 4), widths, 11, 468):
            content += plain_line("F1", 11, 72, y, line)
            y -= 15
        y -= 8
    _save(pdf, content, {"F1": font}, path)
    return path, {"class": "restricted fsType", "producer": "hand-built with pikepdf (Lato renamed, fsType 2)",
                  "features": ["embedded TrueType with fsType 0x0002 (restricted licence embedding)"],
                  "known_phrase": "forbids embedding", "known_sentence": ""}  # fmt: skip


BUILDERS = {"tj_kerning": tj_kerning, "cff_font": cff, "type1_font": type1, "restricted_font": restricted}
