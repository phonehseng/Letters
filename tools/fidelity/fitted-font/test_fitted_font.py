"""unit tests for the fitted font: composites keep exact offsets, rules pick the right glyph, edits fall back.

shaping is checked with harfbuzz (uharfbuzz, apache-2.0), which follows the
opentype spec the way word's shaper did in c1; word itself is not needed.
run:  ..\\.venv\\Scripts\\python.exe -m unittest test_fitted_font -v
"""

from __future__ import annotations

import io
import unittest
from types import SimpleNamespace

import uharfbuzz as hb
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

import fitted_font as ff
import fitted_pieces as fp
import fitted_plan as fl

LETTERS = "abcdx"


def _square(width: int) -> object:
    pen = TTGlyphPen(None)
    pen.moveTo((10, 0))
    pen.lineTo((10, 500))
    pen.lineTo((width - 10, 500))
    pen.lineTo((width - 10, 0))
    pen.closePath()
    return pen.glyph()


def tiny_font() -> TTFont:
    """five square letters of different widths and a space, 1000 units per em."""
    builder = FontBuilder(1000, isTTF=True)
    order = [".notdef", "space", *LETTERS]
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap({ord(" "): "space", **{ord(c): c for c in LETTERS}})
    widths = {".notdef": 500, "space": 250, **{c: 400 + 50 * i for i, c in enumerate(LETTERS)}}
    glyphs = {name: _square(widths[name]) for name in order}
    glyphs["space"] = TTGlyphPen(None).glyph()
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics({name: (widths[name], 0) for name in order})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({"familyName": "Tiny Test", "styleName": "Regular"})
    builder.setupOS2()
    builder.setupPost()
    return builder.font


def reloaded(font: TTFont) -> TTFont:
    buffer = io.BytesIO()
    font.save(buffer)
    buffer.seek(0)
    return TTFont(buffer)


def shaped(font: TTFont, text: str, features: dict[str, bool]) -> list[str]:
    buffer = io.BytesIO()
    font.save(buffer)
    hb_font = hb.Font(hb.Face(buffer.getvalue()))
    hb_buffer = hb.Buffer()
    hb_buffer.add_str(text)
    hb_buffer.guess_segment_properties()
    hb.shape(hb_font, hb_buffer, features)
    order = font.getGlyphOrder()
    return [order[info.codepoint] for info in hb_buffer.glyph_infos]


def natural(font: TTFont, name: str, text: str, feature: str = "liga", backtrack=()) -> ff.Composite:
    hmtx = font["hmtx"]
    components, pen = [], 0
    for char in text:
        glyph = font.getBestCmap()[ord(char)]
        components.append(ff.Component(glyph, pen, 0))
        pen += hmtx[glyph][0]
    chars = [font.getBestCmap()[ord(c)] for c in text]
    return ff.Composite(name, chars, components, pen, feature, list(backtrack))


ONLY_LIGA = {"liga": True, "kern": False}


class CompositeGlyphs(unittest.TestCase):
    def test_component_offsets_are_kept_exactly_and_never_rounded_to_the_grid(self):
        font = tiny_font()
        ff.add_composites(font, [ff.Composite("piece", ["a", "b"], [ff.Component("a", 3, -7),
                                                                     ff.Component("b", 4123, 11)], 900)])  # fmt: skip
        glyph = reloaded(font)["glyf"]["piece"]
        self.assertEqual([(c.glyphName, c.x, c.y) for c in glyph.components], [("a", 3, -7), ("b", 4123, 11)])
        round_to_grid = 0x0004
        self.assertFalse(any(c.flags & round_to_grid for c in glyph.components))

    def test_the_advance_is_the_one_asked_for_not_the_components_sum(self):
        font = tiny_font()
        ff.add_composites(font, [natural(font, "piece", "ab")])
        font["hmtx"].metrics["piece"] = (1234, font["hmtx"]["piece"][1])
        self.assertEqual(reloaded(font)["hmtx"]["piece"][0], 1234)

    def test_spaces_are_left_out_of_a_composite(self):
        font = tiny_font()
        ff.add_composites(font, [natural(font, "piece", "a b")])
        self.assertEqual([c.glyphName for c in font["glyf"]["piece"].components], ["a", "b"])

    def test_a_component_beyond_the_int16_range_is_refused(self):
        composite = ff.Composite("piece", ["a"], [ff.Component("a", ff.MAX_COMPONENT_OFFSET + 1, 0)], 10)
        with self.assertRaises(ValueError):
            ff.check_offsets(composite)

    def test_the_signature_sees_through_components(self):
        font = tiny_font()
        ff.add_composites(font, [ff.Composite("piece", ["a"], [ff.Component("a", 100, 0)], 400)])
        self.assertNotEqual(ff.outline_signature(font, "piece"), ff.outline_signature(font, "a"))
        moved = ff.outline_signature(font, "piece")[1][0]
        self.assertEqual(moved[0], ff.outline_signature(font, "a")[1][0][0] + 100)


