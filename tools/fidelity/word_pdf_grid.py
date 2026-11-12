"""look for the coordinate grid in a pdf that word exported.

word does not write arbitrary coordinates: this reads the text matrices (Tm),
font sizes (Tf) and glyph pen positions relative to each line start, and tests
them against candidate grids. the result explains the floor under e2 and e3.

usage:  .venv\\Scripts\\python.exe word_pdf_grid.py out\\e2\\e2_frames.word.pdf [more.pdf ...]
"""

from __future__ import annotations

import re
import sys
import zlib
from collections import defaultdict

import numpy as np

from pdf_analysis import extract_chars

CANDIDATE_GRIDS_PT = {
    "1/600 in (0.12 pt)": 0.12,
    "twip (0.05 pt)": 0.05,
    "1/3000 in (0.024 pt)": 0.024,
}
# word prints about 5 significant digits, so coordinates >= 100 pt carry up to 0.005 pt of print rounding
PRINT_ROUNDING_PT = 0.005


def content_streams(pdf_path: str) -> list[bytes]:
    data = open(pdf_path, "rb").read()
    streams = []
    for match in re.finditer(rb"stream\r?\n", data):
        end = data.find(b"endstream", match.end())
        try:
            streams.append(zlib.decompress(data[match.end() : end]))
        except zlib.error:
            continue
    return streams


def text_matrices_and_sizes(pdf_path: str) -> tuple[np.ndarray, list[float]]:
    positions, sizes = [], set()
    for stream in content_streams(pdf_path):
        positions += [(float(x), float(y)) for x, y in re.findall(rb"1 0 0 1 ([\d.]+) ([\d.]+) Tm", stream)]
        sizes |= {float(s) for s in re.findall(rb"/F\d+ ([\d.]+) Tf", stream)}
    return np.array(positions), sorted(sizes)


def distance_to_grid(values: np.ndarray, unit: float) -> np.ndarray:
    steps = values / unit
    return np.abs(steps - np.round(steps)) * unit


def pen_positions_from_line_start(pdf_path: str) -> np.ndarray:
    lines = defaultdict(list)
    for char in extract_chars(pdf_path):
        lines[(char.page, round(char.origin_y, 2))].append(char.origin_x)
    return np.concatenate([np.sort(xs) - min(xs) for xs in lines.values()])


def report(pdf_path: str) -> None:
    matrices, sizes = text_matrices_and_sizes(pdf_path)
    matrices = matrices[matrices[:, 0] > 1]  # skip the page-origin matrices word writes for empty boxes
    relative = pen_positions_from_line_start(pdf_path)
    print(f"== {pdf_path}")
    print(f"   font sizes written (Tf): {sizes}")
    for label, unit in CANDIDATE_GRIDS_PT.items():
        tm = np.concatenate([distance_to_grid(matrices[:, 0], unit), distance_to_grid(matrices[:, 1], unit)])
        on_grid = (tm <= PRINT_ROUNDING_PT + 1e-6).mean()
        pen = distance_to_grid(relative, unit)
        print(
            f"   {label:<22} line starts (Tm) on grid: {100 * on_grid:5.1f}%   "
            f"pen positions from line start: max off-grid {pen.max():.4f} pt, mean {pen.mean():.4f} pt"
        )


def main() -> None:
    for path in sys.argv[1:]:
        report(path)


if __name__ == "__main__":
    main()
