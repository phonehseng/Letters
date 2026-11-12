"""experiment c1: which gsub rules does word apply to an embedded font, from which run settings?

one embedded truetype font carries a rule per feature tag (liga, clig, rlig,
calt, dlig, hlig, ss01, ss05, ss20), each turning one word into a marker glyph.
every word is set once per run setting (no w14 element, w14:ligatures values,
w14:cntxtAlts, w14:stylisticSets). a second page tests what the product needs:
ligatures across spaces, how long a ligature word accepts, runs split in two,
character spacing, single substitutions and chaining context.
a rule "applied" when word's pdf draws the rule's own glyph (found by outline).

usage:  ..\\.venv\\Scripts\\python.exe probe_ligatures.py
"""

from __future__ import annotations

import fidelity_path

# isort: split

import json
import os
import string
from dataclasses import dataclass

from fontTools.ttLib import TTFont

import docx_writer as dw
import fitted_docx as fd
import fitted_font as ff
import font_prep
import test_fonts
from pdf_glyphs import drawn_glyphs
from word_session import WordSession

OUT = os.path.join(fidelity_path.OUT_DIR, "c1_ligatures")
FAMILY = "Fitted Probe Lig"
SIZE_HALF_POINTS = 20
LINE_TWIPS = 320
PAGE_W, PAGE_H = 1584, 1224  # word's largest page, so long ligatures fit on one line
COLUMN_TWIPS = 10400
ROWS_PER_COLUMN = 70
FIRST_ROW_TWIPS = 720

FEATURE_WORDS = {
    "liga": "office",
    "clig": "Effect",
    "rlig": "baffle",
    "calt": "muffin",
    "dlig": "waffle",
    "hlig": "toffee",
    "ss01": "coffee",
    "ss05": "duffel",
    "ss20": "jiffy",
}
VARIANTS = {
    "absent": fd.RunFeatures(ligatures=None),
    "none": fd.RunFeatures(ligatures="none"),
    "standard": fd.RunFeatures(ligatures="standard"),
    "contextual": fd.RunFeatures(ligatures="contextual"),
    "standardContextual": fd.RunFeatures(ligatures="standardContextual"),
    "historical": fd.RunFeatures(ligatures="historical"),
    "discretional": fd.RunFeatures(ligatures="discretional"),
    "all": fd.RunFeatures(ligatures="all"),
    "cntxtAlts": fd.RunFeatures(ligatures=None, contextual=True),
    "ss1": fd.RunFeatures(ligatures=None, stylistic_set=1),
    "ss5": fd.RunFeatures(ligatures=None, stylistic_set=5),
    "ss20": fd.RunFeatures(ligatures=None, stylistic_set=20),
    "standard+ss1": fd.RunFeatures(ligatures="standard", stylistic_set=1),
}
LENGTHS = (2, 3, 8, 16, 32, 64, 100, 127, 128, 200, 255, 256, 300)
LIGATURE_SETTINGS = ("absent", "standard", "all")


@dataclass
class Case:
    label: str
    runs: list[tuple[str, dict]]  # (text, extra run fields) in one line
    rule_glyph: str
    variant: str


def _alphabet(length: int) -> str:
    letters = string.ascii_lowercase
    return "Q" + "".join(letters[i % 26] for i in range(length - 1))


def base_font() -> TTFont:
    path = os.path.join(OUT, "base.ttf")
    font_prep.make_renamed_font(test_fonts.font_path("CrimsonText-Regular.ttf"), path, FAMILY, units_per_em=1000)
    font = TTFont(path)
    # only our own rules may act, so the font's own layout tables go
    for table in ("GSUB", "GPOS", "kern", "GDEF"):
        if table in font:
            del font[table]
    return font


