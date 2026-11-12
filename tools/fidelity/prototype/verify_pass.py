"""one or two checking exports: find glyphs word moved off the planned pixel and re-plan those lines.

every glyph of the docx starts its own text object, so word's pdf gives each
glyph's exact layout pixel (word_writer_model). with the balanced spacing plan
(grid_model.spacing_for_pixel_steps) word follows the plan for all but a few
glyphs per thousand: a glyph whose advance on word's device differs by a
pixel from the grid font's, which word repairs some glyphs later (the journal's
italic 'W' was one). for each line with a miss, the first wrong step is asked
for one pixel shorter or longer and the line is planned again; one export per
pass, at most MAX_PASSES, and a line keeps its best plan.
"""

from __future__ import annotations

import os

import grid_model as gm
import word_writer_model as wm
from word_export import export_docx_to_pdf
from word_pdf_raw import read_text_objects

MAX_PASSES = 2


def produced_offsets(pdf_path: str, plans: list[dict], tables: dict[int, wm.StepTable]) -> list[list[int] | None]:
    """per plan, word's layout pixel offsets from the planned start pixel, or None if the line was not found."""
    by_page: dict[int, list] = {}
    for obj in read_text_objects(pdf_path):
        if obj.glyphs and abs(obj.tm[1]) < 1e-6:
            by_page.setdefault(obj.page, []).append(obj)
    result = []
    for plan in plans:
        table = tables[plan["page"]]
        start_pt = wm.exported_x(plan["start_pixel"], table)
        row = sorted((o for o in by_page.get(plan["page"], []) if abs(o.y - plan["word_y"]) < 0.01
                      and o.x >= start_pt - 0.01), key=lambda o: o.x)[: len(plan["text"])]  # fmt: skip
        if len(row) != len(plan["text"]) or any(len(o.glyphs) != 1 for o in row):
            result.append(None)
            continue
        pixels = [wm.decode_x(o.x, table) for o in row]
        result.append(None if None in pixels else [p - plan["start_pixel"] for p in pixels])
    return result


def first_miss(plan: dict, offsets: list[int]) -> tuple[int, int] | None:
    """(glyph index, pixels word put it off the plan) of the first wrong glyph."""
    for index, (want, got) in enumerate(zip(plan["targets_px"], offsets, strict=True)):
        if want != got:
            return index, got - want
    return None


def correct(docx_builder, plans: list[dict], tables: dict[int, wm.StepTable], work_dir: str) -> dict:
    """run checking exports; docx_builder(spacing_by_plan) rewrites the docx and returns its path."""
    os.makedirs(work_dir, exist_ok=True)
    asked = [list(plan["targets_px"]) for plan in plans]  # what each line asks word for
    best: dict[int, tuple[int, list[int]]] = {}  # plan index -> (glyphs off plan, spacing)
    report = {"passes": 0, "lines_with_misses": [], "glyphs_off_plan": [], "seconds": 0.0}
    current: dict[int, list[int]] = {}  # spacing overrides in the docx being checked
    docx = docx_builder(current)
    for number in range(MAX_PASSES + 1):
        pdf = os.path.join(work_dir, f"check_{number}.word.pdf")
        info = export_docx_to_pdf(docx, pdf)
        report["seconds"] += sum(info["timings"].values())
        offsets = produced_offsets(pdf, plans, tables)
        changed = {}
        missing_lines = off_glyphs = 0
        for index, (plan, produced) in enumerate(zip(plans, offsets, strict=True)):
            if produced is None:
                continue
            misses = sum(1 for want, got in zip(plan["targets_px"], produced, strict=True) if want != got)
            spacing = gm.spacing_for_pixel_steps(plan["advances_px"], asked[index])
            if index not in best or misses < best[index][0]:
                best[index] = (misses, spacing)
            if not misses:
                continue
            missing_lines += 1
            off_glyphs += misses
            miss = first_miss(plan, produced)
            if miss is None or number == MAX_PASSES:
                continue
            glyph, delta = miss
            # ask for that one step shorter (or longer) by the pixels word added there, keep the rest
            asked[index] = asked[index][:glyph] + [t - delta for t in asked[index][glyph:]]
            changed[index] = gm.spacing_for_pixel_steps(plan["advances_px"], asked[index])
        report["lines_with_misses"].append(missing_lines)
        report["glyphs_off_plan"].append(off_glyphs)
        report["passes"] = number
        if not changed:
            break
        current.update(changed)
        docx = docx_builder(current)
    if current:
        # every line keeps the spacing of the pass where it had the fewest misses
        docx_builder({index: spacing for index, (_, spacing) in best.items()})
    report["glyphs_off_plan_final"] = sum(misses for misses, _ in best.values())
    return report
