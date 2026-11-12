"""corpus pages printed by a browser: microsoft edge (chromium, skia pdf) in headless mode.

skia writes truetype web fonts as cid fonts (Type0, CIDFontType2, Identity-H)
and turns cff web fonts into Type 3 fonts, and places every glyph with its
own Tj and a fractional Td, which makes it a real producer for three corpus
paths no other producer here covers: a cid subset with more than 256 glyphs,
a type 3 font, and glyph positions that sit on no grid.

edge runs with a private profile folder under out\\edge_profile, so the user's
own browser profile and windows are never touched; it exits when the page is
printed.
"""

from __future__ import annotations

import os
import subprocess

from corpus_builder import fonts
from corpus_builder.text import paragraph_text

EDGE = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROFILE = os.path.join(HERE, "out", "edge_profile")


def _font_url(path: str) -> str:
    return "file:///" + os.path.abspath(path).replace("\\", "/")


def print_html(html: str, pdf_path: str) -> str:
    """write the page next to the pdf and have a headless edge print it."""
    os.makedirs(PROFILE, exist_ok=True)
    html_path = os.path.splitext(pdf_path)[0] + ".html"
    with open(html_path, "w", encoding="utf-8") as handle:
        handle.write(html)
    if os.path.exists(pdf_path):
        os.remove(pdf_path)
    subprocess.run(
        [EDGE, "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
         f"--user-data-dir={PROFILE}", "--no-pdf-header-footer", f"--print-to-pdf={pdf_path}",
         _font_url(html_path)],
        check=True, timeout=120, capture_output=True,
    )  # fmt: skip
    if not os.path.exists(pdf_path):
        raise RuntimeError(f"edge printed nothing for {html_path}")
    return pdf_path


def _page(font_face: str, body: str, extra_css: str = "") -> str:
    return (
        "<!doctype html><html><head><meta charset='utf-8'><style>"
        f"{font_face}@page {{ size: 612pt 792pt; margin: 54pt 72pt; }}"
        "body { margin: 0; } p { margin: 0 0 8pt 0; } h1 { font-weight: normal; margin: 0 0 10pt 0; }"
        f"{extra_css}</style></head><body>{body}</body></html>"
    )


def cid_subset(out_dir: str) -> tuple[str, dict]:
    """crimson text as a truetype web font: more than 256 glyphs in one Identity-H cid font."""
    face = f"@font-face {{ font-family: 'CorpusSerif'; src: url('{_font_url(fonts.font_file('serif'))}'); }}"
    # 0x149 and 0x17f are not in crimson text (edge would fall back to a system font), 0xad is the soft hyphen
    extended = "".join(chr(c) for c in range(0x00C0, 0x0180) if c not in (0x00D7, 0x00F7, 0x0149, 0x017F))
    latin1 = "".join(chr(c) for c in range(0x00A1, 0x00C0) if c != 0x00AD)
    paragraphs = "".join(f"<p>{paragraph_text(700 + i, 4)}</p>" for i in range(3))
    body = (
        "<h1 style='font-size:20pt'>A subset with more than 256 glyphs</h1>"
        f"<div style='text-align:justify; font-kerning:normal'>{paragraphs}</div>"
        f"<p style='font-size:10pt'>{extended[:64]}</p><p style='font-size:10pt'>{extended[64:128]}</p>"
        f"<p style='font-size:10pt'>{extended[128:]}</p>"
        f"<p style='font-size:10pt'>0123456789 !\"#$%&amp;'()*+,-./:;&lt;=&gt;?@[\\]^_`{{|}}~ {latin1}</p>"
    )
    path = print_html(_page(face, body, "body { font-family: 'CorpusSerif'; font-size: 11pt; }"),
                      os.path.join(out_dir, "cid_subset.pdf"))  # fmt: skip
    return path, {"class": "cid subset > 256 glyphs", "producer": "Microsoft Edge 157 headless (Skia/PDF)",
                  "features": ["Type0 CIDFontType2 Identity-H", "more than 256 glyphs of one face",
                               "justified text, kerning on", "fractional glyph positions (Td per glyph)"],
                  "known_phrase": "more than 256 glyphs", "known_sentence": ""}  # fmt: skip


def type3(out_dir: str) -> tuple[str, dict]:
    """source serif 4 (cff) as a web font: skia writes it as a Type 3 font."""
    face = f"@font-face {{ font-family: 'CorpusCff'; src: url('{_font_url(os.path.join(HERE, 'fonts', 'SourceSerif4-Regular.otf'))}'); }}"
    paragraphs = "".join(f"<p>{paragraph_text(800 + i, 4)}</p>" for i in range(4))
    body = f"<h1 style='font-size:20pt'>Glyphs drawn by Type 3 procedures</h1>{paragraphs}"
    path = print_html(_page(face, body, "body { font-family: 'CorpusCff'; font-size: 11pt; }"),
                      os.path.join(out_dir, "type3_font.pdf"))  # fmt: skip
    return path, {"class": "Type 3 font", "producer": "Microsoft Edge 157 headless (Skia/PDF), cff web font",
                  "features": ["Type3 font with glyph procedures", "fractional glyph positions"],
                  "known_phrase": "Type 3 procedures", "known_sentence": ""}  # fmt: skip


def mixed_styles(out_dir: str) -> tuple[str, dict]:
    """lato as a web font: five sizes, colours, superscripts and subscripts, small caps."""
    face = (
        f"@font-face {{ font-family: 'CorpusSans'; src: url('{_font_url(fonts.font_file('sans'))}'); }}"
        f"@font-face {{ font-family: 'CorpusSans'; font-weight: bold; src: url('{_font_url(fonts.font_file('sans', 'bold'))}'); }}"
    )
    body = (
        "<p style='font-size:24pt; color:#1F3864'><b>Mixed styles on one page</b></p>"
        "<p style='font-size:16pt; color:#C00000'>Sixteen point red heading text</p>"
        "<p style='font-size:12pt'>Twelve point body with x<sup>2</sup> + y<sup>2</sup> = r<sup>2</sup> and "
        "H<sub>2</sub>O, CO<sub>2</sub>, and the 1<sup>st</sup> and 2<sup>nd</sup> terms.</p>"
        "<p style='font-size:10pt; font-variant: small-caps'>Ten point small caps: Department of Physics, Spring Term</p>"
        "<p style='font-size:8pt; color:#2E7D32'>Eight point green footnote text with <span style='color:#7B1FA2'>a "
        "purple phrase</span> inside it.</p>"
        f"<p style='font-size:12pt'>{paragraph_text(900, 3)}</p>"
    )
    path = print_html(
        _page(face, body, "body { font-family: 'CorpusSans'; }"), os.path.join(out_dir, "mixed_styles.pdf")
    )
    return path, {"class": "mixed styles", "producer": "Microsoft Edge 157 headless (Skia/PDF)",
                  "features": ["five sizes 24/16/12/10/8 pt", "coloured text", "superscripts and subscripts",
                               "synthesised small caps"],
                  "known_phrase": "Mixed styles", "known_sentence": ""}  # fmt: skip


BUILDERS = {"cid_subset": cid_subset, "type3_font": type3, "mixed_styles": mixed_styles}
