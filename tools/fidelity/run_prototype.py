"""convert one or more corpus documents and measure each through the harness.

usage:  .venv\\Scripts\\python.exe run_prototype.py syllabus research_paper slides
output: out\\prototype\\<name>\\ (docx, convert.json, harness report, heatmaps)
"""

from __future__ import annotations

import json
import os
import sys

from harness import print_summary, run_harness
from prototype.convert import convert

HERE = os.path.dirname(os.path.abspath(__file__))


def run(name: str, verify_cache: bool = False) -> tuple[dict, dict]:
    out_dir = os.path.join(HERE, "out", "prototype", name)
    os.makedirs(out_dir, exist_ok=True)
    pdf = os.path.join(HERE, "corpus", f"{name}.pdf")
    docx = os.path.join(out_dir, f"{name}.docx")
    conversion = convert(pdf, docx, os.path.join(out_dir, "work"), verify_cache)
    with open(os.path.join(out_dir, f"{name}.convert.json"), "w", encoding="utf-8") as handle:
        json.dump(conversion, handle, indent=2, default=str)
    report = run_harness(pdf, docx, out_dir, keep_pairs=True, exact_positions=True)
    print_summary(report, label=f"{name}: {conversion['lines']} lines, {conversion['grid_fonts']} grid fonts, "
                                f"{conversion['shapes']} shapes, {conversion['rasterised_vector_items']} vector items rasterised")  # fmt: skip
    return conversion, report


if __name__ == "__main__":
    for document in sys.argv[1:]:
        run(document)
