"""frame twips chosen by measurement, with a cache per word build.

round 2 showed word maps frame twips to its 1/600 inch grid by a deterministic
rule that depends only on the twip value (x) or on the twip value and the
exact line height (y). so each measured pair is stored once per word build in
out\\calibration_cache\\word-<build>.json and reused by later documents; only
twip values never seen before are probed. verify=True probes everything again
and counts disagreements with the cache, which is how the cache is tested.
"""

from __future__ import annotations

import json
import os
from collections import defaultdict
from dataclasses import dataclass

import docx_writer as dw
import test_fonts
from pdf_analysis import extract_chars
from word_export import export_docx_to_pdf

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE_DIR = os.path.join(HERE, "out", "calibration_cache")
CANDIDATES = range(-4, 5)
PROBE_SIZE_PT = 12.0
BASELINE_SHARE = 0.804  # an exact line puts the baseline about this far down (e2, e8)


@dataclass
class Request:
    page: int
    x_pt: float
    baseline_from_top_pt: float
    line_twips: int


@dataclass
class Placement:
    x_twips: int
    y_twips: int
    x_pt: float  # where word will put the line start
    baseline_from_top_pt: float


def guess_twips(request: Request) -> tuple[int, int]:
    top = request.baseline_from_top_pt - BASELINE_SHARE * request.line_twips / dw.TWIPS_PER_PT
    return round(request.x_pt * dw.TWIPS_PER_PT), round(top * dw.TWIPS_PER_PT)


class CalibrationCache:
    def __init__(self, word_build: str):
        os.makedirs(CACHE_DIR, exist_ok=True)
        self.path = os.path.join(CACHE_DIR, f"word-{word_build.replace(' ', '_')}.json")
        self.x: dict[int, float] = {}
        self.y: dict[str, float] = {}
        if os.path.exists(self.path):
            with open(self.path, encoding="utf-8") as handle:
                data = json.load(handle)
            self.x = {int(k): v for k, v in data["x"].items()}
            self.y = data["y"]

    @staticmethod
    def y_key(line_twips: int, y_twips: int) -> str:
        return f"{line_twips}:{y_twips}"

    def save(self) -> None:
        with open(self.path, "w", encoding="utf-8") as handle:
            json.dump({"x": self.x, "y": self.y}, handle)


MIN_PROBE_DISTANCE_X_PT = 12.0
MIN_PROBE_DISTANCE_Y_PT = 4.0


def _probe_groups(requests: list[Request]) -> list[list[Request]]:
    """requests split so no two on one probe page are close enough to be confused.

    a subscript next to its base line once matched the wrong probe line and put
    a wrong x into the cache, so close neighbours go to different pages."""
    groups: list[list[Request]] = []
    for request in sorted(requests, key=lambda r: (r.baseline_from_top_pt, r.x_pt)):
        for group in groups:
            if all(abs(r.x_pt - request.x_pt) >= MIN_PROBE_DISTANCE_X_PT
                   or abs(r.baseline_from_top_pt - request.baseline_from_top_pt) >= MIN_PROBE_DISTANCE_Y_PT
                   for r in group):  # fmt: skip
                group.append(request)
                break
        else:
            groups.append([request])
    return groups


def _probe_document(requests: list[Request], out_dir: str, name: str):
    """one probe page per source page and candidate offset, a 2-char sample per request."""
    family = test_fonts.TRUETYPE_FAMILY
    embedded = dw.EmbeddedFont(family, open(test_fonts.truetype_font(), "rb").read())
    parts, layout = [], []
    probe_page = 0
    for group in _probe_groups(requests):
        for offset in CANDIDATES:
            if probe_page > 0:
                parts.append(dw.page_break_paragraph_xml())
            for request in group:
                x, y = guess_twips(request)
                parts.append(
                    dw.frame_paragraph_xml(
                        dw.run_xml("nn", family, PROBE_SIZE_PT),
                        x_twips=x + offset,
                        y_twips=y + offset,
                        width_twips=600,
                        line_twips=request.line_twips,
                    )
                )
                layout.append((probe_page, request, x + offset, y + offset))
            probe_page += 1
    width = max(612.0, max(r.x_pt for r in requests) + 60)
    height = max(792.0, max(r.baseline_from_top_pt for r in requests) + 40)
    docx = dw.write_docx(os.path.join(out_dir, f"{name}.docx"), "".join(parts), family=family,
                         default_size_pt=PROBE_SIZE_PT, embedded=embedded, page_width_pt=width, page_height_pt=height)  # fmt: skip
    return docx, layout, height


