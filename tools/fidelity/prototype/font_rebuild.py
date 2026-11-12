"""embedded pdf font programs -> clean truetype fonts that word can embed and use.

a pdf subset font is often unusable as it stands: no cmap or a symbolic one,
glyph names instead of unicode, cubic cff outlines word will not embed (see
e1), several subsets of one face. so each face is rebuilt from scratch with
fontTools: only the glyphs the document draws, truetype outlines, a cmap from
the pdf's own tounicode or encoding, the pdf's widths as advances and a unique
family name. glyphs whose text is missing, ambiguous or a ligature get a
private use code point; the report counts them because their text in the docx
is not the real text.
"""

from __future__ import annotations

import hashlib
import io
import os
import tempfile
from dataclasses import dataclass, field

from fontTools.cffLib import CFFFontSet
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.recordingPen import DecomposingRecordingPen, RecordingPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

from prototype.pdf_fonts import PdfFont

PRIVATE_USE_START = 0xE000
# cubic to quadratic conversion tolerance, in font units at 1000 units per em
CUBIC_TOLERANCE_UNITS = 1.0
FS_TYPE_RESTRICTED = 0x0002
FS_TYPE_PREVIEW_PRINT = 0x0004
FS_TYPE_EDITABLE = 0x0008


def embedding_permission(fs_type: int | None) -> str:
    """what os/2 fsType allows for a font embedded in an editable document."""
    if fs_type is None:
        return "not stated (cff program without os/2)"
    if fs_type & FS_TYPE_EDITABLE:
        return "editable embedding"
    if fs_type & FS_TYPE_PREVIEW_PRINT:
        return "preview and print only: word will open the docx read-only"
    if fs_type & FS_TYPE_RESTRICTED:
        return "restricted: must not be embedded"
    return "installable"


def may_embed(fs_type: int | None) -> bool:
    if fs_type is None:
        return True
    restricted_only = fs_type & FS_TYPE_RESTRICTED and not fs_type & (FS_TYPE_PREVIEW_PRINT | FS_TYPE_EDITABLE)
    return not restricted_only


# ---------------------------------------------------------------- reading programs


class GlyphSource:
    """draws glyphs of one embedded program by char code."""

    def __init__(self, font: PdfFont):
        self.font = font
        self.cubic = False
        self.units_per_em = 1000
        self.fs_type: int | None = None
        self._glyph_set = None
        self._glyph_order: list[str] = []
        self._cmaps: dict[tuple[int, int], dict[int, str]] = {}
        self._cff_top = None
        kind = font.program_kind
        if kind in ("truetype", "opentype"):
            self._load_sfnt(font.program)
        elif kind == "cff":
            self._load_cff(font.program)
        elif kind == "type1":
            self._load_type1(font.program)
        else:
            raise ValueError(f"no usable program for {font.base_name}")

    def _load_sfnt(self, data: bytes) -> None:
        sfnt = TTFont(io.BytesIO(data))
        self.units_per_em = sfnt["head"].unitsPerEm
        self.cubic = "CFF " in sfnt
        self._glyph_set = sfnt.getGlyphSet()
        self._glyph_order = sfnt.getGlyphOrder()
        if "OS/2" in sfnt:
            self.fs_type = sfnt["OS/2"].fsType
        if "cmap" in sfnt:
            for table in sfnt["cmap"].tables:
                self._cmaps[(table.platformID, table.platEncID)] = dict(table.cmap)
        if self.cubic:
            self._cff_top = sfnt["CFF "].cff.topDictIndex[0]

    def _load_cff(self, data: bytes) -> None:
        cff = CFFFontSet()
        cff.decompile(io.BytesIO(data), None)
        top = cff[cff.fontNames[0]]
        self._cff_top = top
        self.cubic = True
        matrix = getattr(top, "FontMatrix", [0.001])
        self.units_per_em = round(1 / matrix[0]) if matrix[0] else 1000
        self._glyph_set = top.CharStrings
        self._glyph_order = list(top.charset)

    def _load_type1(self, data: bytes) -> None:
        from fontTools.t1Lib import T1Font

        # t1Lib reads from a path; the bytes are written to a private temp file
        handle, path = tempfile.mkstemp(suffix=".pfa")
        try:
            with os.fdopen(handle, "wb") as out:
                out.write(data)
            t1 = T1Font(path)
            t1.parse()
            self._glyph_set = t1.getGlyphSet()
            self._glyph_order = list(self._glyph_set.keys())
            self.cubic = True
        finally:
            os.remove(path)

    def glyph_name(self, code: int) -> str | None:
        font = self.font
        if font.is_cid:
            cid = code
            if self._cff_top is not None and hasattr(self._cff_top, "ROS"):
                name = f"cid{cid:05d}"
                return name if name in self._glyph_set else None
            gid = cid
            if font.cid_to_gid is not None and 2 * cid + 1 < len(font.cid_to_gid):
                gid = int.from_bytes(font.cid_to_gid[2 * cid : 2 * cid + 2], "big")
            return self._glyph_order[gid] if gid < len(self._glyph_order) else None
        name = font.glyph_names.get(code)
        # truetype simple fonts: the pdf spec's cmap lookup order (9.6.6.4)
        for key, lookup in (((3, 0), code), ((3, 0), 0xF000 + code), ((1, 0), code)):
            mapping = self._cmaps.get(key)
            if mapping and lookup in mapping and (font.symbolic or not name or key != (3, 0)):
                return mapping[lookup]
        text = font.text(code)
        if (3, 1) in self._cmaps and len(text) == 1 and ord(text) in self._cmaps[(3, 1)]:
            return self._cmaps[(3, 1)][ord(text)]
        if name and name in self._glyph_set:
            return name
        if (
            self._cff_top is not None
            and hasattr(self._cff_top, "Encoding")
            and isinstance(self._cff_top.Encoding, list)
        ):
            encoded = self._cff_top.Encoding[code] if code < len(self._cff_top.Encoding) else None
            if encoded and encoded in self._glyph_set:
                return encoded
        for mapping in self._cmaps.values():
            if code in mapping:
                return mapping[code]
        return None

    def record(self, glyph_name: str) -> RecordingPen:
        pen = DecomposingRecordingPen(self._glyph_set) if not self.cubic or self._cff_top is None else RecordingPen()
        self._glyph_set[glyph_name].draw(pen)
        return pen


