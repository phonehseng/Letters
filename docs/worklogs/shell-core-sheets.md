# Worklog: spreadsheets (shell-core)

## 2026-11-12 open workbooks, edits, undo, saving and recovery

Started `src/sheets` for the spreadsheet screen. Unlike the writer, which
sends its whole model to the ui, the shell keeps each workbook and the ui
asks only for the cells it draws (`look.rs`): a 200,000-row sheet is about
190 MB in the engine and must never cross into the web view. Each workbook
has its own lock, so saving one never stops another window scrolling.

Every edit goes through `history.rs`. An edit is one of the engine's
operations or a size change; the engine has no operation for column widths
and row heights, so `layout.rs` sets exact states and returns the states it
replaced as its undo. One action (a paste of 300 cells, a sort) is one undo
step, as in Excel. If an action fails halfway, the edits before it are taken
back so the workbook is never half changed.

After an edit only the formulas depending on the changed cells are
calculated again (`Engine::cells_changed`); edits that move rows, columns or
sheets rebuild the engine, because every reference may have moved. Filters,
freezing and sizes calculate nothing. Opening calculates the whole workbook
once, so starter workbooks (which have no stored values) and files from
other programs show this engine's values from the start.

Formatting a selection keeps each cell's own style and changes one thing
(`formats.rs`), because the engine's `SetStyle` gives a range one style and
would wipe a bold cell's colour when only the number format changes. Whole
columns stop at the last used row so formatting column A doesn't make a
million cells.

Saving as csv keeps one sheet; for a workbook with more sheets the window
stays "edited" after the save, so closing still asks before the other
sheets are lost. An .xlsm saved under an .xlsx name is refused, because the
package keeps its macro content type and Excel refuses such a file.

Recovery copies are the workbook written as the engine saves it (xlsx, or
ods for ods files), with a small note naming the original; untitled
workbooks have no copy yet.

Measured with the ignored test in `sessions_tests.rs` (200,000 rows by five
columns, one formula per row, debug build): unoptimised the engine took
7.1 s to open the file and 4.2 s to insert a row, so the workspace's
Cargo.toml now builds letters-sheets with opt-level 2 in debug too: 1.7 s
and 1.4 s. Typing into a cell and recalculating took 0.25 ms, 61 rows of
cells for the grid 0.24 ms, the status bar's numbers for a whole column
167 ms the first time after an edit.

## 2026-11-12 the shell's commands, and cursors counted as javascript does

The commands (app/shell/src/sheets_commands.rs and sheets_help_commands.rs)
are thin doors over this module and the engine's help. `entry.rs` wraps the
engine's formula-typing helpers because they count the cursor in bytes (and
the argument hint in characters) while the web view counts utf-16 units: a
formula with an accent or an emoji before the cursor would otherwise put the
engine's cursor inside a letter and panic.
