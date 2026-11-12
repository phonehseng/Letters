# letters-sheets worklog

## 2026-11-12 crate skeleton, model and xml reader

Started the spreadsheet engine. Added the shared workbook model
(`src/model`): sheets with a sparse cell grid (rows in a sorted map, each row
a short sorted list, so a 200,000-row sheet costs only its cells), styles kept
once in a table, and every sheet feature excel and libreoffice files carry
(merges, conditional formats, validations, links, notes, tables, filters,
print setup, drawings). The `carry` keeps what the model does not read, so a
save loses nothing. Added a streaming xml reader (`src/xml`) because sheets
can be far larger than word documents and need no tree in memory, and the
zip package reader with size caps copied in spirit from letters-office.

## 2026-11-12 formula parser, printer and reference rewriting

Added `src/formula`: a parser for excel syntax, openformula (ods) syntax and
r1c1 references; a printer back to text (plain, with the `_xlfn.` prefixes
xlsx files need, and openformula); and reference rewriting for copy/fill,
row and column insertion and deletion, sheet rename and removal. Nesting is
capped at 64 levels, excel's own limit, which also keeps the recursive parser
inside the stack (a 500-level formula overflowed a test thread at 200).
Property tests check that inserting then deleting the same rows restores every
reference.

## 2026-11-12 parser split into readable files

Split the formula parser into operator levels, literals, references and table
references, following docs/CONTRIBUTING.md (files under about 500 lines).
Fixed two reading bugs found by tests: `TRUE()` in ods formulas and external
workbook references written without quotes (`[1]Sheet1!A1`).

## 2026-11-12 xlsx reading and writing with preservation

Added `src/xlsx`. Worksheets are read in one streaming pass (`read_cells`):
shared formulas are expanded to each cell's own text, rich text keeps its
original runs, `_xlfn.` prefixes are removed for the model and put back on
save. Every worksheet and workbook child the model does not read is kept as
its original xml and written back in schema order; children it does read
keep their original xml too, with a fingerprint of what the writer made of
them on reading, so an untouched element is written byte for byte (this is
what keeps a macro workbook's `codeName` and unknown view attributes).
Unknown parts (pivot tables, slicers, vba, drawings, charts) are carried
with their content types and relationships. The calculation chain is
dropped on save because a stale one makes excel repair the file. Styles keep
the file's own font, fill and border lists in place because named styles
refer to them by position. Dynamic array formulas are written with excel's
`cm="1"` cell metadata and a metadata part.

## 2026-11-12 calculation engine and function library

Added `src/calc`: an evaluator with excel's coercions (text that reads as a
number counts in arithmetic, comparisons treat numbers equal to 15
significant digits as equal, blank is 0, "" or FALSE depending on the other
side), array broadcasting, implicit intersection for formulas from files
without dynamic arrays, spilling with #SPILL! and re-spilling once the
blocking cell is cleared, and excel's snapping of a final addition that
leaves only rounding noise. The engine indexes which formulas read which
cells (per cell for small references, per column interval for long ones) so
an edit recalculates only its dependents; formulas are calculated after
their precedents by a depth-first walk with an explicit stack, so a chain of
50,000 cells is safe. Circular references are reported and show 0, as in
excel. A formula using a function letters lacks keeps its stored value, is
flagged `notRecalculated` and listed in the report. Ten function families,
about 200 functions, one file each.

## 2026-11-12 number formats

Added `src/numfmt`: format codes parsed into sections (colours, conditions,
currency locale tags, elapsed times, sub-seconds) and rendered: digit
placeholders filled from the right (so "000-0000" phone formats work),
thousands and scaling commas, percent, scientific and engineering
notation, fractions by a farey search, dates and times with excel's
minute/month rule, and excel's width-limited General (11 characters,
whichever of fixed or scientific keeps more digits).

## 2026-11-12 verification harness against excel and libreoffice

Added `tests/verify_apps.rs` with `tests/harness` and
`tests/tools/excel_check.ps1`. About 4,500 formula cases across the function
families and every pair of operand kinds, and 690 number format cases, are
written into workbooks; a private hidden excel (found by the process owning
its window, stopped by that id, with a watchdog) recalculates and reports
values and shown text; libreoffice runs headless with its own profile set
to recalculate on load. Fixes made from the first run: error order in
operators (excel reads the left side first), AND/OR skip typed text, MODE
skips typed text, XLOOKUP's next-smaller compares across kinds, SIN/COS/TAN
refuse angles from 2^27, day zero is 1900-01-00, DATE(1900,2,29) is 60, date
parts round to the second, DATEDIF "MD" uses excel's arithmetic, IPMT signs,
TEXT with "@" and negative dates, number to text uses at most 20
characters before switching to an exponent, numbers in formulas keep 15
significant digits, fractions use continued fraction convergents as excel
does. Result: 99.8% of formula cases match excel.

