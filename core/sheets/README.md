# letters-sheets

The spreadsheet engine of Letters: it reads `.xlsx` (and `.xlsm`), `.ods`
and delimited text into one shared workbook model, calculates formulas the
way Excel does, and writes the model back losing as little as possible. The
grid UI works on the model through the functions below; this crate never
draws anything.

```
 .xlsx/.xlsm ─read_xlsx─┐                         ┌─write_xlsx─> .xlsx
 .ods ────────read_ods──┼──> Workbook ──Engine────┼─write_ods──> .ods
 .csv/.tsv ───read_csv──┘   (model, JSON)  calc   ├─write_csv / to_html
                              │                   └─print::layout (pages)
              edit::apply ────┤  pure operations returning their undo
              view::cells_in ─┤  what the grid shows
              help::* ────────┘  catalogue, hints, explanations, tips
```

## Files

| Folder | What it holds |
|---|---|
| `src/model` | the workbook model (`Workbook`, `Sheet`, `Cell`, styles, features) and `carry`, what a file keeps for a lossless save |
| `src/xml` | a streaming xml reader (byte positions kept, no tree) and a writer; `Fragment` for partly read elements |
| `src/package.rs` | the zip container with size caps; format detection |
| `src/xlsx` | spreadsheetml reading (`read_*`) and writing (`write_*`), styles, relationships |
| `src/ods` | opendocument reading and writing, styles, lengths |
| `src/formula` | the parser (excel, openformula, r1c1), the printer, reference rewriting |
| `src/calc` | evaluator, engine (dependencies, order, spills, cycles), coercions, criteria, one file per function family in `functions/` |
| `src/numfmt` | number format codes parsed and rendered, General, built-in ids, odf data styles |
| `src/conditional` | conditional formats evaluated for the grid: rule tests, range statistics, colour scales and data bars, differential formats |
| `src/edit` | editing operations with undo |
| `src/csv.rs`, `src/export.rs`, `src/print.rs`, `src/view.rs`, `src/analysis.rs` | csv, html, print pages, viewport, selection statistics |
| `src/help` | the newcomer features: catalogue, autocomplete, hints, explanations, error help, tips, shortcuts, presets, templates, compatibility |
| `tests/` | round trips, engine, edits, ods, large files; `verify_apps.rs` and `apps_roundtrip.rs` drive excel and libreoffice (`--ignored`) |

## The model

`Workbook { sheets, definedNames, styles, dateSystem, calculation, metadata, activeSheet, hasMacros, carry }`,
serde camelCase. Rows and columns are zero based everywhere in the API.

- `Sheet { name, visibility, cells, columns, rows, defaultColumnWidth, defaultRowHeight, view, merged, conditionalFormats, validations, hyperlinks, comments, tables, autoFilter, print, drawings, protected, carry }`
- `cells` is a sparse grid (`CellGrid`): rows in a sorted map, each row a
  short list sorted by column; json `{"row": {"col": cell}}`. A cell is
  `{ value, formula, style, notRecalculated, extra }` (about 100 bytes).
  `value` is `{"type": "number"|"text"|"bool"|"error"|"blank", "value": ...}`.
  `formula` is `{ text, kind }`, text in excel syntax without "=", kind
  `normal` (from files without dynamic arrays: implicit intersection),
  `dynamic` (typed in Letters or excel 365; `spill` is where it spilled),
  `array` (ctrl-shift-enter, fixed range), `shared` (a shared formula group,
  each cell holding its own translated text) or `dataTable`.
