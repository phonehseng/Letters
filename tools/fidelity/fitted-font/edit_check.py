"""what one edit does: a word of one line is replaced through word itself, then every glyph is compared.

the product promise is "add a line or two without altering anything else",
so three things are measured against the unedited export:
    other lines: are all their glyphs (outline, page, x, y to 1/10000 pt) unchanged?
    the edited line before the edit: still exact (pieces kept), or how far off?
    the edited line after the edit: it moves by the new word's extra width, as
        it must; what matters is whether its letters keep the source spacing,
        so the error is measured after removing that one shift (the median).
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import os
import statistics

from pdf_glyphs import DrawnGlyph, drawn_glyphs
from word_session import WD_DO_NOT_SAVE_CHANGES, WordSession, export_open_document

WD_FORMAT_DOCUMENT = 16  # wdFormatXMLDocument (.docx)
REPLACEMENT = "rewritten"
POSITION_DIGITS = 4  # glyphs at the same place to 1/10000 pt count as not moved


def choose_edit(runs_by_line: list[list]) -> tuple[int, str, str, int] | None:
    """(line number, three-word context, word to replace, its word index): the longest line, page 1 first.

    the context must occur once in the whole document, or word's find lands on
    another line (it did in the first corpus run). the slides have no line of
    five words, so any line of three or more words on any page is a fallback."""
    texts = ["".join(r.text for r in runs) for runs in runs_by_line]
    everything = "\n".join(texts)
    candidates = []
    for number, runs in enumerate(runs_by_line):
        if not runs or not any(r.pieces for r in runs):
            continue
        words = texts[number].split()
        if len(words) >= 3:
            preferred = runs[0].line.page == 0 and len(words) >= 5
            candidates.append((preferred, len(words), number, words))
    for _, _, number, words in sorted(candidates, reverse=True):
        middle = len(words) // 2
        context = " ".join(words[middle - 1 : middle + 2])
        if everything.count(context) == 1:
            return number, context, words[middle], middle
    return None


def keys(glyphs: list[DrawnGlyph]) -> set[tuple]:
    return {(g.page, round(g.x, POSITION_DIGITS), round(g.y, POSITION_DIGITS), g.signature) for g in glyphs}


def edit_check(session: WordSession, docx: str, reference_pdf: str, runs_by_line: list[list], out_dir: str) -> dict:
    choice = choose_edit(runs_by_line)
    if choice is None:
        return {"edited": False, "reason": "no line on page 1 with five words and a unique three-word context"}
    line_number, context, target, word_index = choice
    edited_pdf = os.path.join(out_dir, "edited.word.pdf")
    saved_docx = os.path.join(out_dir, "edited.docx")
    document = session.open(docx, read_only=False)
    try:
        found_range = document.Content
        if not found_range.Find.Execute(FindText=context, MatchCase=True, Forward=True, Wrap=0):
            return {"edited": False, "reason": f"word could not find {context!r}"}
        offset = context.index(target, len(context.split(" ")[0]))
        document.Range(found_range.Start + offset, found_range.Start + offset + len(target)).Text = REPLACEMENT
        export_open_document(document, edited_pdf)
        document.SaveAs2(FileName=os.path.abspath(saved_docx), FileFormat=WD_FORMAT_DOCUMENT, AddToRecentFiles=False)
    finally:
        document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
    reopened_pdf = os.path.join(out_dir, "edited_reopened.word.pdf")
    session.export(saved_docx, reopened_pdf)
    runs = runs_by_line[line_number]
    result = {"edited": True, "line": "".join(r.text for r in runs)[:80], "replaced": target, "with": REPLACEMENT}
    result.update(compare(reference_pdf, edited_pdf, runs, word_index))
    result["saved_and_reopened_identical"] = keys(drawn_glyphs(edited_pdf)) == keys(drawn_glyphs(reopened_pdf))
    result["saved_docx_bytes"] = os.path.getsize(saved_docx)
    return result


def compare(reference_pdf: str, edited_pdf: str, runs: list, word_index: int) -> dict:
    line = runs[0].line

    def on_line(glyph: DrawnGlyph) -> bool:
        return glyph.page == line.page and abs(glyph.y - line.y) < 0.6 * line.max_size

    before, after = drawn_glyphs(reference_pdf), drawn_glyphs(edited_pdf)
    others_before, others_after = (
        keys([g for g in before if not on_line(g)]),
        keys([g for g in after if not on_line(g)]),
    )
    line_before, line_after = [g for g in before if on_line(g)], [g for g in after if on_line(g)]
    new = others_after - others_before
    result = {
        "other_glyphs": len(others_before),
        "other_glyphs_unchanged": len(others_before & others_after),
        "other_glyphs_new_or_moved": len(new),
        # a longer line can wrap inside its frame: its tail then sits under the line
        "new_glyphs_below_edited_line": sum(1 for page, _, y, _ in new if page == line.page and y < line.y),
        "edited_line_glyphs_before": len(keys(line_before)),
        "edited_line_glyphs_kept": len(keys(line_before) & keys(line_after)),
    }
    result.update(_edited_line(line_after, keys(line_before), runs, word_index))
    return result


def _items_by_side(runs: list, word_index: int) -> tuple[list, list]:
    """inked source items of the line before and after the replaced word."""
    before, after = [], []
    word, previous, seen_text = 0, None, False
    for run in runs:
        for item in run.items:
            # counted the way str.split() counts the line's words in choose_edit
            if item.char != " ":
                if seen_text and previous == " ":
                    word += 1
                seen_text = True
            previous = item.char
            if item.source is None or not item.char.strip() or item.hidden:
                continue
            if word < word_index:
                before.append(item)
            elif word > word_index:
                after.append(item)
    return before, after


def _edited_line(drawn: list[DrawnGlyph], kept_keys: set, runs: list, word_index: int) -> dict:
    before_items, after_items = _items_by_side(runs, word_index)
    kept = [g for g in drawn if g.signature is not None and keys([g]) <= kept_keys]
    kept_chars = sum(1 for g in kept for c in g.text if not c.isspace())
    ordinary = sorted((g for g in drawn if len(g.text) == 1 and not g.text.isspace() and g not in kept),
                      key=lambda g: g.x)  # fmt: skip
    loose_before = before_items[kept_chars:] if kept_chars <= len(before_items) else []
    tail = ordinary[len(ordinary) - len(after_items) :] if len(ordinary) >= len(after_items) else []
    head = ordinary[: len(loose_before)]
    result = {
        "edited_line_chars_before_edit": len(before_items),
        "edited_line_chars_before_edit_in_kept_pieces": kept_chars,
    }
    if head and all(g.text == i.char for g, i in zip(head, loose_before, strict=True)):
        errors = [abs(g.x - i.source.x) for g, i in zip(head, loose_before, strict=True)]
        result["before_edit_fallback_max_pt"] = max(errors)
        result["before_edit_fallback_mean_pt"] = sum(errors) / len(errors)
    if tail and all(g.text == i.char for g, i in zip(tail, after_items, strict=True)):
        offsets = [g.x - i.source.x for g, i in zip(tail, after_items, strict=True)]
        shift = statistics.median(offsets)
        spacing = [abs(o - shift) for o in offsets]
        result.update({"after_edit_chars": len(tail), "after_edit_shift_pt": shift,
                       "after_edit_spacing_error_max_pt": max(spacing),
                       "after_edit_spacing_error_mean_pt": sum(spacing) / len(spacing)})  # fmt: skip
    else:
        result["after_edit_chars"] = 0
    return result
