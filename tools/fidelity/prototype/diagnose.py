"""where does a converted document lose accuracy? per-line error summary from a harness report.

usage:  .venv\\Scripts\\python.exe -m prototype.diagnose path\\to\\name.report.json [top_n]
prints the lines with the largest glyph error, the error of each line's first
glyph (placement) and the worst drift inside it (spacing).
"""

from __future__ import annotations

import json
import sys
from collections import defaultdict

from compare import match_chars
from pdf_analysis import extract_chars_exact as extract_chars


def main() -> None:
    report = json.load(open(sys.argv[1], encoding="utf-8"))
    top_n = int(sys.argv[2]) if len(sys.argv) > 2 else 12
    pairs = match_chars(extract_chars(report["source_pdf"]), extract_chars(report["word_pdf"]))
    lines = defaultdict(list)
    for source, word in pairs:
        lines[(source.page, round(source.origin_y, 2))].append((source, word))
    rows = []
    for key, chars in lines.items():
        chars.sort(key=lambda pair: pair[0].origin_x)
        errors = [w.origin_x - s.origin_x for s, w in chars]
        rows.append(
            (
                max(map(abs, errors)),
                key,
                errors[0],
                "".join(s.char for s, _ in chars)[:50],
                chars[0][0].font_size,
                chars[0][0].font_name,
            )
        )
    rows.sort(reverse=True)
    for worst, (page, y), first, text, size, font in rows[:top_n]:
        print(f"p{page + 1} y={y:7.2f} size {size:5.2f} worst {worst:.3f} first {first:+.3f} {font[:24]:<24} {text!r}")


if __name__ == "__main__":
    main()
