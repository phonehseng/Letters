"""turning TextLine lists into docx body xml, one function per positioning method.

word positions the top of a line box, while a pdf positions the baseline, so
each method needs the distance from line top to baseline. that distance is
measured once by e2_positioning.py --calibrate and passed in as baseline_offsets.
"""

from __future__ import annotations

from collections import defaultdict
from collections.abc import Callable

import docx_writer as dw
from test_pdf import PAGE_HEIGHT_PT, TextLine

# exact line height used by every method, 13.6 pt is 272 twips
LINE_HEIGHT_TWIPS = 272
BOX_SLACK_PT = 24.0  # extra width so word never wraps a positioned line

BaselineOffsets = dict[float, float]  # font size -> pt from line top to baseline
RunBuilder = Callable[[TextLine], str]
WidthOf = Callable[[TextLine], float]  # natural width of a line in pt


def line_top_pt(line: TextLine, baseline_offsets: BaselineOffsets) -> float:
    """top of the line box in points from the top edge of the page."""
    return PAGE_HEIGHT_PT - line.baseline_y - baseline_offsets.get(line.size, 0.0)


def lines_by_page(lines: list[TextLine]) -> dict[int, list[TextLine]]:
    pages: dict[int, list[TextLine]] = defaultdict(list)
    for line in lines:
        pages[line.page].append(line)
    return dict(sorted(pages.items()))


def plain_runs(family: str) -> RunBuilder:
    return lambda line: dw.run_xml(line.text, family, line.size)


def frames_body(
    lines: list[TextLine], run_builder: RunBuilder, width_of: WidthOf, baseline_offsets: BaselineOffsets
) -> str:
    parts = []
    for page, page_lines in lines_by_page(lines).items():
        if page > 0:
            parts.append(dw.page_break_paragraph_xml())
        for line in page_lines:
            parts.append(
                dw.frame_paragraph_xml(
                    run_builder(line),
                    x_twips=dw.pt_to_twips(line.x),
                    y_twips=dw.pt_to_twips(line_top_pt(line, baseline_offsets)),
                    width_twips=dw.pt_to_twips(width_of(line) + BOX_SLACK_PT),
                    line_twips=LINE_HEIGHT_TWIPS,
                )
            )
    return "".join(parts)


def textboxes_body(
    lines: list[TextLine], run_builder: RunBuilder, width_of: WidthOf, baseline_offsets: BaselineOffsets
) -> str:
    parts = []
    shape_id = 1
    box_height_emu = dw.pt_to_emu(LINE_HEIGHT_TWIPS / dw.TWIPS_PER_PT + 4)
    for page, page_lines in lines_by_page(lines).items():
        runs = []
        for line in page_lines:
            inner = dw.paragraph_xml(run_builder(line), line_twips=LINE_HEIGHT_TWIPS)
            runs.append(
                dw.textbox_run_xml(
                    inner,
                    x_emu=dw.pt_to_emu(line.x),
                    y_emu=dw.pt_to_emu(line_top_pt(line, baseline_offsets)),
                    width_emu=dw.pt_to_emu(width_of(line) + BOX_SLACK_PT),
                    height_emu=box_height_emu,
                    shape_id=shape_id,
                )
            )
            shape_id += 1
        parts.append(dw.anchor_paragraph_xml("".join(runs), page_break_before=page > 0))
    return "".join(parts)


def _rows(page_lines: list[TextLine]) -> list[list[TextLine]]:
    """lines sharing a baseline become one paragraph, left to right."""
    rows: dict[float, list[TextLine]] = defaultdict(list)
    for line in page_lines:
        rows[round(line.baseline_y, 4)].append(line)
    ordered = [sorted(row, key=lambda line: line.x) for _, row in sorted(rows.items(), key=lambda item: -item[0])]
    return ordered


def flow_body(lines: list[TextLine], run_builder: RunBuilder, baseline_offsets: BaselineOffsets) -> str:
    """ordinary paragraphs: indent for x, spacing before for y, tab stops for later columns.

    spacing is whole twips, so each paragraph's spacing is chosen against the
    absolute target, which keeps rounding from adding up down the page."""
    parts = []
    for page, page_lines in lines_by_page(lines).items():
        cursor_twips = 0  # bottom of the previous line box, from the page top
        if page > 0:
            # word drops spacing before on the first paragraph of a page that
            # follows a page break (measured: page 2 moved up by ~50 pt), so a
            # 1 pt break paragraph goes first and real lines are never first
            parts.append(dw.page_break_paragraph_xml())
            cursor_twips = dw.PAGE_BREAK_PARAGRAPH_TWIPS
        for row in _rows(page_lines):
            first = row[0]
            target_top_twips = round(line_top_pt(first, baseline_offsets) * dw.TWIPS_PER_PT)
            before_twips = target_top_twips - cursor_twips
            if before_twips < 0:
                raise ValueError("rows overlap, spacing before cannot be negative")
            runs = run_builder(first)
            tab_stops = []
            for later in row[1:]:
                tab_stops.append(dw.pt_to_twips(later.x))
                runs += dw.tab_run_xml() + run_builder(later)
            parts.append(
                dw.paragraph_xml(
                    runs,
                    line_twips=LINE_HEIGHT_TWIPS,
                    before_twips=before_twips,
                    indent_twips=dw.pt_to_twips(first.x),
                    tab_stops_twips=tuple(tab_stops),
                )
            )
            cursor_twips = target_top_twips + LINE_HEIGHT_TWIPS
    return "".join(parts)


def build_body(
    method: str, lines: list[TextLine], run_builder: RunBuilder, width_of: WidthOf, baseline_offsets: BaselineOffsets
) -> str:
    if method == "frames":
        return frames_body(lines, run_builder, width_of, baseline_offsets)
    if method == "textboxes":
        return textboxes_body(lines, run_builder, width_of, baseline_offsets)
    if method == "flow":
        return flow_body(lines, run_builder, baseline_offsets)
    raise ValueError(f"unknown method {method}")


METHODS = ("frames", "textboxes", "flow")
