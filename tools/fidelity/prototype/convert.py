"""pdf -> "exact match" docx, the round 2 recipe applied to any text pdf.

usage:  .venv\\Scripts\\python.exe -m prototype.convert input.pdf output.docx [--work DIR] [--verify-cache]

pipeline: read the content stream (pdf_content) -> rebuild every font face
(font_rebuild) -> grid font per face and size (font_prep.make_grid_font) ->
group glyphs into lines (lines) -> calibrate frame twips (calibration, cached
per word build) -> per-character spacing by the floor model (grid_model) ->
pictures and shapes (vectors) -> docx (docx_out). a json report says what was
embedded, substituted, rasterised or skipped, and why.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import time
from collections import defaultdict

import win32api

import docx_writer as dw
import font_prep
import grid_model as gm
from prototype import calibration, docx_out, font_rebuild, lines, rotated, vectors
from prototype.pdf_content import Glyph, read_document

WORD_EXE = r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE"
SIZE_KEY_DIGITS = 2


def word_build() -> str:
    """word's file version, read from the executable; nothing is started."""
    info = win32api.GetFileVersionInfo(WORD_EXE, "\\")
    ms, ls = info["FileVersionMS"], info["FileVersionLS"]
    return f"{ms >> 16}.{ms & 0xFFFF}.{ls >> 16}.{ls & 0xFFFF}"


def line_twips_for(size_pt: float) -> int:
    """exact line height: tall enough that word does not clip ascenders, in whole points."""
    return 20 * max(2, math.ceil(size_pt * 1.25))


class FontPlan:
    """rebuilt faces and their per-size grid fonts for one document."""

    def __init__(self, glyphs: list[Glyph], work_dir: str, tag: str, report: dict):
        self.work_dir = os.path.join(work_dir, "fonts")
        os.makedirs(self.work_dir, exist_ok=True)
        self.tag = tag
        self.report = report
        self.faces: dict[str, font_rebuild.RebuiltFace] = {}
        self.grid_fonts: dict[tuple[str, float], tuple[str, str, float]] = {}
        self._build_faces(glyphs)

    def _build_faces(self, glyphs: list[Glyph]) -> None:
        by_face = defaultdict(dict)
        used = defaultdict(set)
        for glyph in glyphs:
            by_face[font_rebuild.face_key(glyph.font)][glyph.font.key] = glyph.font
            used[glyph.font.key].add(glyph.code)
        for key, fonts in by_face.items():
            fonts = list(fonts.values())
            family = font_rebuild.unique_family(key, self.tag)
            path = os.path.join(self.work_dir, f"{family.replace(' ', '_')}.ttf")
            entry = {"pdf_font": fonts[0].base_name, "subsets": len(fonts), "program": fonts[0].program_kind,
                     "pdf_problems": sorted({p for f in fonts for p in f.problems})}  # fmt: skip
            face = None
            if fonts[0].embedded:
                try:
                    face = font_rebuild.rebuild_face(fonts, used, family, path)
                    if not font_rebuild.may_embed(face.fs_type):
                        entry["refused"] = (
                            f"fsType {face.fs_type}: {face.permission}; a stand-in is used, tell the user"
                        )
                        face = None
                except Exception as error:  # an unreadable program falls back to a stand-in, and says so
                    entry["rebuild_error"] = f"{type(error).__name__}: {error}"
            if face is None:
                face = font_rebuild.rebuild_substitute(fonts, used, family, path)
            entry.update({
                "family": face.family, "embedded_in_pdf": fonts[0].embedded, "substitute": face.substitute_for,
                "fs_type": face.fs_type, "permission": face.permission, "cubic_source": face.cubic_source,
                "outline_error_em": face.outline_error_em, "private_use_glyphs": face.private_use,
                "problems": face.problems[:10],
            })  # fmt: skip
            self.report["fonts"].append(entry)
            self.faces[key] = face

    def grid_font(self, glyph: Glyph) -> tuple[str, str, float]:
        """(family, path, stored size) of the grid font for this glyph's face and size."""
        face = self.faces[font_rebuild.face_key(glyph.font)]
        size = round(glyph.size, SIZE_KEY_DIGITS)
        key = (face.family, size)
        if key not in self.grid_fonts:
            stored = gm.nearest_half_point(size)
            tag = f"{size:g}".replace(".", "h")
            family = f"{face.family} {tag}"
            path = os.path.join(self.work_dir, f"{family.replace(' ', '_')}.ttf")
            font_prep.make_grid_font(face.path, path, family, stored, size)
            self.grid_fonts[key] = (family, path, stored)
        return self.grid_fonts[key]

    def char(self, glyph: Glyph) -> str:
        face = self.faces[font_rebuild.face_key(glyph.font)]
        return face.char_for.get((glyph.font.key, glyph.code), " " if glyph.text == " " else "")


