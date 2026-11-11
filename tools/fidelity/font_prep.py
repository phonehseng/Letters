"""preparing test fonts with fontTools: unique names, subsets and rewritten advances.

renaming matters because word must not be able to fall back on an installed copy,
otherwise we could not tell whether the embedded font was really used. the ofl
also asks that modified fonts do not keep reserved names.
"""

from __future__ import annotations

import winreg

from fontTools import subset as font_subset
from fontTools.ttLib import TTFont
from fontTools.ttLib.scaleUpem import scale_upem

from grid_model import UNITS_PER_PIXEL, drawn_font_size, snap_advance_units, snapped_units_per_em

FONT_REGISTRY_KEYS = [
    (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts"),
    (winreg.HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts"),
]


def installed_font_entries() -> list[str]:
    """names of fonts registered with windows, machine-wide and per user."""
    names = []
    for hive, path in FONT_REGISTRY_KEYS:
        try:
            with winreg.OpenKey(hive, path) as key:
                index = 0
                while True:
                    try:
                        names.append(winreg.EnumValue(key, index)[0])
                    except OSError:
                        break
                    index += 1
        except OSError:
            continue
    return names


def is_family_installed(family: str) -> bool:
    wanted = family.lower().replace(" ", "")
    return any(wanted in entry.lower().replace(" ", "") for entry in installed_font_entries())


def rename_family(font: TTFont, family: str) -> None:
    """give the font a new family name everywhere word and pdf readers look."""
    postscript = family.replace(" ", "") + "-Regular"
    new_names = {
        1: family,
        2: "Regular",
        3: f"{postscript};fidelity-test",
        4: f"{family} Regular",
        6: postscript,
        16: family,
        17: "Regular",
    }
    name_table = font["name"]
    # drop typographic and variation names that would still carry the old family
    name_table.names = [n for n in name_table.names if n.nameID not in (16, 17, 18, 21, 22, 25)]
    for name_id, value in new_names.items():
        name_table.setName(value, name_id, 3, 1, 0x409)
        name_table.setName(value, name_id, 1, 0, 0)
    if "CFF " in font:
        cff = font["CFF "].cff
        top_dict = cff.topDictIndex[0]
        cff.fontNames = [postscript]
        top_dict.FamilyName = family
        top_dict.FullName = f"{family} Regular"


def make_renamed_font(source_path: str, target_path: str, family: str, units_per_em: int | None = None) -> str:
    font = TTFont(source_path)
    if units_per_em is not None and font["head"].unitsPerEm != units_per_em:
        scale_upem(font, units_per_em)
    rename_family(font, family)
    font.save(target_path)
    return target_path


def make_subset_font(source_path: str, target_path: str, text: str, family: str) -> str:
    """keep only the glyphs for text, the way an exporter would to save space."""
    options = font_subset.Options()
    options.name_IDs = ["*"]  # keep our new names
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.layout_features = []  # no kerning or ligature lookups to surprise us
    font = TTFont(source_path)
    subsetter = font_subset.Subsetter(options=options)
    subsetter.populate(text=text)
    subsetter.subset(font)
    rename_family(font, family)
    font.save(target_path)
    return target_path


def make_unhinted_font(source_path: str, target_path: str, family: str) -> str:
    """the same font without truetype instructions or device metrics tables.

    hinting may round advance widths to whole device pixels, which is one
    suspected reason word's advances differ from the font's ideal ones."""
    options = font_subset.Options()
    options.name_IDs = ["*"]
    options.name_languages = ["*"]
    options.glyph_names = True
    options.notdef_outline = True
    options.layout_features = []
    options.hinting = False
    font = TTFont(source_path)
    subsetter = font_subset.Subsetter(options=options)
    subsetter.populate(unicodes=font.getBestCmap().keys())
    subsetter.subset(font)
    for table in ("hdmx", "LTSH", "VDMX", "fpgm", "prep", "cvt "):
        if table in font:
            del font[table]
    # bit 4: instructions may alter advance widths, no longer true
    font["head"].flags &= ~(1 << 4)
    rename_family(font, family)
    font.save(target_path)
    return target_path


def make_grid_font(
    source_path: str,
    target_path: str,
    family: str,
    word_size_pt: float,
    pdf_size_pt: float | None = None,
    units_per_pixel: int = UNITS_PER_PIXEL,
) -> dict:
    """a font whose every advance is a whole 1/600 inch pixel at word_size_pt.

    with pdf_size_pt set, outlines and metrics are also scaled by pdf/word size,
    so text set at a size word can store (half points) looks like the pdf's odd
    size. one variant is needed per (word size, pdf size) pair, that is the cost.
    returns the units per em and how far the effective size is from pdf_size_pt."""
    pdf_size_pt = pdf_size_pt or word_size_pt
    units_per_em = snapped_units_per_em(word_size_pt, units_per_pixel)
    # word lays out at the stored size but draws glyphs at the size it writes
    # into the pdf (whole pixels per em, 10.5 pt is drawn at 10.56), so outlines
    # are scaled against the drawn size
    drawn_size_pt = drawn_font_size(word_size_pt)
    outline_units_per_em = round(units_per_em * pdf_size_pt / drawn_size_pt)
    font = TTFont(source_path)
    # scale outlines and metrics to the outline em, then relabel the em so word
    # draws them at pdf size while the em stays a whole number of pixels
    scale_upem(font, outline_units_per_em)
    font["head"].unitsPerEm = units_per_em
    hmtx = font["hmtx"]
    for glyph, (advance, left_side_bearing) in list(hmtx.metrics.items()):
        hmtx[glyph] = (snap_advance_units(advance, units_per_pixel), left_side_bearing)
    for table in ("GPOS", "kern", "hdmx", "LTSH", "VDMX"):
        if table in font:
            del font[table]
    font["hhea"].advanceWidthMax = max(width for width, _ in hmtx.metrics.values())
    rename_family(font, family)
    font.save(target_path)
    effective_size = drawn_size_pt * outline_units_per_em / units_per_em
    return {
        "units_per_em": units_per_em,
        "effective_size_pt": effective_size,
        "size_error_pt": effective_size - pdf_size_pt,
    }


def advance_pixels(font_path: str, text: str, units_per_pixel: int = UNITS_PER_PIXEL) -> list[int]:
    """whole-pixel advances of a grid font, one per char of text."""
    font = TTFont(font_path)
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]
    pixels = []
    for char in text:
        units = hmtx[cmap[ord(char)]][0]
        if units % units_per_pixel:
            raise ValueError(f"{char!r} advance {units} is not a whole pixel")
        pixels.append(units // units_per_pixel)
    return pixels


def glyph_advance_units(font: TTFont, char: str) -> int:
    glyph = font.getBestCmap()[ord(char)]
    return font["hmtx"][glyph][0]


def make_font_with_advances(
    source_path: str, target_path: str, family: str, advances_pt: dict[str, float], size_pt: float
) -> dict:
    """rewrite hmtx so each char advances by the given width at size_pt.

    advances are integers in font units, so the rounding error per glyph is
    reported back, it is the floor on what this method can reach."""
    font = TTFont(source_path)
    rename_family(font, family)
    units_per_em = font["head"].unitsPerEm
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]
    rounding_pt = {}
    for char, advance_pt in advances_pt.items():
        glyph = cmap[ord(char)]
        exact_units = advance_pt * units_per_em / size_pt
        units = round(exact_units)
        left_side_bearing = hmtx[glyph][1]
        hmtx[glyph] = (units, left_side_bearing)
        rounding_pt[char] = (units - exact_units) * size_pt / units_per_em
    # kerning in gpos would add to these advances, drop it so they are the only truth
    for table in ("GPOS", "kern"):
        if table in font:
            del font[table]
    font["hhea"].advanceWidthMax = max(width for width, _ in hmtx.metrics.values())
    font.save(target_path)
    return rounding_pt
