# Worklog: the shell's writer (app/shell-core/src/writer)

## 2026-11-12 word only through the engine's bridge

export.rs and layout.rs now call letters_office::bridge (print_pdf,
word_layout) and the writer's own word_export.ps1 and word_layout.ps1 are
gone, because the first round's scripts leaked hidden Word processes and the
bridge finds its own Word by a unique window caption and has a watchdog.
layout.rs traces each of Word's lines to a model block and character offset
so the editor can break lines where Word does; layouts are cached by file
fingerprint (a cold Word took ~21 s, a cached answer 18 ms).

## 2026-11-12 objects, clipboard, formats

objects.rs runs the engine's floating and compat functions on the merged
document (text blocks, in line or free, stacking, aligning, Word fixes) and
answers with the whole model so the editor can replace only changed blocks.
clipboard.rs wraps clipboard::copy_blocks, add_image and
from_html_with_images; pasted pictures join the document's media under ids
that cannot clash. sessions.rs saves copies in every save_as format
(markdown with its pictures beside it) and opens .rtf and .doc by conversion
into a .docx beside the original, so the original is never overwritten.

## 2026-11-12 letters' own layout as the second provider

engine_layout.rs gives core/layout's lines and pages in the same shape as
Word's, behind the layoutEngine setting. On the 16-page proof essay it broke
all 580 lines and 16 pages where Word did.

## 2026-11-12 unchanged on open and save

Added a test that opening and saving without edits keeps blocks, styles and
sections as the engine read them, text blocks and preserved content included.

## 2026-11-12 readable files

Moved the session tests to sessions_tests.rs and the end-to-end documents
to proof_documents.rs (small named functions instead of one long test), so
sessions.rs and objects.rs stay readable. Added a README listing the folder.

## 2026-11-12 following a moved file

WriterSessions::follow_move points an open document at its file's new place,
so Save writes there after organising moves or renames it.
