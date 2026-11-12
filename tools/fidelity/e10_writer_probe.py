"""e10: how word's pdf writer prints the positions its layout chose.

probe documents use a grid font (every advance a whole 1/600 inch pixel), so
pen offsets inside a line are known exactly:
    x     one single-glyph frame per row, frame x swept over the page width
    y     single-glyph frames on 16 columns, frame y swept down the page
    line  lines of text at several sizes and start positions, no spacing, so
          the pen offset of glyph i is the sum of the advances before it
the raw Tm/TJ operands are saved to out\\e10\\*.json for word_writer_model.py
to fit. (Range.Information was tried for word's layout positions: it reports
screen pixels, 0.75 pt steps, so it cannot tell 1/600 inch pixels apart.)

usage:  .venv\\Scripts\\python.exe e10_writer_probe.py [x] [y] [line] [--dense] [--page letter|a4|wide]
    --dense  every twip instead of windows (x: 11,640 frames, y: 15,000 frames)
"""

from __future__ import annotations

import json
import os
import sys

import docx_writer as dw
import font_prep
import test_fonts
from test_pdf import sample_text
from word_export import export_docx_to_pdf
from word_pdf_raw import read_text_objects

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e10")
ROW_TWIPS = 240
COLUMNS = 16
COLUMN_TWIPS = 700
# same-column frames in the y probe stay at least this far apart, so none overlap
Y_PAGES = 18
PAGE_SIZES_PT = {"letter": (612.0, 792.0), "a4": (595.3, 841.9), "wide": (1008.0, 612.0)}


def option(name: str, default: str) -> str:
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


DENSE = "--dense" in sys.argv
PAGE = option("--page", "letter")
PAGE_W_PT, PAGE_H_PT = PAGE_SIZES_PT[PAGE]


def grid_font(size_pt: float) -> tuple[str, dw.EmbeddedFont]:
    tag = f"{size_pt:g}".replace(".", "h")
    family = f"Fidelity Grid {tag}"
    test_fonts.assert_not_installed(family)
    path = test_fonts.build_path(f"FidelityGrid-{tag}.ttf")
    if not os.path.exists(path):
        font_prep.make_grid_font(test_fonts.truetype_font(), path, family, size_pt)
    with open(path, "rb") as handle:
        return family, dw.EmbeddedFont(family, handle.read())


def x_values() -> list[int]:
    right = round(PAGE_W_PT * 20) - 700
    if "--range" in sys.argv:
        start = int(sys.argv[sys.argv.index("--range") + 1])
        return list(range(start, int(sys.argv[sys.argv.index("--range") + 2])))
    if DENSE:
        return list(range(0, right))
    values = set(range(0, right, 23))
    for start in (0, 1430, 2170, 4310, 5990, 7200, 9990, 11500):
        values.update(v for v in range(start, start + 40) if v < right)
    return sorted(values)


def y_values() -> list[int]:
    bottom = round(PAGE_H_PT * 20) - 840
    if DENSE:
        return list(range(0, bottom))
    values = set(range(0, bottom, 37))
    for start in (700, 5000, 9000, 13900):
        values.update(v for v in range(start, start + 64) if v < bottom)
    return sorted(values)


def tag(kind: str) -> str:
    extent = "_range" if "--range" in sys.argv else "_dense" if DENSE else ""
    return f"e10_{kind}{extent}" + ("" if PAGE == "letter" else f"_{PAGE}")


def run_export(name: str, body: str, family: str, size: float, fonts) -> str:
    docx = dw.write_docx(os.path.join(OUT_DIR, f"{name}.docx"), body, family=family, default_size_pt=size,
                         embedded=fonts, page_width_pt=PAGE_W_PT, page_height_pt=PAGE_H_PT)  # fmt: skip
    pdf = os.path.join(OUT_DIR, f"{name}.word.pdf")
    info = export_docx_to_pdf(docx, pdf)
    print(f"{name}: {info['word_version']}, pages {info['word_pages']}, export {info['timings']['export_s']:.1f} s")
    return pdf


