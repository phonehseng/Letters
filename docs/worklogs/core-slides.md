# letters-slides worklog

What changed in this crate and why, newest last.

## 2026-11-12 crate dependencies

Added letters-office (its xml reader, zip package helpers and base64), letters-layout
(font lookup with metric-compatible stand-ins and the pdf font embedding), lopdf,
serde, serde_json and unicode-linebreak. All MIT or Apache-2.0. Reusing the office
xml reader keeps one tested parser for every office format; reusing the layout font
library means slides and documents substitute fonts the same way.

## 2026-11-12 presentation model

Added `model/`: a deck, slides, layouts, masters, themes, notes, transitions and
animations (kept), comments, sections, embedded fonts and the media table; objects
mirroring drawingml (shapes, pictures, groups, connectors, graphic frames with tables,
charts, diagrams, ole objects); text with per-level styles, bullets, spacing and
autofit; colours left unresolved (scheme slot plus transforms) so a theme change
reaches every colour that follows it. Units are pptx's own (emu, 60000ths of a
degree, hundredths of a point) so reading and writing pptx never rounds. letters-office
has no canvas page shape types that keep scheme colours yet, so the slide types are
defined here; see README for how they map onto office's `Shape` and `Group`.

## 2026-11-12 preservation helpers

Added `preserve.rs`: every element remembers its original xml and a fingerprint of
the model as read. Writers put the original back while the fingerprint matches and
otherwise rebuild only the changed children, copying every other byte. This is what
"nothing moves when you click" rests on: an untouched object is byte-identical.

## 2026-11-12 pptx reading and writing of drawingml

Added `pptx/rels.rs` (relationships and content types), `pptx/dml.rs` (colours,
fills, lines, transforms, geometry, style references), `pptx/text.rs` (text bodies,
paragraphs, runs, level styles), `pptx/objects.rs` (the shape tree), `pptx/theme.rs`
and `pptx/read.rs` (the package into the model). Unknown attributes and children are
kept as raw xml with the name of the modelled element they followed, so a rebuilt
element puts them back in the same place.

## 2026-11-12 split the shape tree code by object kind

`pptx/objects.rs` had grown past the readable size; split into `pptx/objects/`
with one file per object kind and one for the shared non-visual properties, so each
file can be read on its own (docs/CONTRIBUTING.md asks for files under ~500 lines).

## 2026-11-12 own copies of the xml reader, zip helpers and font lookup

letters-office and letters-layout were mid-edit by another engineer and did not
compile, which blocked every build of this crate. Their xml reader, package helpers,
base64, font lookup (`fonts/`) and pdf font and picture embedding (`render/pdf_*`)
are small, self-contained and this repository's own code, so they were copied here
(`xml/`, `package.rs`, `base64.rs`, `fonts/`, `render/`). The xml copy adds the
presentationml and opendocument presentation namespaces. To unify later: move them
into one shared crate that office, layout and slides all use.

## 2026-11-12 pptx writer

