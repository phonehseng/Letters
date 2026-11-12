# letters-convert

Turns PDFs into other formats (pictures, plain text, web pages,
spreadsheets, slides), pictures into PDFs, and office documents into PDFs
through the programs installed on the computer. It also reads the
*structure* of a PDF (headings, paragraphs, lists, tables, figures, headers
and footers, reading order), which the "Easy to edit" writers build on.

The pixel-exact PDF to Word ("Exact match" .docx) engine is developed
separately and is not part of this crate. Word and OpenDocument "Easy to
edit" output is written by the office crate from the structure model made
here (see "The structure model" below).

## Every conversion

| From → to | How | Quality | Limits |
|---|---|---|---|
| PDF → PNG | the app's renderer draws each page; encoded here | exact picture at the chosen dpi, colour or gray, resolution stored (pHYs) | text becomes pixels |
| PDF → JPEG | same, quality 1–100 | lossy; resolution stored (JFIF density) | whole dpi only in the header |
| PDF → TIFF | same, LZW, Deflate or none; one file per page or all pages in one | lossless; resolution stored (XResolution, inch) | |
| PDF → text, reading order ("Easy to edit") | structure analysis | paragraphs on one line each, blank line between blocks, columns read in turn, hyphens at line ends rejoined (optional), headers/footers left out (optional), page separators (none, form feed, or a "--- Page N ---" line) | tables come out as tab separated rows |
| PDF → text, layout ("Exact match") | every line placed on a monospace grid | keeps columns, indents and gaps (up to 3 blank lines) | a grid can only approximate proportional text |
| PDF → HTML, Exact match | each page a positioned box; each run of text a real, selectable span at its place; a picture of everything that isn't text behind it | looks like the PDF; text selectable, searchable, translatable | browser fonts differ a little in width; see fonts below |
| PDF → HTML, Easy to edit | structure analysis | h1–h6, p, nested ul/ol, tables with th scope and colspan/rowspan, figure with figcaption and alt text, links, footnotes in an aside section; no inline styles | layout is not kept, by design |
| PDF → XLSX | structure analysis, then a spreadsheet written by hand (zip + xml) | real cells: numbers typed with their display format, dates as dates, merged cells, column widths and row heights from the PDF, font, bold, italic, colour, fill, borders, alignment; one sheet per page (with the text around tables in place) or per table | only tables become a grid; see the picture decision below |
| PDF → PPTX ("Exact match") | slides written by hand; a picture of the page without text and rebuilt parts as background | text never rasterised: real text boxes per line with font, size, colour, bold, italic, links; pictures and simple shapes as real objects; report per slide of what is in the background | one slide size for the deck (pages of other sizes are fitted and centred) |
| PDF → DOCX, Exact match | separate engine | – | not in this crate |
| PDF → DOCX / ODT, Easy to edit | the office crate's writers, from `analyze_structure` | – | not written here |
| PNG / JPEG / TIFF → PDF | letters-ocr's `images_to_pdf`, saved here | JPEG kept as it is; others lossless; optional invisible text layer from OCR | first page of a multi-page TIFF only (letters-ocr) |
| DOCX, DOC, ODT, RTF → PDF | Microsoft Word (Windows, COM) or LibreOffice | as the program lays it out | needs one of them installed |
| XLSX, XLS, ODS → PDF | Microsoft Excel or LibreOffice | as the program prints it | |
| PPTX, PPT, ODP → PDF | Microsoft PowerPoint or LibreOffice | as the program exports it | |

`capabilities(&find_office_converters())` returns this list for the
computer it runs on, each entry with `from`, `to`, `style` (exact match or
easy to edit), `how`, a one sentence `note` for the export dialog, and
whether it is `available`. The two sentences that explain the styles are
`EXACT_MATCH_EXPLAINED` and `EASY_TO_EDIT_EXPLAINED`.

## Using it from the shell

