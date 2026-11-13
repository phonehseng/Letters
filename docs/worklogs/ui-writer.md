# Worklog: the writer's interface (app/ui/src/lib/writer)

## 2026-11-12 text blocks in the editor

Added the `text_box` node (schema.ts) and its conversion (convert.ts) for
the engine's `textBox` block, so text blocks are edited as ordinary blocks
instead of kept chips. Round trips are tested on files Word and LibreOffice
made (fixtures/*-text-boxes.json, written through the engine's bridge).
Split into Text Blocks, Make Text Block and Return to Text run in the engine
and come back as one transaction (textBlocks.ts applyEngineModel), so each is
one undo step and untouched blocks stay the same nodes.

## 2026-11-12 free objects placed and wrapped on the page

Free pictures and text blocks are placed from their model position by node
decorations (objectGeometry.ts, objectLayer.ts) instead of from their
paragraph's corner, because the engine's float_image puts them relative to
the page. Text wraps around them through css floats with shape-outside
(wrap.ts); floats were chosen over measured spacers because the browser
re-wraps on every keystroke for free and nothing reflows while dragging.

## 2026-11-12 the object frame: move, resize, crop, turn, snap

ObjectFrame.svelte and ObjectBar.svelte replace ImageFrame.svelte and work for
pictures and text blocks. Dragging previews on the object's own element and
commits once on release (one undo step, steady frame times). Snapping is a
port of the engine's floating::snap (snapping.ts) with its test case because
it runs on every pointer move. Crop and rotation maths are in crop.ts.

## 2026-11-12 clipboard through the engine

clipboardBridge.ts: copying a picture puts png bytes on the system clipboard
(engine clipboard::copy_blocks); pasting pictures, files and html with
pictures goes through clipboard::add_image and import::from_html_with_images.

## 2026-11-12 strict word mode from the engine

strictMode.ts now reads the engine's feature_matrix and word_compatibility
instead of a local list; CompatibilitySheet.svelte shows each issue with Go To
and Fix. wordLines.ts forces Word's line breaks from the bridge's layout;
layoutSource.ts runs the check after a 4 s pause (a cold Word takes ~21 s) and
says when it was last checked.

## 2026-11-12 pages, footnotes, formats, print, tracked formatting, fonts

pagination.ts splits paragraphs between lines with widow/orphan control and
keep-with-next, and footnotes sit on their reference's page. formats.ts reads
the engine's format table. print.ts opens Windows' own print dialog through
the shell. tracked.ts and trackedFormatting.ts record formatting changes and
paragraph splits and joins as Word revisions. The Format tab uses the font
library's FontPicker; the Drive sheet sends the client id to Settings.

## 2026-11-12 nothing moves when you click

Added converter tests that object kinds the engine is adding (shapes, groups)
pass through untouched, and a real-app check (scratchpad still.mjs) that no
block or object box changes when clicking, selecting or toggling panels.

## 2026-11-12 readable files

Split the files that had grown past the size a person can hold: the page
surface's content styles moved to pageContent.css and its measuring to
pageMeasure.ts; WriterScreen's Word and engine checks to layoutCheck.svelte.ts,
Save As, Export and Print to fileActions.ts and the notice bars to
WriterNotices.svelte; the session's table edits to tableEdits.ts and picture
commands to pictureCommands.ts. No behaviour changed. Added an opening
comment to every test file and a README listing the folder.

## 2026-11-12 search hits, moved files and converted documents

neighbours.ts: a search result that opens a Word document lands on its
paragraph with its words in Find (lib/library arrival.ts; FindReplace takes
an initial query), and a document whose file is moved while open follows it
(the "document-moved" event, then writer_follow_move), so Save never writes to
the old place. Documents converted from another file (open_converted, .rtf,
.doc) start as changed (WriterSession.neverSaved) and their first Save asks
where to put the file, because they have no file of their own yet.

## 2026-11-12 goals, focus mode and section counts

writingGoals.svelte.ts puts the long-form goals line in the status bar (the
document's word goal, this session's words and time, the streak), recording
words written once a minute; Focus Mode (View, Ctrl Shift Enter) hides the
ruler and fades the toolbar and status bar; the outline shows each section's
words (lib/longform manuscript.ts).

## 2026-11-12 accessibility fixes from the audit

From docs/ACCESSIBILITY.md: Add Header and Add Footer now #5f5f66 on white
(6.3:1, was 3.4:1); the Strict Word pill uses the label colour on the soft
accent; the status bar's controls and the typing switches are 24 px tall at
least; the writer's stage is the main landmark, so the toolbar, ruler, page
and status bar sit inside one. Not done: sideways scrolling at 200% text.

## 2026-11-12 printing on a mac

A mac's web view ignores window.print(), so Print, Cmd P and Export as PDF
without Word or LibreOffice did nothing there. writer_system_print now opens
the mac's print panel through the web view's print operation (macOS 11 and
later), which also has Save as PDF, and every print path in the writer goes
through openPrintDialog instead of calling window.print() itself. Linux keeps
the web view's own printing, which works there.
