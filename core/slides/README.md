# letters-slides

The file layer of presentation mode: it reads `.pptx` and `.odp` into one
shared model, draws slides to PDF on its own (thumbnails, the editor's canvas,
Export as PDF), edits decks with pure functions that undo exactly, and writes
either format back, losing as little as possible. The slide editor's interface
(a later step) works on the model as JSON.

```
 .pptx ──read_pptx──┐                        ┌──write_pptx──> .pptx
                    ├──> Presentation ───────┤
 .odp  ──read_odp───┘   (model, JSON)        └──write_odp───> .odp
                          │
                          ├─ inherit, color   effective values (placeholders, theme)
                          ├─ edit::apply      operations with exact undo
                          ├─ render, export   PDF (slides, notes pages, handouts)
                          └─ outline          outline view, outline text, deck from an outline
```

## Public API

| Call | What it does |
|---|---|
| `read_pptx(bytes)`, `read_odp(bytes)` | a deck from a file; broken files give a `SlidesError`, never a panic |
| `write_pptx(&deck)`, `write_odp(&deck) -> (bytes, notes)` | the deck as a file; `notes` lists what ODP could only approximate |
| `inherit::{effective_transform, effective_paragraph, effective_run, effective_body, resolve_typeface, Chain}` | effective values through slide, layout, master and theme |
| `color::resolve(&ColorRef, &ColorContext) -> Rgba` | a theme colour with its transforms, as RGB |
| `geometry::outline(geometry, w, h)` | a preset or custom outline as paths, with its text rectangle |
| `render::render_pdf(&deck, slides, &mut fonts) -> (pdf, RenderReport)` | slides as PDF pages; the report lists font substitutions and anything approximated |
| `render::slide_scene(&deck, index, ...) -> Scene` | one slide as drawing operations (for a canvas) |
| `export::export_pdf(&deck, PdfLayout::{Slides, NotesPages, Handouts(n)}, ...)`, `export::slide_pdfs` | deck exports; one PDF per slide for images (the app's PDF renderer, letters-pdf, makes the pixels) |
| `edit::apply(&mut deck, &Operation) -> Applied { changed_slides, all_slides, inverse }` | every edit below; applying `inverse` undoes it exactly |
| `outline::{outline, outline_text, from_outline}` | the outline view, the outline as text, a deck from headings |
| `blank::{blank_presentation, new_slide}` | a new deck (one master, three layouts) and slides on a layout |
| `office_bridge::{deck_from_document, to_canvas_pages, from_canvas_pages}` (feature `office`) | a letters-office document's headings as a deck; slides as canvas pages and back |
| `fonts::FontLibrary`, `fonts::FontSource` | font lookup with metric-compatible stand-ins; core/fonts can plug in through the trait |

Editing operations (`edit::Operation`, JSON tagged by `op`): add, duplicate,
delete and move slides; apply a layout (placeholders keep their text and take
the new layout's place); set an object's text (unchanged paragraphs keep their
XML); add, delete, replace, move, resize, rotate, fill and outline objects;
bring to front, send to back, forward, backward; align (to the selection or the
slide) and distribute; group and ungroup (positions kept); set a table cell's
text, insert and delete rows and columns (merges kept whole); replace a picture
keeping its crop and effects; change theme colours and fonts (everything that
follows the theme follows); change the slide size (don't scale, fit, stretch);
set notes; find and replace across slides, tables and notes.

## The model

`Presentation`: slide size and notes size, slides, layouts, masters, themes,
the default text style, sections, embedded fonts, the media table, metadata and
`carry` (every package part as read). Units are pptx's own: emu (914400 to the
inch), 60000ths of a degree, hundredths of a point, thousandths of a percent.

- **Slide**: id, layout, background, objects back to front, notes, transition
  (kept as XML, its kind named), animations (entrance, emphasis, exit and path
  effects in play order with their triggers, read from the kept timing tree),
  hidden flag, comments (legacy and modern), colour map override.
- **Master**: theme, colour map, title, body and other text styles per level,
  placeholders and decoration, its layouts. **Layout**: master, type,
  placeholders and decoration. **Theme**: the twelve colour slots, major and
  minor fonts per script, fill, line, effect and background style lists.
- **Objects** (`Object`, JSON tagged by `type`): `shape` (autoshape, text box or
  placeholder: geometry preset with handle values or custom paths, fill, line,
  theme style references, text), `picture` (crop, recolouring kept, video and
  audio posters), `group` (with its child space), `connector` (with the objects
  and connection sites its ends are glued to), `frame` (table with spans,
  borders, fills and text; chart, SmartArt and OLE kept with their parts and
  any stand-in picture), `preserved` (anything else, as XML).
- **Text**: body (insets, anchoring, wrap, autofit none / shrink with the
  stored font scale and line reduction / resize shape, columns, vertical text
  kept), the shape's own list style, paragraphs (level, alignment, margins,
  indent, line and paragraph spacing, character, auto-number and picture
  bullets with their font, colour and size, tabs) and runs (latin, east asian
  and complex fonts incl. theme references, size, bold, italic, underline,
  strike, caps, baseline, spacing, kerning, fill, highlight), line breaks,
  fields. Colours stay unresolved: a scheme slot or RGB value plus transforms.

Every object, paragraph, table row and cell, slide, layout, master, theme and
notes page keeps its original XML and a fingerprint of the model as read
(`Source`); see below.

### How the shapes relate to letters-office's

letters-office gained drawings (`Shape`, `Group`, `TextBox`, connectors and
canvas pages) while this crate was written. Its shapes are positioned in
twips relative to a page or paragraph and carry resolved hex colours and Word
text blocks; slides need scheme colours with transforms (so theme changes
follow), placeholders with inheritance and DrawingML text levels, so the slide
types are defined here, mirroring DrawingML. `office_bridge` maps between them:
geometry presets and handle values, position and size (exact emu kept), rotation
and flips, solid fills, picture fills and text with bold, italic, size, colour
and font marks. Merging later means teaching office's `Shape` the scheme colour
and placeholder fields, or giving office a `Slide`-like canvas that holds these
objects.

## Round-trip safety and "nothing moves when you click"

- **PPTX**: saving without edits writes every part byte for byte (all 20 test
  decks, made by PowerPoint and by LibreOffice). An edited object is rebuilt
  from its original XML, replacing only the pieces that changed: a moved shape
  differs in `a:off` only, a retyped shape keeps its geometry and properties
  XML, a retyped paragraph leaves the other paragraphs' XML alone; every other
  object, slide and part stays byte-identical (tests compare each object's XML).
  Relationship ids are kept (a duplicated slide copies its original's), parts
  no relationship reaches any more are dropped, unknown parts are copied.
- **ODP**: saving without edits writes every part byte for byte except the
  manifest (rewritten, same version as the content). Edited pages are rebuilt
  with unchanged objects copied and new automatic styles added.
- **Converting**: a deck converted to the other format carries an exact copy of
  itself (`shadow.rs`) with a fingerprint of every slide as written; Letters
  reads that copy back when nothing changed, so PPTX -> ODP -> PPTX (and back)
  is lossless through Letters, and a file another application changed is read
  as it now is.

### What is modelled, preserved or lost

| Feature | PPTX | ODP |
|---|---|---|
| Slides, layouts, masters, themes, placeholders with inheritance | modelled | masters and their presentation styles modelled; one layout per master |
| Text: paragraphs, levels, bullets, numbering, spacing, alignment, runs | modelled (unknown pieces kept in place) | modelled, formatting resolved onto paragraphs and runs |
| Autofit | stored scale and reduction modelled and drawn | shrink-to-fit read; a scale is worked out when drawing |
| Preset shapes with handle values, custom geometry | modelled; 24 presets drawn exactly, others as their box | presets and plain-number custom paths; formula paths keep their box |
| Fills: solid, gradient, picture, pattern; lines | modelled | solid, gradient, bitmap, line colour, width, dash, arrow ends |
| Pictures with crops, recolouring | modelled (recolouring kept) | crops converted from lengths to shares |
| Groups, connectors with glue | modelled | modelled (glue not kept) |
| Tables with spans, borders, fills | modelled; default table style drawn | modelled |
| Charts, SmartArt, OLE, video, audio | kept with their parts and relationships | kept as XML on the page |
| Notes, comments, sections, transitions, animations | notes and sections modelled; comments, transitions, animations read and kept | notes modelled; transitions read and kept |
| Effects (shadow, glow, 3D), custom XML, macros, unknown parts | kept byte for byte | kept on unchanged pages |

Known losses: converting with the writers alone (not through the shadow)
resolves theme colours and inherited formatting to plain values; ODP connectors
lose their glue; ODP groups lose a group rotation; picture fills of shapes are
dropped in ODP; animations are not converted between formats.

## Rendering and measured fidelity

`render` lays text out in each shape's box (`layout`: bullets, numbering,
indents, UAX 14 line breaking, spacing, alignment, columns, anchoring, the
stored autofit scale) and draws backgrounds, master decoration, shapes with
theme style references, gradients (axial and radial shadings), pictures with
crops, groups, connectors and tables into a PDF with subset fonts (real,
searchable text). Missing fonts are replaced by metric-compatible stand-ins and
listed in the report.

Measured on 12 decks (16 slides) made by PowerPoint 16 and exported by it,
against LibreOffice 26.8 rendering the same decks (`FIDELITY_REPORT.md`):

| comparison with PowerPoint's PDF | lines broken alike | mean baseline Δ | mean SSIM | closer on |
|---|---|---|---|---|
| **Letters** | **100 %** | **0.30 pt** | **0.972** | **12 of 16 slides** |
| LibreOffice | 51.2 % | 3.71 pt | 0.937 | 4 of 16 |

On LibreOffice's own ODP decks (10 slides) Letters is about as close to
LibreOffice as PowerPoint is (SSIM 0.890 vs 0.895): the box layout follows
PowerPoint's line rules, not LibreOffice's.

Back and forth (`tests/back_and_forth.rs`): PPTX -> ODP -> PPTX through Letters
renders in PowerPoint identically to the original on all 12 decks (SSIM 1.000;
PowerPoint's own chain 0.940-1.000); ODP -> PPTX -> ODP through Letters renders
in LibreOffice identically on all 12 (LibreOffice's own chain 0.990-1.000).

Converted decks open in the applications: 10 PowerPoint decks changed by one
word and saved by Letters open in PowerPoint without repair with every shape's
name, position, size and text unchanged but the word; 10 LibreOffice decks
likewise in LibreOffice; 10 decks converted from ODP and a deck made from
nothing open in PowerPoint.

Speed (release build, this machine): a 300-slide deck opens in 14 ms, renders
every slide to PDF in 195 ms and saves in 21 ms.

## Tests

```
set CARGO_TARGET_DIR=C:\Letters\target-slides
cargo test -p letters-slides                       # everything that needs no application
cargo test -p letters-slides --features office     # plus the letters-office bridge
cargo test -p letters-slides --test applications -- --ignored --nocapture --test-threads=1
cargo test -p letters-slides --release --test fidelity -- --ignored --nocapture
cargo test -p letters-slides --release --test back_and_forth -- --ignored --nocapture
cargo clippy -p letters-slides --all-targets --features office
cargo fmt -p letters-slides
```

The ignored tests need the corpus: `tests/fidelity/powerpoint_corpus.ps1 -Out
tests/fidelity/out/corpus` (PowerPoint), then `tests/fidelity/libreoffice_convert.ps1`
for LibreOffice's PDF, ODP and PPTX versions (see WORKLOG for the exact calls).
The PowerPoint scripts refuse to run beside another PowerPoint, start their own
(minimised; PowerPoint cannot run invisible), remember its process id and end
only that one, in a `finally` block. LibreOffice runs headless with a private
profile and is always waited for.

`tests/decks` holds small decks made by PowerPoint (`pp_*`) and saved by
LibreOffice (`lo_*`) for the tests that need no application.

## Files

| Folder | What it holds |
|---|---|
| `model/` | the deck, slides, layouts, masters, themes, objects, text, fills, sources |
| `pptx/` | relationships, DrawingML pieces (`dml/`), text (`text/`), the shape tree (`objects/`), reading (`read/`) and writing (`write/`) |
| `odp/` | styles, text, shapes, geometry, paint, tables, pictures, reading and writing |
| `preserve.rs`, `shadow.rs` | original XML reuse and patching; the lossless back-and-forth copy |
| `inherit.rs`, `color.rs`, `geometry/` | effective values, colour transforms, outlines |
| `layout/`, `render/`, `export.rs` | text in boxes, scenes and PDF, exports |
| `edit/`, `outline.rs`, `blank.rs`, `office_bridge.rs` | editing operations, outlines, new decks, the office bridge |
| `xml/`, `package.rs`, `base64.rs`, `fonts/`, `render/pdf_fonts.rs`, `render/pdf_images.rs` | copies of letters-office's and letters-layout's helpers (see below) |

### To unify later

`xml/`, `package.rs` and `base64.rs` are copies of letters-office's modules and
`fonts/`, `render/pdf_fonts.rs`, `render/pdf_images.rs` of letters-layout's,
made while those crates were mid-edit and did not build; they belong in one
shared crate. The box text layout (`layout/`) shares ideas with core/layout's
`text` and `paragraph` stages and could share their shaping (it does not kern
yet).

## Libraries

| Library | Used for | License |
|---|---|---|
| [lopdf](https://crates.io/crates/lopdf) | writing PDF | MIT |
| [ttf-parser](https://crates.io/crates/ttf-parser) | reading fonts | MIT or Apache-2.0 |
| [unicode-linebreak](https://crates.io/crates/unicode-linebreak) | UAX #14 break opportunities | Apache-2.0 |
| [zip](https://crates.io/crates/zip) (deflate through zlib-rs) | the package container | MIT; Zlib |
| [png](https://crates.io/crates/png) | decoding pictures for the PDF | MIT or Apache-2.0 |
| [serde](https://serde.rs), serde_json, [thiserror](https://crates.io/crates/thiserror) | the model as JSON, fingerprints, errors | MIT or Apache-2.0 |
| letters-office (optional), letters-pdf and pdfium-render (tests only) | the bridge; reading and rendering PDFs for the harness | this repository; MIT or Apache-2.0; PDFium Apache-2.0 |

Nothing here restricts distribution. No LibreOffice source was used; its
output was only observed.
