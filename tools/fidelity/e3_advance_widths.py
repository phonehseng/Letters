"""e3: can word reproduce character spacing that differs from the font's own advances?

the source pdf has four groups of lines, all 12 pt, placed with frames:
    natural     plain text, the font's own advances
    charspace   reportlab charSpace 0.37 pt (the pdf Tc operator)
    scaled      one glyph per text object, every advance = font advance * 1.043
    jitter      one glyph per text object, advance = font advance + a fixed pseudo-random
                amount in [-0.3, 0.3] pt, different for every occurrence

docx variants:
    plain                 no spacing, word uses the font's advances
    spacing_open          per character w:spacing in twips, cumulative target rounded,
                          so rounding never adds up along a line (error diffusion)
    spacing_closed_1/2    spacing_open corrected once / twice with word's measured errors
                          (needs word in the loop, shows what is reachable at all)
    hmtx                  charspace and scaled lines use fonts whose hmtx was rewritten
                          to the pdf widths, no w:spacing; natural uses the plain font,
                          jitter falls back to spacing_open since no single width fits
    unhinted_plain        the font without truetype instructions, no spacing
    unhinted_spacing_open the font without instructions, spacing_open

usage:  .venv\\Scripts\\python.exe e3_advance_widths.py
output: out\\e3\\  (e3_summary.json, reports and heatmaps per variant)
"""

from __future__ import annotations

import json
import os
import random
from collections import defaultdict
from dataclasses import replace

import numpy as np
from fontTools.ttLib import TTFont
from reportlab.pdfbase import pdfmetrics

import docx_writer as dw
import font_prep
import placement
import test_fonts
from harness import print_summary, run_harness
from test_pdf import TextLine, natural_advances, register_font, sample_text, write_verified_pdf

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e3")
SIZE_PT = 12.0  # a whole number of 1/600 inch units, so word's size rounding stays out of it
PDF_FONT_NAME = "FidelityProbeSerif"
CHAR_SPACE_PT = 0.37
SCALE = 1.043
GROUPS = ("natural", "charspace", "scaled", "jitter")
UNHINTED_FAMILY = "Fidelity Probe Unhinted"
SPACED_FAMILY = "Fidelity Probe Spaced"
WIDE_FAMILY = "Fidelity Probe Wide"
# baseline offset for 12 pt in a 13.6 pt exact line, close to e2's frame calibration
BASELINE_OFFSETS = {SIZE_PT: 10.91}

LineKey = tuple[int, float]


def key_of(line: TextLine) -> LineKey:
    return (line.page, round(line.baseline_y, 4))


class FontMetrics:
    """exact font advances in points, the ideal word would use without rounding."""

    def __init__(self, font_path: str):
        font = TTFont(font_path)
        self.units_per_em = font["head"].unitsPerEm
        self.cmap = font.getBestCmap()
        self.hmtx = font["hmtx"]

    def advance_pt(self, char: str, size: float) -> float:
        return self.hmtx[self.cmap[ord(char)]][0] * size / self.units_per_em


def build_lines() -> tuple[list[TextLine], dict[LineKey, str]]:
    lines = []
    group_of = {}
    rng = random.Random(1234)
    baseline = 720.53
    index = 0
    for group in GROUPS:
        for _ in range(8):
            text = sample_text(900 + index, 7)
            line = TextLine(0, 72.37, round(baseline, 4), SIZE_PT, text)
            natural = [pdfmetrics.stringWidth(c, PDF_FONT_NAME, SIZE_PT) for c in text]
            if group == "charspace":
                line.char_space = CHAR_SPACE_PT
            elif group == "scaled":
                line.advances = [w * SCALE for w in natural]
            elif group == "jitter":
                line.advances = [w + rng.uniform(-0.3, 0.3) for w in natural]
            lines.append(line)
            group_of[key_of(line)] = group
            baseline -= 19.37
            index += 1
    return lines, group_of


def target_advances(line: TextLine) -> list[float]:
    return line.advances if line.advances is not None else natural_advances(PDF_FONT_NAME, line)


def diffused_spacing(target_offsets_pt: list[float]) -> list[int]:
    """twips after each char so the running total tracks target_offsets_pt.

    target_offsets_pt has one entry per char plus one for the line end: entry i
    is the extra shift wanted at the start of char i (entry 0, the line start,
    is 0). rounding the running total, not each step, keeps every char within
    half a twip of its target, so rounding never adds up along the line."""
    rounded = [round(offset * dw.TWIPS_PER_PT) for offset in target_offsets_pt]
    return [rounded[i + 1] - rounded[i] for i in range(len(rounded) - 1)]


def open_loop_spacing(line: TextLine, metrics: FontMetrics) -> list[int]:
    targets = target_advances(line)
    offsets = [0.0]
    for char, target in zip(line.text, targets, strict=True):
        offsets.append(offsets[-1] + target - metrics.advance_pt(char, line.size))
    return diffused_spacing(offsets)


