# letters-layout worklog

Newest entry last. Each entry says what changed and why.

## 2026-11-12 round one recorded

The first model (30 documents, scores in ORACLE_REPORT.md) was committed
before this worklog existed. Its rules and their measurements are in
ORACLE_REPORT.md ("Rules fitted on the measurements") and next to each rule
in the code.

## 2026-11-12 a wider, more honest corpus

The round-one corpus was 27 documents written by our own docx writer plus 3
Word files, all used for fitting, so a high score could mean overfitting.
Added tests/oracle/corpus2.rs: 58 documents from their own seeds (essays in
eight fonts and sizes, mixed formatting, justification probes, syllabi,
lab reports, nested lists, footnote-heavy papers, two and three columns,
landscape sections, first-page headers, page borders, long tables with
merged cells and repeated headers, fixed, percent and automatic widths, row
heights, cell margins, keeps, floats). Features our writer has no field for
are written by patching document.xml after writing (`corpus2::Patch`),
which needs `zip` as a dev-dependency (MIT).

Added documents made by the applications themselves, because files people
really have carry Word's and LibreOffice's own styles and settings:
author_word.py (16 documents through Word's object model from its normal
template: Aptos defaults, older Calibri defaults set on the document only,
Times MLA) and author_lo.py (12 ODT files through LibreOffice's UNO bridge
from its default template). Both run private instances and end them.

Every document is in a fit or a held-out set, alternating within each
class, so the report shows whether rules generalise.

## 2026-11-12 more measures in every score

Scores now also count documents with the same page count, keep every value
so p95 is exact over a set (it used to be the largest p95 of the parts),
compare vertical rules (table column edges, page borders, column
separators) and picture positions read from each pdf's page objects, and
render at 150 dpi (was 300; 150 is what the brief asked for and is four
times faster). The report shows fit, held-out and whole-corpus headline
tables, per class tables with the other application as the baseline to
beat in every row, and the open-fonts-only comparison.

## 2026-11-12 libreoffice conversions in batches

Starting LibreOffice for each file cost about 11 s a file. run_oracles.py
now converts up to 16 files per process (a docx and an odt of the same
name never share a batch, since both would be written as name.pdf first):
about 1.3 s a file.

## 2026-11-12 the font library behind FontSource

Added fonts/library.rs: `LibraryFonts` implements `FontSource` over
letters-fonts' index. It finds faces by typographic family and weight (so
the legacy name "Calibri Light" is Calibri at 300), offers the library's
metric twins first (Carlito, Arimo, Tinos, Cousine), then look-alikes as a
new `SubstitutionReason::Similar` (Caladea for Cambria: the font library
measured it 2.5 % of the em off, so it is not metric-compatible and was
removed from our metric table), and follows the library's fallback chains.
Variable fonts (Arimo ships as one) are loaded at the asked weight
(`LoadedFace::load_instance`, hvar advances); their pdf glyphs are stroked
because the file's default outlines are the regular ones.

## 2026-11-12 incremental relayout stops early

The engine used to place every page again after any edit. Placing now
resumes at the last old page that began safely before the edit and stops
as soon as a new page begins where an old one did after the edit (same
block, line, section, page kind and hard-break state), reusing the old
pages with block and page numbers moved. Page starts inside a paragraph
laid out around a float, a paragraph with anchored objects, a split table
row, notes carried over or more than one column are never resumed from,
because their state is not in the page start. Block keys no longer search
the whole document per block (labels and note numbers are indexed once):
that search was quadratic. Lines carry a stable `id`, and
`LayoutEngine::changes` lists changed and only renumbered pages.

## 2026-11-12 provider, fields and the grid option

provider.rs answers in `letters_office::bridge::WordLayout`'s shape (from a
Document or its json), so the editor can switch between Word's layout and
ours. Page and page count fields in body text now show the page they land
on and the count of a first pass; DATE fields show today's date in their
picture's format (fields.rs). grid.rs snaps a finished layout to Word's
1/600 inch grid for side-by-side comparison only (`word_grid_drawing`,
off by default): drawing stays exact.

## 2026-11-12 tables: word's autofit measured, insets, style borders

Table probes (tests/oracle/probes.rs: 23 one-table pages, with and without
cell margins, exported by both applications) pinned word's autofit: each
column gets its narrowest width and the rest of the room in proportion to
how much wider it would like to be (the rule we had), but natural widths
count at most 1584 pt (22 inches), and each side of a column keeps the
larger of its cell margin and its border (probes without margins were 1.0
pt wider than their text with half-point borders, those with 5.4 pt
margins exactly 10.8 pt). The room is measured between the outer borders'
centres (467.6 pt on a 468 pt measure). 19 of 19 over-wide probes now
match within 0.1 pt. Libreoffice does not refit docx tables at all: all 23
probes kept the file's grid, so the LibreOffice flavor keeps it.

Cell text starts the larger of margin and border right of the cell edge,
and the left rule's centre sits half its width right of the margin (both
measured, word 72.26 pt, libreoffice 72.3 pt). Tables word makes keep their
borders in the table style ("Table Grid"); they are now read from the
style chain, which gave w_tables_aptos its rules back. A split row now
follows widow and orphan control in its cells in word (a flavor rule:
libreoffice splits without it, measured both ways). tables.rs grew past
1000 lines, so it became tables/ (mod, widths, draw, place).

Whole corpus against word: page breaks 72.6 -> 78.4 %, same lines
93.4 -> 95.7 %, vertical rules 12.8 -> 2.9 pt mean.

## 2026-11-12 justification and floats

fit_justify_probes (in tests/oracle.rs) reads, for six fonts, which
justified probe lines each application squeezed one more word into.
LibreOffice 26.8 squeezed exactly the same words as Word on docx, so its
flavor now squeezes up to a quarter of the spaces too (it had none): its
justified class went from 51 to 92 % of lines like LibreOffice. The probes
also show that a share of the space width alone does not separate what
Word squeezes from what it refuses (refusals from 0.12, squeezes up to
0.25 in Arial 10), so the rest of Word's rule is still unknown.

Square wrap now puts text on both sides of a float that stands inside the
column (`LineSpace::second`), as Word does by default: the piece left of
the float takes no height and the piece right of it carries the line; a
piece is used only when its first word fits whole. images (fit) went from
54.5 to 99.3 % of lines like Word.

## 2026-11-12 the office model's table fields, files split, round two scored

The office crate now reads w:tblLayout, w:tblCellMar, w:tblInd, w:tcMar
and exact row heights into the model instead of keeping them as xml, so
the engine reads those fields (falling back to the xml for styles and
older files) and gives exact rows their stated height. Shapes and groups,
new in the model, are reported and skipped. incremental.rs was split into
incremental/ (engine, keys, pages) and its layout function into named
steps, to keep files and functions a size a person can hold. Final run
("9 round two final"): whole corpus against Word 77.8 % page breaks and
96.9 % lines (LibreOffice on the same files 49.6 % / 78.5 %); against
LibreOffice 72.6 % / 94.1 % (Word 49.6 % / 76.8 %); LibreOffice-made ODT
56.4 % / 93.1 % (Word 54.9 % / 84.7 %). Held-out results are within a few
points of the fit set (see ORACLE_REPORT.md).
