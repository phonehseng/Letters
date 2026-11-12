# The shell's writer (app/shell-core/src/writer)

Open word processor documents, saving, recovery, and everything the editor
asks of the office engine (core/office) and its Word bridge. Why each piece
changed is in WORKLOG.md.

| File | What it is for |
|---|---|
| `clipboard.rs` | copy and paste between the writer and other apps, through the office engine's `clipboard` and html import |
| `document.rs` | one open word processor document, as the shell keeps it |
| `engine_layout.rs` | Letters' own page layout (`core/layout`), as the second provider of line and page breaks, behind a setting |
| `engine.rs` | the office engine's whole-document helpers, called with the model json the ui holds |
| `export.rs` | "Export as PDF" and printing for word processor documents |
| `generic-page.png` | the plain page picture Recents shows for a word document |
| `layout.rs` | Word's own layout, for Strict Word mode |
| `mod.rs` | the shell's side of word-processor mode (.docx and .odt) |
| `objects.rs` | moving things around the page, through the office engine's `floating` module |
| `proof_documents.rs` | the documents the writer's end-to-end checks open |
| `recovery.rs` | recovery copies |
| `routing.rs` | which screen a file opens in, decided by its name |
| `save.rs` | writing a word processor document to disk, safely |
| `sessions_tests.rs` | tests of every open word processor document (sessions.rs) |
| `sessions.rs` | every word processor document open in any window |
| `settings.rs` | the person's typing settings for the writer |
| `thumbnail.rs` | the picture Recents shows for a word document |