class AdvanceTable:
    """whole-pixel advances of grid fonts, read once per font."""

    def __init__(self):
        self._tables: dict[str, tuple[dict, dict]] = {}

    def pixels(self, path: str, char: str) -> int:
        if path not in self._tables:
            from fontTools.ttLib import TTFont

            font = TTFont(path)
            self._tables[path] = (font.getBestCmap(), font["hmtx"])
        cmap, hmtx = self._tables[path]
        units = hmtx[cmap.get(ord(char), ".notdef")][0]
        return units // gm.UNITS_PER_PIXEL


def build_runs(line: lines.Line, plan: FontPlan, advances: AdvanceTable, start_along_pt: float, report: dict):
    """char runs with spacing so each glyph lands on the pixel nearest its pdf position."""
    angle = math.radians(line.angle)
    items = []  # (char, family, path, stored, color, hidden, target along the baseline)
    text_glyphs = [g for g in line.glyphs if g.text != " "] or line.glyphs
    for index, glyph in enumerate(line.glyphs):
        along = (glyph.x - line.x) * math.cos(angle) + (glyph.y - line.y) * math.sin(angle)
        # a space takes the font of the text before it (or after it at the line start):
        # word pdfs draw list tabs as spaces in a font with no other glyph, and word
        # did not honour a docx font that holds nothing but a space
        font_glyph = glyph
        if glyph.text == " ":
            before = [g for g in line.glyphs[:index] if g.text != " "]
            font_glyph = before[-1] if before else text_glyphs[0]
        family, path, stored = plan.grid_font(font_glyph)
        if index in line.virtual_spaces:
            previous = line.glyphs[index - 1]
            space_at = (
                (previous.x - line.x) * math.cos(angle) + (previous.y - line.y) * math.sin(angle) + previous.advance
            )
            items.append((" ", family, path, stored, glyph.color, glyph.render_mode in (3, 7), space_at))
        char = plan.char(glyph)
        if not char:
            report["glyphs_without_char"] += 1
            continue
        if abs(glyph.horizontal_ratio - 1) > 0.005:
            report["horizontally_scaled_glyphs"] += 1
        items.append((char, family, path, stored, glyph.color, glyph.render_mode in (3, 7), along))
    if not items:
        return []
    offset = start_along_pt  # where word really starts the line, relative to the pdf's line start
    targets = [round(gm.pt_to_pixels(item[6] - offset)) for item in items]
    targets[0] = 0  # the first glyph is the line start word produced
    pixel_advances = [advances.pixels(item[2], item[0]) for item in items]
    spacing = gm.spacing_for_pixel_targets(pixel_advances, targets)
    report.setdefault("line_plans", []).append({
        "page": line.page, "x": line.x, "y": line.y, "offset_pt": offset, "text": "".join(i[0] for i in items),
        "targets_px": targets, "advances_px": pixel_advances, "spacing_twips": spacing,
    })  # fmt: skip
    return [
        docx_out.CharRun(item[0], item[1], round(item[3] * 2), docx_out.hex_color(item[4]), twips, item[5])
        for item, twips in zip(items, spacing, strict=True)
    ]


