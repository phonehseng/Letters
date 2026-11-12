"""build the round 4 corpus additions: font paths and text features the first six classes never ran.

    browser    cid subset > 256 glyphs, type 3 font, mixed styles (edge headless, skia pdf)
    handmade   bare cff (FontFile3), type 1 (FontFile), restricted fsType, justified TJ kerning
               (pikepdf, operator by operator: no producer on this machine writes these)

usage:  .venv\\Scripts\\python.exe make_corpus_extra.py
output: corpus\\extra\\<name>.pdf and corpus\\extra\\manifest.json
"""

from __future__ import annotations

import json
import os

from corpus_builder import browser_documents, handmade_documents

HERE = os.path.dirname(os.path.abspath(__file__))
EXTRA_DIR = os.path.join(HERE, "corpus", "extra")


def main() -> None:
    os.makedirs(EXTRA_DIR, exist_ok=True)
    manifest = []
    for builders in (browser_documents.BUILDERS, handmade_documents.BUILDERS):
        for name, build in builders.items():
            pdf, entry = build(EXTRA_DIR)
            entry.update({"name": name, "pdf": os.path.join("extra", os.path.basename(pdf))})
            manifest.append(entry)
            print(f"{name}: {os.path.getsize(pdf):,} bytes")
    with open(os.path.join(EXTRA_DIR, "manifest.json"), "w", encoding="utf-8") as handle:
        json.dump(manifest, handle, indent=2)


if __name__ == "__main__":
    main()
