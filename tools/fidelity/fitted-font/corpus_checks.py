"""checks on a converted document besides glyph positions: its fonts and its text.

fonts: every font that draws in word's pdf must be embedded and be ours,
judged by outline (word renames embedded fonts, so names prove nothing).
text: the docx's text through word, and the text a pdf reader extracts from
word's pdf, both compared word by word with the source's text.
the edit check is in edit_check.py.
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import difflib
import re
from collections import Counter, defaultdict

import pypdfium2 as pdfium

import fitted_font as ff
from pdf_glyphs import drawn_glyphs
from prototype.lines import group_lines
from prototype.pdf_content import read_document
from word_session import WD_DO_NOT_SAVE_CHANGES, WordSession


def normalised_words(text: str) -> list[str]:
    return re.sub(r"\s+", " ", text.replace("\r", " ").replace("\x07", " ")).strip().split(" ")


def source_text(pdf_path: str) -> str:
    glyphs = [g for p in read_document(pdf_path) for g in p.glyphs if g.render_mode not in (3, 7)]
    return " ".join("".join(g.text for g in line.glyphs) for line in group_lines(glyphs))


def pdf_text(pdf_path: str) -> str:
    document = pdfium.PdfDocument(pdf_path)
    try:
        parts = []
        for page in document:
            text_page = page.get_textpage()
            parts.append(text_page.get_text_bounded())
            text_page.close()
            page.close()
        return " ".join(parts)
    finally:
        document.close()


def similarity(a: str, b: str) -> float:
    return difflib.SequenceMatcher(a=normalised_words(a), b=normalised_words(b), autojunk=False).ratio()


def chars_present(source: str, other: str) -> float:
    """share of the source's non-space characters found in the other text, in any order.

    pdf readers split and join words at text object boundaries (a superscript,
    a glyph from a second font), so words can differ while no character is lost."""
    wanted = Counter(c for c in source if not c.isspace())
    found = Counter(c for c in other if not c.isspace())
    return sum((wanted & found).values()) / max(1, sum(wanted.values()))


def words_present(source: str, other: str) -> float:
    """share of the source's words (with repeats) found in the other text, in any order.

    reading order differs in tables and columns, which lowers the similarity
    above without losing a word; this says whether any text was lost."""
    wanted, found = Counter(normalised_words(source)), Counter(normalised_words(other))
    return sum((wanted & found).values()) / max(1, sum(wanted.values()))


def font_check(word_pdf: str, face_fonts: dict[str, str]) -> list[dict]:
    """per font drawing in word's pdf: is it embedded, and are its drawn outlines ours?"""
    from fontTools.ttLib import TTFont

    ours = set()
    for path in face_fonts.values():
        font = TTFont(path)
        ours.update(ff.outline_signature(font, name) for name in font.getGlyphOrder())
    counts = defaultdict(lambda: {"drawn": 0, "ours": 0, "no_program": 0})
    for glyph in drawn_glyphs(word_pdf):
        entry = counts[glyph.font_resource]
        entry["drawn"] += 1
        if glyph.signature is None:
            entry["no_program"] += 1
        elif glyph.signature in ours:
            entry["ours"] += 1
    return [{"font": name, **entry} for name, entry in counts.items()]


def text_checks(session: WordSession, docx: str, word_pdf: str, source_pdf: str, phrase: str, sentence: str) -> dict:
    document = session.open(docx)
    try:

        def found(text: str) -> bool | None:
            if not text:
                return None
            finder = document.Content.Duplicate.Find
            finder.ClearFormatting()
            return bool(finder.Execute(FindText=text[:255], MatchCase=True, Forward=True, Wrap=0))

        docx_text = document.Content.Text
        result = {"find_phrase_in_docx": found(phrase), "find_sentence_in_docx": found(sentence)}
    finally:
        document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
    source = source_text(source_pdf)
    extracted = pdf_text(word_pdf)
    flat = " ".join(normalised_words(extracted))
    result.update({
        "docx_text_similarity": similarity(source, docx_text),
        "docx_words_present": words_present(source, docx_text),
        "word_pdf_text_similarity": similarity(source, extracted),
        "word_pdf_words_present": words_present(source, extracted),
        "docx_chars_present": chars_present(source, docx_text),
        "word_pdf_chars_present": chars_present(source, extracted),
        "find_phrase_in_word_pdf": " ".join(normalised_words(phrase)) in flat if phrase else None,
        "find_sentence_in_word_pdf": " ".join(normalised_words(sentence)) in flat if sentence else None,
    })  # fmt: skip
    return result