def _image_drawing(package: docx_out.Package, image, page_height: float) -> str:
    a, b, c, d, e, f = image.matrix
    relationship = package.add_media(image.extension, image.data)
    if abs(b) < 1e-6 and abs(c) < 1e-6:
        width, height = abs(a), abs(d)
        left, bottom = e + min(0.0, a), f + min(0.0, d)
        return docx_out.picture_drawing(package.shape_id(), relationship, left, page_height - bottom - height,
                                        width, height, flip_h=a < 0, flip_v=d < 0)  # fmt: skip
    width, height = math.hypot(a, b), math.hypot(c, d)
    centre_x, centre_y = e + (a + c) / 2, f + (b + d) / 2
    angle = math.degrees(math.atan2(b, a))
    return docx_out.picture_drawing(package.shape_id(), relationship, centre_x - width / 2,
                                    page_height - centre_y - height / 2, width, height, rotation_deg=-angle)  # fmt: skip


def _shape_drawing(package: docx_out.Package, shape, page_height: float) -> str:
    if isinstance(shape, vectors.RectShape):
        return docx_out.rect_drawing(
            package.shape_id(), shape.left, page_height - shape.top, shape.right - shape.left, shape.top - shape.bottom,
            docx_out.hex_color(shape.fill) if shape.fill else None,
            docx_out.hex_color(shape.stroke) if shape.stroke else None, shape.line_width,
        )  # fmt: skip
    return docx_out.line_drawing(package.shape_id(), shape.x0, page_height - shape.y0, shape.x1, page_height - shape.y1,
                                 docx_out.hex_color(shape.color), shape.line_width)  # fmt: skip


