"""unit tests for the prototype's pure helpers: run with
.venv\\Scripts\\python.exe -m unittest test_prototype -v
"""

from __future__ import annotations

import unittest

from prototype import calibration, docx_out, font_rebuild, lines, vectors
from prototype.pdf_content import Glyph, PathItem, apply, multiply
from prototype.pdf_fonts import PdfFont, parse_to_unicode, split_subset_prefix


def _font() -> PdfFont:
    return PdfFont(("t", 1), "Test", "", "TrueType", 1, {}, 500.0, {}, {}, None, None, None, False, 750, -250)


def _glyph(text: str, x: float, y: float = 700.0, size: float = 10.0, sequence: int = 0, page: int = 0) -> Glyph:
    return Glyph(page, _font(), ord(text[0]), text, x, y, size, 1.0, 0.0, 5.0, (0, 0, 0), 0, sequence)


class PdfFontTest(unittest.TestCase):
    def test_subset_prefix(self):
        self.assertEqual(split_subset_prefix("ABCDEF+Lato-Bold"), ("Lato-Bold", "ABCDEF"))
        self.assertEqual(split_subset_prefix("Helvetica"), ("Helvetica", ""))

    def test_to_unicode_chars_ranges_and_ligatures(self):
        cmap = b"""beginbfchar <0003> <0020> <0011> <00660069> endbfchar
                   beginbfrange <0024> <0026> <0041> <0030> <0031> [<0078> <0079>] endbfrange"""
        mapping = parse_to_unicode(cmap)
        self.assertEqual(mapping[0x03], " ")
        self.assertEqual(mapping[0x11], "fi")  # a ligature glyph maps to two chars
        self.assertEqual([mapping[c] for c in (0x24, 0x25, 0x26)], ["A", "B", "C"])
        self.assertEqual((mapping[0x30], mapping[0x31]), ("x", "y"))


class MatrixTest(unittest.TestCase):
    def test_multiply_then_apply(self):
        translate = (1, 0, 0, 1, 10, 20)
        scale = (2, 0, 0, 3, 0, 0)
        self.assertEqual(apply(multiply(scale, translate), 1, 1), (12, 23))


class EmbeddingPermissionTest(unittest.TestCase):
    def test_fs_type_rules(self):
        self.assertTrue(font_rebuild.may_embed(0))
        self.assertTrue(font_rebuild.may_embed(0x0008))
        self.assertTrue(font_rebuild.may_embed(0x0004))
        self.assertFalse(font_rebuild.may_embed(0x0002))
        self.assertIn("read-only", font_rebuild.embedding_permission(0x0004))
        self.assertIn("not stated", font_rebuild.embedding_permission(None))

    def test_unique_family_is_stable_and_short(self):
        name = font_rebuild.unique_family("Lato|truetype", "syllabus")
        self.assertEqual(name, font_rebuild.unique_family("Lato|truetype", "syllabus"))
        self.assertNotEqual(name, font_rebuild.unique_family("Lato|truetype", "slides"))
        self.assertLessEqual(len(name), 31)


class LineGroupingTest(unittest.TestCase):
    def test_same_baseline_forward_is_one_line(self):
        glyphs = [_glyph(c, 72 + 5 * i, sequence=i) for i, c in enumerate("word")]
        self.assertEqual(len(lines.group_lines(glyphs)), 1)

    def test_column_gap_and_baseline_change_split(self):
        glyphs = [_glyph("a", 72, sequence=0), _glyph("b", 77, sequence=1), _glyph("c", 300, sequence=2),
                  _glyph("2", 305, y=703, size=6, sequence=3)]  # fmt: skip
        self.assertEqual([len(line.glyphs) for line in lines.group_lines(glyphs)], [2, 1, 1])

    def test_pages_do_not_interleave(self):
        glyphs = [
            _glyph("a", 72, sequence=0, page=0),
            _glyph("b", 72, sequence=0, page=1),
            _glyph("c", 77, sequence=1, page=0),
        ]
        grouped = lines.group_lines(glyphs)
        self.assertEqual([(line.page, len(line.glyphs)) for line in grouped], [(0, 2), (1, 1)])

    def test_virtual_space_for_a_wide_gap(self):
        glyphs = [_glyph("a", 72, sequence=0), _glyph("b", 79.5, sequence=1)]  # 2.5 pt gap after a 5 pt advance
        self.assertEqual(lines.group_lines(glyphs)[0].virtual_spaces, [1])

    def test_right_to_left_and_complex_scripts_are_counted(self):
        counts = lines.script_warnings([_glyph("א", 72), _glyph("क", 80), _glyph("a", 90)])
        self.assertEqual(counts, {"right_to_left": 1, "complex_script": 2})


class VectorClassificationTest(unittest.TestCase):
    def _item(self, segments, fill=True, stroke=False, clip=None):
        return PathItem(0, segments, fill, stroke, False, (0, 0, 0) if fill else None, (0, 0, 0) if stroke else None,
                        1.0, False, 1.0, clip or [], False, b"", 1)  # fmt: skip

    def test_rectangle_becomes_a_rect_shape(self):
        item = self._item([("re", [(10, 10), (60, 10), (60, 20), (10, 20)])])
        shapes = vectors.classify(item)
        self.assertEqual(len(shapes), 1)
        self.assertIsInstance(shapes[0], vectors.RectShape)

    def test_curve_must_be_rasterised(self):
        item = self._item([("m", [(0, 0)]), ("c", [(1, 1), (2, 2), (3, 0)])])
        self.assertIsNone(vectors.classify(item))

    def test_stroked_polyline_becomes_segments(self):
        item = self._item([("m", [(0, 0)]), ("l", [(10, 0)]), ("l", [(10, 10)])], fill=False, stroke=True)
        self.assertEqual(len(vectors.classify(item)), 2)

    def test_clip_that_cuts_the_path_forces_a_raster(self):
        clip = [[("re", [(0, 0), (20, 0), (20, 20), (0, 20)])]]
        item = self._item([("re", [(10, 10), (60, 10), (60, 20), (10, 20)])], clip=clip)
        self.assertIsNone(vectors.classify(item))


class CalibrationTest(unittest.TestCase):
    def test_close_requests_go_to_different_probe_pages(self):
        requests = [calibration.Request(0, 399.55, 213.9, 260), calibration.Request(0, 401.11, 213.0, 260),
                    calibration.Request(0, 72, 100, 260)]  # fmt: skip
        groups = calibration._probe_groups(requests)
        for group in groups:
            for a in group:
                for b in group:
                    if a is not b:
                        self.assertTrue(
                            abs(a.x_pt - b.x_pt) >= 12 or abs(a.baseline_from_top_pt - b.baseline_from_top_pt) >= 4
                        )
        self.assertEqual(sum(len(g) for g in groups), 3)


class DocxOutTest(unittest.TestCase):
    def test_hex_color(self):
        self.assertEqual(docx_out.hex_color((1, 0.5, 0)), "FF8000")
        self.assertEqual(docx_out.hex_color(None), "000000")


if __name__ == "__main__":
    unittest.main()
