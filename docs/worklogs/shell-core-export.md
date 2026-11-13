# Worklog: app/shell-core/src/export

## 2026-11-11 export jobs, capabilities and the result line

Added `request.rs` (the job and outcome the Export dialog sends and gets),
`choices.rs` (core/convert's capability list, with the entries this crate
builds marked available, so the dialog reads one list), `run.rs` (one export
through the right engine) and `summary.rs` (each engine's report in plain
words), because the dialog needed one door into every conversion and the
engines report in numbers, not sentences.

## 2026-11-11 easy to edit word and opendocument files

Added `easy_edit/`, mapping `analyze_structure`'s model onto the office
model as core/convert's README describes, because no engine writes these
files: core/convert reads the structure and core/office writes documents.
Body font and size go into the Normal style so runs only carry differences,
which keeps the result restylable. Tests on hand-made structures and on a
generated study guide pdf, round-tripped through write_docx and write_odt.

## 2026-11-11 pages drawn by the app's own pdf engine

Added `renderer.rs`: whole pages put together from the worker's tiles, the
same way core/convert's tests draw them, so the converters and core/exact use
the engine the app already runs instead of loading pdfium twice.

## 2026-11-12 exact match through core/exact

Added `exact.rs` once core/exact landed: the adapter passes a cache folder
(measured Word facts are reused), a temp work folder (Word can't open files in
the app cache), verify when Word is installed, the renderer, progress and
cancel; the outcome carries the engine's verification sentence, the measured
numbers and each font stand-in. Tested with use_word off.

## 2026-11-12 office files to pdf

Added `office.rs`: spreadsheets and presentations are shown as pdfs that
Office or LibreOffice makes in the temp folder, so nothing is written beside
the person's file just to look at it.

## 2026-11-12 word presence reported with the capabilities

`export_capabilities` now says whether Word is installed, because the dialog
asked separately and could show "can't be checked here" before that answer
arrived (seen in the real-app proof).

## 2026-11-12 the shell's commands listed per feature

`app/shell/src/lib.rs` `run` had grown to 309 lines because every feature
listed its commands inline. The commands now live in
`app/shell/src/command_routes.rs`, one small function per feature built with
one `feature!` list (which makes both the handler and the names, so they
can't disagree); `run` registers that one handler, and setting up states and
closing a window's parts are their own functions. No command was added,
removed or renamed: a script compared all 221 names before the change.

## 2026-11-12 readability

Split the long test builders (`handbook`, `study_guide_pdf`) into one
function per page, so each fits on a screen.

## 2026-11-12 Convert File…

`convert_file.rs` saves any file Letters reads as another type without
opening it in a window: documents between .docx and .odt (.rtf and .doc
too, as sources), spreadsheets between .xlsx, .ods and .csv, presentations
between .pptx and .odp. Each goes through the same reader and writer the
editors use, so a conversion keeps what saving from the editor keeps, and
says what the new type couldn't (odt_losses, save_losses, odp's notes).
Every office file can also become a PDF through Microsoft Office or
LibreOffice; without one, PDF stays in the list, greyed, saying what it
needs. PDFs and pictures are not converted here: Export To… and New from
Images… have the choices those need (pages, picture quality, text
recognition), so the sheet sends them there. A name typed without an
extension gets the chosen type's, and converting onto the file itself is
refused.