def _line_starts(pdf_path: str, page_height: float) -> dict[int, list[tuple[float, float]]]:
    rows = defaultdict(list)
    for char in extract_chars(pdf_path):
        rows[(char.page, round(char.origin_y, 3))].append(char.origin_x)
    starts = defaultdict(list)
    for (page, baseline), xs in rows.items():
        xs = sorted(xs)
        group = xs[0]
        for previous, current in zip(xs, xs[1:], strict=False):
            if current - previous > 20:
                starts[page].append((group, page_height - baseline))
                group = current
        starts[page].append((group, page_height - baseline))
    return starts


def calibrate(requests: list[Request], cache: CalibrationCache, out_dir: str, name: str, verify: bool = False) -> dict:
    """placements for every request, probing word only for twip values the cache lacks."""
    os.makedirs(out_dir, exist_ok=True)
    unique = {}
    for request in requests:
        key = (request.page, round(request.x_pt, 3), round(request.baseline_from_top_pt, 3), request.line_twips)
        unique.setdefault(key, request)
    missing = []
    for request in unique.values():
        x, y = guess_twips(request)
        needs_x = any(x + o not in cache.x for o in CANDIDATES)
        needs_y = any(cache.y_key(request.line_twips, y + o) not in cache.y for o in CANDIDATES)
        if verify or needs_x or needs_y:
            missing.append(request)
    disagreements = {"x": 0, "y": 0, "compared": 0}
    probe_seconds = 0.0
    if missing:
        docx, layout, height = _probe_document(missing, out_dir, name)
        pdf = os.path.join(out_dir, f"{name}.word.pdf")
        info = export_docx_to_pdf(docx, pdf)
        probe_seconds = sum(info["timings"].values())
        starts = _line_starts(pdf, height)
        for probe_page, request, x_twips, y_twips in layout:
            expected_x = x_twips / dw.TWIPS_PER_PT
            expected_baseline = y_twips / dw.TWIPS_PER_PT + BASELINE_SHARE * request.line_twips / dw.TWIPS_PER_PT
            found = min(starts[probe_page], key=lambda s: abs(s[0] - expected_x) + abs(s[1] - expected_baseline))
            if abs(found[0] - expected_x) > 1.0 or abs(found[1] - expected_baseline) > 3.0:
                # too far from where it was asked for: better no cache entry than a wrong one
                disagreements.setdefault("unmatched", 0)
                disagreements["unmatched"] += 1
                continue
            y_key = cache.y_key(request.line_twips, y_twips)
            if x_twips in cache.x and y_key in cache.y:
                disagreements["compared"] += 1
                disagreements["x"] += abs(cache.x[x_twips] - found[0]) > 1e-3
                disagreements["y"] += abs(cache.y[y_key] - found[1]) > 1e-3
            cache.x[x_twips] = found[0]
            cache.y[y_key] = found[1]
        cache.save()
    placements = {}
    for key, request in unique.items():
        x, y = guess_twips(request)
        xs = [x + o for o in CANDIDATES if x + o in cache.x]
        ys = [y + o for o in CANDIDATES if cache.y_key(request.line_twips, y + o) in cache.y]
        if not xs or not ys:
            # nothing measured for this line: place it by formula and say so
            placements[key] = Placement(x, y, request.x_pt, request.baseline_from_top_pt)
            disagreements.setdefault("uncalibrated_lines", 0)
            disagreements["uncalibrated_lines"] += 1
            continue
        best_x = min(xs, key=lambda t: abs(cache.x[t] - request.x_pt))
        best_y = min(ys, key=lambda t: abs(cache.y[cache.y_key(request.line_twips, t)] - request.baseline_from_top_pt))
        placements[key] = Placement(best_x, best_y, cache.x[best_x], cache.y[cache.y_key(request.line_twips, best_y)])
    return {
        "placements": placements,
        "probed_lines": len(missing),
        "cached_lines": len(unique) - len(missing) if not verify else 0,
        "disagreements": disagreements,
        "probe_seconds": probe_seconds,
    }


def placement_key(page: int, x_pt: float, baseline_from_top_pt: float, line_twips: int) -> tuple:
    return (page, round(x_pt, 3), round(baseline_from_top_pt, 3), line_twips)