## 2026-11-12 editing operations

Added `src/edit`: every edit is an `Operation` applied by `apply`, which
returns the changed cells and the inverse operations; tests apply each
inverse and compare the whole workbook with the one before. Insert and
delete of rows and columns rewrite formulas on every sheet, names,
conditional format and validation formulas and every range-holding feature;
copy shifts relative references, a move makes references follow, fill
continues series (least squares for numbers, calendar steps for dates, day
and month names, numbered text), sort moves rows with their formulas, plus
filter, find and replace, merge, format painter, tables to and from ranges
(structured references become plain ones), sheet operations, freeze panes,
text to columns and remove duplicates.

## 2026-11-12 ods, csv, html, print layout and viewport

Added `src/ods` (streaming table reader with repeat caps so a row repeated
a million times costs nothing, styles and data styles both ways through
`numfmt/odf.rs`, named ranges, validations, notes, links, frozen panes
from settings.xml, kept parts and unknown table elements), `src/csv.rs`
(encoding and delimiter detection, fields starting with "=" stay text so a
csv never runs formulas, leading zeros kept), `src/export.rs` (html table),
`src/print.rs` (pages with paper sizes, margins, scaling and repeated
titles) and `src/view.rs` (`cells_in` for the grid).

## 2026-11-12 performance and round trips through the applications

`tests/large.rs` measures a 200,000-row workbook (1,000,000 cells, 400,000
formulas). Changes it led to: references and table references are boxed in
the formula tree (the engine went from about 1 kB to 0.8 kB per formula),
lookups and conditional functions borrow cell values instead of copying
them, wildcard matching and text comparison skip copying when there is no
wildcard or the text is ascii, and the used range is worked out once per
calculation. `tests/apps_roundtrip.rs` has excel save a letters file with a
chart and a shape added, edits it in letters and checks excel sees the same
merges, validations, conditional formats, shapes, charts, notes and links
afterwards; the same for files libreoffice wrote, as xlsx and as ods.

## 2026-11-12 features for newcomers

Added `src/help` and `src/analysis.rs` for the grid ui: a catalogue of all
197 functions (plain descriptions, arguments, two worked examples each
checked by calculating it, common mistakes, related functions, notes on
other families), autocomplete by name, spelling and purpose, argument hints,
range picking, closing brackets, reference colours, F4 cycling, references
to table references, plain-sentence explanations and step-by-step values,
error help with one-click fixes, fourteen tips in one table with their rules
(each tested for firing and for not firing; flash fill only offers a rule
when every fitting rule agrees), selection statistics from running totals
and sparse tables, shortcut sets for excel windows, excel mac and google
sheets (not yet checked line by line against the vendors' pages, marked so
in the data), friendly formats, style presets, five starter workbooks and
compatibility notes. A typed formula with an unknown function now shows
#NAME? as in excel; one from a file keeps its stored value.

## 2026-11-12 readability pass

Ran tools/check-readability.py and split the files it listed: reference
functions out of lookup.rs, reference text out of the formula printer, the
workbook part and the worksheet parts out of the xlsx reader and writer,
the tip detectors into data and formula groups, the starter workbooks into
one function each, and the harness cases in two. The long functions left
are `match` dispatchers with one short arm per function or operation, and
test data builders. clippy is clean.

## 2026-11-12 the shortcut table compared with the published lists

Every row of help/shortcuts.rs was compared with Microsoft's "Keyboard
shortcuts in Excel" (Windows and macOS tables) and Google's "Keyboard
shortcuts for Google Sheets" (PC list). Corrected: Excel for Mac edits a
cell with F2, enters the date with Ctrl ; and the time with Cmd ;, starts a
line with Ctrl Option Return, inserts cells with Ctrl Shift =, hides rows and
columns with Ctrl 9 and Ctrl 0 and finds with Ctrl F; Excel for Windows
inserts cells with Ctrl Shift +; Google Sheets changes sheet with Alt Down
and Alt Up and shows formulas with Ctrl ~. The claim that Ctrl U edits a
cell in Excel for Mac was not on the page and is gone. Rows a page doesn't
confirm in full stay unchecked, with the reason beside the list.
