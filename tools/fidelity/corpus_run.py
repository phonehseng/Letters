"""run the prototype over the whole corpus and write corpus_report.md and corpus_report.json.

for each document: convert, export through word, measure (spec-exact glyph
origins), check fonts, pictures and vectors, open the docx in word again for an
editability check, and keep a reduced heatmap of the worst page. at the end the
calibration cache is checked by probing every document's twips again.

usage:  .venv\\Scripts\\python.exe corpus_run.py [name ...]
output: corpus_report.md, corpus_report.json, corpus_report_assets\\*.png, out\\prototype\\<name>\\
"""

from __future__ import annotations

import difflib
import json
import os
import re
import sys
import time

import numpy as np
from PIL import Image

import word_writer_model as wm
from harness import run_harness
from prototype import calibration, floor
from prototype.convert import convert, line_twips_for, word_build
from prototype.lines import ANGLE_TOLERANCE_DEG, group_lines
from prototype.pdf_content import read_document
from word_export import export_docx_to_pdf

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus")
ASSETS = os.path.join(HERE, "corpus_report_assets")
THRESHOLD_MAX_PT = 0.125
THRESHOLD_MEAN_PT = 0.05


def glyph_stats(pairs: list[dict]) -> dict:
    same_page = [p for p in pairs if p["page"] == p["candidate_page"]]
    distance = np.array([np.hypot(p["dx"], p["dy"]) for p in same_page])
    return {
        "glyphs": int(distance.size),
        "max": float(distance.max()),
        "mean": float(distance.mean()),
        "p99": float(np.percentile(distance, 99)),
        "within_0.025": float((distance <= 0.025).mean()),
        "within_0.06": float((distance <= 0.06).mean()),
        "within_0.125": float((distance <= 0.125).mean()),
        "dx_max": float(max(abs(p["dx"]) for p in same_page)),
        "dy_max": float(max(abs(p["dy"]) for p in same_page)),
    }


def _image_box(image) -> tuple[float, float, float, float]:
    a, b, c, d, e, f = image.matrix
    xs = [e, e + a, e + c, e + a + c]
    ys = [f, f + b, f + d, f + b + d]
    return min(xs), min(ys), max(xs), max(ys)


def image_errors(source_pdf: str, word_pdf: str) -> dict:
    """corner error of every picture, matched in order and by size per page."""
    source, word = read_document(source_pdf), read_document(word_pdf)
    errors = []
    unmatched = 0
    for page_source, page_word in zip(source, word, strict=False):
        word_boxes = [_image_box(i) for i in page_word.images]
        for image in page_source.images:
            box = _image_box(image)
            if not word_boxes:
                unmatched += 1
                continue
            best = min(word_boxes, key=lambda w: sum(abs(p - q) for p, q in zip(box, w, strict=True)))
            errors.append(max(abs(p - q) for p, q in zip(box, best, strict=True)))
    return {
        "pictures": len(errors) + unmatched,
        "unmatched": unmatched,
        "max_corner_error_pt": max(errors) if errors else None,
    }


def normalised_words(text: str) -> list[str]:
    return re.sub(r"\s+", " ", text.replace("\r", " ").replace("\x07", " ")).strip().split(" ")


def editability(docx: str, source_pdf: str, phrase: str, sentence: str, out_dir: str) -> dict:
    """open the docx in word: does find work, how many paragraphs, is the text in reading order?"""

    def prepare(word, document):
        def found(text: str) -> bool | None:
            if not text:
                return None  # no known sentence for this document
            finder = document.Content.Duplicate.Find
            finder.ClearFormatting()
            return bool(finder.Execute(FindText=text[:255], MatchCase=True, Forward=True, Wrap=0))

        return {
            "find_phrase": found(phrase),
            "find_sentence": found(sentence),
            "paragraphs": document.Paragraphs.Count,
            "text": document.Content.Text,
        }

    info = export_docx_to_pdf(docx, os.path.join(out_dir, "editability.pdf"), prepare=prepare)
    notes = info["notes"]
    source_text = " ".join(
        "".join(g.text for g in line.glyphs)
        for line in group_lines([g for p in read_document(source_pdf) for g in p.glyphs if g.render_mode not in (3, 7)])
    )
    word_words, source_words = normalised_words(notes.pop("text")), normalised_words(source_text)
    notes["reading_order_similarity"] = difflib.SequenceMatcher(a=source_words, b=word_words, autojunk=False).ratio()
    return notes


