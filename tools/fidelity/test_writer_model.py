"""unit tests for word_writer_model and the raw pdf reader's pen arithmetic: run with
.venv\\Scripts\\python.exe -m unittest test_writer_model -v

the expected numbers are values word 16.0.20527 printed in the e10 probes.
"""

from __future__ import annotations

import random
import unittest

import grid_model as gm
import word_writer_model as wm
from prototype import floor, font_rebuild, verify_pass
from word_pdf_raw import RawGlyph, TextObject

LETTER_X = wm.StepTable(wm.MEASURED_STEPS[("x", 5100)])
LETTER_Y = wm.StepTable(wm.MEASURED_STEPS[("y", 6600)])


class PrintingTest(unittest.TestCase):
    def test_five_significant_digits_at_most_three_decimals(self):
        self.assertEqual(wm.printed_text(72.024), "72.024")
        self.assertEqual(wm.printed_text(108.864), "108.86")
        self.assertEqual(wm.printed_text(752.4), "752.4")
        self.assertEqual(wm.printed_text(6.36), "6.36")
        self.assertEqual(wm.printed_text(0.0), "0")
        self.assertEqual(wm.printed_text(1008.0), "1008")

    def test_units_candidates_cover_print_rounding(self):
        self.assertEqual(wm.units_candidates(72.024), [3001])
        self.assertEqual(wm.units_candidates(108.86), [4536])


class TwipsRuleTest(unittest.TestCase):
    def test_floor_rule(self):
        # measured: 0,1,2 -> 0; 3,4 -> 1; 5,6 -> 2; 7,8,9 -> 3; 10,11 -> 4; 12 -> 5
        self.assertEqual([wm.twips_pixel(t) for t in range(13)], [0, 0, 0, 1, 1, 2, 2, 3, 3, 3, 4, 4, 5])

    def test_pixel_edges_follow_the_60_twip_pattern(self):
        # 5t + 1 divisible by 12: up for t mod 60 in (7, 31, 43), down for (19, 55)
        self.assertEqual(wm.twips_pixel(19), 7)
        self.assertEqual(wm.twips_pixel(31), 13)
        self.assertEqual(wm.twips_pixel(55), 22)
        self.assertEqual(wm.twips_pixel(1219), 507)  # e10: printed 60.864, pixel 507

    def test_every_pixel_has_a_twip_value(self):
        for pixel in range(0, 3000):
            self.assertEqual(wm.twips_pixel(wm.twips_for_pixel(pixel)), pixel)

    def test_period(self):
        for t in range(0, 600):
            self.assertEqual(wm.twips_pixel(t + 60), wm.twips_pixel(t) + 25)


class StepTableTest(unittest.TestCase):
    def test_measured_values(self):
        # values word printed for frames in the e10 x probe on a letter page
        self.assertEqual(wm.exported_x(600, LETTER_X), 72.024)
        self.assertEqual(wm.exported_x(500, LETTER_X), 60.0)
        self.assertEqual(wm.exported_x(507, LETTER_X), 60.864)
        self.assertEqual(wm.printed_text(wm.exported_x(907, LETTER_X)), "108.86")
        self.assertEqual(wm.printed_text(wm.exported_x(4830, LETTER_X)), "579.72")

    def test_fifth_step_wraps_to_the_next_pixel_value(self):
        # at the fifth step 5 p + 5 equals 5 (p + 1): decoding must still find p
        units = LETTER_X.units(4823)
        self.assertEqual(units, 5 * 4824)
        self.assertEqual(LETTER_X.pixel_for_units(units), 4823)

    def test_decode_inverts_export(self):
        for pixel in range(0, 5000, 7):
            self.assertEqual(wm.decode_x(wm.exported_x(pixel, LETTER_X), LETTER_X), pixel)
        for pixel in range(81, 6500, 11):
            self.assertEqual(wm.decode_y(wm.exported_y(pixel, LETTER_Y, 6600), LETTER_Y, 6600), pixel)

    def test_nearest_exported_pixel_is_never_beaten(self):
        for target in (72.03, 99.99, 300.017, 450.06, 61.0):
            best = wm.nearest_exported_pixel(target, LETTER_X)
            for other in range(best - 3, best + 4):
                self.assertLessEqual(abs(wm.exported_x(best, LETTER_X) - target),
                                     abs(wm.exported_x(other, LETTER_X) - target) + 1e-12)  # fmt: skip
            self.assertLessEqual(wm.lattice_error(target, LETTER_X), 0.072 + 0.005)

    def test_page_pixels_round_the_media_box(self):
        self.assertEqual(wm.page_pixels(595.3), 4961)
        self.assertEqual(wm.page_pixels(841.9), 7016)


