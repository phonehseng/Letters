# library_search worklog

## 2026-11-12 the indexing job
One background thread keeps the index matching the chosen files: recents,
library folders and the pinned folders the person opted in, less excluded
folders. It re-reads a file only when size or time changed and its checksum
too, rests after every file (longer after a long one) so the pdf engine
stays free for the windows, and checks for pause between pages so pausing a
500 page file is immediate. Locked files are indexed by name only; no
password is ever asked for or stored. Settings live in library-search.json.

## 2026-11-12 moves follow without re-reading
`document_moved` re-points the index entry when Letters moves a file.

## 2026-11-12 tests in their own file
job.rs was 793 lines; its tests moved to job_tests.rs.
