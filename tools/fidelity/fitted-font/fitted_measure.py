"""where word put each piece, and how far every source glyph's ink lands from the source.

a piece's glyph is found in word's pdf by its outline (fitted_font.outline_signature),
on the right page and nearest where it was expected. its ink is exact by
construction: component k is drawn at the glyph's printed origin plus the
offset we wrote, scaled by the size word draws at. so a source glyph's error
is |origin + R * offset_k * size / em - source origin|, nothing estimated.
a glyph whose piece did not become one glyph (a rule word did not apply) is
looked for among word's ordinary glyphs, by its text, near its source origin.
"""

from __future__ import annotations

import math
from collections import defaultdict
from dataclasses import dataclass

from fontTools.ttLib import TTFont

import fitted_font as ff
from fitted_pieces import Piece, Run
from fitted_plan import expected_origin
from pdf_glyphs import DrawnGlyph

# a piece is matched to a drawn glyph only this close to where it was expected.
# word may print a later piece of a run up to 1.7 pt from its planned pen (the
# syllabus: a Tc of 0.57 pt where -1.09 would close the gap), so 4 pt is allowed;
# the residual is baked in either way, and other lines are 8 pt or more away
MATCH_RADIUS_PT = 4.0
# an ordinary glyph standing in for a piece's glyph may have drifted further
FALLBACK_RADIUS_PT = 3.0


@dataclass
class GlyphError:
    page: int
    text: str
    dx: float
    dy: float
    in_piece: bool  # drawn through the piece's composite (false: an ordinary glyph stood in)

    @property
    def distance(self) -> float:
        return math.hypot(self.dx, self.dy)


def signatures_of(font_paths: dict[str, str]) -> dict[str, dict[tuple, list[str]]]:
    """per face family: outline signature -> glyph names with that outline."""
    table = {}
    for family, path in font_paths.items():
        font = TTFont(path)
        by_signature = defaultdict(list)
        for name in font.getGlyphOrder():
            by_signature[ff.outline_signature(font, name)].append(name)
        table[family] = by_signature
    return table


def locate_pieces(runs: list[Run], drawn: list[DrawnGlyph], piece_signature) -> dict:
    """set measured origin, size and direction on every piece word drew as one glyph."""
    by_signature = defaultdict(list)
    for glyph in drawn:
        if glyph.signature is not None:
            by_signature[glyph.signature].append(glyph)
    found = missing = 0
    for run in runs:
        for piece in run.pieces:
            piece.measured_origin = None
            if not piece.inked:
                continue
            expected = expected_origin(piece, run)
            candidates = [g for g in by_signature.get(piece_signature(run, piece), ()) if g.page == run.line.page]
            best = min(candidates, key=lambda g: math.hypot(g.x - expected[0], g.y - expected[1]), default=None)
            if best is None or math.hypot(best.x - expected[0], best.y - expected[1]) > MATCH_RADIUS_PT:
                missing += 1
                continue
            piece.measured_origin = (best.x, best.y)
            piece.measured_size = best.size
            piece.measured_direction = best.direction
            found += 1
    return {"pieces_drawn_as_one_glyph": found, "pieces_not_found": missing}


def glyph_errors(runs: list[Run], drawn: list[DrawnGlyph]) -> list[GlyphError]:
    errors = []
    ordinary = defaultdict(list)
    for glyph in drawn:
        if len(glyph.text) == 1:
            ordinary[(glyph.page, glyph.text)].append(glyph)
    for run in runs:
        for piece in run.pieces:
            if piece.measured_origin is not None:
                errors.extend(_piece_errors(run, piece))
                continue
            for item in piece.inked:
                near = [g for g in ordinary[(run.line.page, item.char)]
                        if math.hypot(g.x - item.source.x, g.y - item.source.y) <= FALLBACK_RADIUS_PT]  # fmt: skip
                best = min(near, key=lambda g: math.hypot(g.x - item.source.x, g.y - item.source.y), default=None)
                if best is not None:
                    errors.append(GlyphError(run.line.page, item.char, best.x - item.source.x,
                                             best.y - item.source.y, False))  # fmt: skip
    return errors


def _piece_errors(run: Run, piece: Piece) -> list[GlyphError]:
    origin_x, origin_y = piece.measured_origin
    cos, sin = piece.measured_direction
    scale = piece.measured_size / piece.units_per_em
    errors = []
    for item, (along_units, across_units) in zip(piece.inked, piece.offsets, strict=True):
        along, across = along_units * scale, across_units * scale
        x = origin_x + along * cos - across * sin
        y = origin_y + along * sin + across * cos
        errors.append(GlyphError(run.line.page, item.char, x - item.source.x, y - item.source.y, True))
    return errors


def summary(errors: list[GlyphError], source_glyphs: int) -> dict:
    distances = sorted(e.distance for e in errors)
    if not distances:
        return {"glyphs_measured": 0, "source_glyphs": source_glyphs}
    count = len(distances)
    return {
        "source_glyphs": source_glyphs,
        "glyphs_measured": count,
        "glyphs_through_piece": sum(e.in_piece for e in errors),
        "max": distances[-1],
        "mean": sum(distances) / count,
        "p99": distances[min(count - 1, int(0.99 * count))],
        "within_0.025": sum(d <= 0.025 for d in distances) / count,
        "within_0.01": sum(d <= 0.01 for d in distances) / count,
        "max_dx": max(abs(e.dx) for e in errors),
        "max_dy": max(abs(e.dy) for e in errors),
    }
