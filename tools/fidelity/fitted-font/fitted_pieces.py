"""which characters become one pre-composed glyph: lines -> runs -> pieces.

a run is what the docx holds as one w:r: consecutive characters of a line
with the same grid font, colour and visibility (the prototype's rules for
which font a space takes are kept, so the text is exactly the prototype's).
a piece is what one gsub rule turns into one glyph: the whole run ("line"
mode) or each word with its following spaces ("word" mode), cut shorter
where a glyph would reach past the int16 limit of glyf coordinates.
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import math
from dataclasses import dataclass, field

import grid_model as gm
from prototype import docx_out, lines
from prototype.pdf_content import Glyph

# a piece's components must stay within this many units of its origin; the
# margin below the int16 limit leaves room for the baked residual (under 1 pt)
PIECE_REACH_UNITS = 30000
# and within this many ems: word's pdf export failed on any glyph whose ink
# reached 32.7 em or more from its origin, at 500 and at 1000 units per em
# (c2 --em-width); 32 em worked, so 31.5 leaves room for the last glyph's width
PIECE_REACH_EMS = 31.5


@dataclass
class Item:
    char: str
    family: str
    font_path: str
    half_points: int
    color: str
    hidden: bool
    source: Glyph | None  # None for a virtual space the converter inserted
    along: float  # pt along the baseline from the line's first glyph, where the source puts it


@dataclass
class Piece:
    items: list[Item]
    name: str = ""
    # filled when fonts are planned and after word has drawn it
    measured_origin: tuple[float, float] | None = None
    measured_size: float | None = None
    measured_direction: tuple[float, float] = (1.0, 0.0)
    # the composite's component offsets (font units) as last written, one per inked item
    offsets: list[tuple[int, int]] = field(default_factory=list)
    units_per_em: int = 0

    @property
    def text(self) -> str:
        return "".join(item.char for item in self.items)

    @property
    def inked(self) -> list[Item]:
        return [i for i in self.items if i.source is not None and i.char.strip() and not i.hidden]


@dataclass
class Run:
    line: lines.Line
    items: list[Item]
    pieces: list[Piece] = field(default_factory=list)
    face: int = 0  # which copy of the fitted font holds this run's rules
    feature: str = "liga"
    # the run after this one on the same line, whatever its font: its start ends this run's layout
    next_in_line: Run | None = None

    @property
    def text(self) -> str:
        return "".join(item.char for item in self.items)

    @property
    def style(self) -> tuple:
        first = self.items[0]
        return (first.family, first.half_points, first.color, first.hidden)


def line_items(line: lines.Line, plan) -> list[Item]:
    """the characters the prototype writes for this line (convert.build_runs), with their sources."""
    items = []
    angle = math.radians(line.angle)
    text_glyphs = [g for g in line.glyphs if g.text != " "] or line.glyphs
    for index, glyph in enumerate(line.glyphs):
        along = (glyph.x - line.x) * math.cos(angle) + (glyph.y - line.y) * math.sin(angle)
        # a space takes the font of the text before it (or after it at the line start)
        font_glyph = glyph
        if glyph.text == " ":
            before = [g for g in line.glyphs[:index] if g.text != " "]
            font_glyph = before[-1] if before else text_glyphs[0]
        family, path, stored = plan.grid_font(font_glyph)
        hidden = glyph.render_mode in (3, 7)
        color = docx_out.hex_color(glyph.color)
        if index in line.virtual_spaces:
            # the prototype puts a virtual space at the previous glyph's pen plus its advance
            previous = items[-1] if items else None
            space_at = previous.along + previous.source.advance if previous and previous.source else along
            items.append(Item(" ", family, path, round(stored * 2), color, hidden, None, space_at))
        char = plan.char(glyph)
        if not char:
            continue
        items.append(Item(char, family, path, round(stored * 2), color, hidden, glyph, along))
    return items


def is_outside_basic_plane(char: str) -> bool:
    return ord(char) > 0xFFFF


def split_runs(line: lines.Line, items: list[Item]) -> list[Run]:
    """same-style runs, also cut before and after every character outside the basic plane.

    word shapes such a character (the prototype's U+F0000 ligature glyphs, math
    letters) on its own: a rule containing one never applied, while a single
    substitution of it alone did, and so did rules just before and after it (c4).
    so it is its own run, and the text after it starts a fresh run whose first
    piece needs no context from before."""
    runs: list[Run] = []
    for item in items:
        style = (item.family, item.half_points, item.color, item.hidden)
        breaks = is_outside_basic_plane(item.char) or (runs and is_outside_basic_plane(runs[-1].items[-1].char))
        if runs and runs[-1].style == style and not breaks:
            runs[-1].items.append(item)
        else:
            runs.append(Run(line, [item]))
    for run, following in zip(runs, runs[1:], strict=False):
        run.next_in_line = following
    return runs


def _reach_units(items: list[Item]) -> float:
    """how far the last glyph sits from the first, in font units of the grid font."""
    sources = [i.source for i in items if i.source is not None]
    if len(sources) < 2:
        return 0.0
    first, last = sources[0], sources[-1]
    distance = math.hypot(last.x - first.x, last.y - first.y) + last.advance
    units_per_em = gm.snapped_units_per_em(items[0].half_points / 2)
    return distance * units_per_em / gm.drawn_font_size(items[0].half_points / 2)


def _words(items: list[Item]) -> list[list[Item]]:
    """each word with the spaces after it; spaces at the run start go with the first word."""
    words: list[list[Item]] = []
    for item in items:
        starts_word = item.char != " " and words and words[-1][-1].char == " "
        if not words or starts_word:
            words.append([item])
        else:
            words[-1].append(item)
    return words


def _cut_by_reach(items: list[Item]) -> list[list[Item]]:
    units_per_em = gm.snapped_units_per_em(items[0].half_points / 2)
    limit = min(PIECE_REACH_UNITS, PIECE_REACH_EMS * units_per_em)
    pieces = [[]]
    for item in items:
        if pieces[-1] and _reach_units(pieces[-1] + [item]) > limit:
            pieces.append([])
        pieces[-1].append(item)
    return pieces


def make_pieces(run: Run, mode: str) -> None:
    if run.items[0].hidden:
        return  # word does not draw hidden text, so there is nothing to place
    groups = _words(run.items) if mode == "word" else [run.items]
    for group in groups:
        for chunk in _cut_by_reach(group):
            run.pieces.append(Piece(chunk))
