# Worklog: the Scan tab and New from Images (lib/ocr)

What changed in this folder and why, newest last.

## 2026-11-11 Scan tab: recognize, show, correct, remove

Added the Scan tab's interface on top of core/ocr. A run happens in the
shell on its own thread, so the session (`scanSession.svelte.ts`) asks how
it is doing about three times a second instead of waiting on one long
call, which keeps the document usable. The run's phases are a pure state
machine (`ocrJob.ts`) so a cancel that lands as the last page finishes is
tested: the pages are then dropped, because the person asked to stop. A
finished run is added through the viewer's change queue as one edit, so
one Undo takes every page back. Languages default to the computer's own
and stop at two (`languages.ts`), because the engine README measured that
mixing languages costs accuracy. The banner (`ScanBanner.svelte`) only
counts scanned pages nobody has read, so it never returns for them.

## 2026-11-11 New from Images

A sheet (`NewFromImagesSheet.svelte`) puts pictures in order and makes a
pdf in the shell, which opens unsaved. `newDocuments.ts` remembers such
documents so Save asks where, starting beside the first picture, since
the pdf has no file of its own yet.

## 2026-11-12 corrections shown without asking again

A corrected word's overlay is kept for the revision the correction made
and set when the viewer reaches that revision; setting it straight away
was wiped by the revision change that followed.
