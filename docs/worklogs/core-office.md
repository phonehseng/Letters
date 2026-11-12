# letters-office worklog

What changed in this crate and why, one entry per piece of work, newest
last. The reasons for single lines live in the code's comments; this is the
record of decisions that span files.

## earlier rounds (as recorded in README.md)

- One model for docx and odt. Anything a reader does not understand is
  kept as its original xml where it occurred, and package parts the model
  does not read are carried byte for byte, because "open, change one word,
  save" must not destroy what the model cannot describe.
- Text boxes and floating pictures got full geometry (exact emu sizes and
  offsets, stacking order, rotation, crop), because converted pdf pages and
  Word documents place them to the emu and twips alone drift.
- `floating.rs` (float, inline, align, distribute, snap), `compat.rs`
  (strict Word mode and the feature matrix) and `bridge/` (a private hidden
  Word and a headless LibreOffice with its own profile) were added so the
  editor can check what Word would do without Letters having Word's layout.
- RTF and Markdown readers and writers for Save As.

## 2026-11-12 drawings become model objects

Shapes, groups and connector lines were preserved as opaque xml: kept, but
neither editable nor convertible between formats. Added `model/drawing.rs`
(`Shape`, `Group`, `Geometry` preset or free path, `Fill` none, solid,
gradient or picture, `Stroke` with dashes, caps, joins and arrowheads, text
inside, connectors) and `Block::Shape` and `Block::Group`. Every drawing
keeps the xml it was read from with a fingerprint of its model
(`OriginalObject`); while the fingerprint still matches, the writer puts the
original back byte for byte, which is what makes "nothing moves when you
click" testable. Docx: `docx/drawingml.rs` reads and writes colours (with
the theme's colour scheme and lumMod/shade/tint), outlines, fills and
lines; `docx/read/shapes.rs` reads `wps:wsp` and `wpg:wgp`; `docx/read/vml.rs`
reads vml-only shapes from older files; `docx/write/shapes.rs` writes both
with a vml copy for Word 2007. Word's own corpus (made through its object
model, `tests/tools/word_corpus.ps1`) showed shapes take their line and
fill from the theme through `wps:style`, so the palette is read from the
theme part.

## 2026-11-12 drawings remember their anchor paragraph