def convert(pdf_path: str, docx_path: str, work_dir: str, verify_cache: bool = False) -> dict:
    started = time.perf_counter()
    os.makedirs(work_dir, exist_ok=True)
    tag = os.path.splitext(os.path.basename(pdf_path))[0]
    report = {"source": os.path.abspath(pdf_path), "docx": os.path.abspath(docx_path), "fonts": [], "problems": [],
              "glyphs_without_char": 0, "horizontally_scaled_glyphs": 0, "rotated_lines": 0, "hidden_lines": 0,
              "images": 0, "jpeg_kept": 0, "shapes": 0, "rasterised_vector_items": 0, "raster_pictures": 0}  # fmt: skip
    pages = read_document(pdf_path)
    all_glyphs = [g for page in pages for g in page.glyphs]
    report["glyphs"] = len(all_glyphs)
    report["scripts"] = lines.script_warnings(all_glyphs)
    for page in pages:
        report["problems"].extend(f"page {page.index + 1}: {p}" for p in page.problems)
    plan = FontPlan(all_glyphs, work_dir, tag, report)
    page_lines = lines.group_lines(all_glyphs)
    report["lines"] = len(page_lines)

    requests = {}
    for line in page_lines:
        if abs(line.angle) > lines.ANGLE_TOLERANCE_DEG:
            continue
        height = pages[line.page].height
        line_twips = line_twips_for(line.max_size)
        key = calibration.placement_key(line.page, line.x, height - line.y, line_twips)
        requests[key] = calibration.Request(line.page, line.x, height - line.y, line_twips)
    cache = calibration.CalibrationCache(word_build())
    calibrated = calibration.calibrate(list(requests.values()), cache, os.path.join(work_dir, "calibration"),
                                       f"{tag}_calibration", verify=verify_cache) if requests else {"placements": {}}  # fmt: skip
    report["calibration"] = {k: v for k, v in calibrated.items() if k != "placements"}
    report["calibration"]["word_build"] = word_build()

    package = docx_out.Package()
    advances = AdvanceTable()
    rotated_lines: list[rotated.RotatedLine] = []
    page_parts = []
    for page in pages:
        drawings = []
        simple_items, complex_items = [], list(page.shadings)
        for item in page.paths:
            shapes = vectors.classify(item)
            if shapes is None:
                complex_items.append(item)
            else:
                simple_items.extend(shapes)
        raster = vectors.rasterise(pdf_path, page, complex_items)
        ordered = [(s.sequence, "shape", s) for s in simple_items] + [(i.sequence, "image", i) for i in page.images]
        if raster is not None:
            ordered.append((raster.sequence, "raster", raster))
            report["rasterised_vector_items"] += raster.items
            report["raster_pictures"] += 1
        for _, kind, item in sorted(ordered, key=lambda entry: entry[0]):
            if kind == "shape":
                drawings.append(_shape_drawing(package, item, page.height))
                report["shapes"] += 1
            elif kind == "image":
                drawings.append(_image_drawing(package, item, page.height))
                report["images"] += 1
                report["jpeg_kept"] += item.extension == "jpeg"
            else:
                relationship = package.add_media("png", item.png)
                drawings.append(docx_out.picture_drawing(package.shape_id(), relationship, item.left,
                                                         page.height - item.top, item.right - item.left, item.top - item.bottom))  # fmt: skip
        frames = []
        for line in (line for line in page_lines if line.page == page.index):
            line_twips = line_twips_for(line.max_size)
            report["hidden_lines"] += line.hidden
            if abs(line.angle) > lines.ANGLE_TOLERANCE_DEG:
                report["rotated_lines"] += 1
                runs = build_runs(line, plan, advances, 0.0, report)
                rotated_lines.append(rotated.RotatedLine(
                    line.page, runs, (line.x, page.height - line.y), line.angle, line.width() + 20, line_twips,
                    calibration.BASELINE_SHARE * line_twips / dw.TWIPS_PER_PT, "".join(r.char for r in runs)))  # fmt: skip
                continue
            placement = calibrated["placements"][
                calibration.placement_key(line.page, line.x, page.height - line.y, line_twips)
            ]
            runs = build_runs(line, plan, advances, placement.x_pt - line.x, report)
            width = min(line.width() + 40, max(page.width - placement.x_pt, line.width() + 4))
            frames.append(docx_out.frame_paragraph(runs, placement.x_twips, placement.y_twips,
                                                   round(width * dw.TWIPS_PER_PT), line_twips))  # fmt: skip
        page_parts.append((page, drawings, frames))
    for family, path, _ in plan.grid_fonts.values():
        with open(path, "rb") as handle:
            package.fonts.append(dw.EmbeddedFont(family, handle.read()))
    report["rotated"] = rotated.measure_offsets(
        rotated_lines, [(p.width, p.height) for p in pages], package.fonts, os.path.join(work_dir, "rotated")
    )
    for page, drawings, frames in page_parts:
        drawings += [line.drawing(package.shape_id()) for line in rotated_lines if line.page == page.index]
        package.body.append(docx_out.anchor_paragraph("".join(drawings)))
        package.body.extend(frames)
        if page.index + 1 < len(pages):
            following = pages[page.index + 1]
            if (following.width, following.height) == (page.width, page.height):
                package.body.append(dw.page_break_paragraph_xml())
            else:
                package.body.append(docx_out.section_break_paragraph(page.width, page.height))
    last = pages[-1]
    package.final_section = docx_out.section_properties(last.width, last.height)
    package.save(docx_path)
    report["grid_fonts"] = len(plan.grid_fonts)
    report["docx_bytes"] = os.path.getsize(docx_path)
    report["pdf_bytes"] = os.path.getsize(pdf_path)
    report["seconds"] = time.perf_counter() - started
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("pdf")
    parser.add_argument("docx")
    parser.add_argument("--work", default=None)
    parser.add_argument("--verify-cache", action="store_true")
    args = parser.parse_args()
    work = args.work or os.path.join(os.path.dirname(os.path.abspath(args.docx)), "work")
    report = convert(args.pdf, args.docx, work, args.verify_cache)
    with open(os.path.splitext(args.docx)[0] + ".convert.json", "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2, default=str)
    print(json.dumps({k: v for k, v in report.items() if k != "fonts"}, indent=1, default=str)[:2000])


if __name__ == "__main__":
    main()
