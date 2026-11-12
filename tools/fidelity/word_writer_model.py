"""how word's pdf writer turns layout pixels into the numbers it prints.

measured with e10_writer_probe.py on word 16.0.20527 (letter, a4 and a wide page):

    layout   positions are whole 1/600 inch pixels ("px", 0.12 pt)
    writer   a text object's Tm is printed in 1/3000 inch units ("u", 0.024 pt):
                 u = 5 * p + k(p)
             p is the layout pixel counted from the page's left edge (x) or top
             edge (y); k(p) counts how many of the writer's step pixels lie at
             or below p. the steps are about 1/4.73 of the page size apart (a
             skew of about 0.95 px over the page) and are measured per page
             size, because they are not exactly linear (StepTable)
    print    the value u * 0.024 is printed with 5 significant digits and at
             most 3 decimals, trailing zeros dropped (72.024, 108.86, 752.4)
    page     the media box is the page size rounded to whole pixels
             (a4 595.3 x 841.9 pt becomes 595.32 x 841.92)
    pdf y    y = page height - u_top * 0.024, with u_top from the top edge

inside one text object word places later glyphs with Tc and TJ numbers in
1/1000 em and only corrects when its error passes about 3/1000 em (e10 line
probe), so those positions are not on this lattice. the converter avoids that
by giving every glyph its own text object (see e11_run_split.py).

pure functions only: no word, no files, so the rules are unit tested.
"""

from __future__ import annotations

import bisect
import math
from dataclasses import dataclass

PT_PER_PIXEL = 0.12
PT_PER_UNIT = 0.024  # 1/3000 inch, the writer's print grid
UNITS_PER_PIXEL = 5
SIGNIFICANT_DIGITS = 5
MAX_DECIMALS = 3


def decimals_for(value: float) -> int:
    """decimals word prints for a coordinate of this size."""
    magnitude = abs(value)
    if magnitude < 1:
        return MAX_DECIMALS
    digits_before_point = int(math.floor(math.log10(magnitude))) + 1
    return max(0, min(MAX_DECIMALS, SIGNIFICANT_DIGITS - digits_before_point))


def printed(value: float) -> float:
    """value as word prints it (no ties occur: u * 0.024 never ends in a 5 at the cut)."""
    return round(value, decimals_for(value))


def printed_text(value: float) -> str:
    places = decimals_for(value)
    text = f"{value:.{places}f}"
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    return text


def units_candidates(printed_value: float) -> list[int]:
    """every whole 1/3000 inch value that word could have printed as printed_value."""
    half = 0.5 * 10 ** -decimals_for(printed_value)
    low = (printed_value - half) / PT_PER_UNIT
    high = (printed_value + half) / PT_PER_UNIT
    return [u for u in range(math.floor(low), math.ceil(high) + 1) if low - 1e-9 <= u <= high + 1e-9]


@dataclass(frozen=True)
class StepTable:
    """the layout pixels at which the writer adds one more 1/3000 inch unit (k steps up).

    steps[j] is the first pixel with k = j + 1. measured per page size and axis."""

    steps: tuple[int, ...]

    def k(self, pixel: int) -> int:
        return bisect.bisect_right(self.steps, pixel)

    def units(self, pixel: int) -> int:
        return UNITS_PER_PIXEL * pixel + self.k(pixel)

    def pixel_for_units(self, units: int) -> int | None:
        """the layout pixel whose printed unit value is units, or None if no pixel prints it."""
        pixel = units // UNITS_PER_PIXEL
        for candidate in (pixel, pixel - 1, pixel - 2):
            if candidate >= 0 and self.units(candidate) == units:
                return candidate
        return None


# measured with e10_writer_probe.py --dense on word 16.0.20527; key: page size in whole pixels
MEASURED_STEPS: dict[tuple[str, int], tuple[int, ...]] = {
    # a step is the first pixel printed one unit further; the fifth unit lands on a
    # whole pixel boundary, so a decoder that assumes k < 5 sees it one pixel late
    ("x", 5100): (506, 1586, 2665, 3743, 4823),  # letter width
    ("y", 6600): (654, 2049, 3444, 4839, 6234),  # letter height
    ("x", 4961): (492, 1542, 2591, 3640, 4690),  # a4 width
    ("y", 7016): (696, 2180, 3663, 5147, 6630),  # a4 height
    ("x", 8400): (746, 2337, 3927, 5518, 7108),  # 1008 pt wide page
    ("y", 5100): (494, 1548, 2602, 3655, 4709),  # 612 pt high (landscape) page; not the letter width table
}


TWIPS_PERIOD = 60  # 60 twips = 25 pixels exactly, so every twip rule below repeats every 60 twips
EDGE_DOWN_RESIDUES = (19, 55)  # twips mod 60 that sit on a pixel edge and go to the lower pixel


