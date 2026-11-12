# Worklog: the spreadsheet screen (ui)

## 2026-11-12 the sheets screen

.xlsx, .xlsm, .ods and .csv now open in a screen of their own instead of a
PDF made by Office; Office's PDF stays only for files the engine can't read
(an .xls, a damaged file), with a sentence saying why.

The grid is a canvas inside a scroll area as large as the sheet: only the
rows and columns on screen are drawn, and their cells are fetched from the
shell in blocks of 64 rows by 32 columns. Positions come from the default
size plus the few lines that differ, so finding row 150,000 never adds up
150,000 heights. Measured in the debug app on 200,000 rows by five columns:
a paint takes 0.6 ms (median), 1.1 ms (95th percentile) and 4.2 ms at most
over 433 paints while the wheel scrolled 24,000 rows, with no long tasks.

Typing follows Excel: typing over a cell is "enter" mode, where the arrows
finish the entry or, while a formula waits for a reference, point at cells;
F2 or a double-click is "edit" mode. Autocomplete, the argument hint,
reference colours and F4 come from the engine's help on every key. The keys
are Excel's (Windows and Mac) and Google Sheets', from the table in
core/sheets, which was compared with both companies' published lists; keys
the grid handles never reach the window's own shortcuts (ctrl r is fill
right here, not rotate).

Copying puts tab-separated text and an html table on the clipboard. A paste
of Letters' own copy is a copy of cells, so formulas move as in Excel; a
paste from elsewhere reads the html table first (it keeps cells with line
breaks apart) and types each field in.

Tips are offered 1.5 s after the selection settles, one at a time, never
while typing, and not again for 45 s after one is answered; Not Now and
Don't Show Again are remembered by the shell.

Charts, pictures and shapes are shown as dashed placeholders with their
title and saved exactly as they came; editing them is not part of Letters.

Found in the real app and fixed: the toolbar's menus opened under the
formula bar (the toolbar now stacks above it), and the keys stayed on a
menu button after a menu command (they go back to the grid now).