def save_worst_heatmap(report: dict, name: str) -> str | None:
    pages = [p for p in report["raster"]["pages"] if "ssim" in p]
    if not pages:
        return None
    worst = min(pages, key=lambda p: p["ssim"])
    path = os.path.join(os.path.dirname(report["word_pdf"]), worst["heatmap"])
    os.makedirs(ASSETS, exist_ok=True)
    image = Image.open(path)
    image.thumbnail((1100, 1100))
    target = os.path.join(ASSETS, f"{name}_page{worst['page'] + 1}.png")
    image.save(target, optimize=True)
    return os.path.relpath(target, HERE).replace("\\", "/")


def word_substitutions(report: dict) -> list[str]:
    """fonts in word's pdf that are not ones we embedded (word renames embedded fonts ___WRD_EMBED_SUB_nn)."""
    return [f["base_name"] for f in report["fonts_in_word_pdf"] if "WRD_EMBED" not in f["base_name"]]


def run_document(entry: dict) -> dict:
    name = entry["name"]
    out_dir = os.path.join(HERE, "out", "prototype", name)
    os.makedirs(out_dir, exist_ok=True)
    pdf = os.path.join(CORPUS, entry["pdf"])
    docx = os.path.join(out_dir, f"{name}.docx")
    conversion = convert(pdf, docx, os.path.join(out_dir, "work"))
    with open(os.path.join(out_dir, f"{name}.convert.json"), "w", encoding="utf-8") as handle:
        json.dump(conversion, handle, indent=2, default=str)
    report = run_harness(pdf, docx, out_dir, keep_pairs=True, exact_positions=True)
    stats = glyph_stats(report["pairs"])
    cache = calibration.ModelCache(word_build())
    tables = {key: wm.StepTable(tuple(steps)) for key, steps in cache.steps.items()}
    lattice_floor = floor.summary(floor.glyph_floors(pdf, tables))
    substituted_by_converter = [f["pdf_font"] for f in conversion["fonts"] if f.get("substitute")]
    substituted_by_word = word_substitutions(report)
    position_pass = stats["max"] <= THRESHOLD_MAX_PT and stats["mean"] < THRESHOLD_MEAN_PT
    fonts_pass = not substituted_by_converter and not substituted_by_word
    result = {
        "name": name,
        "class": entry["class"],
        "producer": entry["producer"],
        "features": entry["features"],
        "glyphs": stats,
        "floor": lattice_floor,
        "check_pass": conversion.get("check_pass"),
        "fonts": {
            "faces": len(conversion["fonts"]),
            "rebuilt_from_pdf": sum(1 for f in conversion["fonts"] if not f.get("substitute")),
            "substituted_by_converter": substituted_by_converter,
            "substituted_by_word": substituted_by_word,
            "details": conversion["fonts"],
        },
        "pictures": {**image_errors(pdf, report["word_pdf"]), "jpeg_kept": conversion["jpeg_kept"]},
        "vectors": {
            "shapes": conversion["shapes"],
            "rasterised_items": conversion["rasterised_vector_items"],
            "raster_pictures": conversion["raster_pictures"],
        },  # fmt: skip
        "text": {
            "rotated_lines": conversion["rotated_lines"],
            "hidden_lines": conversion["hidden_lines"],
            "scripts": conversion["scripts"],
            "glyphs_without_char": conversion["glyphs_without_char"],
            "horizontally_scaled_glyphs": conversion["horizontally_scaled_glyphs"],
            "rotated_probe": conversion.get("rotated"),
        },  # fmt: skip
        "ssim_min": min(p["ssim"] for p in report["raster"]["pages"] if "ssim" in p),
        "pages": {"source": len(report["page_sizes"]["source"]), "word": len(report["page_sizes"]["word"])},
        "bytes": {"pdf": conversion["pdf_bytes"], "docx": conversion["docx_bytes"]},
        "seconds": {
            "convert": conversion["seconds"],
            "calibration_probe": conversion["calibration"].get("probe_seconds", 0),
            "word_export": report["word"]["timings"]["export_s"],
            "word_total": report["seconds"]["word_total"],
        },  # fmt: skip
        "calibration": conversion["calibration"],
        "pass": {"positions": position_pass, "fonts": fonts_pass, "overall": position_pass and fonts_pass},
        "heatmap": save_worst_heatmap(report, name),
    }
    result["editability"] = editability(docx, pdf, entry["known_phrase"], entry["known_sentence"], out_dir)
    print(
        f"{name:<15} max {stats['max']:.3f} mean {stats['mean']:.4f} p99 {stats['p99']:.3f} "
        f"<=0.125 {100 * stats['within_0.125']:.1f}%  fonts ok {fonts_pass}  pass {result['pass']['overall']}"
    )
    return result