class Rules(unittest.TestCase):
    def test_a_whole_run_with_spaces_becomes_one_glyph(self):
        font = tiny_font()
        rules = [natural(font, "line", "ab cd")]
        ff.add_composites(font, rules)
        ff.add_rules(font, rules)
        self.assertEqual(shaped(font, "ab cd", ONLY_LIGA), ["line"])

    def test_pieces_chain_on_the_glyph_before_them(self):
        font = tiny_font()
        rules = [natural(font, "first", "ab "), natural(font, "second", "cd", backtrack=["first"])]
        ff.add_composites(font, rules)
        ff.add_rules(font, rules)
        self.assertEqual(shaped(font, "ab cd", ONLY_LIGA), ["first", "second"])

    def test_editing_a_word_drops_only_that_piece_and_the_ones_after_it(self):
        font = tiny_font()
        rules = [natural(font, "p0", "ab "), natural(font, "p1", "cd ", backtrack=["p0"]),
                 natural(font, "p2", "xa", backtrack=["p1"])]  # fmt: skip
        ff.add_composites(font, rules)
        ff.add_rules(font, rules)
        self.assertEqual(shaped(font, "ab cd xa", ONLY_LIGA), ["p0", "p1", "p2"])
        self.assertEqual(shaped(font, "ab cc xa", ONLY_LIGA), ["p0", "c", "c", "space", "x", "a"])

    def test_the_same_word_in_another_slot_gets_its_own_glyph(self):
        font = tiny_font()
        rules = [natural(font, "in_liga", "ab"), natural(font, "in_ss01", "ab", feature="ss01")]
        ff.add_composites(font, rules)
        ff.add_rules(font, rules)
        self.assertEqual(shaped(font, "ab", ONLY_LIGA), ["in_liga"])
        self.assertEqual(shaped(font, "ab", {"liga": False, "ss01": True}), ["in_ss01"])

    def test_a_longer_rule_wins_over_a_shorter_one_with_the_same_start(self):
        font = tiny_font()
        rules = [natural(font, "short", "ab"), natural(font, "long", "abc")]
        ff.add_composites(font, rules)
        ff.add_rules(font, rules)
        self.assertEqual(shaped(font, "abc", ONLY_LIGA), ["long"])
        self.assertEqual(shaped(font, "ab", ONLY_LIGA), ["short"])

    def test_single_and_ligature_rules_never_share_a_lookup(self):
        font = tiny_font()
        rules = [natural(font, "one", "a"), natural(font, "two", "bc")]
        groups = ff._nested_groups(rules)
        self.assertEqual(len(groups), 2)


def _item(char: str, along: float) -> fp.Item:
    source = SimpleNamespace(x=along, y=0.0, advance=5.0)
    return fp.Item(char, "Fam", "path", 20, "000000", False, source if char != " " else None, along)


class Pieces(unittest.TestCase):
    def test_words_keep_their_following_spaces(self):
        items = [_item(c, i * 5.0) for i, c in enumerate("ab  cd e")]
        words = ["".join(i.char for i in word) for word in fp._words(items)]
        self.assertEqual(words, ["ab  ", "cd ", "e"])

    def test_runs_with_the_same_first_piece_get_different_slots(self):
        line = SimpleNamespace(page=0, x=0.0, y=0.0, angle=0.0)
        runs = []
        for _ in range(23):
            run = fp.Run(line, [_item("a", 0.0)])
            run.pieces = [fp.Piece(run.items)]
            runs.append(run)
        family = fl.FittedFamily("Fam", "path", runs)
        fl.assign_slots(family)
        slots = [(run.face, run.feature) for run in runs]
        self.assertEqual(len(set(slots)), 23)
        self.assertEqual(family.faces, 2)
        self.assertEqual(slots[21], (1, "liga"))


if __name__ == "__main__":
    unittest.main()
