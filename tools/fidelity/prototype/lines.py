"""glyphs -> lines: the unit that becomes one positioned frame in the docx.

a line is a run of glyphs in content order that share a baseline and an angle
and move forward without a big jump. a jump wider than LINE_GAP_EM (columns,
table cells) starts a new line, and so does any baseline change, which makes
superscripts and subscripts lines of their own, placed exactly.
"""

from __future__ import annotations

import math
import unicodedata
from dataclasses import dataclass, field

from prototype.pdf_content import Glyph

BASELINE_TOLERANCE_PT = 0.01
LINE_GAP_EM = 1.5  # a gap wider than this many ems is a new line
VIRTUAL_SPACE_EM = 0.18  # a gap this wide between words gets a space char, for search and copy
ANGLE_TOLERANCE_DEG = 0.05

RIGHT_TO_LEFT = {"R", "AL", "RLE", "RLO", "RLI"}
COMPLEX_SCRIPTS = ("ARABIC", "HEBREW", "DEVANAGARI", "BENGALI", "THAI", "TAMIL", "SYRIAC", "THAANA", "KHMER", "MYANMAR")


@dataclass
class Line:
    page: int
    glyphs: list[Glyph]
    angle: float
    virtual_spaces: list[int] = field(default_factory=list)  # insert a space before these glyph indexes

    @property
    def x(self) -> float:
        return self.glyphs[0].x

    @property
    def y(self) -> float:
        return self.glyphs[0].y

    @property
    def max_size(self) -> float:
        return max(g.size for g in self.glyphs)

    @property
    def hidden(self) -> bool:
        return all(g.render_mode in (3, 7) for g in self.glyphs)

    def width(self) -> float:
        last = self.glyphs[-1]
        return math.hypot(last.x - self.x, last.y - self.y) + last.advance


def _continues(line: Line, glyph: Glyph) -> bool:
    last = line.glyphs[-1]
    if abs(glyph.angle - line.angle) > ANGLE_TOLERANCE_DEG or glyph.render_mode != last.render_mode:
        return False
    angle = math.radians(line.angle)
    # distance along and across the baseline from the line's start
    dx, dy = glyph.x - line.x, glyph.y - line.y
    across = -dx * math.sin(angle) + dy * math.cos(angle)
    if abs(across) > BASELINE_TOLERANCE_PT:
        return False
    along_last = (last.x - line.x) * math.cos(angle) + (last.y - line.y) * math.sin(angle)
    along = dx * math.cos(angle) + dy * math.sin(angle)
    gap = along - (along_last + last.advance)
    return -0.5 * last.size < gap < LINE_GAP_EM * max(glyph.size, last.size)


def group_lines(glyphs: list[Glyph]) -> list[Line]:
    lines: list[Line] = []
    # sequence numbers restart on every page, so pages are kept apart first
    for glyph in sorted(glyphs, key=lambda g: (g.page, g.sequence)):
        if lines and lines[-1].page == glyph.page and _continues(lines[-1], glyph):
            lines[-1].glyphs.append(glyph)
        else:
            lines.append(Line(glyph.page, [glyph], glyph.angle))
    for line in lines:
        line.virtual_spaces = _virtual_spaces(line)
        _drop_trailing_spaces(line)
    return [line for line in lines if line.glyphs]


def _virtual_spaces(line: Line) -> list[int]:
    """indexes where words touch nothing but a gap, so the docx text still has the space."""
    positions = []
    for index in range(1, len(line.glyphs)):
        previous, current = line.glyphs[index - 1], line.glyphs[index]
        if previous.text == " " or current.text == " ":
            continue
        gap = math.hypot(current.x - previous.x, current.y - previous.y) - previous.advance
        if gap > VIRTUAL_SPACE_EM * current.size:
            positions.append(index)
    return positions


def _drop_trailing_spaces(line: Line) -> None:
    while len(line.glyphs) > 1 and line.glyphs[-1].text == " ":
        line.glyphs.pop()


def script_warnings(glyphs: list[Glyph]) -> dict[str, int]:
    """right-to-left and complex-script glyphs, which this prototype does not shape."""
    counts = {"right_to_left": 0, "complex_script": 0}
    for glyph in glyphs:
        for char in glyph.text:
            if unicodedata.bidirectional(char) in RIGHT_TO_LEFT:
                counts["right_to_left"] += 1
            name = unicodedata.name(char, "")
            if any(script in name for script in COMPLEX_SCRIPTS):
                counts["complex_script"] += 1
    return counts
