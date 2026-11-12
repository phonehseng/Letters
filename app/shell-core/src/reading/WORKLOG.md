# reading worklog

## 2026-11-12 positions, bookmarks, speed, goals, versions
One small json file keeps, per document, where the reader stopped, their
bookmarks, their seconds per page (glances under 3 s and breaks over 20 min
ignored, recent pages weighted more) and the writer's word goals, plus the
daily writing log for the streak. None of it changes the document. Page
labels are read from the pdf's /PageLabels number tree for Go To. "Save
Bookmarks into the PDF" appends real outline entries through an edit, so it
can be undone and waits for Save. Versions are copies in the data folder;
restoring keeps the current file as a version first.

## 2026-11-12 records follow moved documents
`move_document` on the store and on versions, called after a move or rename.
