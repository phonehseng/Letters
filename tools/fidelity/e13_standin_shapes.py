"""e13: how close are the stand-ins for non-embedded standard fonts, in glyph shape?

a pdf that names Helvetica, Times or Courier without embedding them can only
be matched with stand-ins. two candidate sets:
    ofl      Arimo, Tinos, Cousine (metric-compatible, sil ofl 1.1, shippable)
    windows  Arial, Times New Roman, Courier New from C:\\Windows\\Fonts
             (only if the user's machine has them; never shipped)
neither is the font the pdf names, so the "same fonts" criterion fails by
definition. this measures how far apart the two sets are in shape: every
printable ascii glyph is rendered by freetype (pillow) at 200 pixels per em
with its pen origin on a fixed point, and the ink masks are compared
(intersection over union, and the share of pixels that differ). it also reads
fsType of the windows fonts, which says whether a document may embed them.

usage:  .venv\\Scripts\\python.exe e13_standin_shapes.py
"""

from __future__ import annotations

import os

import numpy as np
from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont

from corpus_builder import fonts as open_fonts

WINDOWS_FONTS = r"C:\Windows\Fonts"
PIXELS_PER_EM = 200
CANVAS = (360, 300)  # wide enough for an em plus side bearings, tall enough for ascender and descender
BASELINE_Y = 230
CHARS = [chr(c) for c in range(0x21, 0x7F)]
PAIRS = {
    "Helvetica": (("arial", "regular"), "arial.ttf"),
    "Helvetica-Bold": (("arial", "bold"), "arialbd.ttf"),
    "Times-Roman": (("times", "regular"), "times.ttf"),
    "Times-Bold": (("times", "bold"), "timesbd.ttf"),
    "Courier": (("mono", "regular"), "cour.ttf"),
}


def glyph_mask(path: str, char: str) -> np.ndarray | None:
    """the glyph's ink at 200 pixels per em, pen origin at a fixed point (freetype, nonzero fill)."""
    if ord(char) not in TTFont(path, lazy=True).getBestCmap():
        return None
    image = Image.new("L", CANVAS, 0)
    face = ImageFont.truetype(path, PIXELS_PER_EM, layout_engine=ImageFont.Layout.BASIC)
    ImageDraw.Draw(image).text((20, BASELINE_Y), char, fill=255, font=face, anchor="ls")
    return np.array(image) >= 128


def compare(path_a: str, path_b: str) -> dict:
    ious, differing, worst = [], [], ("", 1.0)
    for char in CHARS:
        mask_a, mask_b = glyph_mask(path_a, char), glyph_mask(path_b, char)
        if mask_a is None or mask_b is None:
            continue
        union = (mask_a | mask_b).sum()
        if not union:
            continue
        iou = (mask_a & mask_b).sum() / union
        ious.append(iou)
        differing.append((mask_a ^ mask_b).sum() / union)
        if iou < worst[1]:
            worst = (char, iou)
    return {"glyphs": len(ious), "iou_mean": float(np.mean(ious)), "iou_min": float(np.min(ious)),
            "worst_char": worst[0], "differing_share_mean": float(np.mean(differing))}  # fmt: skip


def main() -> None:
    print(f"{'pdf font':<16} {'ofl stand-in':<22} {'windows font':<14} fsType  IoU mean / min (worst)   differing px")
    for pdf_name, ((role, style), windows_file) in PAIRS.items():
        ofl_path = open_fonts.font_file(role, style)
        windows_path = os.path.join(WINDOWS_FONTS, windows_file)
        if not os.path.exists(windows_path):
            print(f"{pdf_name:<16} {os.path.basename(ofl_path):<22} {windows_file} not on this machine")
            continue
        result = compare(ofl_path, windows_path)
        fs_type = TTFont(windows_path, lazy=True)["OS/2"].fsType
        print(f"{pdf_name:<16} {os.path.basename(ofl_path):<22} {windows_file:<14} {fs_type:#06x}  "
              f"{result['iou_mean']:.3f} / {result['iou_min']:.3f} ({result['worst_char']!r})      "
              f"{100 * result['differing_share_mean']:.1f}%")  # fmt: skip


if __name__ == "__main__":
    main()
