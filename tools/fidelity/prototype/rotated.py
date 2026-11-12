"""rotated lines: drawingml text boxes, placed by one measuring pass.

frames cannot rotate, so rotated text goes into rotated text boxes. the first
attempt placed boxes from geometry alone and missed by about 20 pt along the
text (word's rotated boxes do not lay out the way the drawingml geometry
suggests), so the boxes are first exported on their own, the offset between
where each line should start and where word started it is measured, and the
boxes are moved by that offset. the offset is a translation, so one pass is enough.
"""

from __future__ import annotations

import math
import os
from dataclasses import dataclass

import docx_writer as dw
from prototype import docx_out
from prototype.lines import group_lines
from prototype.pdf_content import read_document
from word_export import export_docx_to_pdf


@dataclass
class RotatedLine:
    page: int
    runs: list[docx_out.CharRun]
    origin_top_down: tuple[float, float]
    angle: float
    width_pt: float
    line_twips: int
    baseline_pt: float
    text: str
    correction: tuple[float, float] = (0.0, 0.0)

    def drawing(self, shape_id: int) -> str:
        x, y = self.origin_top_down
        return docx_out.rotated_text_drawing(
            shape_id, self.runs, (x + self.correction[0], y + self.correction[1]), self.angle, self.width_pt,
            self.line_twips, self.baseline_pt,
        )  # fmt: skip


def measure_offsets(rotated: list[RotatedLine], page_sizes: list[tuple[float, float]], fonts, work_dir: str) -> dict:
    """export only the rotated boxes, then set each line's correction from what word did."""
    if not rotated:
        return {"rotated_probe": "none needed"}
    os.makedirs(work_dir, exist_ok=True)
    package = docx_out.Package(fonts=fonts)
    for index, (width, height) in enumerate(page_sizes):
        drawings = "".join(line.drawing(package.shape_id()) for line in rotated if line.page == index)
        package.body.append(docx_out.anchor_paragraph(drawings))
        if index + 1 < len(page_sizes):
            if page_sizes[index + 1] == (width, height):
                package.body.append(dw.page_break_paragraph_xml())
            else:
                package.body.append(docx_out.section_break_paragraph(width, height))
    package.final_section = docx_out.section_properties(*page_sizes[-1])
    docx = package.save(os.path.join(work_dir, "rotated_probe.docx"))
    pdf = os.path.join(work_dir, "rotated_probe.word.pdf")
    export_docx_to_pdf(docx, pdf)
    pages = read_document(pdf)
    measured = group_lines([g for page in pages for g in page.glyphs])
    matched = 0
    for line in rotated:
        height = page_sizes[line.page][1]
        target_x, target_y = line.origin_top_down
        candidates = [m for m in measured if m.page == line.page and abs(m.angle - line.angle) < 0.5
                      and "".join(g.text for g in m.glyphs).replace(" ", "") == line.text.replace(" ", "")]  # fmt: skip
        if not candidates:
            continue
        best = min(candidates, key=lambda m: math.hypot(m.x - target_x, (height - m.y) - target_y))
        line.correction = (target_x - best.x, target_y - (height - best.y))
        matched += 1
    return {"rotated_lines": len(rotated), "rotated_measured": matched,
            "corrections_pt": [tuple(round(v, 3) for v in line.correction) for line in rotated]}  # fmt: skip
