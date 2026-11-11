"""reading pdfs with pypdfium2: page rasters, per-character origins and the fonts used.

everything here is read-only and works in pdf points (1/72 inch), origin bottom-left,
which is pdfium's native page space.
"""

from __future__ import annotations

import ctypes
import io
from dataclasses import dataclass, field

import numpy as np
import pypdfium2 as pdfium
import pypdfium2.raw as pdfium_raw
from fontTools.ttLib import TTFont


@dataclass
class PlacedChar:
    """one character as pdfium sees it: the pen origin is what layout controls,
    the box is the inked bounds and depends on the glyph outline."""

    page: int
    char: str
    origin_x: float
    origin_y: float
    box: tuple[float, float, float, float]  # left, right, bottom, top
    font_size: float
    font_name: str


@dataclass
class FontUse:
    base_name: str
    family_name: str
    embedded: bool
    font_program: str  # truetype, cff or unknown, read from the embedded data
    embedded_family: str  # name id 1 inside the embedded program, if readable
    char_count: int = 0
    pages: set[int] = field(default_factory=set)


def render_pages_gray(pdf_path: str, dpi: int = 300) -> list[np.ndarray]:
    """render every page to an 8-bit grayscale array, white background."""
    document = pdfium.PdfDocument(pdf_path)
    try:
        scale = dpi / 72.0
        pages = []
        for page in document:
            bitmap = page.render(scale=scale, grayscale=True)
            pages.append(np.array(bitmap.to_pil().convert("L")))
            page.close()
        return pages
    finally:
        document.close()


def page_sizes(pdf_path: str) -> list[tuple[float, float]]:
    document = pdfium.PdfDocument(pdf_path)
    try:
        return [document.get_page_size(i) for i in range(len(document))]
    finally:
        document.close()


def _read_utf16_name(getter, handle) -> str:
    # pdfium name getters return the byte length needed, then fill a buffer
    needed = getter(handle, None, 0)
    if needed <= 0:
        return ""
    buffer = ctypes.create_string_buffer(needed)
    getter(handle, buffer, needed)
    return buffer.raw[: needed - 1].decode("utf-8", errors="replace")


def _font_name_of_char(text_page, index: int) -> str:
    needed = pdfium_raw.FPDFText_GetFontInfo(text_page, index, None, 0, None)
    if needed <= 0:
        return ""
    buffer = ctypes.create_string_buffer(needed)
    flags = ctypes.c_int(0)
    pdfium_raw.FPDFText_GetFontInfo(text_page, index, buffer, needed, ctypes.byref(flags))
    return buffer.raw[: needed - 1].decode("utf-8", errors="replace")


def extract_chars(pdf_path: str) -> list[PlacedChar]:
    """all non-generated characters in reading order, page by page.

    generated characters are spaces and line breaks pdfium invents from gaps,
    they have no glyph so they are dropped."""
    document = pdfium.PdfDocument(pdf_path)
    chars: list[PlacedChar] = []
    try:
        for page_index in range(len(document)):
            page = document[page_index]
            text_page = page.get_textpage()
            raw_text_page = text_page.raw
            x = ctypes.c_double()
            y = ctypes.c_double()
            left, right, bottom, top = (ctypes.c_double() for _ in range(4))
            for i in range(text_page.count_chars()):
                if pdfium_raw.FPDFText_IsGenerated(raw_text_page, i) == 1:
                    continue
                code = pdfium_raw.FPDFText_GetUnicode(raw_text_page, i)
                char = chr(code) if code else ""
                if not char or char.isspace():
                    continue
                pdfium_raw.FPDFText_GetCharOrigin(raw_text_page, i, x, y)
                pdfium_raw.FPDFText_GetCharBox(raw_text_page, i, left, right, bottom, top)
                chars.append(
                    PlacedChar(
                        page=page_index,
                        char=char,
                        origin_x=x.value,
                        origin_y=y.value,
                        box=(left.value, right.value, bottom.value, top.value),
                        font_size=pdfium_raw.FPDFText_GetFontSize(raw_text_page, i),
                        font_name=_font_name_of_char(raw_text_page, i),
                    )
                )
            text_page.close()
            page.close()
    finally:
        document.close()
    return chars