def char_runs(family: str, spacing: dict[LineKey, list[int]] | None):
    """one run per char so each can carry its own w:spacing."""

    def build(line: TextLine) -> str:
        if spacing is None or key_of(line) not in spacing:
            return dw.run_xml(line.text, family, line.size)
        return "".join(
            dw.run_xml(char, family, line.size, spacing_twips=twips)
            for char, twips in zip(line.text, spacing[key_of(line)], strict=True)
        )

    return build


def write_variant(
    path: str,
    lines: list[TextLine],
    run_builder,
    fonts: list[dw.EmbeddedFont],
    x_override: dict[LineKey, float] | None = None,
) -> str:
    placed = [replace(line, x=x_override.get(key_of(line), line.x)) if x_override else line for line in lines]

    def width_of(line: TextLine) -> float:
        return sum(target_advances(line)) + 10

    body = placement.frames_body(placed, run_builder, width_of, BASELINE_OFFSETS)
    return dw.write_docx(path, body, family=fonts[0].family, default_size_pt=SIZE_PT, embedded=fonts)


def embedded(path: str, family: str) -> dw.EmbeddedFont:
    with open(path, "rb") as handle:
        return dw.EmbeddedFont(family=family, font_bytes=handle.read())


def per_char_errors(pairs: list[dict], lines: list[TextLine]) -> dict[LineKey, dict[int, float]]:
    """dx per char index for every line, matched through the char's target x."""
    errors: dict[LineKey, dict[int, float]] = defaultdict(dict)
    by_key = {key_of(line): line for line in lines}
    for pair in pairs:
        key = (pair["page"], round(pair["source_y"], 4))
        line = by_key.get(key)
        if line is None or pair["page"] != pair["candidate_page"]:
            continue
        starts = np.cumsum([0.0] + target_advances(line)[:-1]) + line.x
        index = int(np.argmin(np.abs(starts - pair["source_x"])))
        errors[key][index] = pair["dx"]
    return errors


def closed_loop_step(lines, spacing, x_now, pairs):
    """correct spacing and frame x by what word actually did last time."""
    errors = per_char_errors(pairs, lines)
    new_spacing, new_x = {}, {}
    for line in lines:
        key = key_of(line)
        measured = errors.get(key, {})
        if not measured:
            new_spacing[key], new_x[key] = spacing[key], x_now[key]
            continue
        line_start_error = measured.get(0, next(iter(measured.values())))
        old = spacing[key]
        old_offsets = np.concatenate([[0.0], np.cumsum(old) / dw.TWIPS_PER_PT])
        wanted = []
        last_error = 0.0
        for i in range(len(line.text) + 1):
            # spaces have no measured position, they keep the last known error
            if i in measured:
                last_error = measured[i] - line_start_error
            wanted.append(old_offsets[i] - last_error)
        new_spacing[key] = diffused_spacing(wanted)
        new_x[key] = x_now[key] - line_start_error
    return new_spacing, new_x


def group_stats(pairs: list[dict], lines: list[TextLine], group_of: dict[LineKey, str]) -> dict:
    rows = defaultdict(list)
    along = defaultdict(list)
    errors = per_char_errors(pairs, lines)
    for key, by_index in errors.items():
        group = group_of[key]
        first = by_index[min(by_index)]
        for dx in by_index.values():
            rows[group].append(abs(dx))
            along[group].append(abs(dx - first))
    stats = {}
    for group in GROUPS:
        values = np.array(rows[group])
        drift = np.array(along[group])
        stats[group] = {
            "chars": int(values.size),
            "abs_dx_max": float(values.max()),
            "abs_dx_mean": float(values.mean()),
            "abs_dx_p99": float(np.percentile(values, 99)),
            "within_0.025": float((values <= 0.025).mean()),
            "along_line_max": float(drift.max()),
            "along_line_mean": float(drift.mean()),
        }
    return stats


def print_group_table(name: str, stats: dict) -> None:
    print(f"   {name}: per group |dx| in pt (whole error, and drift along the line after removing the line start)")
    for group, s in stats.items():
        print(
            f"     {group:<10} max {s['abs_dx_max']:.4f}  mean {s['abs_dx_mean']:.4f}  p99 {s['abs_dx_p99']:.4f}  "
            f"<=0.025: {100 * s['within_0.025']:5.1f}%   "
            f"along-line max {s['along_line_max']:.4f} mean {s['along_line_mean']:.4f}"
        )


