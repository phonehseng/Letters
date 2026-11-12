# letters-layout

Letters' own page layout for word-processor documents: a document from
`letters-office` (DOCX or ODT) in, pages of positioned glyphs, rules and
pictures out, with no Microsoft Word or LibreOffice involved. The same result
drives exact pagination, Export as PDF and printing.

This is a first model, fitted empirically: Word 16 and LibreOffice 26.8 on
this machine are the reference oracles, and every rule that was changed is
backed by a measurement on the oracle corpus (`tests/oracle`). It is not at
parity with Word; how far it is, document class by document class, is in
[ORACLE_REPORT.md](ORACLE_REPORT.md).

## Public API

| Call | What it does |
|---|---|
| `layout(&Document, &LayoutOptions) -> LayoutResult` | lays out a whole document |
| `LayoutOptions { flavor, fonts }`, `LayoutOptions::flavor(Flavor::Word / LibreOffice)` | which rule set, and where fonts come from |
| `LayoutEngine::new(options).layout(&doc)` | incremental relayout: blocks that did not change are reused (`engine.stats` says how many) |
| `pdf::write_pdf(&result, &doc) -> Vec<u8>` | Export as PDF: subset CID TrueType fonts with ToUnicode, real text, pictures, vector rules |
| `layout_exact(&[ExactPage])`, `layout_mixed(&[MixedPart])`, `ExactPage::new`, `push_run` | exact-position content (a converted PDF page) drawn exactly where given, alone or mixed with flowing parts |
| `LayoutResult::locate(&ModelPosition) -> Caret`, `hit_test(page, x, y)`, `selection_boxes(from, to)` | model position to page coordinates and back, for the editor |
| `fonts::FontSource` (trait), `SystemFonts`, `ProvidedFonts`, `LibraryFonts`, `FontLibrary` | font lookup; `LibraryFonts` is letters-fonts behind the trait (metric twins, look-alikes, fallback chains) |

`LayoutResult` (serde, camelCase) holds `pages` (size, `lines` with `baseline`,
`x`, `top`, `height`, `carets` and glyph `runs` — font id, size, glyph ids,
per-glyph x, the text and its model offsets — plus `rects`, `images`,
`frames`), `fonts`, `substitutions` (every font that had to be replaced, and
why) and `unsupported` (features found but not laid out, with where).

Model positions: a paragraph is named by its `story` (body, a header, a note,
a text box, a positioned page) and a block path (block index; for table cells
also row, cell, block in cell). Offsets count one unit per character of text
and one per tab, break, picture, note reference and field.

## Pipeline

```
fonts      find faces (document-embedded, office cloud, system), metrics, substitutions
style      resolved paragraph and run formatting in points (incl. preserved xml)
text       inlines -> shaped clusters (cmap/hmtx, rustybuzz when needed), UAX #14 breaks
paragraph  greedy first fit, hanging spaces, tabs and leaders, alignment, justification,
           line spacing (vertical.rs)
lists      labels, label alignment, tab or space after the label
tables     column widths (grid or autofit), cell margins, borders, rows across pages,
           repeated header rows, merged and nested cells
floats     floating pictures and text boxes, square / top-and-bottom wrap
footnotes  note area per page (and per column), separators, splitting
columns    balancing at continuous section ends
pages      sections, headers and footers, keeps, widow/orphan control, page breaks
result     the result and the model <-> page map
pdf        the pdf writer
positioned exact-position pages and mixed documents
incremental the caching engine
```

`flavor.rs` holds every rule where Word and LibreOffice were measured to
differ, each with the measurement next to it.

### Breaks imitate, drawing is exact

Rules that decide **where lines and pages break** imitate the oracle,
including Word's rounding where it decides a break. **Where glyphs are drawn**
keeps full precision: the result and our PDF hold fractional points (four
decimals), never snapped to Word's 1/600 inch layout grid or its PDF writer's
1/3000 inch lattice. Our pages are therefore slightly more precise than
Word's own, by up to about 0.12 pt.

