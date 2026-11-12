# Worklog: text recognition helpers (shell-core/src/ocr)

What changed in this folder and why, newest last.

## 2026-11-11 runs, rendering, overview, summary and words

A run reads pages through a `PageReader` (`job.rs`) so the orchestration
is tested with a fake engine, without tesseract or pdfium. Nothing is
written during a run: finished pages wait in `jobs.rs` until the ui adds
them in one edit, so cancelling or a failed page leaves the document as
it was. Pages are drawn at 300 dpi from the worker's tiles (`render.rs`)
because the worker has no whole-page call; they go through its normal
queue so the screen keeps drawing, and a skipped tile is asked for again.
`words.rs` keeps tesseract's words for the overlay only while they match
the invisible text on the page, and corrects a word by writing the page's
layer again, since letters-ocr has no call that changes one word.

## 2026-11-12 tesseract couldn't open its data through \\?\ paths

In the real app every run failed with "Could not initialize tesseract".
Tauri gives the resource folder as a `\\?\C:\...` path, and tesseract
can't open its language data through one. `engine_folder::plain_path`
strips the prefix from every candidate folder.

## 2026-11-12 the overlay keeps tesseract's words more often

Reading the layer back can join or split words where the gaps are small,
so comparing word lists threw tesseract's words (and their confidence)
away right after a run. `same_words` now compares the letters alone.

## 2026-11-12 the shell's New from Images commands get their own file

`app/shell/src/ocr_commands.rs` had grown past 550 lines. Making a pdf
from pictures is a separate job from recognising an open document, so
its commands move to `app/shell/src/ocr_images_commands.rs`.

## 2026-11-12 Make Editable Text

`make_pages_editable` runs letters-edit's `convert_scan_to_text` over
recognised pages inside one edit. A page's invisible text layer (from
Recognize Text) is taken off first; otherwise every word would be on the
page twice and every search would find it twice. A page that fails names
itself in the sentence and nothing changes.