def twips_pixel(twips: int) -> int:
    """the layout pixel word gives a frame edge at this many twips from the page edge.

    floor((5 t + 1) / 12); when 5 t + 1 is a multiple of 12 the value sits on a
    pixel edge and word takes the lower pixel for t mod 60 in (19, 55) and the
    upper one for (7, 31, 43). measured on 85,571 frames (e10), no exception."""
    pixel, remainder = divmod(5 * twips + 1, 12)
    if remainder == 0 and twips % TWIPS_PERIOD in EDGE_DOWN_RESIDUES:
        pixel -= 1
    return pixel


def twips_for_pixel(pixel: int) -> int:
    """the smallest twip value whose frame edge lands on pixel (every pixel has one)."""
    twips = max(0, (12 * pixel - 1) // 5 - 1)
    while twips_pixel(twips) < pixel:
        twips += 1
    if twips_pixel(twips) != pixel:
        raise ValueError(f"no twip value lands on pixel {pixel}")
    return twips


def baseline_offset_pixels(line_twips: int, y_twips: int) -> int:
    """pixels from a frame's top pixel to its baseline, for an exact line height (lineRule="exact").

    measured on 16 line heights x 60 twip phases (calibration.measure_baseline,
    960 of 960 frames): the baseline sits 0.8 of the line below the top, and
        line twips mod 60 = 0 or 20:  top pixel + round(0.8 * line pixels)
        line twips mod 60 = 40:       bottom pixel - round(0.2 * line pixels)
    where top = twips_pixel(y), bottom = twips_pixel(y + line), rounding half up.
    (0.8 of a line is a third of a pixel short of whole in the second case, and
    word then anchors the baseline to the bottom edge instead of the top.)"""
    line_pixels = line_twips * 5 / 12
    if line_twips % TWIPS_PERIOD == 40:
        bottom = twips_pixel(y_twips + line_twips) - math.floor(0.2 * line_pixels + 0.5)
        return bottom - twips_pixel(y_twips)
    return math.floor(0.8 * line_pixels + 0.5)


def page_pixels(size_pt: float) -> int:
    """the page size word writes into the media box, in whole pixels."""
    return round(size_pt / PT_PER_PIXEL)


def step_table(axis: str, page_pixels_along: int) -> StepTable | None:
    steps = MEASURED_STEPS.get((axis, page_pixels_along))
    return StepTable(steps) if steps is not None else None


def estimated_step_table(page_pixels_along: int) -> StepTable:
    """a linear stand-in for page sizes nobody measured: k = floor(p * s + 17/32).

    on letter and a4 portrait s = 0.9458 / page pixels fits within one pixel per
    step; the wide page did not fit it, so this is only used to tell the caller
    to measure (it is never used to plan positions)."""
    slope = 0.9458 * UNITS_PER_PIXEL / page_pixels_along
    steps = []
    j = 1
    while True:
        pixel = math.ceil((j - 17 / 32) / slope)
        if pixel > page_pixels_along:
            break
        steps.append(pixel)
        j += 1
    return StepTable(tuple(steps))


def exported_x(pixel: int, table: StepTable) -> float:
    """the x word prints for a text object whose layout pixel is pixel."""
    return printed(table.units(pixel) * PT_PER_UNIT)


def exported_y(pixel_from_top: int, table: StepTable, page_height_pixels: int) -> float:
    """the pdf y word prints for a baseline pixel_from_top pixels below the page top."""
    return printed((UNITS_PER_PIXEL * page_height_pixels - table.units(pixel_from_top)) * PT_PER_UNIT)


def decode_x(printed_value: float, table: StepTable) -> int | None:
    """the layout pixel behind a printed Tm x, or None if none fits."""
    for units in units_candidates(printed_value):
        pixel = table.pixel_for_units(units)
        if pixel is not None:
            return pixel
    return None


def decode_y(printed_value: float, table: StepTable, page_height_pixels: int) -> int | None:
    """the layout pixel (from the top) behind a printed Tm y."""
    for units in units_candidates(printed_value):
        pixel = table.pixel_for_units(UNITS_PER_PIXEL * page_height_pixels - units)
        if pixel is not None:
            return pixel
    return None


def nearest_exported_pixel(target_pt: float, table: StepTable) -> int:
    """the layout pixel whose exported x is closest to target_pt.

    this is where a glyph should be planned: the lattice of exported positions
    is 0.12 pt apart with a 0.144 pt gap at each step, so a plain round(x / 0.12)
    is off by up to 0.024 pt, and more where a step falls between."""
    guess = round(target_pt / PT_PER_PIXEL)
    candidates = range(max(0, guess - 2), guess + 3)
    return min(candidates, key=lambda p: (abs(exported_x(p, table) - target_pt), p))


def lattice_error(target_pt: float, table: StepTable) -> float:
    """distance from target_pt to the nearest exported x: the floor of any plan on this lattice."""
    return abs(exported_x(nearest_exported_pixel(target_pt, table), table) - target_pt)