Reading a paragraph that only held text boxes dropped the paragraph's own
formatting, and writing gave every box its own new paragraph, so a page with
three boxes in one paragraph gained two empty paragraphs and text below
moved. `ObjectAnchor` now records whether a drawing had a paragraph of its
own (with that paragraph's formatting), shared the previous drawing's, or
sat in the next paragraph of text (and before which inline). The docx
writer puts each drawing back where it was. Drawings in paragraphs of text
are now read too (they were preserved runs), placed before the paragraph as
the model already defined for text boxes.

## 2026-11-12 table, row, cell and section fields for layout tables

Added additive fields for what Word and LibreOffice both support in layout
tables: `Table.position` (floating tables), `fixed_layout`, `cell_margins`,
`indent`; `TableRow.height_rule` (exact rows were preserved xml before);
`TableCell.margins` and `text_direction`; `Section.column_widths` (unequal
columns) and `Section.canvas` (a page of positioned objects only).
`Columns`, `Border`, `CellBorders`, `PageSetup` and `Margins` were left
unchanged because other crates build them field by field.

## 2026-11-12 opendocument drawings

The odt reader and writer now read and write the same drawing objects.
`odt/graphics.rs` turns graphic styles into fills and lines and back,
including the named gradients, arrowheads, dash patterns and fill pictures
of `office:styles` (new ones are named `Letters_20_...`, numbered past any
name the document already uses). `odt/paths.rs` reads svg path data, point
lists and plain-number enhanced paths. `odt/read/drawing.rs` reads custom
shapes, rectangles, ovals, lines, paths, polygons, connectors, groups
(`draw:g`, whose members' page positions become positions inside the
group), text box frames with their chain (`draw:chain-next-name`) and
floating tables (a frame holding only a table). Graphic styles are read
through their parents and the default graphic style, because libreoffice's
default fills shapes blue and only the inherited value says otherwise.
`odt/write/drawing.rs` writes them back: presets as `draw:custom-shape`
with the opendocument type (`shapes.rs` maps the names; presets
opendocument lacks keep libreoffice's own "ooxml-" names), free outlines as
enhanced paths, lines as `draw:line`, turned objects with `draw:transform`
(opendocument turns counterclockwise about the corner, the model clockwise
about the centre). LibreOffice's own conversion of Word's corpus showed it
writes word's text boxes as rectangles holding text, so a plain rectangle
with text reads as a text box (`Shape::as_text_box`), and that it loses
linked boxes' text, which is one thing Letters now keeps.
`anchors.rs` holds the placeholder scheme both readers use to put drawings
back where their paragraph had them; drawings anchored to the page sit
outside paragraphs in odt (`AnchorWith::Page`).

## 2026-11-12 layout tables, unequal columns and column sections read and written

Docx: `w:tblpPr` (with `w:tblOverlap`), `w:tblLayout`, `w:tblCellMar`,
`w:tblInd`, `w:trHeight` with its rule, `w:tcMar` and `w:textDirection` are
now model fields instead of preserved xml, so they convert to odt; unequal
`w:cols` (`w:equalWidth="0"` with one `w:col` each) fill
`Section.column_widths`. Odt (`odt/table_props.rs`): row heights
(`style:row-height` is exact, `style:min-row-height` at least), cell padding
and `style:writing-mode`, a table's left margin, and `style:columns` with
relative widths, where each gap is split into the end indent of one column
and the start indent of the next. Opendocument has no table-wide cell
margins, so every cell carries the table's. A continuous section on the
same page size is now written as a `text:section` with its columns
(`odt/write/sections.rs`) and read back as a continuous section
(`odt/read/sections.rs`); it used to start a new page.

## 2026-11-12 freeform helpers and canvas pages

`freeform.rs`: `canvas_page_from_objects` builds a page of positioned
objects (text boxes, shapes, groups, floating pictures and tables) with the
one paragraph Word needs to anchor them (no spacing, a one-point line, so it
never pushes anything onto a second page); objects are given back to front
and numbered from `CANVAS_FIRST_Z`. `append_canvas_page` adds it as a section
of its own. Both readers call `mark_canvas_sections`, so a section holding
only positioned objects reads back as a canvas page. `table_to_text_boxes`
pulls a table apart into boxes at the cells' places (each box remembers its
cell, row and table in `TextBox.table_cell`), and `text_boxes_to_table` puts
boxes back together when their edges tile a grid exactly once.
`columns_to_linked_boxes` and `linked_boxes_to_columns` turn a section's
columns into one story in a chain of boxes and back.

## 2026-11-12 what the application suites found, fixed

`tests/freeform_apps.rs` renders a corpus Word and LibreOffice made
themselves (`tests/tools/word_corpus.ps1`, `tests/support/lo_corpus.rs`)
before and after Letters opens and saves it, through both applications'
pdf export, and measures glyphs and object boxes (with the exact
converter's pdf reader). It found, and these were fixed:
- Odt manifests said odf 1.3 while the parts kept libreoffice's 1.4, and
  libreoffice refuses such a package outright; every part and the
  manifest now say the document's own version.
- Automatic styles named only by drawings' original xml were dropped, so
  re-used shapes lost their fill and line; `visit_preserved` now looks
  inside drawings.
- Theme fonts were written with the resolved name beside the theme slot;
  libreoffice prefers the name and reflowed the text. The writer now
  writes what the file had.
- Libreoffice's equal section columns hold the gaps inside the columns;
  reading them as equal text widths made Word lay them out wider, so their
  exact widths are kept.
- Two older docx writer bugs: unknown `w:tblPr` children (`w:tblW`,
  `w:tblLook`) were written after the grid, outside the properties, and a
  row's `w:tblPrEx` inside its `w:trPr`. Word ignored the strays;
  libreoffice drew the nested table 0.05 pt narrower.
- The word bridge's new batch mode read a json list as one job under
  Windows PowerShell 5.1 (`ConvertFrom-Json` on a pipeline); jobs are now
  converted as an argument.

## 2026-11-12 back-and-forth matrix fixes, and tidy

The matrix (docx -> Letters -> odt -> Letters -> docx rendered in Word,
odt -> Letters -> docx -> Letters -> odt rendered in LibreOffice, against
Word saving as odt and LibreOffice saving as docx) found: non-native docx
did not declare the `wpg` prefix (Word called the file damaged); the odt
writer's list of drawings waiting for the next paragraph was shared, so a
shape's own text took them and they came back twice (nested drawings in a
text box hang Word's pdf export, so the docx writer now writes a drawing
nested in another drawing's text as its paragraphs); a column break became
a line break in odt (it is now the paragraph's `fo:break-before`); and
libreoffice does not draw a text frame inside a group, so group members
that are text boxes are written as rectangles holding text
(`TextBox::as_shape`, shared with the docx writer).
`tidy.rs` merges runs, drops formatting and paragraph properties that
repeat the style, drops emu values the twips already say, removes styles
(keeping their bases and followers), lists and pictures nothing mentions
(anything named in kept xml counts as used), and on request folds a stack
of one-line text boxes into one when each box starts where the line before
ends. Drawings that keep their original xml are left alone. The Word and
LibreOffice check found no change in any rendering.

## 2026-11-12 the three longest files split, no behaviour change

Following docs/CONTRIBUTING.md, `odt/write.rs` (1,650 lines) became
`odt/write.rs` (the package and shared helpers) with `write/body.rs`,
`inlines.rs`, `tables.rs`, `styles.rs` and `package.rs`; `odt/read.rs`
(1,520) the same way with `read/styles.rs`, `body.rs`, `inlines.rs`,
`tables.rs` and `package.rs`; `rtf.rs` (1,170) became `rtf/read.rs` and
`rtf/write.rs`; `compat.rs` became `compat/mod.rs` with the new
`compat/drawings.rs`. Each piece is the file's own section, moved whole;
the 145 tests passed before and after.
## 2026-11-12 what each application keeps, said before saving

`compat/drawings.rs` lists, for Word and for LibreOffice, which of a
document's drawings and layout an application changes on the way in, so
the editor can warn before a save instead of after: a preset shape with no
opendocument name, gradients with more than two stops or with
transparency, arrowhead sizes, dash patterns, connector glue, groups that
are turned or flipped, text boxes inside groups, floating tables, tables
that size themselves, unequal columns that libreoffice stretches to fill
the page, continuous sections that change the page size, and Word's
character shaping (kerning and ligatures) that opendocument has no place
for. `libreoffice_compatibility` is new beside `word_compatibility`, and
`feature_matrix` gains a row for each freeform feature (continuous
sections are now kept by both formats). A shape's text colour in
opendocument comes from its graphic style when the paragraphs do not say,
because libreoffice writes it there; it is read into `ShapeText.color` and
written back the same way. The feature table moved, unchanged, to
`compat/features.rs` so `compat/mod.rs` holds only the checks and fixes.