## Measured agreement

See [ORACLE_REPORT.md](ORACLE_REPORT.md) for every table (fit, held-out and
whole corpus; per class with the other application as the baseline to beat;
per document; 150 dpi rendering; layout times; history). Round two widened the
corpus from 30 to 116 documents: 85 written by our docx writer from their own
seeds, 3 Word-made files from the fidelity work, 16 made by Word itself through
its object model from its own templates and 12 ODT files made by LibreOffice
itself. 73 are fitted on, 43 are held out and were never looked at while
fitting. Whole corpus, before (round-one rules on the new corpus) and after:

| comparison | page breaks agree | lines at the same words | baseline mean (pt) |
|---|---|---|---|
| LibreOffice vs Word (the baseline to beat) | 49.6 % | 78.5 % | 32.5 |
| **Letters (Word flavor) vs Word**, before | 72.6 % | 93.4 % | 24.7 |
| **Letters (Word flavor) vs Word**, after | **77.8 %** | **96.9 %** | 15.4 |
| held-out set only, after | 72.1 % | 96.0 % | 14.7 |
| Word vs LibreOffice (the baseline to beat) | 49.6 % | 76.8 % | 32.6 |
| **Letters (LibreOffice flavor) vs LibreOffice**, before | 71.9 %* | 89.9 %* | 13.0 |
| **Letters (LibreOffice flavor) vs LibreOffice**, after | **72.6 %** | **94.1 %** | 14.7 |
| ODT made by LibreOffice: Word vs LibreOffice | 54.9 % | 84.7 % | 30.2 |
| ODT made by LibreOffice: Letters, before | 52.2 % | 76.3 % | 18.7 |
| ODT made by LibreOffice: Letters, after | **56.4 %** | **93.1 %** | 20.7 |

\* after the table rules; the round-one LibreOffice flavor on the new corpus
was 70.8 % / 86.0 % on the fit set. The ODT gain comes mostly from the office
crate's ODT reader, which now reads LibreOffice's column sections and page
borders (another part of the project), and from the squeeze rule below.

Rendered at 150 dpi, our PDF is closer to Word's than LibreOffice's is (mean
SSIM 0.921 against 0.812) and closer to LibreOffice's than Word's is (0.889
against 0.812).

**Without Microsoft's fonts** (only the open fonts in vendor/fonts, what a
student without Office has): 59.7 % of page breaks and 82.5 % of lines like
Word, still ahead of LibreOffice with Microsoft's fonts (49.6 % / 78.5 %).
Carlito, Arimo, Tinos and Cousine stand in exactly for Calibri, Arial, Times
New Roman and Courier New; Cambria, Georgia, Aptos, Segoe UI and Verdana have
no metric twin, and that is where the loss is.

## Known gaps, by impact

1. ODT import of some LibreOffice features is the office crate's (sections,
   page border padding); layout follows what the reader gives it.
2. Long Word-made tables still land rows on other pages (lines match).
3. Justified text: the squeeze share alone is not Word's rule.
4. Word re-fits earlier columns when later columns' footnotes grow the
   shared note area; this model does not go back.
5. Floats: the file's wrap distances, tight and through wrap polygons and
   text boxes that grow with their text are approximate.
6. LibreOffice page flow (header room, widows) is fitted coarsely.
7. Word's 1/600 inch grid is deliberately not copied for drawing
   (`word_grid_drawing` snaps for side-by-side checks only).
8. Hyphenation is off (no clearly permissive pattern set; both
   applications leave it off by default); right-to-left, East Asian and
   complex scripts are reported and laid out left to right; shapes and
   groups are reported and skipped.

## Editing: incremental relayout and the provider