def _describe_font_program(data: bytes) -> tuple[str, str]:
    """say whether an embedded font program is truetype or cff and read its family name.

    this is the strongest proof of which font really got used, a substitute
    would carry its own name table."""
    if not data:
        return "none", ""
    if data[:4] in (b"\x00\x01\x00\x00", b"true", b"OTTO"):
        try:
            font = TTFont(io.BytesIO(data), lazy=True)
            program = "cff-opentype" if "CFF " in font else "truetype"
            family = (font["name"].getDebugName(1) or "") if "name" in font else ""
            return program, family
        except Exception as error:  # broken subsets still tell us the flavour
            return ("cff-opentype" if data[:4] == b"OTTO" else "truetype"), f"unreadable: {error}"
    if data[:1] == b"\x01" and len(data) > 4:
        return "bare-cff", ""
    return "unknown", ""


def _font_data(font_handle) -> bytes:
    needed = ctypes.c_size_t(0)
    pdfium_raw.FPDFFont_GetFontData(font_handle, None, 0, ctypes.byref(needed))
    if needed.value == 0:
        return b""
    buffer = (ctypes.c_uint8 * needed.value)()
    pdfium_raw.FPDFFont_GetFontData(font_handle, buffer, needed.value, ctypes.byref(needed))
    return bytes(buffer)


def embedded_font_programs(pdf_path: str) -> dict[str, bytes]:
    """the raw font program behind each base font name that draws text."""
    document = pdfium.PdfDocument(pdf_path)
    programs: dict[str, bytes] = {}
    try:
        for page_index in range(len(document)):
            page = document[page_index]
            text_page = page.get_textpage()
            for i in range(text_page.count_chars()):
                text_object = pdfium_raw.FPDFText_GetTextObject(text_page.raw, i)
                if not text_object:
                    continue
                font_handle = pdfium_raw.FPDFTextObj_GetFont(text_object)
                base = _read_utf16_name(pdfium_raw.FPDFFont_GetBaseFontName, font_handle)
                if base not in programs:
                    programs[base] = _font_data(font_handle)
            text_page.close()
            page.close()
    finally:
        document.close()
    return programs


def _truetype_outline_keys(font: TTFont) -> set[tuple]:
    """a shape fingerprint for every non-empty truetype glyph, independent of glyph names and ids."""
    keys = set()
    glyf = font["glyf"]
    for name in font.getGlyphOrder():
        coordinates, end_points, _flags = glyf[name].getCoordinates(glyf)
        if len(coordinates):
            keys.add((tuple(end_points), tuple(map(tuple, coordinates))))
    return keys


def outline_match_fraction(embedded_program: bytes, reference_font_path: str) -> float | None:
    """share of glyph outlines in a pdf's embedded font that also exist in the reference font.

    a pdf font name like ___WRD_EMBED_SUB_45 says nothing, but outlines cannot lie.
    returns None when either side is not truetype."""
    try:
        embedded = TTFont(io.BytesIO(embedded_program))
        reference = TTFont(reference_font_path)
    except Exception:
        return None
    if "glyf" not in embedded or "glyf" not in reference:
        return None
    embedded_keys = _truetype_outline_keys(embedded)
    if not embedded_keys:
        return None
    return len(embedded_keys & _truetype_outline_keys(reference)) / len(embedded_keys)


def fonts_used(pdf_path: str) -> list[FontUse]:
    """fonts that actually draw characters, keyed by the pdf base font name."""
    document = pdfium.PdfDocument(pdf_path)
    found: dict[str, FontUse] = {}
    try:
        for page_index in range(len(document)):
            page = document[page_index]
            text_page = page.get_textpage()
            for i in range(text_page.count_chars()):
                if pdfium_raw.FPDFText_IsGenerated(text_page.raw, i) == 1:
                    continue
                text_object = pdfium_raw.FPDFText_GetTextObject(text_page.raw, i)
                if not text_object:
                    continue
                font_handle = pdfium_raw.FPDFTextObj_GetFont(text_object)
                base = _read_utf16_name(pdfium_raw.FPDFFont_GetBaseFontName, font_handle)
                if base not in found:
                    is_embedded = bool(pdfium_raw.FPDFFont_GetIsEmbedded(font_handle))
                    program, family = _describe_font_program(_font_data(font_handle))
                    found[base] = FontUse(
                        base_name=base,
                        family_name=_read_utf16_name(pdfium_raw.FPDFFont_GetFamilyName, font_handle),
                        embedded=is_embedded,
                        font_program=program,
                        embedded_family=family,
                    )
                found[base].char_count += 1
                found[base].pages.add(page_index)
            text_page.close()
            page.close()
    finally:
        document.close()
    return list(found.values())
