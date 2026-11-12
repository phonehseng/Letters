# Worklog: comparing in the shell (app/shell-core/src/compare)

## 2026-11-11 the compare job

Added `job.rs` (one comparison on its own thread with progress, cancel,
overlay pictures, the report, and `close`), `jobs.rs` (every job by window,
folders under the cache emptied at start) and `renderer.rs` (whole pages
stitched from the worker's tiles). Why both lopdf and the worker open each
file: the engine reads text with lopdf, and the window draws pages with the
worker's tiles under ids of their own. Why lopdf first: a locked file then
gets the engine's own sentence and nothing is left open in the worker.

## 2026-11-11 tests against pdfium

`tests.rs`: progress after every page (9 reports for 3 pages a side), a
changed word, identical documents written differently, stopping, closing
leaves no document open and no folder, a locked file, stitched tiles, pair
pictures and a pdf report the engine can open. `test_files.rs` writes the pdfs
byte by byte so the tests don't depend on a pdf writer being right.

## 2026-11-12 clippy

`decode_png` reads rgb pixels with `as_chunks`, as clippy asks for a constant
chunk size.