def natural_composite(font: TTFont, name: str, text: str, feature: str, backtrack: str = "",
                      marker: bool = False) -> ff.Composite:  # fmt: skip
    """the rule's glyph: the text's own glyphs at their natural advances.

    with marker set the glyph only needs to be recognisable, not to fit int16
    offsets (long strings): its components are the first glyph twice."""
    names = [ff.glyph_name_for_char(font, c) for c in text]
    hmtx = font["hmtx"]
    # hmtx advances are unsigned 16-bit, so a very long rule cannot keep its full width
    advance = min(sum(hmtx[n][0] for n in names), ff.MAX_ADVANCE)
    if marker:
        components = [ff.Component(names[0], 0, 0), ff.Component(names[0], len(text), 300)]
    else:
        components, pen = [], 0
        for glyph_name in names:
            components.append(ff.Component(glyph_name, pen, 0))
            pen += hmtx[glyph_name][0]
    before = [ff.glyph_name_for_char(font, c) for c in backtrack]
    return ff.Composite(name, names, components, advance, feature, before)


def feature_cases(font: TTFont) -> tuple[list[ff.Composite], list[Case]]:
    composites, cases = [], []
    for tag, word in FEATURE_WORDS.items():
        name = f"rule_{tag}"
        composites.append(natural_composite(font, name, word, tag))
        for variant in VARIANTS:
            cases.append(Case(f"{tag}:{word}", [(word, {})], name, variant))
    return composites, cases


def product_cases(font: TTFont) -> tuple[list[ff.Composite], list[Case]]:
    composites = []
    specs = []  # (label, runs, rule text, backtrack, marker)
    for length in LENGTHS:
        text = _alphabet(length)
        specs.append((f"length {length}", [(text, {})], text, "", length > 40))
    for text in ("Zap zip", "Zed and a zoo", "Yes, this is a much longer sentence with spaces in it.",
                 "Vow ", " Vex", "Up  up"):  # fmt: skip
        specs.append((f"spaces {text!r}", [(text, {})], text, "", False))
    specs.append(("two identical runs 'Ho'+'ffa'", [("Ho", {}), ("ffa", {})], "Hoffa", "", False))
    two_languages = [("Hi", {"east_asian_lang": "en-US"}), ("ppo", {"east_asian_lang": "en-GB"})]
    specs.append(("runs differing in east asian lang 'Hi'+'ppo'", two_languages, "Hippo", "", False))
    specs.append(("run with w:spacing 10", [("Wiffle", {"spacing_twips": 10})], "Wiffle", "", False))
    specs.append(("single substitution K", [("K", {})], "K", "", True))
    specs.append(("chain: N before 'ow'", [("Now", {})], "ow", "N", False))
    specs.append(("chain across a space: 'The ' before 'end'", [("The end", {})], "end", "The ", False))
    cases = []
    for index, (label, runs, rule_text, backtrack, marker) in enumerate(specs):
        name = f"rule_p{index}"
        composites.append(natural_composite(font, name, rule_text, "liga", backtrack, marker))
        for variant in LIGATURE_SETTINGS:
            cases.append(Case(label, runs, name, variant))
    return composites, cases


def case_runs(case: Case) -> list[fd.TextRun]:
    runs = []
    for text, extra in case.runs:
        features = VARIANTS[case.variant]
        if "east_asian_lang" in extra:
            features = fd.RunFeatures(features.ligatures, features.contextual, features.stylistic_set,
                                      extra["east_asian_lang"])  # fmt: skip
        runs.append(fd.TextRun(text, FAMILY, SIZE_HALF_POINTS, spacing_twips=extra.get("spacing_twips", 0),
                               features=features))  # fmt: skip
    return runs


def layout(cases: list[Case], wide: bool) -> tuple[str, list[tuple[float, float, float]]]:
    """frames in columns (or full width); returns the body and each case's (x0, x1, baseline y) in pdf points."""
    paragraphs, boxes = [], []
    for index, case in enumerate(cases):
        column, row = (0, index) if wide else divmod(index, ROWS_PER_COLUMN)
        x = 720 + column * COLUMN_TWIPS
        y = FIRST_ROW_TWIPS + row * LINE_TWIPS
        width = PAGE_W * 20 - 1440 if wide else COLUMN_TWIPS - 200
        paragraphs.append(fd.frame_paragraph(case_runs(case), x, y, width, LINE_TWIPS))
        baseline_from_top = (y + 0.8 * LINE_TWIPS) / 20
        boxes.append((x / 20, (x + width) / 20, PAGE_H - baseline_from_top))
    return "".join(paragraphs), boxes


