"""unit tests for the parts that do not need word: run with
.venv\\Scripts\\python.exe -m unittest test_units -v
"""

from __future__ import annotations

import random
import unittest

import docx_writer as dw
from compare import match_chars
from e3_advance_widths import diffused_spacing
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


if __name__ == "__main__":
    unittest.main()
