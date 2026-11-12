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

Every error is an `OfficeError` whose message is written for the person
using the app. No input makes the crate panic (see the tests).

## The model

`model::Document` holds `metadata`, `settings`, `styles`, `numbering`,
`sections`, `blocks`, `headers_footers`, `notes`, `comments`, `media` and
`carry`. Lengths are twips (1/1440 inch), font sizes half points.

- **Blocks** (`{"type": ...}`): `paragraph`, `heading` (level 1-6, the
  paragraph's fields flattened in), `table`, `pageBreak`, `sectionBreak`,
  `preserved`. There is one more `Section` than `sectionBreak` blocks;
  section *i* ends at the *i*-th break.
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
| Shapes, text boxes, charts, SmartArt, equations, content controls, fields not listed | preserved | preserved |
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
- Odt: `text:section` wrappers are dropped (their content is kept);
  a page break inside a line becomes a line break; continuous section
  changes are not modelled; tracked deletions spanning paragraphs are
  joined with line breaks; date field formats are lost.
- Odt: a comment range inside a header, footer or note shrinks to its
  first piece of text (ranges are tracked for the main text only).

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
broken file is reported. LibreOffice is not installed on the development
machine, so odt output has only been checked by this crate's own reader.

## Libraries

| Library | Used for | License |
|---|---|---|
| [zip](https://crates.io/crates/zip) | the package container | MIT |
| [flate2](https://crates.io/crates/flate2), [zlib-rs](https://crates.io/crates/zlib-rs), crc32fast | deflate, in pure Rust | MIT or Apache-2.0; Zlib |
| [serde](https://serde.rs) | the model as JSON | MIT or Apache-2.0 |
| [thiserror](https://crates.io/crates/thiserror) | error messages | MIT or Apache-2.0 |

XML is read and written by the crate's own small reader (`src/xml`),
because preservation needs each element's exact source text. Nothing here
restricts distribution.