def classify(pdf_path: str, font: TTFont, cases: list[Case], boxes) -> list[dict]:
    names_by_signature = {}
    for name in font.getGlyphOrder():
        names_by_signature.setdefault(ff.outline_signature(font, name), name)
    glyphs = drawn_glyphs(pdf_path)
    rows = []
    for case, (x0, x1, baseline) in zip(cases, boxes, strict=True):
        mine = [g for g in glyphs if x0 - 1 <= g.x <= x1 and abs(g.y - baseline) < LINE_TWIPS / 40]
        drawn = [names_by_signature.get(g.signature, "?") for g in mine]
        rows.append({
            "case": case.label, "variant": case.variant, "applied": case.rule_glyph in drawn,
            "glyphs_drawn": len(mine), "chars": sum(len(t) for t, _ in case.runs),
            "unknown_glyphs": drawn.count("?"), "text_objects": sum(g.starts_object for g in mine),
            "to_unicode_of_rule": next((g.text for g, n in zip(mine, drawn, strict=True)
                                        if n == case.rule_glyph), None),
        })  # fmt: skip
    return rows


def export_and_classify(session, name: str, cases: list[Case], wide: bool, embedded, font: TTFont) -> list[dict]:
    body, boxes = layout(cases, wide)
    docx = fd.write_probe_docx(os.path.join(OUT, f"{name}.docx"), body, embedded, PAGE_W, PAGE_H)
    pdf = os.path.join(OUT, f"{name}.pdf")
    session.export(docx, pdf)
    return classify(pdf, font, cases, boxes)


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    font = base_font()
    feature_composites, matrix_cases = feature_cases(font)
    product_composites, long_cases = product_cases(font)
    ff.add_composites(font, feature_composites + product_composites)
    ff.add_rules(font, feature_composites + product_composites)
    font_path = os.path.join(OUT, "probe_lig.ttf")
    font.save(font_path)
    font = TTFont(font_path)
    embedded = [dw.EmbeddedFont(FAMILY, open(font_path, "rb").read())]
    results = {}
    with WordSession() as session:
        results["word"] = session.version
        results["word_pid"] = session.pid
        results["matrix"] = export_and_classify(session, "matrix", matrix_cases, False, embedded, font)
        # the product cases go one document each: one of them made word's export fail
        # in the first run, and a failure must not hide the other answers
        results["product"] = []
        labels = list(dict.fromkeys(case.label for case in long_cases))
        for number, label in enumerate(labels):
            cases = [case for case in long_cases if case.label == label]
            try:
                results["product"] += export_and_classify(session, f"product_{number}", cases, True, embedded, font)
            except Exception as error:
                results["product"].append({"case": label, "variant": "all three", "export_error": str(error)})
    with open(os.path.join(OUT, "c1_results.json"), "w", encoding="utf-8") as handle:
        json.dump(results, handle, indent=1)
    print_matrix(results)


def print_matrix(results: dict) -> None:
    print(results["word"], "pid", results["word_pid"])
    by_case: dict[str, dict[str, bool]] = {}
    for row in results["matrix"]:
        by_case.setdefault(row["case"], {})[row["variant"]] = row["applied"]
    print("feature".ljust(14) + " ".join(v[:9].rjust(9) for v in VARIANTS))
    for case, applied in by_case.items():
        print(case.ljust(14) + " ".join(("yes" if applied[v] else "-").rjust(9) for v in VARIANTS))
    print()
    for row in results["product"]:
        if "export_error" in row:
            print(f"{row['case'][:60]:60} export failed: {row['export_error'][:120]}")
            continue
        print(f"{row['case'][:60]:60} {row['variant']:9} applied={row['applied']!s:5} "
              f"glyphs={row['glyphs_drawn']:3} chars={row['chars']:3} objects={row['text_objects']} "
              f"unicode={row['to_unicode_of_rule']!r}")  # fmt: skip


if __name__ == "__main__":
    main()