`LayoutEngine` reuses every unchanged block's lines and every page it can:
placing restarts at the last safe old page before the edit and stops when a
new page starts exactly where an old one did, reusing the old pages with
block and page numbers moved. Lines carry a stable `id`;
`LayoutEngine::changes` lists changed and only renumbered pages and
`changed_pages` returns them. The property test (tests/incremental.rs)
checks incremental equals full layout after 24 random edits on a document
with lists, long tables with header rows, floating pictures, text boxes and
footnotes. Measured in a release build on a 565-page double-spaced document:
full layout 277 ms; one character typed: median 96 ms, worst 121 ms, with
557 of 565 pages reused on average.

`provider::word_layout(&Document, &LayoutOptions)` and
`provider::word_layout_json(json, flavor)` answer in
`letters_office::bridge::WordLayout`'s shape (pages, lines in twips with
story, paragraph in Word's order and main-text character range, floating
objects), so the editor can switch between Word's layout and ours.
## Plain statement

This first model reproduces Word's line and page breaks closely for ordinary
student documents (body text in common fonts, spacing, lists, headings,
footnotes, simple tables, headers and footers, columns) and draws them more
precisely than Word's own PDF. It is not a replacement for Word yet: complex
tables, floating layouts, fields, complex scripts and Word's many
compatibility options are either approximate or missing, and every number here
was measured on one machine with one Word build (16.0.20527) and one
LibreOffice build (26.8.2).

## Tests and the oracle harness

```
set CARGO_TARGET_DIR=C:\Letters\target-layout
cargo test -p letters-layout                     # unit, engine and incremental tests
cargo test -p letters-layout --test roundtrip -- --ignored --nocapture   # exact positions round trip
cargo test -p letters-layout --release --test incremental -- --ignored --nocapture   # timings
powershell tests\oracle\run.ps1 -Iteration "name" [-Render]          # the whole oracle loop
cargo clippy -p letters-layout --all-targets
cargo fmt -p letters-layout
```

The oracle loop writes the corpus (`tests/oracle/corpus.rs`, letters-office's
writer, plus three Word-made documents from the fidelity work), exports it with
a private, invisible Word (one process, its id recorded, ended in a finally
block and by a watchdog; documents opened read-only; no options changed) and a
headless LibreOffice with a private profile, converts it to ODT with
LibreOffice and exports those with both, reads every PDF's glyphs with pdfium,
lays the corpus out with both flavors, writes our PDFs and reads them back with
the same reader, and scores. `ORACLE_RENDER=1` adds 300 dpi rendering scores
(letters-pdf).

## Libraries

| Library | Used for | License |
|---|---|---|
| [ttf-parser](https://crates.io/crates/ttf-parser) | reading fonts | MIT or Apache-2.0 |
| [rustybuzz](https://crates.io/crates/rustybuzz) | shaping (kerning, ligatures, complex scripts) | MIT |
| [unicode-linebreak](https://crates.io/crates/unicode-linebreak) | UAX #14 break opportunities | Apache-2.0 |
| [unicode-bidi](https://crates.io/crates/unicode-bidi), [unicode-segmentation](https://crates.io/crates/unicode-segmentation) | script and text analysis | MIT or Apache-2.0 |
| [lopdf](https://crates.io/crates/lopdf) | writing PDF | MIT |
| [png](https://crates.io/crates/png) | decoding pictures for the PDF | MIT or Apache-2.0 |
| [serde](https://serde.rs), [serde_json](https://crates.io/crates/serde_json), [thiserror](https://crates.io/crates/thiserror) | the result as JSON, the provider's JSON entry point, errors | MIT or Apache-2.0 |
| letters-fonts (this workspace) | faces, metric-compatible stand-ins, fallback chains | the project's own |
| [zip](https://crates.io/crates/zip) (tests only) | patching corpus documents after writing | MIT |
| pdfium-render, letters-pdf (tests only) | reading glyph positions and rendering for the harness | MIT or Apache-2.0; PDFium Apache-2.0 |

Nothing here restricts distribution. No LibreOffice source was used; its PDF
output was only observed as an oracle.