def build_fonts(lines: list[TextLine]) -> dict[str, str]:
    base = test_fonts.truetype_font()
    for family in (UNHINTED_FAMILY, SPACED_FAMILY, WIDE_FAMILY):
        test_fonts.assert_not_installed(family)
    chars = sorted({c for line in lines for c in line.text})
    natural = {c: pdfmetrics.stringWidth(c, PDF_FONT_NAME, SIZE_PT) for c in chars}
    spaced_rounding = font_prep.make_font_with_advances(
        base,
        test_fonts.build_path("FidelityProbeSpaced.ttf"),
        SPACED_FAMILY,
        {c: w + CHAR_SPACE_PT for c, w in natural.items()},
        SIZE_PT,
    )
    wide_rounding = font_prep.make_font_with_advances(
        base,
        test_fonts.build_path("FidelityProbeWide.ttf"),
        WIDE_FAMILY,
        {c: w * SCALE for c, w in natural.items()},
        SIZE_PT,
    )
    print(
        f"hmtx rounding to font units: spaced max {max(map(abs, spaced_rounding.values())):.4f} pt, "
        f"wide max {max(map(abs, wide_rounding.values())):.4f} pt per glyph"
    )
    return {
        "base": base,
        "unhinted": font_prep.make_unhinted_font(
            base, test_fonts.build_path("FidelityProbeUnhinted.ttf"), UNHINTED_FAMILY
        ),
        "spaced": test_fonts.build_path("FidelityProbeSpaced.ttf"),
        "wide": test_fonts.build_path("FidelityProbeWide.ttf"),
    }


def run_variant(name, source, docx, lines, group_of, summary):
    report = run_harness(source, docx, OUT_DIR, keep_pairs=True)
    print_summary(report, label=f"e3 {name}")
    stats = group_stats(report["pairs"], lines, group_of)
    print_group_table(name, stats)
    summary[name] = {"glyphs": report["glyphs"], "groups": stats, "seconds": report["seconds"]}
    return report


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    register_font(test_fonts.truetype_font(), PDF_FONT_NAME)
    lines, group_of = build_lines()
    fonts = build_fonts(lines)
    source = write_verified_pdf(os.path.join(OUT_DIR, "e3_source.pdf"), PDF_FONT_NAME, lines)
    base_font = embedded(fonts["base"], test_fonts.TRUETYPE_FAMILY)
    metrics = FontMetrics(fonts["base"])
    summary = {}

    def path(name):
        return os.path.join(OUT_DIR, f"e3_{name}.docx")

    plain = write_variant(path("plain"), lines, char_runs(base_font.family, None), [base_font])
    run_variant("plain", source, plain, lines, group_of, summary)

    spacing = {key_of(line): open_loop_spacing(line, metrics) for line in lines}
    docx = write_variant(path("spacing_open"), lines, char_runs(base_font.family, spacing), [base_font])
    report = run_variant("spacing_open", source, docx, lines, group_of, summary)

    x_now = {key_of(line): line.x for line in lines}
    for step in (1, 2):
        spacing, x_now = closed_loop_step(lines, spacing, x_now, report["pairs"])
        docx = write_variant(
            path(f"spacing_closed_{step}"), lines, char_runs(base_font.family, spacing), [base_font], x_override=x_now
        )
        report = run_variant(f"spacing_closed_{step}", source, docx, lines, group_of, summary)

    spaced_font = embedded(fonts["spaced"], SPACED_FAMILY)
    wide_font = embedded(fonts["wide"], WIDE_FAMILY)
    jitter_spacing = {
        key_of(line): open_loop_spacing(line, metrics) for line in lines if group_of[key_of(line)] == "jitter"
    }

    def hmtx_runs(line: TextLine) -> str:
        group = group_of[key_of(line)]
        if group == "charspace":
            return dw.run_xml(line.text, SPACED_FAMILY, line.size)
        if group == "scaled":
            return dw.run_xml(line.text, WIDE_FAMILY, line.size)
        return char_runs(base_font.family, jitter_spacing)(line)

    docx = write_variant(path("hmtx"), lines, hmtx_runs, [base_font, spaced_font, wide_font])
    run_variant("hmtx", source, docx, lines, group_of, summary)

    unhinted_font = embedded(fonts["unhinted"], UNHINTED_FAMILY)
    unhinted_metrics = FontMetrics(fonts["unhinted"])
    docx = write_variant(path("unhinted_plain"), lines, char_runs(UNHINTED_FAMILY, None), [unhinted_font])
    run_variant("unhinted_plain", source, docx, lines, group_of, summary)
    unhinted_spacing = {key_of(line): open_loop_spacing(line, unhinted_metrics) for line in lines}
    docx = write_variant(
        path("unhinted_spacing_open"), lines, char_runs(UNHINTED_FAMILY, unhinted_spacing), [unhinted_font]
    )
    run_variant("unhinted_spacing_open", source, docx, lines, group_of, summary)

    with open(os.path.join(OUT_DIR, "e3_summary.json"), "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)


if __name__ == "__main__":
    main()