- `view { tabColor, zoom, showGridLines, showFormulas, rightToLeft, activeCell, frozen, selected }`
- conditional formats `{ ranges, rules: [{ kind, operator, formulas, format (differential index), priority, stopIfTrue, text, attributes, detailsXml }] }`,
  validations `{ ranges, kind, operator, formula1, formula2, allowBlank, hideDropdown, messages... }`,
  hyperlinks `{ range, target | location, tooltip, display }`, comments
  `{ row, col, author, text }`, tables `{ id, name, range, headerRow, totalsRow, columns, style, banding, filter }`,
  `autoFilter { range, columns: [(offset, values|custom|top|other)], sort }`,
  `print { area, titleRows, titleCols, rowBreaks, colBreaks, orientation, paperSize, scale, fitTo, margins, header, footer, centering, gridLines, headings }`,
  drawings `chart { anchor, chart { kind, title, series [{name, categories, values}], legend }, part } | image | shape`.
- `styles { styles: [CellStyle], differential, themeColors, original }`;
  `CellStyle { numberFormat, font, fill, border, alignment, protection, namedStyle }`,
  each kept once (`StyleTable::intern`, `derive`).
- `definedNames [{ name, formula, sheet (local scope), hidden, comment }]`,
  `dateSystem excel1900|excel1904`, `calculation { automatic, iterate…, fullCalcOnLoad, calcId }`,
  `metadata { title, subject, creator, keywords, description, lastModifiedBy, created, modified }`.

## Public API

| Call | What it does |
|---|---|
| `xlsx::read_xlsx(bytes)`, `xlsx::write_xlsx(&book)` | excel workbooks |
| `ods::read_ods(bytes)`, `ods::write_ods(&book)` | opendocument spreadsheets |
| `csv::read_csv(bytes, &CsvOptions)`, `csv::write_csv(&book, sheet, &CsvWriteOptions)` | delimited text, encoding and delimiter detected |
| `package::detect(bytes)` | `Xlsx`, `Ods` or `Csv`; `.xls`/`.xlsb` and encrypted files get a clear error |
| `calc::recalculate(&mut book)` | every formula once; returns `CalcReport { changed, circular, notRecalculated, calculated, milliseconds }` |
| `calc::Engine::new(&book)`, `engine.recalculate_all(&mut book)`, `engine.cells_changed(&mut book, &[(sheet,row,col)])` | incremental recalculation after edits |
| `edit::apply(&mut book, &Operation) -> Applied { changed { cells, structure }, inverse }` | every edit; applying `inverse` undoes it |
| `edit::find(&book, sheet, text, &FindOptions)` | find (wildcards, case, whole cell, values or formulas) |
| `view::cells_in(&book, sheet, range, &Locale) -> [ViewCell]`, `view::edit_text`, `view::shown` | the grid's cells: text, alignment, colours, flags, merges, and what conditional formats draw (`conditional`, sent only when a rule changes something; empty cells a rule marks come too) |
| `view::own_cells_in(...)`, `conditional::looks_in(&book, sheet, range)` | the cells without conditional formats (copying, filter lists); the conditional looks alone |
| `analysis::StatsCache::new(&sheet).stats(&sheet, &ranges)`, `analysis::selection_stats` | status bar numbers and distribution |
| `export::to_html(&book, sheet, range)`, `print::layout(&sheet)` | html table; pages for printing |
| `numfmt::format_value(&value, code, &Locale, system)` | a value as a format shows it |
| `formula::{parse, parse_r1c1, parse_open_formula, to_string, to_r1c1, to_open_formula}`, `formula::refs::*` | formulas as trees and text |

`Operation` (json `{"op": "insertRows", "sheet": 0, "at": 4, "count": 2}`):
`setCell` (typed input: `=formula`, numbers, `12%`, dates, `'text`), `putCells`,
`clear`, `insertRows`, `deleteRows`, `insertColumns`, `deleteColumns`, `move`,
`copy` (all, values, formats, formulas), `fill`, `sort` (multi-key, header),
`filter`, `clearFilter`, `replace`, `merge`, `unmerge`, `copyFormats`,
`setStyle`, `toTable`, `tableToRange`, `addSheet`, `renameSheet`,
`moveSheet`, `duplicateSheet`, `deleteSheet`, `freeze`, `textToColumns`,
`removeDuplicates`, plus the undo-only `restoreSheet`, `removeSheet`,
`reinsertSheet`, `restoreNames`, `restoreFormulas`.

