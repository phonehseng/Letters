"""a model of how word places glyphs, used to predict instead of correct.

what the probes showed (word 16.0.20527, grid_probe.py and e5 probes):
    - inside a line word keeps pen positions on a 1/600 inch grid (0.12 pt)
    - with whole-pixel glyph advances, the pen after n glyphs sits at
      floor(sum of exact advances + w:spacing converted at 5/12 px per twip)
    - a twip is 5/12 of a pixel, so twips and pixels only meet every 12 twips (0.6 pt)
pixels here always mean 1/600 inch device pixels.
"""

from __future__ import annotations

import math

PT_PER_PIXEL = 72 / 600  # 0.12 pt
PIXELS_PER_TWIP = 5 / 12
TWIPS_PER_GRID_COMMON_STEP = 12  # 12 twips = 5 pixels = 0.6 pt, the smallest step both units hit exactly
# font units per pixel in grid fonts. a size of h half points is 25h/6 pixels per
# em, so 12 units per pixel (units per em = 50h) is whole for every storable size
UNITS_PER_PIXEL = 12


def pt_to_pixels(points: float) -> float:
    return points / PT_PER_PIXEL


def pixels_per_em(size_pt: float) -> float:
    return pt_to_pixels(size_pt)


def snapped_units_per_em(size_pt: float, units_per_pixel: int = UNITS_PER_PIXEL) -> int:
    """a units-per-em at which one pixel at size_pt is a whole number of font units.

    the em is size_pt / 0.12 pixels; at 10.5 pt that is 87.5, so 12 units per
    pixel gives 1050. the result must stay inside truetype's 16..16384 range."""
    upm = pixels_per_em(size_pt) * units_per_pixel
    if abs(upm - round(upm)) > 1e-6:
        raise ValueError(f"{size_pt} pt has no whole-unit pixel at {units_per_pixel} units per pixel")
    upm = round(upm)
    if not 16 <= upm <= 16384:
        raise ValueError(f"units per em {upm} is outside the truetype range")
    return upm


def snap_advance_units(advance_units: float, units_per_pixel: int = UNITS_PER_PIXEL) -> int:
    """the advance rounded to a whole number of pixels, in font units."""
    return round(advance_units / units_per_pixel) * units_per_pixel


def drawn_font_size(size_pt: float) -> float:
    """the font size word writes into its pdf: a whole number of pixels per em.

    measured: 9 and 12 pt stay, 10.5 becomes 10.56, 14 becomes 14.04."""
    return round(pt_to_pixels(size_pt)) * PT_PER_PIXEL


def nearest_half_point(size_pt: float) -> float:
    """the closest size a docx can store (w:sz is in half points)."""
    return round(size_pt * 2) / 2


def nearest_pixel_offsets(target_offsets_pt: list[float]) -> list[int]:
    """each target pen offset from the line start, as the nearest whole pixel."""
    return [round(pt_to_pixels(offset)) for offset in target_offsets_pt]


def spacing_for_pixel_targets(advance_pixels: list[int], target_pixels: list[int]) -> list[int]:
    """w:spacing twips after each glyph so word's floor lands on each target pixel.

    advance_pixels[i] is glyph i's snapped advance, target_pixels[i] the wanted pen
    offset of glyph i (target_pixels[0] is the line start and must be 0). with
    cumulative spacing c twips the extra pen offset is floor(c * 5/12) pixels, so c
    is aimed at a quarter pixel above the wanted extra: there floor and round
    agree, so the result holds whichever of the two word uses (a probe saw it
    break the floor rule at an exact half pixel)."""
    if not target_pixels or target_pixels[0] != 0:
        raise ValueError("the first glyph is the line start, its target offset must be 0")
    spacing = []
    previous_twips = 0
    advance_sum = 0
    for i in range(1, len(target_pixels)):
        advance_sum += advance_pixels[i - 1]
        extra_pixels = target_pixels[i] - advance_sum
        cumulative_twips = centred_twips_for_floor(extra_pixels)
        spacing.append(cumulative_twips - previous_twips)
        previous_twips = cumulative_twips
    spacing.append(0)  # nothing follows the last glyph
    return spacing


def centred_twips_for_floor(extra_pixels: int) -> int:
    """the twip count whose pixel value is closest to extra_pixels + 0.25.

    twips come in 5/12 pixel steps, so the chosen value lies within
    [extra + 0.04, extra + 0.46] pixels: floor and round both give extra."""
    return round((extra_pixels + 0.25) / PIXELS_PER_TWIP)


def predicted_pen_pixels(advance_pixels: list[int], spacing_twips: list[int]) -> list[int]:
    """pen offsets the model expects word to produce, for tests and reports."""
    pens = [0]
    exact = 0.0
    for advance, twips in zip(advance_pixels[:-1], spacing_twips[:-1], strict=True):
        exact += advance + twips * PIXELS_PER_TWIP
        pens.append(math.floor(exact + 1e-9))
    return pens
