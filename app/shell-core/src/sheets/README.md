# The shell's spreadsheets (app/shell-core/src/sheets)

Open workbooks for the spreadsheet screen, their edits, undo and saving,
on top of the engine in core/sheets. Why each piece is as it is: see
docs/worklogs/shell-core-sheets.md.

| File | What it is for |
|---|---|
| `document.rs` | one open workbook: the model, its calculation engine, undo and redo |
| `entry.rs` | the engine's formula-typing helpers (argument hints, reference colours, F4), with utf-16 cursors |
| `files.rs` | reading each format, writing it safely, what csv and ods leave out |
| `formats.rs` | number formats, bold, colours and style presets on a selection, as one edit |
| `history.rs` | an edit (an engine operation or a size change), undo steps |
| `layout.rs` | column widths, row heights, hidden rows and columns |
| `look.rs` | what the grid draws: tabs, a sheet's sizes, styles, the cells on screen, copied text |
| `mod.rs` | the module list |
| `paste.rs` | tab-separated text from other apps, typed into cells |
| `recovery.rs` | recovery copies offered after a crash |
| `sessions.rs` | every open workbook: opening, new and starter workbooks, saving, recovery |
| `sessions_tests.rs` | tests of `sessions.rs` through files on disk, and the 200,000-row timing |
| `tips.rs` | the tips a person dismissed, kept between runs |