def single_glyph_frame(family: str, x: int, y: int) -> str:
    return dw.frame_paragraph_xml(dw.run_xml("n", family, 12.0), x_twips=x, y_twips=y, width_twips=600,
                                  line_twips=ROW_TWIPS)  # fmt: skip


def probe_x() -> None:
    family, font = grid_font(12.0)
    rows_per_page = (round(PAGE_H_PT * 20) - 1200) // ROW_TWIPS
    parts, layout = [], []
    for index, twips in enumerate(x_values()):
        page, row = divmod(index, rows_per_page)
        if row == 0 and page > 0:
            parts.append(dw.page_break_paragraph_xml())
        y = 600 + row * ROW_TWIPS
        parts.append(single_glyph_frame(family, twips, y))
        layout.append({"page": page, "x_twips": twips, "y_twips": y})
    name = tag("x")
    save(name, run_export(name, "".join(parts), family, 12.0, font), layout)


def probe_y() -> None:
    family, font = grid_font(12.0)
    pages: dict[int, list[int]] = {}
    for value in y_values():
        # consecutive values go to consecutive columns; a column repeats only after
        # COLUMNS * Y_PAGES twips on the same page, so frames never overlap
        pages.setdefault((value // COLUMNS) % Y_PAGES, []).append(value)
    parts, layout = [], []
    for page_number, page in enumerate(sorted(pages)):
        if page_number > 0:
            parts.append(dw.page_break_paragraph_xml())
        for value in pages[page]:
            x = 400 + (value % COLUMNS) * COLUMN_TWIPS
            parts.append(single_glyph_frame(family, x, value))
            layout.append({"page": page_number, "x_twips": x, "y_twips": value})
    name = tag("y")
    save(name, run_export(name, "".join(parts), family, 12.0, font), layout)


LINE_SIZES = (9.0, 10.5, 12.0, 20.0, 36.0)
LINE_STARTS_TWIPS = (1440, 1447, 4321, 7206, 9000)


def probe_line() -> None:
    fonts = [grid_font(size) for size in LINE_SIZES]
    page_h_twips = round(PAGE_H_PT * 20)
    page_w_twips = round(PAGE_W_PT * 20)
    parts, layout = [], []
    y, page, index = 600, 0, 0
    for (family, _), size in zip(fonts, LINE_SIZES, strict=True):
        line_twips = 20 * max(2, round(size * 1.3))
        path = test_fonts.build_path(f"FidelityGrid-{size:g}".replace(".", "h") + ".ttf")
        for start in LINE_STARTS_TWIPS:
            for _ in range(3):
                if y + line_twips > page_h_twips - 800:
                    parts.append(dw.page_break_paragraph_xml())
                    y, page = 600, page + 1
                # keep the line well inside the frame so it never wraps
                available_pt = (page_w_twips - start) / 20 - 20
                text = sample_text(900 + index, 12)[: int(available_pt / (0.55 * size))].rstrip()
                index += 1
                runs = "".join(dw.run_xml(char, family, size) for char in text)
                parts.append(dw.frame_paragraph_xml(runs, x_twips=start, y_twips=y, width_twips=page_w_twips - start,
                                                    line_twips=line_twips))  # fmt: skip
                layout.append({"page": page, "x_twips": start, "y_twips": y, "size": size, "family": family,
                               "text": text, "advances_px": font_prep.advance_pixels(path, text)})  # fmt: skip
                y += line_twips + 40
    name = tag("line")
    save(name, run_export(name, "".join(parts), fonts[2][0], 12.0, [f for _, f in fonts]), layout)


def save(name: str, pdf: str, layout: list) -> None:
    objects = [
        {"page": o.page, "font": o.font, "size_text": o.size_text, "tm_text": o.tm_text, "tc": o.char_spacing,
         "glyphs": [(g.code, g.width, g.adjust_before) for g in o.glyphs]}
        for o in read_text_objects(pdf)
    ]  # fmt: skip
    with open(os.path.join(OUT_DIR, f"{name}.json"), "w", encoding="utf-8") as handle:
        json.dump({"page_pt": [PAGE_W_PT, PAGE_H_PT], "layout": layout, "objects": objects}, handle)
    print(f"   {len(layout)} probes, {len(objects)} text objects saved")


def twips_to_pixel_candidates(twips: int) -> tuple[int, ...]:
    """layout pixels word may give a frame at this twip value.

    measured: floor((5 * twips + 1) / 12), except that when 5 * twips + 1 is a
    multiple of 12 (the value sits exactly on a pixel edge) word takes either
    that pixel or the one before, depending on the position (float rounding)."""
    exact = (5 * twips + 1) // 12
    return (exact, exact - 1) if (5 * twips + 1) % 12 == 0 else (exact,)


def analyse(name: str) -> None:
    """check word_writer_model and the twips rule against every probe of one saved run."""
    import pikepdf

    import word_writer_model as wm

    with open(os.path.join(OUT_DIR, f"{name}.json"), encoding="utf-8") as handle:
        data = json.load(handle)
    with pikepdf.open(os.path.join(OUT_DIR, f"{name}.word.pdf")) as pdf:
        box = [float(v) for v in pdf.pages[0].mediabox]
    width_px, height_px = wm.page_pixels(box[2]), wm.page_pixels(box[3])
    glyph_objects = [o for o in data["objects"] if o["glyphs"] and o["glyphs"][0][0] == ord("n")]
    axis = "x" if "_x" in name else "y"
    table = wm.step_table(axis, width_px if axis == "x" else height_px)
    if table is None:
        print(f"{name}: no step table for {axis} {width_px if axis == 'x' else height_px} px")
        return
    printed_ok = rule_ok = boundary = 0
    misses = []
    by_page: dict[int, list] = {}
    for obj in glyph_objects:
        by_page.setdefault(obj["page"], []).append(obj)
    for probe in data["layout"]:
        if axis == "x":
            obj = min(
                by_page[probe["page"]],
                key=lambda o: abs((box[3] - float(o["tm_text"][5])) - probe["y_twips"] / 20 - 9.6),
            )
            value = float(obj["tm_text"][4])
            pixel = wm.decode_x(value, table)
            expected_text = wm.printed_text(wm.exported_x(pixel, table)) if pixel is not None else None
            frame_pixel = pixel
        else:
            candidates = [
                o for o in by_page[probe["page"]] if abs(float(o["tm_text"][4]) - probe["x_twips"] / 20) < 1.5
            ]
            obj = min(candidates, key=lambda o: abs((box[3] - float(o["tm_text"][5])) - probe["y_twips"] / 20 - 9.6))
            value = float(obj["tm_text"][5])
            pixel = wm.decode_y(value, table, height_px)
            expected_text = wm.printed_text(wm.exported_y(pixel, table, height_px)) if pixel is not None else None
            frame_pixel = None if pixel is None else pixel - 80  # a 12 pt exact line: baseline 80 px below the top
        text = obj["tm_text"][4 if axis == "x" else 5]
        printed_ok += expected_text == text
        twips = probe["x_twips" if axis == "x" else "y_twips"]
        options = twips_to_pixel_candidates(twips)
        rule_ok += frame_pixel in options
        if expected_text != text or frame_pixel not in options:
            misses.append((twips, text, expected_text, frame_pixel, options))
        boundary += len(options) > 1
    total = len(data["layout"])
    print(f"{name}: {total} probes, printed value reproduced {printed_ok}, twips rule holds {rule_ok} "
          f"({boundary} sat on a pixel edge)")  # fmt: skip
    for miss in misses[:4]:
        print(f"   miss: twips {miss[0]} printed {miss[1]} model {miss[2]} pixel {miss[3]} rule {miss[4]}")


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    if "--analyse" in sys.argv:
        for name in sys.argv[sys.argv.index("--analyse") + 1 :]:
            analyse(name)
        return
    wanted = {a for a in sys.argv[1:] if a in ("x", "y", "line")} or {"x", "y", "line"}
    if "x" in wanted:
        probe_x()
    if "y" in wanted:
        probe_y()
    if "line" in wanted:
        probe_line()


if __name__ == "__main__":
    main()
