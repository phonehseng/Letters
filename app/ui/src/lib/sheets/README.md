# The spreadsheet screen (app/ui/src/lib/sheets)

The screen that opens .xlsx, .xlsm, .ods and .csv files. The shell keeps
the workbook (app/shell-core/src/sheets); this folder shows the cells on
screen and sends every change as the engine's edits. Why each piece is as
it is: docs/worklogs/ui-sheets.md.

| File | What it is for |
|---|---|
| `SheetsScreen.svelte` | the screen: toolbar, formula bar, grid, status bar, tips, Explain, menus |
| `SheetGrid.svelte` | the canvas grid inside a scroll area as large as the sheet; pointer, fill handle, resizing |
| `CellEditorField.svelte` | the text field laid over the cell being typed into |
| `FormulaBar.svelte` | the name box, fx, and the active cell's contents |
| `FormulaPopups.svelte` | autocomplete and the argument hint under a cell or the formula bar |
| `SheetsToolbar.svelte` | the floating toolbar with its four menus |
| `SheetsStatusBar.svelte` | the sheet tabs' strip with average, count, sum, circular references and zoom |
| `SheetTabs.svelte` | the sheet tabs: show, add, rename, move, duplicate, delete |
| `SheetsExplain.svelte` | Explain: the active cell's formula step by step, error help, tools and shortcuts |
| `SheetsFind.svelte` | find and replace |
| `FilterMenu.svelte` | a filter button's menu |
| `TipCard.svelte` | one tip at the moment it helps |
| `NewSpreadsheetSheet.svelte` | Recents' New Spreadsheet…: blank and starter workbooks |
| `sheetsApi.ts` | every call into the shell, with the answers' shapes |
| `sheetsSession.svelte.ts` | the workbook's name, file and state; changes sent one at a time; saving |
| `sheetView.svelte.ts` | the sheet on show: sizes, zoom, styles, fetched cells, selection |
| `cellCache.ts` | the fetched cells, in blocks |
| `cellEditor.svelte.ts` | typing into a cell: modes, autocomplete, hints, pointing, F4 |
| `gridActions.ts` | moving, typing, filling and the clipboard |
| `sheetActions.ts` | formats, rows and columns, sort, filter, freeze, autosum, tabs |
| `screenKeys.ts` | what each key does, in the grid and while typing |
| `gridKeys.ts` | which key is which action, as Excel and Google Sheets have them |
| `gridHit.ts` | what a press on the canvas lands on |
| `gridLayout.ts` | rows and columns to pixels and back, with frozen panes |
| `geometry.ts` | row and column sizes, cell names |
| `painter.ts` | drawing the cells |
| `overlays.ts` | drawing the selection, references, placeholders and headers |
| `selection.ts` | selection rules |
| `formulaEntry.ts` | formula-typing rules that run on every key |
| `clipboard.ts` | html tables from other apps, repeating a paste |
| `sheetsMenus.ts` | the toolbar menus' items |
| `sheetsTips.svelte.ts` | when a tip is offered |
| `sheetsTools.ts` | the plain-English words for every tool |
| `sheetsCommands.ts` | the new-spreadsheet command and the starter workbooks |
| `grid.test.ts`, `entry.test.ts` | tests of the rules above |