## Features for newcomers (the grid UI builds on these)

All results are serde camelCase.

| Call | JSON shape |
|---|---|
| `help::catalogue::catalogue()`, `function_info(name)` | `[{ name, category, description, arguments: [{ name, description, optional, repeating, kind: "range"\|"number"\|"text"\|"condition"\|"logical"\|"any" }], examples: [{ formula, result }], mistakes, related: [..], otherFamily }]` — every one of the 197 functions; the examples are calculated by the tests |
| `help::search::complete(typed, limit)` | `[{ name, description, matchedBy: "name"\|"spelling"\|"purpose" }]` — by prefix, loose spelling, or intent ("average", "count if", "look up", "combine text", "today's date") |
| `help::entry::argument_hint(formula, caret)` | `{ function: FunctionInfo, argument }` — the innermost call and the argument the cursor is in |
| `help::entry::reference_spans(formula)` | `[{ start, end, text, colour }]`; `REFERENCE_COLOURS` gives excel's eight colours |
| `help::entry::insert_reference(formula, caret, "A1:B5")` | `(text, caret)` for point-and-click range picking; `expects_reference(formula, caret)` says when a click should insert |
| `help::entry::auto_close(formula)`, `cycle_reference(formula, caret)` (F4), `to_structured(&book, sheet, formula)` | closing brackets and quotes; `A1 → $A$1 → A$1 → $A1`; `B2:B9 → Table1[Price]` (`TableToRange` converts back) |
| `help::explain::explain_formula(text)` | `[{ depth, part, sentence }]`, outermost first: "Looks up cell A2 in the first column of all of columns A:C on sheet Prices and returns the matching value from column 3; exact match only." |
| `help::explain::evaluate_steps(&book, sheet, cell, text)` | the same with `value` per step |
| `help::errors::diagnose(&book, sheet, cell)` | `{ error, meaning, why, fixes: [{ title, operation }] }` — which reference is empty or text, the closest function name for an unknown one, which value a lookup didn't find, which cells block a spill; `circular(cells, &book)` for loops |
| `help::suggest::suggest(&book, sheet, selection, recent, dismissed)` | `[{ id, title, why, preview, operations }]` — the table `TIPS` lists each tip with its rule: add a total, fill down, fill by example, freeze header, turn into a table, sort, remove duplicates, convert text to numbers or dates, exact lookup, name a repeated number, bounded ranges in volatile formulas, IFS for nested IFs, summarise by category |
| `analysis::StatsCache` | `{ count, numericCount, sum, average, min, max, distinct, histogram: [{ from, to, count }] }` |
| `help::shortcuts::shortcuts()`, `keys_for(op, "excelWindows"\|"excelMac"\|"googleSheets")` | `[{ operation, excelWindows, excelMac, googleSheets, description, conflict, checked }]` |
| `help::presets::friendly_formats()`, `style_presets()`, `template(name)` | `[{ name, code, example }]`, `[{ name, style }]`, starter workbooks `budget`, `grades`, `lab`, `schedule`, `invoice` |
| `help::compat::compatibility(&book, Target::Xlsx\|Ods)` | `[{ text, cells }]` — excel 365 functions saved to ods, google sheets functions (QUERY, ARRAYFORMULA, IMPORTRANGE...) and DUMMYFUNCTION wrappers, features a format can't hold |

The shortcut table was compared line by line with microsoft's and google's
shortcut pages on 12 November 2026; 13 rows stay `checked: false` because a
page doesn't name one of their keys (`UNCONFIRMED` in help/shortcuts.rs says why).

## What is modelled, preserved or lost

