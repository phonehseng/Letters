# letters-office

The file layer of word-processor mode: it reads `.docx` and `.odt` into one
shared document model and writes that model back to either format, losing
as little as possible. The editor (a ProseMirror-style web editor, later)
works on the model as JSON; this crate never draws anything.

```
 .docx ──read_docx──┐                  ┌──write_docx──> .docx
                    ├──> Document ─────┤
 .odt  ──read_odt───┘   (model, JSON)  └──write_odt───> .odt
                          │
        pure functions:   ├─ outline, apply_heading, presets (MLA, APA 7)
                          ├─ typing helpers (smart quotes, autocorrect...)
                          ├─ ruler state, counts, list labels
                          └─ plain text / html export and import
```

## Public API

| Call | What it does |
|---|---|
| `open(bytes) -> Result<Document, OfficeError>` | detects the format by content and reads it |
| `read_docx`, `read_odt` | read one format |
| `write_docx(&Document)`, `write_odt(&Document)`, `save(&Document, FileKind)` | write a package (`Vec<u8>`) |
| `detect(bytes) -> FileKind` | `Docx` or `Odt`; spreadsheets, presentations, `.doc` and encrypted files get a clear error |
| `outline::outline`, `outline::apply_heading`, `outline::heading_style` | the outline panel and one-step headings |
| `presets::apply_mla`, `presets::apply_apa`, `presets::apply_reference_indents` | paper presets |
| `indent::{set_first_line_indent, set_hanging_indent, fix_manual_indents, make_indents_consistent}` | smart indentation |
| `ruler::describe_ruler(&doc, block)` | indent and tab positions as numbers and as plain sentences |
| `typing::on_type(before, typed, &TypingSettings) -> Option<Replacement>` | smart quotes, autocorrect, capitals; `TypingSettings::toggles()` lists every switch for the settings panel and status bar |
| `counts::count(&doc)` | words, characters, paragraphs, note words, page estimate |
| `numbering::list_labels(&doc)`, `numbering::ListCounter` | the label each list paragraph shows ("2.", "b)", "iv.") |
| `styles::{resolved_paragraph_props, resolved_run_props, heading_level}` | style inheritance resolved |
| `export::{to_plain_text, to_html}`, `import::{from_plain_text, from_html}` | conversion and paste |
| `freeform::canvas_page_from_objects(PageSetup, Vec<CanvasObject>) -> CanvasPage`, `freeform::append_canvas_page` | a page of positioned objects with no text flow |
| `freeform::table_to_text_boxes(&mut doc, block, Option<&TableGeometry>)`, `freeform::text_boxes_to_table(&mut doc, &[usize])` | a table pulled apart into text boxes at the cells' places, and snapped back |
| `freeform::columns_to_linked_boxes(&mut doc, section)`, `freeform::linked_boxes_to_columns(&mut doc, chain)` | a section's columns as a chain of linked text boxes, and back |
| `tidy::tidy(&doc, &TidyOptions) -> (Document, TidyReport)` | a cleaner copy that looks the same |
| `compat::word_compatibility`, `compat::libreoffice_compatibility`, `compat::feature_matrix` | what each application would change, before saving |
| `shapes::{odf_type, preset_for_odf_type, nearest_odf_type}` | preset shape names between drawingml and opendocument |

Every error is an `OfficeError` whose message is written for the person
using the app. No input makes the crate panic (see the tests).

## The model

`model::Document` holds `metadata`, `settings`, `styles`, `numbering`,
`sections`, `blocks`, `headers_footers`, `notes`, `comments`, `media` and
`carry`. Lengths are twips (1/1440 inch), font sizes half points.