def verify_cache(entries: list[dict]) -> dict:
    """measure every cached fact the corpus uses again (step tables, baseline offsets) and count disagreements."""
    cache = calibration.ModelCache(word_build())
    sizes, heights = set(), set()
    for entry in entries:
        pages = read_document(os.path.join(CORPUS, entry["pdf"]))
        sizes |= {(p.width, p.height) for p in pages}
        heights |= {line_twips_for(line.max_size) for line in group_lines([g for p in pages for g in p.glyphs])
                    if abs(line.angle) <= ANGLE_TOLERANCE_DEG}  # fmt: skip
    started = time.perf_counter()
    result = calibration.ensure_measured(cache, sizes, heights, os.path.join(HERE, "out", "cache_check"), verify=True)
    return {"documents": len(entries), "compared": result["compared"], "disagreements": result["disagreements"],
            "step_differences": result.get("step_differences", {}), "seconds": time.perf_counter() - started}  # fmt: skip


def _round3_results() -> dict[str, dict]:
    path = os.path.join(HERE, "corpus_report_round3.json")
    if not os.path.exists(path):
        return {}
    data = json.load(open(path, encoding="utf-8"))
    return {r["name"]: r["glyphs"] for r in data["results"]}


def markdown(results: list[dict], cache_check: dict) -> str:
    lines = [
        "# corpus report: pdf to docx prototype, measured through word's pdf export",
        "",
        f"Target: every glyph within {THRESHOLD_MAX_PT} pt of the source, mean under {THRESHOLD_MEAN_PT} pt, the same",
        "embedded fonts. Glyph origins are read with the spec-exact interpreter (`pdf_analysis.extract_chars_exact`),",
        "distances combine x and y. Generated by `corpus_run.py`; numbers below are from that run.",
        "",
        "| class | glyphs | max | mean | p99 | <=0.025 | <=0.06 | <=0.125 | positions | fonts | overall |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in results:
        g = r["glyphs"]
        lines.append(
            f"| {r['class']} | {g['glyphs']} | {g['max']:.3f} | {g['mean']:.4f} | {g['p99']:.3f} | "
            f"{100 * g['within_0.025']:.1f}% | {100 * g['within_0.06']:.1f}% | {100 * g['within_0.125']:.1f}% | "
            f"{'pass' if r['pass']['positions'] else 'FAIL'} | {'pass' if r['pass']['fonts'] else 'FAIL'} | "
            f"{'pass' if r['pass']['overall'] else 'FAIL'} |"
        )
    round3 = _round3_results()
    lines += ["", "Against round 3 (`corpus_report_round3.json`) and the lattice floor (`prototype/floor.py`: each source",
              "glyph's distance to the nearest position word can print, the best any plan can reach):", "",
              "| class | round 3 max / mean | round 4 max / mean | floor max / mean | floor x mean / y mean | glyphs off plan after checking |",
              "|---|---|---|---|---|---|"]  # fmt: skip
    for r in results:
        g, f = r["glyphs"], r.get("floor") or {}
        before = round3.get(r["name"])
        before_text = f"{before['max']:.3f} / {before['mean']:.4f}" if before else "new"
        floor_text = f"{f['max']:.3f} / {f['mean']:.4f}" if f else "-"
        axes = f"{f['x_mean']:.4f} / {f['y_mean']:.4f}" if f else "-"
        check = r.get("check_pass") or {}
        lines.append(f"| {r['class']} | {before_text} | {g['max']:.3f} / {g['mean']:.4f} | {floor_text} | {axes} | "
                     f"{check.get('glyphs_off_plan_final', '-')} |")  # fmt: skip
    lines += ["", "| class | fonts rebuilt / substituted | pictures (max corner error) | vectors as shapes / rasterised | SSIM min | pdf -> docx bytes | convert s | word export s |",
              "|---|---|---|---|---|---|---|---|"]  # fmt: skip
    for r in results:
        f, p, v = r["fonts"], r["pictures"], r["vectors"]
        substituted = (
            ", ".join(f["substituted_by_converter"] + [f"word: {n}" for n in f["substituted_by_word"]]) or "none"
        )
        picture = (
            f"{p['pictures']} ({p['max_corner_error_pt']:.3f} pt)"
            if p["max_corner_error_pt"] is not None
            else f"{p['pictures']}"
        )
        lines.append(
            f"| {r['class']} | {f['rebuilt_from_pdf']} / {substituted} | {picture}, jpeg kept {p['jpeg_kept']} | "
            f"{v['shapes']} / {v['rasterised_items']} in {v['raster_pictures']} pictures | {r['ssim_min']:.4f} | "
            f"{r['bytes']['pdf']:,} -> {r['bytes']['docx']:,} | {r['seconds']['convert']:.1f} | {r['seconds']['word_export']:.1f} |"
        )
    lines += ["", "| class | find phrase | find sentence | paragraphs | reading order similarity | rotated lines | hidden lines |",
              "|---|---|---|---|---|---|---|"]  # fmt: skip
    for r in results:
        e, t = r["editability"], r["text"]
        lines.append(
            f"| {r['class']} | {e['find_phrase']} | {e['find_sentence']} | {e['paragraphs']} | "
            f"{e['reading_order_similarity']:.3f} | {t['rotated_lines']} | {t['hidden_lines']} |"
        )
    lines += ["", f"Calibration cache check: {cache_check['compared']} cached facts (writer step tables and baseline "
                  f"offset tables) measured again for {cache_check['documents']} documents, "
                  f"{cache_check['disagreements']} disagreements ({cache_check['seconds']:.0f} s).", ""]  # fmt: skip
    lines.append("## worst page heatmaps (blue: only in the source, red: only in word's output)")
    lines.append("")
    for r in results:
        if r["heatmap"]:
            lines.append(f"- {r['class']}: ![{r['name']}]({r['heatmap']})")
    return "\n".join(lines) + "\n"


def load_manifest(extra: bool) -> list[dict]:
    """the six round 3 classes, plus the round 4 additions (corpus\\extra, make_corpus_extra.py) with extra."""
    manifest = json.load(open(os.path.join(CORPUS, "manifest.json"), encoding="utf-8"))
    extra_path = os.path.join(CORPUS, "extra", "manifest.json")
    if extra and os.path.exists(extra_path):
        manifest += json.load(open(extra_path, encoding="utf-8"))
    return manifest


def main() -> None:
    arguments = [a for a in sys.argv[1:] if not a.startswith("--")]
    manifest = load_manifest("--extra" in sys.argv or bool(arguments))
    wanted = set(arguments)
    entries = [e for e in manifest if not wanted or e["name"] in wanted]
    results = [run_document(entry) for entry in entries]
    cache_check = verify_cache(entries)
    print(f"cache check: {cache_check}")
    with open(os.path.join(HERE, "corpus_report.json"), "w", encoding="utf-8") as handle:
        json.dump({"results": results, "cache_check": cache_check}, handle, indent=2, default=str)
    with open(os.path.join(HERE, "corpus_report.md"), "w", encoding="utf-8") as handle:
        handle.write(markdown(results, cache_check))


if __name__ == "__main__":
    main()