Added `pptx/write/`: part names (`names.rs`), the output package (`package.rs`),
the shared `p:cSld` patching (`canvas.rs`), themes, masters and layouts (`parts.rs`),
slides and notes (`slides.rs`), presentation.xml, content types and dropping parts
no relationship reaches (`deck.rs`), and parts made from nothing (`fresh.rs`).
Relationship ids of a part are kept (a duplicated slide copies its original's),
unchanged relationship and content-type lists keep their bytes.

## 2026-11-12 corpus and round-trip tests

`tests/fidelity/powerpoint_corpus.ps1` builds 12 decks with powerpoint's automation
(themes, two content, table with a merge, cropped and rotated pictures, autoshapes
with adjust values, gradients, groups, glued connectors, shrink-on-overflow text,
notes, 4:3 and A4 sizes) and exports them to pdf; `libreoffice_convert.ps1` makes
libreoffice's pdf, odp and pptx versions. Small decks are copied to `tests/decks`.
`tests/pptx_roundtrip.rs`: saving without edits keeps every part byte for byte (all
20 pptx decks, both applications); one retyped word changes only that object; a moved
shape changes only `a:off`; broken input never panics.
## 2026-11-12 colours, inheritance and geometry

`color.rs` resolves drawingml colours: scheme slots through the colour map, then each
transform in order (luminance, saturation and hue in hsl; tint and shade on linear
light). Checked against powerpoint's palette: accent 1 lighter 40 % is 8FAADC and
darker 25 % is 2F5597. `inherit.rs` finds a placeholder's layout and master
placeholders (by index first, then by kind) and resolves effective transforms,
paragraph, run and body properties through master text styles, list styles and the
shape. `geometry/` evaluates drawingml guide formulas and draws 24 preset outlines
(written from the standard's shape definitions) and custom paths; any other preset
draws as its box and is reported.

## 2026-11-12 slide renderer

`layout/` lays out shape text (bullets and auto numbers, indents, greedy uax 14 line
breaking, spacing, alignment, columns, anchoring, the stored autofit scale); `render/`
turns a slide into a scene (backgrounds, master and layout decoration, shapes with
theme style references, pictures with crops, groups, connectors, tables with the
default table style) and writes it as pdf with subset fonts. A separate box layout
was chosen over core/layout because shape text follows powerpoint's spacing rules
and needs no pages; README says what to unify later.

## 2026-11-12 fidelity harness and the first measured rules

`tests/fidelity.rs` renders the corpus, reads powerpoint's, libreoffice's and our pdfs
with the same pdfium reader and scores lines, baselines, boxes and ssim per slide.
First run: lines agreed 100 % but baselines were 11.5 pt off on average. The
measurements showed powerpoint's line pitch is 1.2 x size x spacing for any font and
the baseline splits a line in the font's hhea ascent to descent proportion (every
baseline of pp_title_bullets within 0.1 pt); with that rule the mean baseline error
fell to 0.30 pt (libreoffice: 3.71 pt). Also fixed from the renders: picture clips
were popped before drawing, gradients ignored stop alpha (ion's 7 % glow drew as a
grey disc), duotone theme backgrounds are now their two colours' mix, and table cells
used the frame's insets instead of the cell's.

## 2026-11-12 application checks

`tests/applications.rs` (ignored): ten powerpoint decks changed by one word and saved
by letters open in powerpoint without repair with every shape's name, position, size
and text as before; ten libreoffice decks likewise in libreoffice. Powerpoint itself
refuses libreoffice 26.8's own pptx saves of the corpus ("corrupted"), so the
libreoffice-made baseline uses its odp saves.
## 2026-11-12 odp reader and writer

`odp/`: styles resolved through their parent chains (`styles.rs`), text with lists,
spans and fields flattened onto each paragraph and run (`read_text.rs`), frames,
custom shapes (libreoffice's "ooxml-*" names map back to drawingml presets with their
handle values; plain-number enhanced paths become custom paths), lines, connectors,
groups, pictures with crops (odf crops by length, drawingml by share, so the picture's
natural size is read from its png or jpeg header), tables and embedded objects
(`read_shapes.rs`, `images.rs`), masters with their presentation styles as text
styles (`read.rs`). The writer keeps an odp deck's styles.xml, settings and every
unchanged page and object byte for byte, and writes fresh parts for decks from pptx
with formatting resolved (`write*.rs`). Lengths are written with six decimals of a
centimetre so every emu survives (three decimals moved objects by up to 180 emu).

## 2026-11-12 lossless back and forth

A converted deck carries its original as an extra package part with a fingerprint of
every slide as written (`shadow.rs`); letters reads it back when no slide changed, so
pptx -> odp -> pptx and odp -> pptx -> odp through letters render identically to the
original (ssim 1.000 on all 24 decks; powerpoint's own chain 0.940-1.000,
libreoffice's 0.990-1.000). A file another application changed is read as it now is.

## 2026-11-12 what powerpoint refused in converted decks

Found with `tests/applications.rs` (diagnose variants): a slide master may not hold a
subtitle placeholder (libreoffice's title-slide master pages have one; now mapped to
the body or a plain text box), a preset name drawingml does not define makes the file
"corrupted" ("non-primitive" from libreoffice's free shapes; now a custom path, and
the writer falls back to "rect" for any unknown name), handle values need the
preset's own names (a star's are adj, hf, vf), every master needs its own theme part,
and opendocument xml kept as preserved objects must not be written into pptx.
Libreoffice refused our odp when the manifest named odf 1.3 for a 1.4 document; the
manifest now takes content.xml's version.
## 2026-11-12 editing operations, exports and outlines

`edit/`: every edit the editor needs as an `Operation` applied by `edit::apply`,
which returns the changed slides and an inverse. Inverses restore snapshots of what
the edit touched (a slide, the slide list, the themes, the geometry), so undo is exact
and untouched objects keep their original xml through edit and undo. Placeholders
that inherit their position get it written down when first moved. `export.rs`: the
deck as pdf with a slide per page, notes pages, or handouts with 2, 3, 4 or 6 slides
a page, and a pdf per slide for pictures. `outline.rs`: the outline view, the outline
as text and a deck from an outline; `blank.rs` makes new decks and slides.

## 2026-11-12 the letters-office bridge, as a feature

Once letters-office built again: `office_bridge.rs` turns a document's headings into
a deck and slides into canvas pages and back (text with basic marks, presets with
handle values, exact emu positions, picture fills). It is behind the `office` feature
so this crate keeps building while letters-office changes.

## 2026-11-12 readable sizes

Split the files the readability check listed: `pptx/dml/` (colours, fills and lines,
geometry), `pptx/read/` (package, parts, slide extras), `pptx/text/` (read, write),
`model/geometry.rs` and `model/frames.rs`, `odp/read_geometry.rs`, `read_paint.rs`
and `read_tables.rs`, and the long functions in part naming, the odp path reader and
two tests. Removed the chart model (never filled; charts are kept and drawn by the
applications) and the diagnosis test whose findings are recorded above.

## 2026-11-12 libreoffice's own decks in the harness, and shrink-to-fit

The second fidelity set is now libreoffice's odp saves (powerpoint cannot open its
pptx saves): letters reading the odp is about as close to libreoffice as powerpoint
is (ssim 0.890 vs 0.895) but breaks few lines alike, because the box layout follows
powerpoint's rules. Text that shrinks on overflow with no stored scale now gets one
worked out, only for decks from odp: doing it for pptx made pp_autofit_notes worse,
since powerpoint draws the stored state and never recomputes when exporting.