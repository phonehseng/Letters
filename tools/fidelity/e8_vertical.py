"""e8: vertical placement. where do baselines land, and do font sizes move them?

part 1, the grid: frames whose y grows by 1 twip at a time (same line height,
same size); the distinct baselines word produces show the vertical grid.
part 2, sizes: the same 9 pt text in four situations, baseline measured each time:
    a 9 pt frame on its own
    a 9 pt frame with a 14 pt frame beside it on the same y
    one paragraph whose line holds 9 pt and 14 pt runs, exact line height
    the same mixed paragraph with "at least" line height
(e5 and e7 already measure calibrated frame y on full test sets, see README.)

usage:  .venv\\Scripts\\python.exe e8_vertical.py
output: out\\e8\\
"""

from __future__ import annotations

import json
import os
from collections import defaultdict

import numpy as np

import docx_writer as dw
import grid_model as gm
import test_fonts
from pdf_analysis import extract_chars
from test_pdf import PAGE_HEIGHT_PT
from word_export import export_docx_to_pdf

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e8")
SWEEP = 60
LINE_TWIPS = 272


def at_least_paragraph(runs: str, x_twips: int, y_twips: int) -> str:
    """a framed paragraph whose line height may grow with its tallest run."""
    frame = (
        f'<w:framePr w:w="6000" w:wrap="through" w:vAnchor="page" w:hAnchor="page" w:x="{x_twips}" w:y="{y_twips}"/>'
    )
    return (
        f"<w:p><w:pPr>{frame}{dw.LAYOUT_NEUTRAL_PPR}"
        f'<w:spacing w:before="0" w:after="0" w:line="{LINE_TWIPS}" w:lineRule="atLeast"/></w:pPr>{runs}</w:p>'
    )


def frame(runs: str, x_twips: int, y_twips: int) -> str:
    return dw.frame_paragraph_xml(runs, x_twips=x_twips, y_twips=y_twips, width_twips=3000, line_twips=LINE_TWIPS)


CASE_X_PT = {
    "alone, exact": (150, 4008),
    "beside a 14 pt frame, exact": (250, 4008),
    "9 + 14 pt runs in one line, exact": (400, 4008),
    "alone, at least": (150, 8004),
    "9 + 14 pt runs in one line, at least": (250, 8004),
}


def build(family: str) -> tuple[str, list]:
    parts, expectations = [], []
    for k in range(SWEEP):
        y = 720 + k * 241 + k  # one twip of sub-step per line
        parts.append(
            dw.frame_paragraph_xml(
                dw.run_xml("Hn", family, 9), x_twips=1440, y_twips=y, width_twips=1200, line_twips=LINE_TWIPS
            )
        )
        expectations.append(("sweep", y))
    # every case sits on the same frame top and differs only in x, so word's
    # rounding of the top is identical and only the size effect remains
    exact_y, at_least_y = 4008, 8004
    cases = [
        ("alone, exact", [frame(dw.run_xml("Hx", family, 9), 3000, exact_y)]),
        (
            "beside a 14 pt frame, exact",
            [frame(dw.run_xml("Hx", family, 9), 5000, exact_y), frame(dw.run_xml("HH", family, 14), 6200, exact_y)],
        ),
        (
            "9 + 14 pt runs in one line, exact",
            [frame(dw.run_xml("Hx", family, 9) + dw.run_xml("   HH", family, 14), 8000, exact_y)],
        ),
        ("alone, at least", [at_least_paragraph(dw.run_xml("Hx", family, 9), 3000, at_least_y)]),
        (
            "9 + 14 pt runs in one line, at least",
            [at_least_paragraph(dw.run_xml("Hx", family, 9) + dw.run_xml("   HH", family, 14), 5000, at_least_y)],
        ),
    ]
    for name, paragraphs in cases:
        parts.extend(paragraphs)
        expectations.append((name, None))
    return "".join(parts), expectations


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    family = test_fonts.TRUETYPE_FAMILY
    embedded = dw.EmbeddedFont(family, open(test_fonts.truetype_font(), "rb").read())
    body, _ = build(family)
    docx = dw.write_docx(
        os.path.join(OUT_DIR, "vertical.docx"), body, family=family, default_size_pt=9, embedded=embedded
    )
    pdf = os.path.join(OUT_DIR, "vertical.word.pdf")
    export_docx_to_pdf(docx, pdf)
    chars = extract_chars(pdf)

    sweep = defaultdict(list)
    for char in chars:
        if char.origin_x < 100:
            sweep[round(PAGE_HEIGHT_PT - char.origin_y, 3)].append(char)
    baselines = np.array(sorted(sweep))
    tops = np.array([(720 + k * 242) / dw.TWIPS_PER_PT for k in range(SWEEP)])
    offsets = baselines - tops[: len(baselines)]
    pixels = baselines / gm.PT_PER_PIXEL
    phases = sorted({round(p % 1, 2) for p in pixels})
    print(
        f"sweep: {len(baselines)} lines, top-to-baseline {offsets.min():.3f}..{offsets.max():.3f} pt "
        f"(spread {offsets.max() - offsets.min():.3f}), baseline pixel phases {phases}"
    )

    measured = {}
    for name, (x_pt, y_twips) in CASE_X_PT.items():
        top = y_twips / dw.TWIPS_PER_PT
        nine_point = [
            c
            for c in chars
            if abs(c.origin_x - x_pt) < 1 and c.font_size < 10 and 0 < PAGE_HEIGHT_PT - c.origin_y - top < 30
        ]
        measured[name] = (PAGE_HEIGHT_PT - nine_point[0].origin_y - top) if nine_point else None
    for name, offset in measured.items():
        shown = "not found" if offset is None else f"{offset:.3f} pt below its frame top"
        print(f"{name:<38} 9 pt baseline {shown}")
    with open(os.path.join(OUT_DIR, "e8_summary.json"), "w", encoding="utf-8") as handle:
        json.dump(
            {
                "sweep_offset_range": [float(offsets.min()), float(offsets.max())],
                "pixel_phases": phases,
                "sizes": measured,
            },
            handle,
            indent=2,
        )


if __name__ == "__main__":
    main()
