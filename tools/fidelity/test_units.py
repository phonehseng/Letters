"""unit tests for the parts that do not need word: run with
.venv\\Scripts\\python.exe -m unittest test_units -v
"""

from __future__ import annotations

import random
import unittest

import numpy as np

import docx_writer as dw
import grid_model as gm
from compare import match_chars
from e3_advance_widths import diffused_spacing
from grid_probe import coarsest_fitting_grid
from pdf_analysis import PlacedChar


class FontObfuscationTest(unittest.TestCase):
    def test_key_is_the_guid_read_backwards(self):
        # guid bytes 00..0f, so the key starts with 0x0f and the first font byte flips by it
        key = "{00010203-0405-0607-0809-0A0B0C0D0E0F}"
        data = bytes(64)
        obfuscated = dw.obfuscate_font(data, key)
        self.assertEqual(obfuscated[0], 0x0F)
        self.assertEqual(obfuscated[15], 0x00)
        self.assertEqual(obfuscated[16], 0x0F)  # the 16-byte key repeats over 32 bytes
        self.assertEqual(obfuscated[32:], data[32:])  # only the first 32 bytes change

    def test_obfuscation_is_its_own_inverse(self):
        key = dw.EmbeddedFont(family="x", font_bytes=b"").font_key
        data = bytes(random.Random(1).randrange(256) for _ in range(100))
        self.assertEqual(dw.obfuscate_font(dw.obfuscate_font(data, key), key), data)


class UnitConversionTest(unittest.TestCase):
    def test_word_cannot_store_sizes_between_half_points(self):
        self.assertEqual(dw.size_to_half_points(10.5), 21)
        with self.assertRaises(ValueError):
            dw.size_to_half_points(10.3)

    def test_twips_and_emu(self):
        self.assertEqual(dw.pt_to_twips(72.37), 1447)
        self.assertEqual(dw.pt_to_emu(1), 12700)


class ErrorDiffusionTest(unittest.TestCase):
    def test_running_total_stays_within_half_a_twip(self):
        rng = random.Random(7)
        offsets = [0.0]
        for _ in range(200):
            offsets.append(offsets[-1] + rng.uniform(-0.4, 0.4))
        spacing = diffused_spacing(offsets)
        self.assertEqual(len(spacing), len(offsets) - 1)
        total = 0
        for i, twips in enumerate(spacing):
            total += twips
            self.assertLessEqual(abs(total / dw.TWIPS_PER_PT - offsets[i + 1]), 0.025 + 1e-9)


class MatchCharsTest(unittest.TestCase):
    @staticmethod
    def chars(text: str) -> list[PlacedChar]:
        return [PlacedChar(0, c, float(i), 0.0, (0, 0, 0, 0), 10.0, "f") for i, c in enumerate(text)]

    def test_alignment_survives_a_dropped_char(self):
        pairs = match_chars(self.chars("abcdef"), self.chars("abdef"))
        self.assertEqual("".join(s.char for s, _ in pairs), "abdef")
        self.assertTrue(all(s.char == c.char for s, c in pairs))


class GridModelTest(unittest.TestCase):
    def test_every_storable_size_up_to_163_pt_has_whole_unit_pixels(self):
        # units per em is 50 per half point, truetype stops at 16384, so 163.5 pt
        for half_points in range(2, 328):
            self.assertEqual(gm.snapped_units_per_em(half_points / 2), 50 * half_points)
        with self.assertRaises(ValueError):
            gm.snapped_units_per_em(164)

    def test_drawn_size_is_whole_pixels_per_em(self):
        self.assertAlmostEqual(gm.drawn_font_size(10.5), 10.56)
        self.assertAlmostEqual(gm.drawn_font_size(14), 14.04)
        self.assertAlmostEqual(gm.drawn_font_size(12), 12.0)
        self.assertAlmostEqual(gm.drawn_font_size(9), 9.0)

    def test_nearest_half_point(self):
        self.assertEqual(gm.nearest_half_point(10.3), 10.5)
        self.assertEqual(gm.nearest_half_point(11.27), 11.5)
        self.assertEqual(gm.nearest_half_point(7.9), 8.0)

    def test_quarter_pixel_aim_keeps_floor_and_round_equal(self):
        for extra in range(-40, 41):
            pixels = gm.centred_twips_for_floor(extra) * gm.PIXELS_PER_TWIP
            self.assertGreaterEqual(pixels - extra, 0.04 - 1e-9)
            self.assertLessEqual(pixels - extra, 0.46 + 1e-9)

    def test_spacing_lands_every_glyph_on_its_target_pixel(self):
        rng = random.Random(3)
        advances = [rng.randint(20, 90) for _ in range(120)]
        targets = [0]
        for advance in advances[:-1]:
            targets.append(targets[-1] + advance + rng.randint(-6, 6))
        spacing = gm.spacing_for_pixel_targets(advances, targets)
        self.assertEqual(len(spacing), len(advances))
        self.assertEqual(gm.predicted_pen_pixels(advances, spacing), targets)

    def test_twelve_twips_are_exactly_five_pixels(self):
        self.assertAlmostEqual(gm.TWIPS_PER_GRID_COMMON_STEP * gm.PIXELS_PER_TWIP, 5.0)
        self.assertAlmostEqual(5 * gm.PT_PER_PIXEL, 0.6)


class GridDetectionTest(unittest.TestCase):
    def test_finds_the_coarsest_grid_with_an_offset(self):
        values = np.array([72.024 + 0.12 * k for k in range(30)])
        self.assertAlmostEqual(coarsest_fitting_grid(values, 0.006), 0.12)

    def test_twip_positions_do_not_fit_the_pixel_grid(self):
        values = np.array([72 + 0.05 * k for k in range(30)])
        self.assertAlmostEqual(coarsest_fitting_grid(values, 0.006), 0.05)


if __name__ == "__main__":
    unittest.main()
