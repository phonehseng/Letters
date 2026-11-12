"""split each glyph's error into line start, whole pixels and pdf writer residue.

usage:  .venv\\Scripts\\python.exe -m prototype.decompose out\\prototype\\NAME [--worst N]

for every source line and the word line made from it (same page, same text
without spaces, nearest position):
    start    word line start - source line start (pt)
    pixels   difference of the whole-pixel offsets from each line start
    residue  difference of the sub-pixel parts (writer rounding, both sides)
so error = start + 0.12 * pixels + residue, glyph by glyph.
"""

from __future__ import annotations

import os
import sys
from collections import Counter, defaultdict

import numpy as np

from prototype.lines import group_lines
from prototype.pdf_content import read_document

PT_PER_PIXEL = 0.12


def visible(line):
    return [g for g in line.glyphs if g.text.strip()]


def decompose(source_pdf: str, word_pdf: str) -> list[dict]:
    source_lines = group_lines([g for p in read_document(source_pdf) for g in p.glyphs])
    word_lines = group_lines([g for p in read_document(word_pdf) for g in p.glyphs])
    by_text = defaultdict(list)
    for line in word_lines:
        by_text[(line.page, "".join(g.text for g in visible(line)))].append(line)
    rows = []
    for line in source_lines:
        glyphs = visible(line)
        if not glyphs or abs(line.angle) > 0.05:
            continue
        candidates = by_text.get((line.page, "".join(g.text for g in glyphs)))
        if not candidates:
            continue
        word = min(candidates, key=lambda c: abs(c.y - line.y) + abs(c.x - line.x))
        word_glyphs = visible(word)
        s0, w0 = glyphs[0].x, word_glyphs[0].x
        for index, (s, w) in enumerate(zip(glyphs, word_glyphs, strict=True)):
            source_px = (s.x - s0) / PT_PER_PIXEL
            word_px = (w.x - w0) / PT_PER_PIXEL
            rows.append({
                "page": line.page, "y": line.y, "index": index, "char": s.text, "size": s.size,
                "font": s.font.base_name, "dx": w.x - s.x, "dy": w.y - s.y, "start": w0 - s0,
                "pixels": round(word_px) - round(source_px),
                "source_residue": source_px - round(source_px), "word_residue": word_px - round(word_px),
                "source_x": s.x,
            })  # fmt: skip
    return rows


def summary(rows: list[dict]) -> None:
    dx = np.array([r["dx"] for r in rows])
    start = np.array([r["start"] for r in rows])
    src = np.array([r["source_residue"] for r in rows])
    wrd = np.array([r["word_residue"] for r in rows])
    pixels = Counter(r["pixels"] for r in rows)
    print(f"{len(rows)} glyphs  |dx| max {np.abs(dx).max():.3f} mean {np.abs(dx).mean():.4f}")
    print(f"   line start error: max {np.abs(start).max():.3f} mean {np.abs(start).mean():.4f} pt")
    print(f"   whole-pixel misses: {dict(sorted(pixels.items()))}")
    print(f"   source residue: max {np.abs(src).max():.3f} px mean {np.abs(src).mean():.3f} px")
    print(f"   word residue:   max {np.abs(wrd).max():.3f} px mean {np.abs(wrd).mean():.3f} px")
    big = [r for r in rows if abs(r["dx"]) > 0.125]
    print(f"   glyphs over 0.125 pt: {len(big)}; their pixel misses {Counter(r['pixels'] for r in big)}")


def main() -> None:
    folder = sys.argv[1].rstrip("\\/")
    name = os.path.basename(folder)
    import json

    from corpus_run import CORPUS  # the corpus pdf for this name

    manifest = json.load(open(os.path.join(CORPUS, "manifest.json"), encoding="utf-8"))
    entry = next(e for e in manifest if e["name"] == name)
    rows = decompose(os.path.join(CORPUS, entry["pdf"]), os.path.join(folder, f"{name}.word.pdf"))
    summary(rows)
    if "--worst" in sys.argv:
        count = int(sys.argv[sys.argv.index("--worst") + 1])
        for r in sorted(rows, key=lambda r: -abs(r["dx"]))[:count]:
            print(
                f"   p{r['page'] + 1} y {r['y']:.2f} #{r['index']} {r['char']!r} size {r['size']:.2f} dx {r['dx']:+.3f} "
                f"start {r['start']:+.3f} px {r['pixels']:+d} src {r['source_residue']:+.3f} word {r['word_residue']:+.3f}"
            )


if __name__ == "__main__":
    main()
