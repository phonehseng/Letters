"""experiment a1: does word use an embedded fitted font it cannot find installed, and keep it in its pdf?

one fitted font (a ligature over the whole line, as the corpus converter
makes) is embedded under a family name no machine has installed. it is
embedded four times with a different OS/2 fsType: 0 (installable), 2
(restricted: the licence forbids embedding), 4 (preview and print) and 8
(editable). for each, word opens the docx read-write (to see whether it
forces read-only) and exports it; the pdf says which outlines were drawn.
this only asks how word treats the flag; the converter itself never embeds
a font whose licence forbids it (prototype font_rebuild.may_embed).

usage:  ..\\.venv\\Scripts\\python.exe probe_embedding.py
"""

from __future__ import annotations

import fidelity_path

# isort: split

import json
import os

from fontTools.ttLib import TTFont

import docx_writer as dw
import fitted_docx as fd
import fitted_font as ff
import font_prep
import probe_ligatures as c1
from pdf_glyphs import drawn_glyphs
from word_session import WD_DO_NOT_SAVE_CHANGES, WordSession, export_open_document

OUT = os.path.join(fidelity_path.OUT_DIR, "a1_embedding")
TEXT = "Embedded fitted fonts travel with the document."
FS_TYPES = (0, 2, 4, 8)


def build(fs_type: int) -> tuple[str, str]:
    family = f"{c1.FAMILY} fs{fs_type}"
    font = c1.base_font()
    font_prep.rename_family(font, family)
    font["OS/2"].fsType = fs_type
    rule = c1.natural_composite(font, "line", TEXT, "liga")
    ff.add_composites(font, [rule])
    ff.add_rules(font, [rule])
    path = os.path.join(OUT, f"fs{fs_type}.ttf")
    font.save(path)
    run = fd.TextRun(TEXT, family, 24, features=fd.RunFeatures("standard"))
    body = fd.frame_paragraph([run], 1440, 1440, 9000, 360)
    embedded = [dw.EmbeddedFont(family, open(path, "rb").read())]
    docx = fd.write_probe_docx(os.path.join(OUT, f"fs{fs_type}.docx"), body, embedded)
    return docx, path


def check(session: WordSession, fs_type: int) -> dict:
    docx, font_path = build(fs_type)
    family = f"{c1.FAMILY} fs{fs_type}"
    pdf = os.path.join(OUT, f"fs{fs_type}.pdf")
    try:
        document = session.open(docx, read_only=False)
    except Exception as error:
        # fsType 4 left word busy ("a macro is currently running"): it opened a
        # prompt we cannot see, so this session is not used again
        return {"fs_type": fs_type, "open_read_write": f"failed: {str(error)[60:160]}"}
    try:
        read_only = bool(document.ReadOnly)
        export_open_document(document, pdf)
    finally:
        document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
    font = TTFont(font_path)
    ours = {ff.outline_signature(font, name) for name in font.getGlyphOrder()}
    line_glyph = ff.outline_signature(font, "line")
    drawn = [g for g in drawn_glyphs(pdf) if g.signature is not None or g.text.strip()]
    return {
        "fs_type": fs_type,
        "installed_on_this_machine": font_prep.is_family_installed(family),
        "opened_read_only": read_only,
        "glyphs_drawn": len(drawn),
        "drawn_from_our_outlines": sum(g.signature in ours for g in drawn),
        "whole_line_glyph_drawn": any(g.signature == line_glyph for g in drawn),
        "fonts_without_embedded_program": sorted({g.font_resource for g in drawn if g.signature is None}),
    }


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    rows = []
    for fs_type in FS_TYPES:
        # one word per case: a case that leaves word waiting on a prompt must not affect the next
        with WordSession() as session:
            rows.append({**check(session, fs_type), "word_pid": session.pid})
        print(rows[-1])
    with open(os.path.join(OUT, "a1_results.json"), "w", encoding="utf-8") as handle:
        json.dump(rows, handle, indent=1)


if __name__ == "__main__":
    main()