- **Blocks** (`{"type": ...}`): `paragraph`, `heading` (level 1-6, the
  paragraph's fields flattened in), `table`, `pageBreak`, `sectionBreak`,
  `textBox`, `shape`, `group`, `preserved`. There is one more `Section`
  than `sectionBreak` blocks; section *i* ends at the *i*-th break.
- **Paragraph**: `styleId`, `props` (alignment; indent left/right/first
  line/hanging; spacing before/after/line with auto/exact/at-least;
  keep with next; keep lines together; page break before; list id + level;
  tab stops; outline level; preserved), `content` (inlines), `attributes`.
- **Inlines**: `text`, `tab`, `break` (line/page/column), `image`,
  `noteReference`, `field` (page number, page count, date, table of
  contents), `bookmarkStart`/`bookmarkEnd`, `commentReference`,
  `preservedRun`, `preserved`. Formatted inlines carry **marks**: bold,
  italic, underline (style), strike, double strike, superscript, subscript,
  small caps, all caps, hidden, font family (with theme slot), font size,
  colour, highlight, language, character style, link, insertion and
  deletion (author, date, id), comment, `off` (a style's toggle switched
  off) and preserved run properties.
- **Tables**: style, width, alignment, grid column widths, borders; rows
  (height, header, can't split); cells (column span, row span, width,
  borders, shading, vertical alignment, nested blocks). Covered cells are
  left out, as in HTML; `tables::grid` puts them back for the writers.
  Freeform tables add `position` (floating: `horizontal`, `vertical`,
  `distance`, `overlap`, `anchor`), `fixedLayout`, `cellMargins` and
  `indent` on the table, `heightRule` (`atLeast` or `exact`) on rows, and
  `margins` and `textDirection` (`horizontal`, `upward`, `downward`) on
  cells.
- **Images**: media id, size, alt text, name, inline or floating (position
  relative to page/margin/column/paragraph, offset or alignment, wrap:
  square, tight, through, top and bottom, behind, in front). Bytes live
  once in `media` (base64 in JSON).
- **Styles**: paragraph and character styles with `basedOn`, `next`,
  defaults, theme fonts. **Numbering**: definitions (levels with format,
  label pattern like `%1.%2`, start, indents, label font) and instances;
  instances of one definition count on from each other unless one has a
  start override, which is how word works and how odt's "continue
  numbering" is mapped.
- **Sections**: page size, orientation, margins (header and footer
  distance, gutter), columns, how the section starts, headers and footers
  for default/first/even pages, different first page, page number start.
  `columnWidths` (`[{width, space}]`) gives unequal columns their own
  widths and gaps; `canvas` marks a page of positioned objects only.
- **Drawings** (see below): `shape`, `group` and `textBox` blocks, each
  with an optional `anchor` (which paragraph holds it) and `original` (the
  xml it was read from, written back unchanged while the object is).

## Round-trip safety

The rule: opening a file, changing one word and saving must not destroy
anything the model does not understand.

- **Docx**: every element is read into the model only when all of its
  attributes are understood; otherwise the element is kept as its exact
  original XML, at the place it occurred (a run property, a paragraph
  property, an inline element, a whole block, a table, row or cell
  property, a style or numbering child). The writer puts each one back in
  schema order, so the file stays valid. Package parts the model does not
  read (theme, settings, font table and embedded fonts, web settings,
  custom XML, charts, embeddings, SmartArt, `docProps/app.xml` and
  `custom.xml`, comment extensions, macros) are copied byte for byte, with
  their content types. Relationships that preserved XML points at keep
  their original ids. Root namespace declarations and `mc:Ignorable` are
  kept on every rewritten part. Unchanged pictures are written back with
  their original drawing XML (crops, effects and all). `settings.xml` is
  patched in place: only track changes, the default tab stop and even/odd
  headers are rewritten.
- **Odt**: elements the model does not have (text boxes, embedded objects,
  indexes such as a table of contents, forms, variable declarations) are
  kept as original XML where they occurred. Style properties the model does
  not have are kept as attributes and written into the regenerated styles.
  Automatic styles that preserved XML refers to are written back under
  their original names; the writer's own automatic styles are named so
  they cannot clash. `settings.xml`, thumbnails, `Configurations2` and
  other parts are copied byte for byte and listed in the manifest again.
  `mimetype` is the first entry, stored, with no extra field.
- **Drawings** (shapes, groups, text boxes) remember the XML they were
  read from and a fingerprint of their model. While the fingerprint still
  matches, the writer puts that XML back byte for byte, so a file opened
  and saved, or one where another object was edited, keeps every
  untouched drawing exactly. Changing an object rewrites only that object.
- Converting between formats keeps the model's content; preserved XML from
  the other format cannot be carried over, so only its text is kept.

### What is modelled, preserved or lost

| Feature | Docx | Odt |
|---|---|---|
| Paragraphs, headings 1-6, alignment, indents, spacing, keep, page break before, tabs | modelled | modelled |
| Character formatting listed above | modelled | modelled |
| Other run/paragraph properties (borders, shading, character spacing, east asian layout, glow...) | preserved in place | preserved as style attributes |
| Paragraph-mark formatting, revision ids, paragraph ids | preserved | n/a |
| Styles with inheritance, defaults, theme fonts | modelled (latent styles, table styles' details preserved) | modelled (other style families preserved) |
| Lists: bullets, decimal, letters, roman, multi-level, restart/continue | modelled (picture bullets, extra level settings preserved) | modelled (list styles regenerated) |
| Tables with spans, widths, borders, shading, vertical alignment, header rows, nesting | modelled (table look, layout, margins preserved) | modelled |
| Footnotes, endnotes, separators | modelled | modelled |
| Comments with ranges, author, date | modelled (comment extension parts carried) | modelled |
| Tracked insertions and deletions | modelled as marks | modelled as marks |
| Tracked formatting changes, moves, paragraph-mark changes | preserved | lost |
| Headers and footers, first page, even pages | modelled | modelled through master pages |
| Sections: size, orientation, margins, columns, start | modelled (line numbering, page borders, doc grid preserved) | modelled (master page per section) |
| Pictures, inline and floating | modelled, original XML reused while unchanged | modelled, original frame reused while unchanged |
| Text boxes: position (page, margin, column or paragraph), size or grow-to-fit, wrap, stacking order, rotation, inner margins, vertical alignment, line, fill, any content | modelled; written as a shape for Word 2010+ with a VML copy for older Word | modelled as a frame with a text box; rotation is written as a `draw:transform` but strict Word mode still counts it as Word-only until that is checked in the applications |
| Linked text boxes (text flowing from one box to the next) | modelled (`wps:txbx` id and `wps:linkedTxbx` sequence; VML `o:spid` chains read) | modelled (`draw:chain-next-name`) |
| Floating picture geometry: exact EMU size and offsets, stacking order, rotation, crop, aspect lock, alt text | modelled | position, size, wrap, order and alt text modelled; crop not kept |
| Shapes: preset outlines with their handles, free paths (lines, curves, arcs), fill (none, solid, gradient, picture), line (colour, width, dash, caps, joins, arrowheads), transparency, rotation, flips, text inside, name and alt text | modelled (`wps:wsp` with a VML copy; VML-only shapes read); shadows, glow and 3-D kept as preserved XML | modelled (`draw:custom-shape` with enhanced geometry, `draw:rect`, `draw:ellipse`, `draw:line`, `draw:path`, `draw:polygon`); graphic style properties not modelled are kept |
| Groups (nested, with child coordinates), connectors as lines with their ends | modelled (`wpg:wgp`) | modelled (`draw:g`; connector glue points kept only by name) |
| Floating tables, exact row heights, cell margins and text direction, fixed layout, table indent | modelled (`w:tblpPr`, `w:trHeight w:hRule`, `w:tcMar`, `w:textDirection`) | modelled (a floating table is written in a frame) |
| Columns of unequal width, column breaks, continuous section breaks | modelled | modelled (`style:column` relative widths, `fo:break-before="column"`, `text:section`) |
| Charts, SmartArt, OLE objects, equations, content controls, fields not listed | preserved | preserved |
| Fields: page number, page count, date, table of contents | modelled | page number, count, date modelled; table of contents preserved as a block |
| Metadata | title, subject, author, keywords, description, dates, language modelled; other core properties preserved | same; other meta elements preserved |
| Unknown package parts | byte for byte | byte for byte |

Known losses (by design, for now):

- Docx spell-check markers (`w:proofErr`) and last-rendered page breaks are
  dropped; word recomputes them.
- Character-based indents and "auto" spacing (`w:leftChars`,
  `w:beforeAutospacing`) keep the whole element as preserved XML; if the
  person then changes that indent, the preserved element wins. A preset
  clears direct indents first, so presets are not affected.
- Covered cells of a vertical merge lose any content they had (word shows
  none either). A `gridBefore` row offset is kept but not used when
  placing merged cells.
- Odt: `text:section` wrappers without columns are dropped (their content
  is kept); a page break inside a line becomes a line break; tracked
  deletions spanning paragraphs are joined with line breaks; date field
  formats are lost.
- Word's kerning, ligatures and character spacing (`w:kern`,
  `w14:ligatures`, `w:spacing` on runs) are kept in docx but have no
  opendocument equivalent here, so a docx taken through odt can shift its
  lines by a couple of points.
- A floating table taken through odt lands in a frame, which Word and
  LibreOffice place differently from `w:tblpPr`; Word's own conversion
  does no better (see `ROUNDTRIP_REPORT.md`).
- Odt: a comment range inside a header, footer or note shrinks to its
  first piece of text (ranges are tracked for the main text only).

## Floating objects, copy and paste

- `floating::float_image` / `inline_image` turn a picture between "in the
  line" and "floating". Going back into the line remembers the floating
  geometry (`Image.previousPlacement`), so floating it again is exact.
- `floating::split_paragraph_into_text_boxes`, `text_box_from_blocks` and
  `dissolve_text_box` turn text into movable boxes and back.
- `floating::{bring_to_front, send_to_back, align, distribute, move_to,
  snap}` work in page coordinates (twips from the page corner). Objects
  positioned relative to a paragraph have no page position without a
  layout, so align, distribute and snap leave them alone.
- `clipboard::add_image` takes clipboard picture bytes. PNG, JPEG and GIF
  are kept as they are; BMP is converted to PNG; WebP and SVG are kept as
  they are (no decoder here), and the result says that Word 2019 and
  older can't show them. `import::from_html_with_images` pastes HTML and
  asks a loader the caller supplies for pictures named by file or
  address. `clipboard::copy_blocks` gives HTML, plain text and, for a lone
  picture, its bytes.

## Freeform objects and canvas pages

The goal is that a file can go between Word, LibreOffice and Letters, and
between docx and odt, and still look the same. So drawings are part of the
model rather than opaque XML:

- A **shape** (`{"type": "shape"}`) has a position and size like a text
  box (twips, with `widthEmu`/`heightEmu` for exactness), `geometry`
  (`{"type": "preset", "name": "roundRect", "adjust": [...]}` or
  `{"type": "path", "paths": [{width, height, fill, stroke, commands}]}`
  where a command is `{"op": "move"|"line"|"cubic"|"quadratic"|"close"}`),
  `fill` (`none`, `solid`, `gradient` with stops, `picture`), `stroke`,
  optional `text` (blocks, margins, alignment, colour), `connector`,
  `rotation`, flips, `zOrder`, `wrap`, `name`, `title` and `description`.
- A **group** holds `children` (shapes, text boxes, pictures, groups) in
  its own coordinate space (`childOffsetX/Y`, `childWidth/Height`).
- Every drawing has an `anchor`: whether it sits in its own paragraph, the
  previous or the next one, or on the page, the paragraph itself when it
  is its own, and where in the line it was. Readers and writers use it to
  put the object back exactly where it was in the text.
- Charts, SmartArt, OLE objects and equations stay opaque (preserved).

A **canvas page** is a section with `canvas: true` holding positioned
objects only, the way a poster or a PDF page converted to an editable
document is built. `canvas_page_from_objects` makes one from a page size
and objects given back to front; their stacking numbers start at
`CANVAS_FIRST_Z` (1) and go up by one, per page. The drawings are anchored
to one tiny paragraph (no spacing, a one-point exact line) that also holds
the floating pictures, so the page never grows a second page. Readers mark
such sections as canvas pages again (`mark_canvas_sections`). The exact
PDF converter's frames, per-run spacing and embedded fonts fit this model
without loss; its docx survives an untouched save in Word unchanged.

`table_to_text_boxes` pulls a table apart into one text box per cell at
the cell's place (each box remembers its cell, row and table), and
`text_boxes_to_table` puts them back as the same table. A table in the
flow needs `TableGeometry` (its corner and row heights from a layout).
`columns_to_linked_boxes` turns a section's columns into a chain of
linked text boxes, and `linked_boxes_to_columns` turns a chain back.

`tidy::tidy` makes a cleaner copy that looks the same: neighbouring runs
with equal formatting merged, formatting and paragraph properties that
repeat the style dropped, emu values the twips already say dropped,
unused styles, lists and pictures removed, and, when asked, a stack of
one-line text boxes made into one box. Drawings that still carry their
original XML are left alone. The report counts each change.

## Strict Word mode

`compat::word_compatibility(&doc)` lists everything Word would not keep
or would show differently (OpenDocument-only objects, highlighter colours
Word lacks, list numbering Word lacks, fonts that are neither standard nor
embedded, nested text boxes, WebP and SVG pictures, anchors Word measures
differently), each with a plain sentence and, where possible, a fix that
`compat::apply_fixes` applies. `compat::feature_matrix()` says for every
editor feature whether it is exact in both formats, Word-exact, ODT-exact
or approximate; strict Word mode offers only the Word-exact ones, so the
report stays empty. `compat::libreoffice_compatibility(&doc)` is the same
for LibreOffice. Both also list what an application changes in drawings
and layout: preset shapes with no opendocument name, gradients with more
than two stops or with transparency, arrowhead sizes, dash patterns,
connector glue, turned or flipped groups, text boxes inside groups,
floating tables, self-sizing tables, unequal columns, continuous sections
that change the page size, and Word's character shaping.

**Letters has no page layout of its own**, so by itself it can't promise
Word's line and page breaks. `bridge::word_layout(docx, cache)` asks Word
instead: it opens the file in a private hidden Word and reads every line
from Word's own layout (`Pages`, `Rectangles`, `Lines`): page size, each
line's top, left, height and character range, its paragraph in Word's
order (`bridge::word_paragraph_map` maps that back to the model), and
where each floating object landed. Exporting a PDF and mapping its text
back was rejected: it needs a PDF text extractor and loses the link to
character positions. Results are cached by a hash of the file. Without
Word the call answers `BridgeError::WordNotInstalled`.

## Formats, printing and old Word files

`formats::save_as` writes DOCX, ODT, RTF, HTML, Markdown (with picture
files beside it), plain text and, through Word or LibreOffice, `.doc`.
`formats::format_capabilities()` gives one plain sentence per format for
the Save As dialog. `rtf::read_rtf` reads RTF (text, character and
paragraph formatting, lists, tables, pictures); `formats::open_any` opens
DOCX, ODT, RTF, plain text and, through a converter, `.doc`.
`formats::page_setups` gives each section's paper name, size,
orientation and margins. `bridge::print_pdf` makes the print-ready PDF
through Word (exact) or, failing that, LibreOffice (close; the result
says so). `bridge::convert_word_format` converts `.doc` and `.docx` the
same way. LibreOffice runs headless with its own profile folder and is
always waited for.

## Typing helpers

`TypingSettings` has one switch per feature (smart quotes, capitalize
sentences, capitalize "I", fix two initial capitals, smart dashes,
symbols, common typos) plus a language and custom rules. Quotes follow the
language (English, German, French with no-break spaces, Spanish, Italian,
Portuguese, Russian, Polish, Swedish, Finnish, Japanese and more);
apostrophes are always `’`; a quote after a short number is a prime
(5′10″), but "the 1990's" keeps its apostrophe. Capitalisation skips
common abbreviations, initials and numbered list starts.

## Tests

```
set CARGO_TARGET_DIR=C:\Letters\target-office
cargo test -p letters-office                 # everything that needs no word
cargo test -p letters-office -- --ignored    # checks against microsoft word
cargo clippy -p letters-office --all-targets
cargo fmt -p letters-office
```

Tests build small docx and odt packages from XML strings
(`tests/support`). They cover each feature, write and re-read equality,
byte-for-byte preservation of unknown XML and parts, docx to odt and back,
style inheritance, list counting, tables with spans, notes, headers and
footers, pictures, tracked changes, comments, broken and truncated files
(no panics), unsupported types, typing helpers, presets, exports and a
2,000 paragraph document (about 10 ms to write and 15 ms to read docx in a
release build, 20 and 45 ms in a debug build).

The ignored tests use `tests/tools/word_check.ps1`, which starts its own
hidden Word through COM (alerts off, files opened read-only), only ever
stops the Word it started (Word started for automation carries the
`/Automation` switch) and has a watchdog for hangs. They check that Word
opens our files without repairing them, with the expected paragraphs,
heading styles, footnotes, comments, revisions, pictures, headers and
tables; that a file Word itself saved reads correctly and survives our
save unchanged in Word's eyes; and, as a control, that a deliberately
broken file is reported. Further Word checks cover text boxes (positions
within a twip), a picture floating and then toggled into the line, a
paragraph split into two text boxes, our RTF, `word_layout` on a 16-page
document, printing to PDF and `.doc` conversion.

`tests/libreoffice.rs` (also `--ignored`) checks the same against
LibreOffice: it converts our ODT to PDF and text, reads an ODT that
LibreOffice wrote from our DOCX, round-trips text boxes and floating
pictures through it, and uses it to print and to convert `.doc` files.

`tests/freeform.rs` checks every freeform object through docx, odt and
docx to odt to docx, that a second save is byte identical, that editing
one drawing leaves every other drawing's bytes alone, canvas pages,
unequal columns and column sections. `tests/freeform_apps.rs` (ignored)
uses Word and LibreOffice together: it makes a corpus in each application
(`tests/tools/word_corpus.ps1`, `tests/support/lo_corpus.rs`), renders
each file and our save of it to PDF in the same application and compares
every glyph and object box ("nothing moves"), runs the back-and-forth
matrix against the applications' own conversions, round-trips the exact
converter's docx, and checks that `tidy` never changes a rendering. Word
runs as one private instance for a whole batch (`bridge::word_batch`),
LibreOffice converts many files per start. Results are written to
`%TEMP%\letters-freeform\results` and summarised in `ROUNDTRIP_REPORT.md`.

```
cargo test -p letters-office --test freeform_apps -- --ignored --nocapture --test-threads 1
```

## Libraries

| Library | Used for | License |
|---|---|---|
| [zip](https://crates.io/crates/zip) | the package container | MIT |
| [flate2](https://crates.io/crates/flate2), [zlib-rs](https://crates.io/crates/zlib-rs), crc32fast | deflate, in pure Rust | MIT or Apache-2.0; Zlib |
| [serde](https://serde.rs) | the model as JSON | MIT or Apache-2.0 |
| [thiserror](https://crates.io/crates/thiserror) | error messages | MIT or Apache-2.0 |
| letters-exact (tests only) | reading the PDFs the applications render, for the comparisons | the repository's own crate |

XML is read and written by the crate's own small reader (`src/xml`),
because preservation needs each element's exact source text. Nothing here
restricts distribution.