# ---------------------------------------------------------------- rebuilding a face


@dataclass
class RebuiltFace:
    face_key: str
    family: str
    path: str
    char_for: dict[tuple, str]  # (pdf font key, code) -> the char written in the docx
    units_per_em: int
    fs_type: int | None
    permission: str
    cubic_source: bool
    outline_error_em: float  # bound on the cubic to quadratic error, in em
    private_use: int  # glyphs that got a private use code point
    substitute_for: str | None = None  # set when this is a stand-in for a non-embedded font
    problems: list[str] = field(default_factory=list)


def face_key(font: PdfFont) -> str:
    """several subsets of one face share the name without the subset prefix."""
    return f"{font.base_name}|{font.program_kind}"


def unique_family(face: str, document_tag: str) -> str:
    digest = hashlib.sha1(f"{document_tag}|{face}".encode()).hexdigest()[:8]
    return f"Ltr {digest}"


def _ascii_glyph_name(index: int) -> str:
    return f"g{index}"


def rebuild_face(fonts: list[PdfFont], used_codes: dict[tuple, set[int]], family: str, out_path: str) -> RebuiltFace:
    """one truetype font from every subset of a face, holding only the glyphs in use."""
    sources = {font.key: GlyphSource(font) for font in fonts}
    first = sources[fonts[0].key]
    units_per_em = first.units_per_em
    glyph_order = [".notdef", "space"]
    outlines: dict[str, object] = {}
    advances: dict[str, int] = {".notdef": units_per_em // 2, "space": units_per_em // 4}
    cmap: dict[int, str] = {0x20: "space"}
    by_outline: dict[tuple, str] = {}
    char_for: dict[tuple, str] = {}
    next_private = PRIVATE_USE_START
    problems = []
    error_bound = CUBIC_TOLERANCE_UNITS / 1000 if any(s.cubic for s in sources.values()) else 0.0

    for font in fonts:
        source = sources[font.key]
        scale = units_per_em / source.units_per_em
        for code in sorted(used_codes.get(font.key, ())):
            text = font.text(code)
            name = source.glyph_name(code)
            if name is None:
                problems.append(f"{font.base_name}: code {code} has no glyph in the embedded program")
                continue
            recording = source.record(name)
            signature = (tuple(recording.value), round(font.width(code)))
            if text == " " or not recording.value and text.strip() == "":
                char_for[(font.key, code)] = " "
                continue
            if signature in by_outline:
                glyph = by_outline[signature]
            else:
                glyph = _ascii_glyph_name(len(glyph_order))
                glyph_order.append(glyph)
                outlines[glyph] = _truetype_glyph(recording, source.cubic and source._cff_top is not None, scale)
                advances[glyph] = round(font.width(code) * units_per_em / 1000)
                by_outline[signature] = glyph
            existing = [cp for cp, g in cmap.items() if g == glyph]
            if existing:
                char_for[(font.key, code)] = chr(existing[0])
                continue
            if len(text) == 1 and ord(text) not in cmap and not 0xD800 <= ord(text) <= 0xDFFF:
                point = ord(text)
            else:
                # no text, a ligature or a second glyph for the same char: a private code point keeps the glyph
                point = next_private
                next_private += 1
            cmap[point] = glyph
            char_for[(font.key, code)] = chr(point)

    _write_font(out_path, family, units_per_em, glyph_order, outlines, advances, cmap, fonts[0])
    fs_type = first.fs_type
    return RebuiltFace(
        face_key(fonts[0]), family, out_path, char_for, units_per_em, fs_type, embedding_permission(fs_type),
        any(s.cubic for s in sources.values()), error_bound, next_private - PRIVATE_USE_START, None, problems,
    )  # fmt: skip


def _truetype_glyph(recording: RecordingPen, cubic: bool, scale: float):
    pen = TTGlyphPen(None)
    target = Cu2QuPen(pen, CUBIC_TOLERANCE_UNITS / scale, reverse_direction=True) if cubic else pen
    scaled = _ScalingPen(target, scale)
    recording.replay(scaled)
    return pen.glyph()


class _ScalingPen:
    """passes outline commands on, scaled to the target units per em."""

    def __init__(self, pen, scale: float):
        self.pen, self.scale = pen, scale

    def _s(self, points):
        return [(x * self.scale, y * self.scale) for x, y in points]

    def moveTo(self, p):  # noqa: N802 - pen protocol names
        self.pen.moveTo(*self._s([p]))

    def lineTo(self, p):  # noqa: N802
        self.pen.lineTo(*self._s([p]))

    def curveTo(self, *points):  # noqa: N802
        self.pen.curveTo(*self._s(points))

    def qCurveTo(self, *points):  # noqa: N802
        scaled = self._s([p for p in points if p is not None])
        if points and points[-1] is None:
            scaled.append(None)
        self.pen.qCurveTo(*scaled)

    def closePath(self):  # noqa: N802
        self.pen.closePath()

    def endPath(self):  # noqa: N802
        self.pen.endPath()

    def addComponent(self, name, transform):  # noqa: N802
        raise ValueError("components must be decomposed before scaling")


def _write_font(path, family, units_per_em, glyph_order, outlines, advances, cmap, pdf_font: PdfFont) -> None:
    builder = FontBuilder(units_per_em, isTTF=True)
    builder.setupGlyphOrder(glyph_order)
    builder.setupCharacterMap(cmap)
    empty = TTGlyphPen(None).glyph()
    glyphs = {name: outlines.get(name, empty) for name in glyph_order}
    builder.setupGlyf(glyphs)
    metrics = {}
    for name in glyph_order:
        glyph = glyphs[name]
        glyph.recalcBounds(builder.font["glyf"])
        metrics[name] = (advances.get(name, 0), getattr(glyph, "xMin", 0) or 0)
    builder.setupHorizontalMetrics(metrics)
    ascent = round(pdf_font.ascent * units_per_em / 1000) or round(0.8 * units_per_em)
    descent = round(pdf_font.descent * units_per_em / 1000) or -round(0.2 * units_per_em)
    builder.setupHorizontalHeader(ascent=ascent, descent=descent)
    postscript = family.replace(" ", "")
    builder.setupNameTable({"familyName": family, "styleName": "Regular", "uniqueFontIdentifier": f"{postscript};letters",
                            "fullName": f"{family} Regular", "psName": f"{postscript}-Regular"})  # fmt: skip
    builder.setupOS2(sTypoAscender=ascent, sTypoDescender=descent, usWinAscent=ascent, usWinDescent=-descent,
                     fsType=0, achVendID="LTRS")  # fmt: skip
    builder.setupPost()
    os2 = builder.font["OS/2"]
    os2.fsSelection = 0x40  # regular
    os2.recalcUnicodeRanges(builder.font)
    os2.recalcCodePageRanges(builder.font)
    # word matches fonts against the docx charset; a font that claims no code page
    # at all was silently replaced by times new roman, so latin 1 is always claimed
    os2.ulCodePageRange1 |= 1
    builder.save(path)


# ---------------------------------------------------------------- stand-ins for fonts that are not embedded

SUBSTITUTES = {
    # metric-compatible open fonts (ofl): arimo for helvetica/arial, tinos for times, cousine for courier
    "Helvetica": ("arial", "regular"), "Helvetica-Bold": ("arial", "bold"),
    "Helvetica-Oblique": ("arial", "regular"), "Helvetica-BoldOblique": ("arial", "bold"),
    "Arial": ("arial", "regular"), "ArialMT": ("arial", "regular"), "Arial-BoldMT": ("arial", "bold"),
    "Times-Roman": ("times", "regular"), "Times-Bold": ("times", "bold"),
    "Times-Italic": ("times", "regular"), "Times-BoldItalic": ("times", "bold"),
    "TimesNewRomanPSMT": ("times", "regular"),
    "Courier": ("mono", "regular"), "Courier-Bold": ("mono", "regular"),
    "Courier-Oblique": ("mono", "regular"), "Courier-BoldOblique": ("mono", "regular"),
}  # fmt: skip


def substitute_file(base_name: str, serif_hint: bool) -> tuple[str, str]:
    """an open, embeddable stand-in and a note saying how close it is."""
    from corpus_builder import fonts as open_fonts

    role, style = SUBSTITUTES.get(base_name, ("times" if serif_hint else "arial", "regular"))
    note = "metric-compatible stand-in" if base_name in SUBSTITUTES else "generic stand-in, metrics differ"
    if base_name.endswith(("Oblique", "Italic")) or (base_name.startswith("Courier") and base_name != "Courier"):
        note += ", style not available (upright or regular used)"
    return open_fonts.font_file(role, style), note


def rebuild_substitute(
    fonts: list[PdfFont], used_codes: dict[tuple, set[int]], family: str, out_path: str
) -> RebuiltFace:
    """a non-embedded font: the stand-in's glyphs, the pdf's widths, so positions stay exact."""
    path, note = substitute_file(fonts[0].base_name, fonts[0].base_name.startswith("Times"))
    stand_in = TTFont(path)
    units_per_em = stand_in["head"].unitsPerEm
    glyph_set = stand_in.getGlyphSet()
    best_cmap = stand_in.getBestCmap()
    glyph_order, outlines = [".notdef", "space"], {}
    advances = {".notdef": units_per_em // 2, "space": units_per_em // 4}
    cmap = {0x20: "space"}
    char_for = {}
    problems = []
    for font in fonts:
        for code in sorted(used_codes.get(font.key, ())):
            text = font.text(code)
            if text == " ":
                char_for[(font.key, code)] = " "
                continue
            if len(text) != 1 or ord(text) not in best_cmap:
                problems.append(f"{font.base_name}: {text!r} missing in the stand-in")
                continue
            if ord(text) in cmap:
                char_for[(font.key, code)] = text
                continue
            glyph = _ascii_glyph_name(len(glyph_order))
            glyph_order.append(glyph)
            recording = DecomposingRecordingPen(glyph_set)
            glyph_set[best_cmap[ord(text)]].draw(recording)
            outlines[glyph] = _truetype_glyph(recording, False, 1.0)
            advances[glyph] = round(font.width(code) * units_per_em / 1000)
            cmap[ord(text)] = glyph
            char_for[(font.key, code)] = text
    _write_font(out_path, family, units_per_em, glyph_order, outlines, advances, cmap, fonts[0])
    face = RebuiltFace(face_key(fonts[0]), family, out_path, char_for, units_per_em, 0, embedding_permission(0),
                       False, 0.0, 0, f"{fonts[0].base_name} -> {os.path.basename(path)} ({note})", problems)  # fmt: skip
    return face