```rust
use letters_convert::*;

// every pdf conversion starts from a source; a locked pdf gives
// "This PDF is locked with a password. Unlock it first, then try again."
let source = PdfSource::open(&path)?;
let cancel = CancelFlag::new();               // keep a clone, cancel() from any thread
let mut progress = |p: Progress| emit(p);     // { page, done, total } after each page

// the renderer is the app's: draw one page of the pdf file at `pdf` at
// `scale` pixels per point, onto white, as rgba
struct AppRenderer { /* holds the RenderWorker */ }
impl PageRenderer for AppRenderer {
    fn render(&mut self, pdf: &Path, page: usize, scale: f32) -> Result<RgbaImage, ConvertError> { ... }
}

let files: Vec<PathBuf> = pdf_to_images(&source, &mut renderer, &ImageExportOptions {
    format: ImageFormat::Tiff { compression: TiffCompression::Lzw, multi_page: true },
    dpi: 300.0, pages: PageSelection::Pages(vec![0, 2]), color: ColorMode::Grayscale,
}, &folder, "Essay", &mut progress, &cancel)?;

let text: String = pdf_to_text(&source, &TextExportOptions::default(), &mut progress, &cancel)?;
write_text_file(&source, &options, &path, &mut progress, &cancel)?;

let report: HtmlReport = pdf_to_html(&source, &mut renderer, &HtmlExportOptions {
    mode: HtmlMode::ExactMatch, self_contained: true, ..Default::default()
}, &path, &mut progress, &cancel)?;

let report: XlsxReport = pdf_to_xlsx(&source, &XlsxExportOptions::default(), &path, &mut progress, &cancel)?;
let report: PptxReport = pdf_to_pptx(&source, &mut renderer, &PptxExportOptions::default(), &path, &mut progress, &cancel)?;

let structure: StructuredDocument = analyze_structure(&source, &StructureOptions::default(), &mut progress, &cancel)?;

images_to_pdf_file(&pictures, &ImagesToPdfOptions::default(), None /* or Some((&engine, &ocr_options)) */,
    &path, &mut progress, &cancel)?;

let converters = find_office_converters();    // looks only; starts nothing
let done: OfficeConversion = office_to_pdf(&docx, &pdf, &converters, &OfficeOptions::default(), &cancel)?;

let choices: Vec<Capability> = capabilities(&converters);
```