| Feature | xlsx | ods |
|---|---|---|
| values, formulas, shared and array formulas, dynamic arrays (`cm` metadata) | modelled | modelled (matrix formulas; no dynamic metadata in ods) |
| styles: number format, font, fill, borders, alignment, protection | modelled; the file's own font/fill/border lists kept in place | modelled through automatic styles and data styles |
| merged cells, column widths, row heights, hidden rows and columns | modelled | modelled |
| frozen panes, zoom, tab colour, grid lines | modelled (other view attributes kept) | modelled (settings.xml) |
| conditional formats | modelled (colour scales, data bars, icon sets as kept xml); **drawn** by `conditional::looks_in`: cellIs, expression, text, blanks, errors, top/bottom n and n%, above/below average and standard deviations, duplicate/unique, timePeriod (through its formula), 2 and 3 colour scales, data bars (2007 style, and 2010 style with axis, negative colour and solid fill from the sheet's `extLst`), with priority and stop if true; the differential format's font colour, bold, italic, underline, strike and fill are drawn. **not drawn**: icon sets, a rule's number format and borders, rules that exist only in `extLst` (x14) | kept as xml from ods files; **not written when converting from xlsx** |
| data validations | modelled | list, whole, decimal, date, time, text length modelled |
| hyperlinks, notes | modelled (notes' vml regenerated when they change) | modelled |
| tables | modelled with structured references | **written as plain ranges** (letters still resolves the references) |
| autofilter and sort state | modelled | kept as xml from ods |
| print area, titles, breaks, page setup, header and footer | modelled | page settings from ods styles kept |
| defined names | modelled | modelled |
| charts | summary read (kind, title, series, legend); the parts are carried byte for byte | embedded objects carried |
| pictures, shapes, pivot tables, slicers, external links, custom xml, theme | carried byte for byte | carried |
| macros (`.xlsm`) | carried byte for byte, **never run** | lost when converting to ods |
| calculation chain | dropped (excel rebuilds it; a stale one makes excel repair the file) | n/a |
| row `spans` and `x14ac:dyDescent` | dropped (excel recomputes them) | n/a |

Rich text runs inside cells are kept while the text is unchanged. Charts
made in Letters (the lab template) live in the model but are not written
yet. Edits do not move carried drawings' anchors inside the drawing xml.

## Calculation

IEEE doubles with excel's display (15 significant digits, General in about
11 characters), dates as serials with the 1900 leap-year quirk and the
1904 system, excel's coercions and error order, array broadcasting and
spilling (#SPILL! re-checked when the blocking cell is cleared), implicit
intersection for formulas from older files, excel's snapping of a last
addition or subtraction that leaves only rounding noise. Circular
references are reported and show 0 (iterative calculation is not
supported). A formula from a file that uses a function Letters lacks keeps
its stored value and is listed in `notRecalculated`; a newly typed one
shows #NAME?. Volatile functions: RAND, RANDBETWEEN, NOW, TODAY, OFFSET,
INDIRECT.

Functions (197): math (SUM, PRODUCT, SUMSQ, SUMPRODUCT, ROUND/UP/DOWN,
MROUND, INT, TRUNC, MOD, ABS, SIGN, SQRT, SQRTPI, POWER, EXP, LN, LOG, LOG10,
PI, trig and hyperbolic, DEGREES, RADIANS, CEILING(.MATH), FLOOR(.MATH),
EVEN, ODD, FACT, COMBIN, PERMUT, GCD, LCM, QUOTIENT, RAND, RANDBETWEEN),
statistics (AVERAGE(A), COUNT, COUNTA, COUNTBLANK, MIN(A), MAX(A), MEDIAN,
MODE(.SNGL), STDEV(.S/.P/P), VAR(.S/.P/P), RANK(.EQ/.AVG),
PERCENTILE(.INC/.EXC), QUARTILE(.INC/.EXC), CORREL, PEARSON, LARGE, SMALL,
GEOMEAN, AVEDEV, DEVSQ), conditional (COUNTIF(S), SUMIF(S), AVERAGEIF(S),
MAXIFS, MINIFS), logical (IF, IFS, AND, OR, XOR, NOT, IFERROR, IFNA, SWITCH,
TRUE, FALSE), lookup and references (VLOOKUP, HLOOKUP, XLOOKUP, MATCH,
XMATCH, LOOKUP, INDEX, CHOOSE, OFFSET, INDIRECT, ROW(S), COLUMN(S),
TRANSPOSE), dynamic arrays (FILTER, SORT, UNIQUE, SEQUENCE), text (CONCAT,
CONCATENATE, TEXTJOIN, LEFT, RIGHT, MID, LEN, FIND, SEARCH, SUBSTITUTE,
REPLACE, UPPER, LOWER, PROPER, TRIM, CLEAN, TEXT, VALUE, NUMBERVALUE, EXACT,
REPT, CHAR, UNICHAR, CODE, UNICODE, T, FIXED, DOLLAR), date and time (TODAY,
NOW, DATE, TIME, YEAR, MONTH, DAY, HOUR, MINUTE, SECOND, WEEKDAY, WEEKNUM,
ISOWEEKNUM, EDATE, EOMONTH, DATEDIF, NETWORKDAYS, WORKDAY, DAYS, DAYS360,
YEARFRAC, DATEVALUE, TIMEVALUE), financial (PMT, IPMT, PPMT, PV, FV, NPER,
RATE, NPV, IRR), information (IS functions, NA, TYPE, N, ERROR.TYPE).
Not yet: LET, LAMBDA, SORTBY, TEXTSPLIT and the other newer array
functions, statistical distributions, database functions, CELL, INFO.

## Agreement with Excel and LibreOffice

`tests/verify_apps.rs` writes 4,484 formula cases (every function family
over awkward inputs, and every operator over every pair of value kinds)
and 690 number format cases; Microsoft Excel 16 (a private hidden
instance) and LibreOffice Calc (headless, recalculating on load) compute
them. Last run, 12 November 2026:

| Family | Cases | Same as Excel | Same as LibreOffice | Excel and LibreOffice differ |
|---|---|---|---|---|
| conditional | 183 | 183 (100.0%) | 172 (94.0%) | 11 |
| date and time | 329 | 329 (100.0%) | 251 (76.3%) | 78 |
| dynamic arrays | 21 | 21 (100.0%) | 20 (95.2%) | 1 |
| financial | 24 | 23 (95.8%) | 20 (83.3%) | 4 |
| information | 228 | 228 (100.0%) | 224 (98.2%) | 4 |
| logical | 156 | 156 (100.0%) | 115 (73.7%) | 41 |
| lookup | 134 | 134 (100.0%) | 124 (92.5%) | 10 |
| math | 929 | 929 (100.0%) | 877 (94.4%) | 52 |
| operators | 1,831 | 1,831 (100.0%) | 1,244 (67.9%) | 587 |
| statistics | 220 | 220 (100.0%) | 174 (79.1%) | 46 |
| text | 429 | 429 (100.0%) | 372 (86.7%) | 57 |
| **all formulas** | 4,484 | 4,483 (100.0%) | 3,593 (80.1%) | 891 |
| number formats (shown text) | 690 | 688 (99.7%) | 526 (76.2%) | 165 |

Letters follows Excel where the two differ. The remaining disagreements:

| Formula | Letters | Excel | LibreOffice |
|---|---|---|---|
| `RATE(10,-100,1000,0,1)` (true answer 0) | 1.6E-15 | -1.49E-09 | 4.2E-14 |
| `0` as `##0.0E+0` | 0.0E+0 | 000.0E+0 | 0.0E+0 |
| accounting format zero (`_(* "-"??_)`) | ` -   ` | the dash padded to the cell width | figure spaces |

Where Excel and LibreOffice themselves differ (891 cases), the main
groups are: text in arithmetic (`"1,000"+1`, `"50%"*2`, `"2024-03-01"+0`:
excel reads them as numbers, libreoffice by default gives #VALUE!), error
order (`"abc"+1/0`: excel #VALUE!, libreoffice #DIV/0!), typed text in
AND/OR (excel skips it, libreoffice #VALUE!), day zero (excel 1900-01-00,
libreoffice 1899-12-30), rounding of date parts to the second, DATEDIF,
SIN of huge angles (#NUM! vs #VALUE!), number to text beyond 15 digits
("1E+019" vs "10000000000000000000"), fraction display (excel's
continued-fraction convergents, `3 1/7`, against libreoffice's best
fraction, `3 14/99`), and libreoffice's `#FMT` for dates past 9999.

Round trips (`tests/apps_roundtrip.rs`): a file Letters writes opens in
Excel without repair; Excel saves it with a chart and a shape added; after
Letters reads it, changes one cell and writes it, Excel reports the same
sheets, names, merged ranges, validations, conditional formats, shapes,
charts, notes, tables and links as before. The same holds for files
LibreOffice wrote, as xlsx and as ods, reopened in LibreOffice. ODS
written by Letters is read by LibreOffice with values and features intact
(conditional formats aside, see above).

## Performance

`tests/large.rs`, release build, this machine (one run, 12 November 2026):
200,000 rows by 5 columns, 1,000,000 cells of which 400,000 formulas,
plus SUM, COUNTIF, VLOOKUP and SUMIFS over whole columns.

| Step | Time |
|---|---|
| read the 6.5 MB xlsx (streaming) | 0.8 s |
| parse every formula and index dependencies | 1.4 s |
| full recalculation | 1.0 – 1.3 s |
| write xlsx | 1.4 s |
| recalculation after one edit (6 formulas, 4 of them over whole columns) | 0.19 s |
| recalculation after an edit only a lookup reads | 0.03 s |
| viewport of 50 rows by 10 columns | under 1 ms |
| selection statistics cache / A1:A200000 / A1000:A2000 | 0.11 s / 0.06 s / under 1 ms |

Memory: about 190 MB for the open workbook (roughly 100 bytes per cell plus
strings) and about 310 MB more for the engine (0.8 kB per formula); peak
under 600 MB. The engine keeps every parsed formula; sharing trees between
formulas that differ only by their row (fill-down) would cut that most.

## Tests

```
set CARGO_TARGET_DIR=C:\Letters\target-sheets
cargo test -p letters-sheets      # 141 tests; 6 more are ignored: they need excel, libreoffice or time
cargo test -p letters-sheets --release --test verify_apps -- --ignored --nocapture --test-threads=1
cargo test -p letters-sheets --release --test apps_roundtrip -- --ignored --nocapture --test-threads=1
cargo test -p letters-sheets --release --test large -- --ignored --nocapture
cargo clippy -p letters-sheets --all-targets
cargo fmt -p letters-sheets
```

Unit tests per module; round trips (every modelled feature, second save
identical, unknown parts and elements kept byte for byte after an edit),
broken and truncated packages (no panics), the engine (order, chains of
50,000 cells, cycles, unknown functions, spills, names, tables, legacy
formulas), edits (each undo restores the workbook exactly; property tests
of reference rewriting), ods and conversions, every catalogue example
calculated, every tip with cases where it must and must not fire.

The excel driver (`tests/tools/excel_check.ps1`) runs only if Excel is
registered; it starts its own hidden instance, identifies it by the process
owning its window, opens files read-only with macros disabled, and stops
exactly that process (a watchdog ends it if the script hangs). LibreOffice
runs headless with a private profile under `target-sheets/harness`.

## Libraries

| Library | Used for | License |
|---|---|---|
| [zip](https://crates.io/crates/zip), flate2, zlib-rs | the package container | MIT or Apache-2.0; Zlib |
| [serde](https://serde.rs), serde_json | the model as JSON | MIT or Apache-2.0 |
| [chrono](https://crates.io/crates/chrono) | the local date and time for TODAY and NOW | MIT or Apache-2.0 |
| [thiserror](https://crates.io/crates/thiserror) | error messages | MIT or Apache-2.0 |

XML is read and written by the crate's own small streaming reader. Nothing
here restricts distribution.