class BaselineRuleTest(unittest.TestCase):
    def test_measured_offsets(self):
        # constant offsets measured for these exact line heights (round 4 calibration tables)
        for line_twips, offset in ((140, 47), (240, 80), (260, 87), (300, 100), (380, 127), (900, 300)):
            for y in range(1200, 1260):
                self.assertEqual(wm.baseline_offset_pixels(line_twips, y), offset)

    def test_two_valued_heights_follow_the_bottom_edge(self):
        values = {wm.baseline_offset_pixels(220, y) for y in range(1200, 1260)}
        self.assertEqual(values, {73, 74})
        # the measured table for 220 twips starts 0,1,1,1,1,0 above its minimum at t mod 60 = 0..5
        self.assertEqual([wm.baseline_offset_pixels(220, 1200 + r) - 73 for r in range(6)], [0, 1, 1, 1, 1, 0])


class SpacingRuleTest(unittest.TestCase):
    def test_each_spacing_rounds_half_away_from_zero(self):
        # e15: 6 twips = 2.5 px -> 3, -6 -> -3, 1 -> 0, 2 -> 1
        self.assertEqual([gm.spacing_pixels(t) for t in (6, -6, 1, 2, -1, 0, 12)], [3, -3, 0, 1, 0, 0, 5])

    def test_pairs_add_their_own_roundings(self):
        # e15 pairs: (13, -11) gives 5 - 5 = 0, although their sum (2 twips) alone would round to 1
        self.assertEqual(gm.pen_pixels([53, 53, 53], [13, -11, 0]), [0, 58, 106])

    def test_planned_steps_are_reproduced_and_the_remainder_stays_small(self):
        rng = random.Random(4)
        advances = [rng.randint(15, 80) for _ in range(200)]
        targets = [0]
        for advance in advances[:-1]:
            targets.append(targets[-1] + advance + rng.randint(-15, 15))
        spacing = gm.spacing_for_pixel_steps(advances, targets)
        self.assertEqual(gm.pen_pixels(advances, spacing), targets)
        carried = 0.0
        for i, twips in enumerate(spacing[:-1]):
            step = targets[i + 1] - targets[i] - advances[i]
            carried += twips * 5 / 12 - step
            self.assertLessEqual(abs(carried), 5 / 24 + 1e-9)

    def test_every_pixel_step_has_twips(self):
        for pixels in range(-40, 41):
            self.assertEqual(gm.spacing_pixels(gm.twips_for_spacing_pixels(pixels)), pixels)


class CheckPassTest(unittest.TestCase):
    def test_first_miss(self):
        plan = {"targets_px": [0, 10, 20, 30]}
        self.assertIsNone(verify_pass.first_miss(plan, [0, 10, 20, 30]))
        self.assertEqual(verify_pass.first_miss(plan, [0, 10, 21, 31]), (2, 1))


class FloorTest(unittest.TestCase):
    def test_word_positions_have_zero_floor(self):
        for pixel in (100, 2000, 5000):
            y = wm.exported_y(pixel, LETTER_Y, 6600)
            self.assertAlmostEqual(floor.nearest_exported_y(y, LETTER_Y, 6600), y)
            self.assertAlmostEqual(wm.lattice_error(wm.exported_x(pixel // 2, LETTER_X), LETTER_X), 0.0)


class Type3Test(unittest.TestCase):
    def test_filled_path_with_flipped_font_matrix(self):
        procedure = b"500 0 0 -700 400 0 d1\n0 0 m\n400 0 l\n400 -700 l\nh\nf\n"
        problems: list[str] = []
        pen = font_rebuild.type3_glyph_recording(procedure, (0.001, 0, 0, -0.001, 0, 0), problems)
        points = [args[0] for op, args in pen.value if op in ("moveTo", "lineTo")]
        self.assertEqual(points, [(0, 0), (400, 0), (400, 700)])
        self.assertEqual(problems, [])

    def test_strokes_are_reported(self):
        problems: list[str] = []
        font_rebuild.type3_glyph_recording(b"0 0 m 10 10 l S", (0.001, 0, 0, 0.001, 0, 0), problems)
        self.assertTrue(problems)


class RawPensTest(unittest.TestCase):
    def test_tj_numbers_move_the_pen_back(self):
        glyphs = [RawGlyph(65, 500, 0), RawGlyph(66, 500, 3), RawGlyph(67, 500, 0)]
        obj = TextObject(0, "/F1", "10", 10.0, ("1", "0", "0", "1", "72", "700"), (1, 0, 0, 1, 72, 700), 0.0, glyphs)
        self.assertEqual([round(p, 6) for p in obj.pens()], [0.0, 4.97, 9.97])


if __name__ == "__main__":
    unittest.main()