All options and reports serialise with camelCase names. Output paths
chosen in a save panel are replaced (the panel already asked); files made
beside them (pictures per page, an HTML page's `<name>_files` folder) never
replace anything: a taken name gets " (2)". Every writer writes to a
temporary file next to the target and renames it at the end, so a failed
or cancelled export leaves nothing half written, and picture exports
delete the files they made if they fail.

## Modules

| File | Job |
|---|---|
| `source.rs` | loading (locked and empty PDFs refused), page sizes and the shown page (crop box, /Rotate), page choices |
| `render.rs` | the `PageRenderer` trait, pixels, size limits (20 000 px a side, 200 megapixels) |
| `images.rs`, `naming.rs` | PNG/JPEG/TIFF writing; file names that never replace a file |
| `content/` | a small content stream reader: paths (lines, boxes, shapes) with colours, pictures with their placement, clipping and transparency noticed; `strip.rs` makes copies without text and chosen objects for background pictures |
| `structure/` | the analysis (below) and its model |
| `text.rs` | plain text, both modes |
| `html/` | `fixed.rs` exact match, `semantic.rs` easy to edit, `fonts.rs` the PDF's own fonts |
| `xlsx/` | `values.rs` reading numbers, currency, percentages and dates; `styles.rs` the style sheet; `mod.rs` sheets and the package |
| `pptx/` | `mod.rs` planning each page, `slide.rs` shapes and text boxes, `parts.rs` master, layout, theme |
| `package/` | the zip writer and xml escaping shared by xlsx and pptx |
| `office/` | finding and running Word, Excel, PowerPoint and LibreOffice; `office-to-pdf.ps1` is the script shipped for Office |
| `capabilities.rs` | the list for the export dialog |

## Structure analysis

Page by page, from letters-edit's text lines and this crate's vector reader:

1. **Fragments.** Lines are cut where the gap between two letters is wider
   than 1.4 times the text size, so table columns and page columns that
   share a baseline come apart. Spaces are put in word gaps that have no
   space character. A marker drawn apart from its text ("•" then a gap)
   is joined to it, and small raised numbers become superscripts of the
   text beside them. Link annotations are applied to the characters under
   them.
2. **Headers and footers.** Text in the top or bottom tenth of a page that
   repeats (digits counting as any number) on at least half the pages, at
   the same height, is set apart as `RepeatedText`, with page numbers
   ("7", "Page 7 of 20", "- 7 -") recognised when they count with the page.
3. **Tables.** Ruled tables come from horizontal and vertical lines
   (stroked lines, box edges, and filled boxes thinner than 2.5 points)
   that cross; cells without a line between them are merged into spans;
   each character goes into the cell under its middle; shading, borders
   and alignment are read per cell. Then text lined up in columns: three
   or more rows in a row, each with pieces in two or more of the same
   columns (columns taken from the rows below the first, so a heading over
   two columns doesn't join them). Two columns of long or column-filling
   lines are page columns, not a table. The header row is guessed from
   bold text, shading, or words over columns of numbers.
4. **Reading order.** Gutters are found as upright strips no narrow piece
   of text crosses, with text on both sides. Anything crossing a gutter
   (a heading over both columns, a wide table) splits the page into bands;
   each band is read column by column, top to bottom.
5. **Blocks.** Lines are joined into paragraphs while the spacing stays the
   page's usual line spacing, the size and weight stay the same, and no
   indent, short line, or list marker starts a new one. Headings are short
   paragraphs at least 1.15 times the body size, or bold and short with no
   full stop; levels come from their sizes across the whole document. List
   items start with a marker, keep their wrapped lines (aligned with the
   item's words) and nest by indent. Footnotes are small numbered lines low
   on the page, more certain under a short rule. Captions ("Figure 1",
   "Table 2", or small or italic text just below a picture) are attached to
   their figure or table.

Every block has a `confidence` from 0 to 1, as do table header guesses and
captions. The document is read twice (once for what repeats and the usual
text size, once for blocks), keeping one page's characters at a time.

### The structure model, and how the office writers should map it

`analyze_structure` returns a `StructuredDocument` (serde, camelCase):
`title`, `pages` (size and column count), `blocks` in reading order,
`repeated`, and `bodySize`. Every block carries `page`, `rect` and
`confidence`, and its content is tagged by `type`:

| Block | Word (DOCX) | OpenDocument (ODT) |
|---|---|---|
| `heading { level, runs }` | paragraph with style Heading *level* | `text:h` with `text:outline-level` |
| `paragraph { runs }` | paragraph, Normal style | `text:p` |
| `list { items: [{ level, ordered, marker, runs }] }` | paragraphs with a numbering definition: bullet or decimal per item, `w:ilvl` = level | `text:list` nested by level, `text:list-item` |
| `table { rows, columns, columnWidths, rowHeights, cells: [{ row, column, rowSpan, columnSpan, runs, fill, borders, align }], headerRows, caption }` | `w:tbl` with `w:gridCol` widths (points × 20 = twips), `w:gridSpan`/`w:vMerge` for spans, `w:tblHeader` on header rows, shading and borders per cell; caption as a Caption paragraph | `table:table`, `number-columns-spanned`/`number-rows-spanned`, `table:table-header-rows` |
| `figure { imageId, pixelWidth, pixelHeight, caption }` | inline picture from `letters_edit::extract_image(doc, page, imageId)`, sized to `rect`; caption below | `draw:frame` with `draw:image`, caption |
| `footnote { marker, runs }` | a real footnote (`w:footnoteReference` where the marker is a superscript run in the text above, else at the end of the page's text) | `text:note` |
| `repeated` with kind header/footer | the section header or footer; `pageNumber: true` becomes a PAGE field | `style:header`/`style:footer` with `text:page-number` |

Runs carry `text`, `fontFamily` (the family people know: "Times New
Roman" for `TimesNewRomanPS-BoldMT`), `genericFamily`, `bold`, `italic`,
`size` (points), `color` ([r, g, b]), `link` (a web address, or `#page=N`
for a page of the same document), `superscript` and `fontRef`
(letters-edit's id for the PDF font). Writers should map the PDF's
standard fonts to ones office programs have (Helvetica to Arial, Times to
Times New Roman, Courier to Courier New), as `structure::fonts` does here.

## Exact match HTML

Each page is a `section` of the page's size in points. Each run of text is
a span placed at its first character, its top `0.84 × size` above the
baseline, which is where browsers put the baseline of Arial, Helvetica and
Times in a line box one size tall. Text of a scan's invisible text layer
stays transparent but selectable. Turned text is rotated about its start.

The background picture is drawn from a copy of the page whose text show
operators are removed (every other operator stays, so colours and states
are unchanged); pages that draw nothing but text get no picture. It is a
PNG, or a JPEG when the page has photos.

Fonts: the PDF's embedded TrueType and OpenType fonts are used with
`@font-face` when their embedding permission (OS/2 fsType) allows it and a
browser would find every character used in them by Unicode. Many PDFs keep
cut-down fonts mapped by the PDF's own codes; those would show the wrong
letters, so the page falls back to the family name, a font with the same
widths and the generic family. Fonts that allow it are still offered as
files in the `_files` folder. Fonts that forbid embedding are never
copied. `HtmlReport.fontNotes` says which and why.

## XLSX: why there is no picture behind the cells

A spreadsheet can't put a picture behind cells, only float one over them,
where it would hide the cells it copies, get in the way of selecting,
sorting and typing, and print twice. So the sheet is made to *look* like
the table instead: column widths and row heights come from the PDF
(converted with Excel's own rule for widths), with the fonts, shading,
borders and alignment. On "a sheet per page", all tables of a page share
one grid of column edges, text blocks sit in the column under their left
edge, and gaps on the page become spacer rows, so the sheet keeps the
page's shape.

Values: "1,234.50" is 1234.5 shown as `#,##0.00`; "$1,234.50" keeps the
currency (`"$"#,##0.00`); "($5.00)" is -5 with brackets for negatives;
"12.5%" is 0.125 as `0.0%`; dates in ISO form, with a month name, or
day-first/month-first when the day is over 12, become dates in that form.
Anything doubtful stays text: leading zeros, more than 15 digits, decimal
commas, and dates like 03/04/2024 that could be read two ways.

## PPTX

One slide per page, the slide the size of the first page (kept within
PowerPoint's 1 to 56 inches by scaling the whole deck). Pages of another
size are scaled to fit and centred, and the report says so.

- **Text**: one text box per line (per fragment), not wrapping, no inner
  margins, placed so its baseline is where the PDF's is (measured in
  PowerPoint's own export: within 0.3 point for Times 12, about 2 points
  low for Arial Bold 24). Font family (mapped to office fonts), size,
  colour, bold, italic, superscript and web links are kept. Invisible text
  over a scan stays invisible (fully transparent) but selectable.
- **Pictures** that letters-edit can copy out (JPEG as is, others as PNG)
  are placed at their box, if drawn upright, unclipped and opaque.
- **Shapes**: paths in plain colours with straight sides, drawn on the page
  itself (not inside a form), unclipped and opaque: boxes as rectangles,
  two-point strokes as lines, others as free-form shapes. Up to 1500 a page.
- **Everything else** (curves, gradients, patterns, clipped or see-through
  drawing, drawing inside forms, inline pictures, annotations) is drawn
  into a background picture from a copy of the page without its text and
  without the rebuilt objects, so nothing shows twice. Pages with nothing
  left get no background. `SlideReport.inBackground` lists, in sentences,
  what is in it.

The package has the minimum PowerPoint needs: presentation, one blank
master and layout, a complete theme, presProps, viewProps, tableStyles and
the document properties.

## Office documents to PDF

Letters has no layout engine of its own for office documents, so it asks a
program that has one:

- **Microsoft Office on Windows**, through COM automation, driven by
  `office/office-to-pdf.ps1` run with Windows PowerShell (chosen over a
  COM client in Rust: late-bound COM calls with Office's optional and
  by-reference arguments would need a lot of unsafe code, while the script
  says every step in Office's own words, and a hang inside Office can't
  take Letters with it). The script: starts its own invisible copy, with
  alerts off; opens the file read only, not added to recent files, with a
  made-up password so a locked file fails at once instead of asking;
  exports with `ExportAsFixedFormat` (Word, Excel) or `SaveAs` PDF
  (PowerPoint); closes the file; quits only its own copy. PowerPoint only
  ever runs once, so if it was already open, COM hands over that copy: then
  nothing about it is changed, the file is opened without a window, and
  PowerPoint is left running. The script prints the process id of its own
  copy as soon as it knows it, and Letters stops exactly that process (and
  the script) when the time limit (180 seconds by default) passes or the
  person cancels.
- **LibreOffice** on any system: `soffice --headless --convert-to pdf`,
  with a private profile in the temporary folder so a LibreOffice the
  person has open is never handed the job or changed.
- **macOS**: Pages, Keynote and Numbers (and Office for Mac) can be
  scripted with AppleScript through `osascript` (for example `tell
  application "Pages" to export (open file) to file "out.pdf" as PDF`).
  This is not built: it can't be run or tested on this machine. On a Mac
  only LibreOffice is used today, and the "nothing installed" message names
  the Mac programs it found and says how to export from them by hand.

With nothing installed the message is, on Windows: "Letters can’t turn
Word files into PDFs on this computer, because it needs a program that can
open them and none is installed. Install Microsoft Word or the free
LibreOffice, then try again."

## Errors

`ConvertError` messages are sentences for the person using the app, in
the style of letters-pdf: locked ("Unlock it first, then try again."),
empty, unknown page, cancelled, damaged, could not be saved, unsupported
format, no converter (naming the options), converter failed, timed out
(naming the program and the limit), and picture errors passed on from
letters-ocr.

## Tests

```
cargo test -p letters-convert                         # 74 unit, 37 integration tests
cargo test -p letters-convert -- --ignored            # the 8 that need Microsoft Office
```

Integration tests write PDFs by hand (`tests/support/mod.rs`: standard
fonts, lines, boxes, curves, pictures, links, an encrypted trailer, an
empty page tree) and draw them with letters-pdf (`vendor/pdfium` needed).
The OCR test needs `vendor/tesseract`.

- pictures: sizes, pixels where the red square is, pHYs, JFIF density,
  TIFF pages, compression and resolution, gray, page choice, names that
  never replace a file, progress, cancelling (files removed), locked,
  empty and bad settings
- text: one column, two columns with a heading over both, hyphenation on
  and off, page separators, layout mode, headers and page numbers, cancel
- structure: a three page handbook: block kinds in order, heading levels,
  nested and numbered lists, a ruled table (spans, header, borders,
  caption), a white space table, a figure with caption, a link, a
  footnote, repeated header and page numbers, serde round trip
- HTML: span positions read back from the output and compared with the
  PDF, fonts, links, the background picture checked for no text and for
  the box; semantic structure parsed and checked; nothing left on cancel
- XLSX: the package unzipped and sheet, styles and shared strings parsed:
  values, types, number formats, merges, widths, heights; per page and per
  table; no tables
- PPTX: the package unzipped: slide size, text box positions, sizes, fonts,
  colours, rectangle, line and picture positions, background picture
  holding the curve but no text, box or picture; report sentences
- Office: Word DOCX (hand-written OOXML) and RTF to PDF, page count and
  text read with letters-pdf; a damaged file; cancelling; Excel and
  PowerPoint turning this crate's own XLSX and PPTX back into PDFs; Excel
  opening the XLSX (no repair, values and formatted text read back);
  PowerPoint opening the PPTX (no repair, slide count, size, text and the
  title's position)

## Libraries

| Library | Used for | License |
|---|---|---|
| lopdf | the PDF object model and content streams | MIT |
| letters-edit, letters-ocr | page text and pictures; pictures to PDF | this project |
| image (jpeg only) | JPEG writing with density | MIT or Apache-2.0 |
| png | PNG writing with pHYs | MIT or Apache-2.0 |
| tiff (lzw, deflate) | TIFF writing | MIT |
| flate2 | deflate for the zip writer | MIT or Apache-2.0 |
| ttf-parser | font permissions and character coverage | MIT or Apache-2.0 |
| serde, thiserror | small helpers | MIT or Apache-2.0 |
| roxmltree (tests only) | reading the xml written | MIT or Apache-2.0 |

Nothing restricts distribution. Microsoft Office and LibreOffice are used
only if the person has installed them, as separate programs.

## Known limits

- Reading order handles columns with gutters; text wrapped around
  pictures, and unusual layouts, can come out in another order.
- White space tables need three rows in a row with two or more lined-up
  columns; a row with a single filled cell ends the table. Tables ruled
  only by horizontal lines (no verticals) are found only if their text is
  lined up.
- Paragraphs that continue on the next page stay two paragraphs.
- Captions above pictures are only taken when they start with a caption
  word ("Figure", "Table", ...).
- PPTX text positions depend on the font PowerPoint uses (see above);
  text boxes don't stretch their characters to the PDF's widths.
- Exact match HTML relies on fonts browsers have when the PDF's own
  can't be used, so lines can be a little longer or shorter.
- Text inside annotations (form fields, free text comments) is in the
  background picture of slides and HTML pages, not real text.
- The Word copy is found by comparing Word processes before and after it
  starts; if another program starts Word at the same moment, the document
  window is used to tell them apart. If the conversion is cancelled in the
  first moment, before the script reports its copy, that copy can be left
  running.
- LibreOffice was not installed here, so its path was not run (its
  command, profile and time limit are unit tested only as far as possible).
