"""frame placement from word's measured rules, with what still needs measuring cached per word build.

round 3 probed every twip value it used and cached the printed point values.
round 4 replaces that with the exact model in word_writer_model.py:
    x  frame twips -> layout pixel is a closed rule (twips_pixel); layout pixel
       -> printed x needs the writer's step table for the page width
    y  frame twips -> baseline pixel is twips_pixel(t) + offset, where the
       offset depends on the exact line height and on t mod 60 (60 twips are
       exactly 25 pixels, so 60 values describe a line height completely);
       baseline pixel -> printed y needs the step table for the page height
so only two kinds of facts are measured, once per word build, and cached in
out\\calibration_cache\\word-<build>-model.json:
    "steps"    {"x:<page width px>": [...], "y:<page height px>": [...]}
    "baseline" {"<line twips>": [60 offsets, index t mod 60]}
the printed values never go into the cache: they depend on the page size,
which is why round 3's y cache disagreed with itself (0.9%): its probe pages
were not always the size of the document page.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass

import docx_writer as dw
import test_fonts
import word_writer_model as wm
from word_export import export_docx_to_pdf
from word_pdf_raw import read_text_objects

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE_DIR = os.path.join(HERE, "out", "calibration_cache")
PROBE_SIZE_PT = 12.0
PROBE_LINE_TWIPS = 240
PROBE_BASELINE_OFFSET = 80  # 12 pt exact line: baseline 80 px below the frame top, every t (measured, e10)
BASELINE_SHARE = 0.804  # first guess of the baseline position in an exact line (e2, e8)
FRAME_TWIPS = 200  # probe frames: wide enough for one glyph, narrow enough to reach the page edge
COARSE_STEP_PX = 5


@dataclass
class Placement:
    x_twips: int
    y_twips: int
    x_pixel: int  # layout pixel of the line start, from the left page edge
    baseline_pixel: int  # layout pixel of the baseline, from the top page edge
    x_pt: float  # where word will print the line start
    baseline_from_top_pt: float  # where word will print the baseline, from the top edge


class ModelCache:
    def __init__(self, word_build: str):
        os.makedirs(CACHE_DIR, exist_ok=True)
        self.path = os.path.join(CACHE_DIR, f"word-{word_build.replace(' ', '_')}-model.json")
        self.steps: dict[str, list[int]] = {f"{a}:{n}": list(s) for (a, n), s in wm.MEASURED_STEPS.items()}
        self.baseline: dict[str, list[int]] = {}
        if os.path.exists(self.path):
            with open(self.path, encoding="utf-8") as handle:
                data = json.load(handle)
            self.steps.update(data.get("steps", {}))
            self.baseline.update(data.get("baseline", {}))

    def save(self) -> None:
        with open(self.path, "w", encoding="utf-8") as handle:
            json.dump({"steps": self.steps, "baseline": self.baseline}, handle, indent=0)

    def table(self, axis: str, page_pixels: int) -> wm.StepTable:
        return wm.StepTable(tuple(self.steps[f"{axis}:{page_pixels}"]))


# ---------------------------------------------------------------- measuring


def _probe_font() -> tuple[str, dw.EmbeddedFont]:
    family = test_fonts.TRUETYPE_FAMILY
    return family, dw.EmbeddedFont(family, open(test_fonts.truetype_font(), "rb").read())


def _frame(family: str, x_twips: int, y_twips: int, line_twips: int = PROBE_LINE_TWIPS) -> str:
    return dw.frame_paragraph_xml(dw.run_xml("n", family, PROBE_SIZE_PT), x_twips=x_twips, y_twips=y_twips,
                                  width_twips=FRAME_TWIPS, line_twips=line_twips)  # fmt: skip


def _export(parts: list[str], width_pt: float, height_pt: float, out_dir: str, name: str) -> tuple[str, float]:
    family, font = _probe_font()
    docx = dw.write_docx(os.path.join(out_dir, f"{name}.docx"), "".join(parts), family=family,
                         default_size_pt=PROBE_SIZE_PT, embedded=font, page_width_pt=width_pt,
                         page_height_pt=height_pt)  # fmt: skip
    pdf = os.path.join(out_dir, f"{name}.word.pdf")
    info = export_docx_to_pdf(docx, pdf)
    return pdf, sum(info["timings"].values())


def _paged(frames: list[tuple[int, int]], rows_per_page: int, family: str) -> list[str]:
    parts = []
    for index, (x, y) in enumerate(frames):
        if index and index % rows_per_page == 0:
            parts.append(dw.page_break_paragraph_xml())
        parts.append(_frame(family, x, y))
    return parts


def _glyph_objects(pdf: str):
    return [o for o in read_text_objects(pdf) if o.glyphs and o.glyphs[0].code == ord("n")]


def _x_units_at(pixels: list[int], width_pt: float, height_pt: float, out_dir: str, name: str):
    """printed 1/3000 inch x of a frame at each layout pixel, one frame per row."""
    family, _ = _probe_font()
    height_twips = round(height_pt * 20)
    rows = max(1, (height_twips - 1200) // PROBE_LINE_TWIPS)
    frames = [(wm.twips_for_pixel(p), 600 + (i % rows) * PROBE_LINE_TWIPS) for i, p in enumerate(pixels)]
    pdf, seconds = _export(_paged(frames, rows, family), width_pt, height_pt, out_dir, name)
    objects = _glyph_objects(pdf)
    if len(objects) != len(pixels):
        raise RuntimeError(f"x step probe: {len(objects)} glyphs for {len(pixels)} frames")
    units = {}
    for pixel, obj in zip(pixels, objects, strict=True):
        candidates = wm.units_candidates(obj.x)
        units[pixel] = min(candidates, key=lambda u: abs(u - 5 * pixel))
    return units, seconds


def _y_units_at(pixels: list[int], width_pt: float, height_pt: float, out_dir: str, name: str):
    """printed 1/3000 inch distance from the top of a 12 pt exact line's baseline at each pixel."""
    family, _ = _probe_font()
    columns = max(1, (round(width_pt * 20) - 400) // 700)
    height_px = wm.page_pixels(height_pt)
    # consecutive pixels go to consecutive columns; a column's next frame is at least 14 pt lower
    per_page = []
    page: list[tuple[int, int, int]] = []
    last_in_column: dict[int, int] = {}
    for pixel in sorted(pixels):
        column = len(page) % columns
        if column in last_in_column and pixel - last_in_column[column] < 120:
            per_page.append(page)
            page, last_in_column = [], {}
            column = 0
        page.append((pixel, 400 + column * 700, wm.twips_for_pixel(pixel - PROBE_BASELINE_OFFSET)))
        last_in_column[column] = pixel
    per_page.append(page)
    parts = []
    for number, frames in enumerate(per_page):
        if number:
            parts.append(dw.page_break_paragraph_xml())
        parts.extend(_frame(family, x, y) for _, x, y in frames)
    pdf, seconds = _export(parts, width_pt, height_pt, out_dir, name)
    objects = _glyph_objects(pdf)
    units = {}
    by_page: dict[int, list] = {}
    for obj in objects:
        by_page.setdefault(obj.page, []).append(obj)
    for number, frames in enumerate(per_page):
        for pixel, x, _ in frames:
            obj = min((o for o in by_page.get(number, []) if abs(o.x - x / 20) < 2),
                      key=lambda o: abs((wm.UNITS_PER_PIXEL * height_px * wm.PT_PER_UNIT - o.y) / wm.PT_PER_PIXEL - pixel))  # fmt: skip
            candidates = [wm.UNITS_PER_PIXEL * height_px - u for u in wm.units_candidates(obj.y)]
            units[pixel] = min(candidates, key=lambda u: abs(u - 5 * pixel))
    return units, seconds


def measure_steps(axis: str, width_pt: float, height_pt: float, out_dir: str) -> tuple[list[int], float]:
    """the writer's step pixels along one axis of a page size: a coarse pass every 5 px, then every pixel near a step."""
    along = wm.page_pixels(width_pt if axis == "x" else height_pt)
    margin = 100 if axis == "x" else 60
    first = 0 if axis == "x" else PROBE_BASELINE_OFFSET + 1
    probe = _x_units_at if axis == "x" else _y_units_at
    coarse = list(range(first, along - margin, COARSE_STEP_PX))
    tag = f"steps_{axis}{along}"
    units, seconds = probe(coarse, width_pt, height_pt, out_dir, f"{tag}_coarse")
    k = {p: u - 5 * p for p, u in units.items()}
    fine = []
    for low, high in zip(coarse, coarse[1:], strict=False):
        fine.extend(range(low + 1, high) if k[high] != k[low] else [])
    if fine:
        more, extra = probe(fine, width_pt, height_pt, out_dir, f"{tag}_fine")
        k.update({p: u - 5 * p for p, u in more.items()})
        seconds += extra
    steps, previous = [], 0
    for pixel in sorted(k):
        while k[pixel] > previous:
            steps.append(pixel)
            previous += 1
    return steps, seconds


def measure_baseline(line_twips: int, out_dir: str) -> tuple[list[int], float]:
    """baseline offset (pixels below twips_pixel(t)) for t mod 60 = 0..59 at one exact line height."""
    family, _ = _probe_font()
    table = wm.StepTable(wm.MEASURED_STEPS[("y", 6600)])
    columns, frames = 16, []
    rows_apart = 1 + (line_twips + 200) // wm.TWIPS_PERIOD  # same-column frames at least a line apart
    for residue in range(wm.TWIPS_PERIOD):
        column = residue % columns
        y = 1200 + residue + wm.TWIPS_PERIOD * rows_apart * (residue // columns)
        frames.append((residue, 400 + column * 700, y))
    parts = [_frame(family, x, y, line_twips) for _, x, y in frames]
    pdf, seconds = _export(parts, 612.0, 792.0, out_dir, f"baseline_{line_twips}")
    objects = _glyph_objects(pdf)
    offsets = [0] * wm.TWIPS_PERIOD
    for residue, x, y in frames:
        top = wm.twips_pixel(y)
        guess = top + BASELINE_SHARE * line_twips * 5 / 12
        obj = min((o for o in objects if abs(o.x - x / 20) < 2),
                  key=lambda o: abs((792 - o.y) / wm.PT_PER_PIXEL - guess))  # fmt: skip
        pixel = wm.decode_y(obj.y, table, 6600)
        if pixel is None:
            raise RuntimeError(f"baseline probe: printed y {obj.y} fits no pixel")
        offsets[residue] = pixel - top
    return offsets, seconds


def ensure_measured(cache: ModelCache, page_sizes: set[tuple[float, float]], line_heights: set[int],
                    out_dir: str, verify: bool = False) -> dict:  # fmt: skip
    """measure whatever the cache lacks (or everything, with verify) and count disagreements."""
    os.makedirs(out_dir, exist_ok=True)
    report = {"probe_seconds": 0.0, "measured_steps": [], "measured_baselines": [], "disagreements": 0, "compared": 0}
    for width, height in sorted(page_sizes):
        for axis, along in (("x", wm.page_pixels(width)), ("y", wm.page_pixels(height))):
            key = f"{axis}:{along}"
            if key in cache.steps and not verify:
                continue
            steps, seconds = measure_steps(axis, width, height, out_dir)
            report["probe_seconds"] += seconds
            report["measured_steps"].append(key)
            if key in cache.steps:
                report["compared"] += 1
                report["disagreements"] += cache.steps[key] != steps
                if cache.steps[key] != steps:
                    report.setdefault("step_differences", {})[key] = {"cached": cache.steps[key], "measured": steps}
            cache.steps[key] = steps
    for line_twips in sorted(line_heights):
        key = str(line_twips)
        if key in cache.baseline and not verify:
            continue
        offsets, seconds = measure_baseline(line_twips, out_dir)
        report["probe_seconds"] += seconds
        report["measured_baselines"].append(line_twips)
        if key in cache.baseline:
            report["compared"] += 1
            report["disagreements"] += cache.baseline[key] != offsets
        cache.baseline[key] = offsets
    cache.save()
    return report


# ---------------------------------------------------------------- placing


def baseline_pixel(cache: ModelCache, line_twips: int, y_twips: int) -> int:
    return wm.twips_pixel(y_twips) + cache.baseline[str(line_twips)][y_twips % wm.TWIPS_PERIOD]


def place(cache: ModelCache, x_pt: float, baseline_from_top_pt: float, line_twips: int,
          page_width_pt: float, page_height_pt: float) -> Placement:  # fmt: skip
    """frame twips whose printed line start and baseline are nearest the wanted ones."""
    width_px, height_px = wm.page_pixels(page_width_pt), wm.page_pixels(page_height_pt)
    x_table, y_table = cache.table("x", width_px), cache.table("y", height_px)
    x_pixel = wm.nearest_exported_pixel(x_pt, x_table)
    x_twips = wm.twips_for_pixel(x_pixel)
    pdf_y = page_height_pt - baseline_from_top_pt
    guess = round((baseline_from_top_pt - BASELINE_SHARE * line_twips / dw.TWIPS_PER_PT) * dw.TWIPS_PER_PT)
    candidates = range(guess - 12, guess + 13)

    def printed_error(y_twips: int) -> float:
        pixel = baseline_pixel(cache, line_twips, y_twips)
        return abs(wm.exported_y(pixel, y_table, height_px) - pdf_y)

    y_twips = min(candidates, key=lambda t: (printed_error(t), abs(t - guess)))
    pixel = baseline_pixel(cache, line_twips, y_twips)
    return Placement(x_twips, y_twips, x_pixel, pixel, wm.exported_x(x_pixel, x_table),
                     page_height_pt - wm.exported_y(pixel, y_table, height_px))  # fmt: skip
