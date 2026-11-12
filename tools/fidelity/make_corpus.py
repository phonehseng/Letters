"""build the student document corpus under corpus\\ with a manifest.

producers on this machine: microsoft word (docx written here, exported with
ExportAsFixedFormat in a private hidden instance) and reportlab. libreoffice
and latex are not installed, so they are not used. scanned handouts belong to
the ocr stage and are not part of this corpus.

usage:  .venv\\Scripts\\python.exe make_corpus.py
output: corpus\\<name>.pdf, corpus\\<name>.docx (word sources), corpus\\manifest.json
"""

from __future__ import annotations

import json
import os

from corpus_builder import reportlab_documents, word_documents
from word_export import export_docx_to_pdf

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS_DIR = os.path.join(HERE, "corpus")


def main() -> None:
    os.makedirs(CORPUS_DIR, exist_ok=True)
    manifest = []
    for name, build in word_documents.BUILDERS.items():
        docx, entry = build(CORPUS_DIR)
        pdf = os.path.join(CORPUS_DIR, f"{name}.pdf")
        info = export_docx_to_pdf(docx, pdf)
        entry.update(
            {"name": name, "pdf": os.path.basename(pdf), "pages": info["word_pages"], "word": info["word_version"]}
        )
        manifest.append(entry)
        print(f"{name}: {info['word_pages']} pages by word {info['word_version']}")
    for name, build in reportlab_documents.BUILDERS.items():
        pdf, entry = build(CORPUS_DIR)
        entry.update({"name": name, "pdf": os.path.basename(pdf)})
        manifest.append(entry)
        print(f"{name}: written by reportlab")
    with open(os.path.join(CORPUS_DIR, "manifest.json"), "w", encoding="utf-8") as handle:
        json.dump(manifest, handle, indent=2)


if __name__ == "__main__":
    main()
