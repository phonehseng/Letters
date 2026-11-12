"""turns runs and pieces into fitted fonts: one per grid font, with copies when slots run out.

every run is given a slot: a font copy ("face") and a feature tag the docx
run switches on. in one slot no two runs may start with the same piece text,
because the first piece of a run has no context to tell occurrences apart
(c1: word applies liga with w14:ligatures standard, and ss01..ss20 each with
its own w14:stylisticSets entry, so one font copy gives 21 slots). later
pieces name the glyph of the piece before them as backtrack, so they never
collide. component offsets come from where the piece's glyph is (or, before
word has drawn it, is expected to be) and the source position of each glyph.
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import math
import os
from dataclasses import dataclass, field

from fontTools.ttLib import TTFont

import fitted_docx as fd
import fitted_font as ff
import font_prep
import grid_model as gm
from fitted_pieces import Piece, Run

SLOT_FEATURES = ("liga",) + ff.STYLISTIC_SET_TAGS


def run_features(feature: str) -> fd.RunFeatures:
    if feature == "liga":
        return fd.RunFeatures(ligatures="standard")
    return fd.RunFeatures(ligatures=None, stylistic_set=int(feature[2:]))


@dataclass
class FittedFamily:
    """the fitted fonts made from one grid font."""

    base_family: str
    base_path: str
    runs: list[Run] = field(default_factory=list)
    faces: int = 1

    def face_family(self, face: int) -> str:
        return self.base_family if face == 0 else f"{self.base_family} f{face}"


def assign_slots(family: FittedFamily) -> None:
    taken: dict[tuple[int, str], set[str]] = {}
    for run in family.runs:
        if not run.pieces:
            continue
        first = run.pieces[0].text
        slot_number = 0
        while True:
            face, feature = divmod(slot_number, len(SLOT_FEATURES))
            key = (face, SLOT_FEATURES[feature])
            if first not in taken.setdefault(key, set()):
                taken[key].add(first)
                run.face, run.feature = key
                break
            slot_number += 1
        family.faces = max(family.faces, run.face + 1)


def piece_advance_units(run: Run, index: int, font: TTFont) -> int:
    """the layout advance of a piece: to the next piece's source start, in whole pixels.

    whole pixels keep word's layout on the same 1/600 in grid it uses anyway,
    so the next piece starts where the source has it, to the pixel."""
    piece = run.pieces[index]
    if index + 1 < len(run.pieces):
        following = run.pieces[index + 1].items[0].along
    elif run.next_in_line is not None:
        following = run.next_in_line.items[0].along
    else:
        hmtx, cmap = font["hmtx"], font.getBestCmap()
        natural = sum(hmtx[cmap[ord(item.char)]][0] for item in piece.items)
        return max(0, natural)
    pixels = round((following - piece.items[0].along) / gm.PT_PER_PIXEL)
    return max(0, pixels * gm.UNITS_PER_PIXEL)


def expected_origin(piece: Piece, run: Run) -> tuple[float, float]:
    """where the piece's glyph starts if word puts it exactly at its source position."""
    angle = math.radians(run.line.angle)
    along = piece.items[0].along
    return run.line.x + along * math.cos(angle), run.line.y + along * math.sin(angle)


def component_offsets(piece: Piece, run: Run, units_per_em: int) -> list[tuple[int, int]]:
    """each inked glyph's offset from the piece origin, in font units of the drawn size."""
    if piece.measured_origin is not None:
        origin, size, direction = piece.measured_origin, piece.measured_size, piece.measured_direction
    else:
        angle = math.radians(run.line.angle)
        origin, direction = expected_origin(piece, run), (math.cos(angle), math.sin(angle))
        size = gm.drawn_font_size(piece.items[0].half_points / 2)
    cos, sin = direction
    offsets = []
    for item in piece.inked:
        dx, dy = item.source.x - origin[0], item.source.y - origin[1]
        # page space back into the text space of the drawn glyph (inverse rotation)
        along, across = dx * cos + dy * sin, -dx * sin + dy * cos
        offsets.append((round(along * units_per_em / size), round(across * units_per_em / size)))
    return offsets


def build_family_fonts(family: FittedFamily, out_dir: str, pass_name: str) -> list[tuple[str, str]]:
    """write every face of this family; returns (family name, font path) per face."""
    base = TTFont(family.base_path)
    units_per_em = base["head"].unitsPerEm
    cmap = base.getBestCmap()
    by_face: dict[int, list[ff.Composite]] = {face: [] for face in range(family.faces)}
    for run in family.runs:
        previous_glyph = None
        for index, piece in enumerate(run.pieces):
            offsets = component_offsets(piece, run, units_per_em)
            piece.offsets = offsets
            piece.units_per_em = units_per_em
            components = [ff.Component(cmap[ord(item.char)], dx, dy)
                          for item, (dx, dy) in zip(piece.inked, offsets, strict=True)]  # fmt: skip
            chars = [cmap[ord(item.char)] for item in piece.items]
            advance = piece_advance_units(run, index, base)
            backtrack = [previous_glyph] if previous_glyph else []
            by_face[run.face].append(ff.Composite(piece.name, chars, components, advance, run.feature, backtrack))
            previous_glyph = piece.name
    written = []
    for face, composites in by_face.items():
        font = TTFont(family.base_path)
        if face:
            font_prep.rename_family(font, family.face_family(face))
        ff.add_composites(font, composites)
        ff.add_rules(font, composites)
        path = os.path.join(out_dir, f"{family.face_family(face).replace(' ', '_')}.{pass_name}.ttf")
        font.save(path)
        written.append((family.face_family(face), path))
    return written


def name_pieces(families: list[FittedFamily]) -> None:
    counter = 0
    for family in families:
        for run in family.runs:
            for piece in run.pieces:
                piece.name = f"fit{counter:05d}"
                counter += 1
