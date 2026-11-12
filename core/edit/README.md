# letters-edit

Edits page content directly: text in place, paragraphs that reflow down
their column, tables, vector drawings, images, page margins and boxes, and
scans turned into real text. These are real changes to the page's drawing
instructions, not annotations laid on top, and never a conversion of the
file to another format.

Every operation takes a `lopdf::Document` already in memory and changes it in
place. The app applies each one to its working copy and keeps its own undo
history. Each edit works out all of its changes first and commits them only if
everything worked, so **a failed edit leaves the document exactly as it was**.

## Rules every operation follows

- Pages count from 0. Positions are points from the top left of the page **as
  the person sees it**, with y going down. Rotation, crop boxes and page boxes
  that don't start at zero are handled here.
- **Ids are only valid until the next edit of the page.** An id names the exact
  operators an object was drawn by: the content stream (`c0` is the page's first
  stream; `c0>12` is the form drawn by operator 12 of it), the operator's
  position and the glyph inside it, plus a fingerprint of the page content.
  Every edit returns the page's objects again with fresh ids. A stale id gives
  "That text or picture has changed since it was selected. Select it again."
- Encrypted documents are refused: "Unlock it first, then try again."
- **Signed documents** (a signature field with a value): every operation added
  in round two takes an `EditConsent` and refuses with
  `EditError::SignaturesWouldBreak` ("This PDF is digitally signed by …
  Changing its content will make the signature invalid. Confirm to edit it
  anyway.") until `invalidateSignatures` is true. `content_edit_check(doc)`
  tells the ui before the first edit. (Round one's functions don't check.)
- Errors are sentences for the person using the app (`src/error.rs`).
- Parameters and results use camelCase in JSON. Round two only added: new
  functions, and new fields with defaults (`PageObjects.paths`,
  `PageObjects.pathGroups`, `ImageObject.cropRect`, `.pixelsPerInch`,
  `.opacity`).
- Only the page being edited is rewritten. Other pages' objects stay byte for
  byte as they were (checked in the tests). Reflow reads the page before and
  after to find a repeated footer (`lookAtNeighbourPages: false` turns that
  off); carrying lines writes the next page too.

## Operations

### Reading

| Function | What it gives |
|---|---|
| `page_objects(doc, page)` | text blocks, lines and runs (font, size, colour, character boxes, which characters each font can still draw), images (with crop, pixels per inch, opacity), paths (shape, fill and stroke exactly as set, line width and dash, opacity, clip, editable points) and suggested path groups; changes nothing |
| `page_margins(doc, page)` | the box around all content and the four margins |
| `page_boxes(doc, page)` | MediaBox, CropBox, BleedBox, TrimBox, ArtBox as they apply (inherited ones too) |
| `content_edit_check(doc)` | whether the file is locked, its signatures, and the sentence to confirm |

### Text (round one)

| Function | What it does to the file |
|---|---|
| `replace_text(doc, page, line_or_block_id, text, &options)` | a line: re-sets it from its original start and baseline. A block: re-sets the text over the block, wrapped to its width; reports overflow but **moves nothing below** (use `reflow_paragraph` for that) |
| `insert_text`, `delete_chars`, `delete_text`, `set_text_style`, `add_text` | as in round one: the edited glyphs are taken out of their show operators (replaced by equal TJ moves) and new glyphs written after them; the text state is set back so nothing after them moves |

### Paragraphs, tables and matching new text (round two)

| Function | What it does to the file |
|---|---|
| `reflow_paragraph(doc, page, block_id, text, &ReflowOptions)` | re-sets the block wrapped to its width (a one-line block wraps at its column's edge), keeping its alignment (justified stays justified, no hyphens are added), then moves every text line, image, path and annotation inside the column band below it by the change in height: each show operator is rewritten with a shifted text matrix and the text state set back; each image or path is wrapped in `q <move> cm … Q`; annotation `/Rect`, `/QuadPoints`, `/InkList`, `/Vertices`, `/L` are moved. Shrinking pulls the content up only with `closeGap`. Overflow is handled as `ReflowOptions.overflow` says (below). |
| `edit_table_cell(doc, page, block_id, text, &ReflowOptions)` | re-sets a cell's text to the cell's width (same inset both sides). When the text needs more height the row grows: rows, rules and content below inside the table's width move down, and vertical rules crossing the row's bottom edge get their lower end rewritten in their own coordinates, so line widths stay exact |
| `add_text_matching(doc, page, &MatchedText, &options, &consent)` | adds a text box in the style of the nearest paragraph (or `likeBlock`): its font object (extended if a character is missing), size, character and word spacing, leading, alignment, and its fill colour in its own colour space (a spot or ICC colour is reused, not converted); the summary names what was matched |
| `extend_font(doc, page, font_id, characters, donor_font_bytes, &consent)` | appends glyphs from a look-alike TrueType file to a font already on the page (see "Keeping the original font") |

When the column can't hold the re-set paragraph, `ReflowOptions.overflow`:

| Choice | What happens |
|---|---|
| `ask` (default) | **nothing changes**; the result has `applied: false` and an `OverflowReport`: points past the limit, the limit and why (`caller`, `obstacle`, `footer`, `bottomMargin`), and for each choice whether it would work and a sentence |
| `allowIntoMargin` | applies when the overflow is at most `marginAllowance` points (default 36) |
| `tighten` | sets the paragraph tighter, gentlest first: word spaces down to 85% of their width, letter spacing down to −0.015 em, then line spacing down to 97%; the amount used is in `summary.tightening`. Refused if even the tightest setting doesn't fit |
| `carryToNextPage` | the lines that don't fit go to the top of the same column on the next page, written in the same font objects, spacing and colour space; that column's content is pushed down. Only offered when nothing lies below the paragraph; when the next page then runs past its own limit it is reported, not carried further |
| `leave` | applies and reports the overflow |

The bottom limit is the caller's `bottomLimit`; else the top of anything below
that reaches across the column's edge; else the footer (text in the lowest
fifth repeating on the neighbouring pages, digits ignored); else a bottom margin
equal to the top margin (at least half an inch), never above content that is
already there.

### Vector drawings (round two)

All take a list of path ids, so a logo (a `pathGroups` entry) moves as one.

| Function | What it does to the file |
|---|---|
| `move_paths`, `resize_paths`, `rotate_paths` | wraps each path's own operators (construction to painting) in `q <matrix> cm … Q`; a pattern fill gets a copy of its pattern with the matrix included, since pattern space doesn't follow `cm`; a shading moves with its clip when the clip is its own. Line widths scale with a resize |
| `set_path_style(doc, page, ids, &PathStyle, &consent)` | wraps the path with new fill or stroke colour (`rgb`, `gray`, `cmyk`, `none`), width, dash or an ExtGState for opacity; swaps the painting operator when a fill or outline is added or removed (keeping close and even-odd) |
| `set_path_points(doc, page, id, points, &consent)` | rewrites a rectangle's corners or a line's ends in the path's own space; the width stays. Free shapes are refused ("Only the points of rectangles and lines can be edited") |
| `delete_paths` | empties the path's operators |
| `bring_paths_to_front`, `send_paths_to_back`, `duplicate_paths` | writes the path again at the end or start of the page with its full state (matrix, colours in their spaces, line style, opacity); the original is emptied (or kept, for a duplicate) |

### Images (round two; round one's add, replace, move, resize, rotate, delete, extract and restack remain)

| Function | What it does to the file |
|---|---|
| `crop_image(doc, page, id, rect, &consent)` | wraps that drawing in `q x y w h re W n … Q` in the image's unit square: every pixel stays. `replace_image` afterwards keeps the crop |
| `crop_image_pixels(doc, page, id, rect_or_none, &consent)` | cuts the pixels outside the crop away: a new, smaller image object (stored losslessly) drawn exactly where the kept part was |
| `flip_image(doc, page, id, horizontal_or_vertical, &consent)` | wraps the drawing in a mirroring `cm` |
| `set_image_opacity(doc, page, id, opacity, &consent)` | wraps the drawing with a new ExtGState (`ca`, `CA`) |

### Page margins and boxes (round two)

| Function | What it does to the file |
|---|---|
| `set_page_margins(doc, page, &Margins, scale_or_shift, &consent)` | one `q <matrix> cm` stream before the page's content and a `Q` stream after it; the same matrix moves every annotation, so links and form fields stay on what they mark |
| `resize_page(doc, page, width, height, fit_fill_or_keep_size, &consent)` | sets a new MediaBox (removing CropBox, BleedBox, TrimBox, ArtBox, which described the old paper) and puts the content on it with one matrix, centred |
| `add_page_space(doc, page, side, points, &consent)` | moves one edge of MediaBox (and CropBox) outwards (or inwards for a negative amount), on the side seen on screen; content stays where it is in pdf space |
| `set_page_boxes(doc, page, &PageBoxes, &consent)` | sets the given boxes to exact numbers (pdf points before rotation); every box must lie inside the MediaBox |

core/pages' `crop` hides part of a page by setting /CropBox from a box seen on
screen, and its page operations reorder, rotate and delete pages; they are not
duplicated here.

### Scans (round two)

| Function | What it does to the file |
|---|---|
| `convert_scan_to_text(doc, page, &OcrPage, &ScanOptions)` | turns the page's scan (an image covering at least half the page) into real, visible text, using the caller's recognition result exactly as read. `textOverCleanedImage`: the scan is replaced by a copy with each word's box painted in the paper colour around its line, and the text drawn on top. `textOnly`: the scan is removed and regions of ink no word covers (figures, signatures, stamps) are kept as pictures. Each line comes back with its confidence, a `lowConfidence` mark, the face chosen, size, ink colour and measured style; paragraphs come back with their alignment |

Every result of a round-two edit has a `summary` (`ChangesSummary`): pages
changed, text lines, images, paths and annotations moved, paths reshaped, fonts
extended (with the characters and where the glyphs came from), tightening
applied, and notes.

## Keeping the original font: what it can and can't do

The original font is used for every character it can really draw (round one:
not embedded fonts by their encoding; embedded TrueType, Type1 and Type3 by the
glyphs really in the program; bare CFF only by what the page already uses).

When a character is missing, round two first **extends the same font object**
(`reflow_paragraph`, `edit_table_cell`, `add_text_matching`, `extend_font`):

- The new glyphs come from letters-fonts' completion
  (`letters_fonts::complete::complete_font`, `src/fonts/completion.rs`): the
  same font found on this computer, else a near font fitted to imitate the
  document's font, else letters built from the font's own (accents), else
  the closest font as it is. Generated glyphs are never asked for. The fonts
  that may lend glyphs are the app's font library, lent for one edit with
  `with_font_donors(&FontDonors, || edit)` (a thread-local, so other edits
  are unaffected); without them only the same font installed here, found by
  its PostScript name, is asked. `extend_font` uses the one file it is given.
  A licence that forbids copying glyphs out of a font is respected there.
- `FontExtension` says how: `glyphSource` ("Arial on this computer"),
  `matchMethod` (letters-fonts' label of the least faithful way used: "same
  font", "adjusted near font", "built from the font’s own glyphs",
  "look-alike"), `similarity` (percent, the least alike glyph) and
  `sentence`, one sentence for the person ("New letters: “€” and “Q” from
  Tahoma on this computer, adjusted to match, 93.1% alike.").
- Only embedded TrueType programs (FontFile2) can be extended: composite fonts
  with Identity-H/V codes, and simple TrueType fonts. The glyphs are appended
  under new glyph numbers; glyf, loca (written long), hmtx, maxp and hhea are
  updated, post becomes version 3 (no names), hdmx, LTSH and VDMX are dropped.
  A composite font gets new CIDs, /W entries and (when it has a map stream)
  /CIDToGIDMap entries; a simple font gets free codes (128 to 255 first), its
  /Widths, /Differences names and cmap entries ((3,1) for non-symbolic fonts,
  (3,0) and (1,0) for symbolic ones). /ToUnicode gains every new character.
  The descriptor points at the new program; a /CIDSet is dropped rather than
  left wrong. **Existing glyph numbers, widths and the bytes of existing text
  are untouched.**
- Not extendable: Type1, CFF, Type3, composite fonts with other CMaps. Then
  round one's chain applies: the same font installed here as a new subset
  font, then a similar standard font, each reported as a `FontNotice`.

Kerning in new text repeats the TJ numbers the page already puts between the
same pair of letters in that font (median of what it shows, numbers under 150
thousandths only); pairs the page never shows get none. Only the page being
edited is looked at.

Measured on the corporate-letter test (justified paragraph in an embedded Arial
subset, spot colour; a sentence needing 9 missing characters): with the
installed Arial as donor, 0 of 2131 inked pixels of the added lines differ by
more than a quarter of the range from the same page drawn with the full Arial;
with Tahoma as the only donor, fitted by letters-fonts, 97 of 2170 (4.5%) do
(scaled to the cap height only, as before letters-fonts, it was 101 of 2151).

## Matching the typeface of a scan: what it can and can't do

The text is always written in a standard face (Helvetica, Times or Courier,
with bold and italic), never in a recognised copy of the scanned typeface. The
face is chosen from the scan's pixels: stem width (bold above 0.115 em), stroke
contrast (serif below 0.85), slant (italic above a shear of 0.12) and whether
words' widths per letter are even (monospace). Each word starts exactly at its
recognised box and is scaled across to its width; size is the recognition's,
colour the sampled ink.

Measured on generated 216 dpi jpeg scans with perfect recognition boxes: faces
of the standard set 6 of 6 right, letters exactly placed, ssim 0.999 to 1.000.
Faces outside it: Arial and Verdana read as Helvetica, Georgia as Times,
Courier New as Courier; letters land 0 to 0.31 points from where they were;
ssim 1.00 (Arial), 0.80 (Verdana), 0.81 (Georgia), 1.00 (Courier New). So
"closely matching" means the right family, weight and slant, exact word
positions and widths; letter shapes of a typeface that isn't one of the three
differ visibly (ssim about 0.8). Real recognition boxes are less exact than
these perfect ones; that wasn't measured here.

## Known limits

- Push-down needs text running left to right on screen; turned blocks reflow
  without moving what is below. A show operator that also draws text outside
  the moved blocks stays (and is noted). A clipping path below a paragraph
  stays where it is (and is noted).
- Carrying lines works one page deep, only for a paragraph at the bottom of
  its column, and the carried lines are not tagged in the structure tree (a
  note says so on tagged files).
- Tables are found from straight line paths and thin rectangles; a grid drawn
  as one many-segment path is a free shape and isn't recognised. Only the edited
  cell's text is re-set; other cells of the grown row keep their place.
- Restacking a path or an image drops a clip that cut it where it was. Paths
  that also clip what follows can't be moved, restyled or restacked on their
  own.
- /ActualText and /Alt are updated only on a marked-content sequence with
  inline properties that holds nothing but the edited paragraph. Marked
  content stays balanced and every MCID keeps its content (BDC and EMC are
  never moved); new text written into a page (Match Style, carried lines,
  converted scans) is not added to the structure tree.
- Round one's limits remain: kerning inside text re-set by round one's
  functions isn't kept; vertical writing is read as horizontal; inline images
  can't be saved out or replaced; jpeg 2000, CCITT and JBIG2 images can't be
  decoded, so they can't be cut (`crop_image_pixels`) or used as scans.
- Scan conversion needs every recognised character to be in WinAnsi (the
  standard faces' encoding); otherwise it is refused with the characters, since
  the text must stay what recognition read.

## Source files

| Path | Job |
|---|---|
| `lib.rs`, `model.rs`, `error.rs`, `summary.rs` | the public api, what the app is told, errors, change summaries and consent |
| `interpret/` | running content streams: glyphs, images, paths, colours, marked content |
| `layout/` | grouping glyphs into lines and blocks, ids, and the objects handed out |
| `separators.rs` | rules between lines that keep table rows apart |
| `path_read.rs` | paths as objects: shapes, paints, groups; image crop and resolution |
| `text_edit/`, `text_write.rs`, `text_fonts.rs`, `add_text.rs` | round one's text edits |
| `reflow/` | paragraph reflow, push-down, overflow choices, tables, Match Style |
| `same_font.rs`, `fonts/extend.rs`, `fonts/append.rs`, `fonts/completion.rs`, `fonts/donor.rs` | keeping new text in the original font object, with glyphs from letters-fonts' completion |
| `paths/` | moving, restyling, reshaping and restacking paths |
| `images.rs`, `image_tools.rs`, `image_data.rs` | image edits and pixel data |
| `page_boxes.rs` | margins, paper size, page boxes |
| `scan/` | scan to editable text |
| `moving.rs`, `color_ops.rs`, `edit_run.rs`, `rewrite.rs`, `draft.rs` | shared movers, colours in their own spaces, the edit steps, writing streams, drafts |
| `structure.rs` | signatures, marked-content checks, ActualText upkeep |
| `fonts/` (others) | encodings, cmaps, standard fonts, reading and subsetting font programs |

## Libraries

| Library | Used for | License |
|---|---|---|
| lopdf | the pdf object model | MIT |
| ttf-parser | reading TrueType and OpenType fonts | MIT or Apache-2.0 |
| png | reading and writing png pictures | MIT or Apache-2.0 |
| image | decoding jpeg and png pixels for cropping and scans | MIT or Apache-2.0 |
| letters-ocr (workspace) | the recognition result's types only | the workspace's |
| letters-fonts (workspace) | completing subset fonts with the letters new text needs | the workspace's |
| serde, thiserror | small helpers | MIT or Apache-2.0 |

TrueType subsetting and glyph appending are written here. The standard font
widths come from Adobe's freely usable Core 14 AFM metrics, and the encodings and
glyph names from the pdf standard and the Adobe Glyph List.

## Tests

```
cargo test -p letters-edit
```

122 tests: 38 unit tests and 84 integration tests in `tests/` (text, fonts,
images, structure from round one; paths, reflow, tables, corporate,
font_extension, image_tools, page_boxes, scan, structure_upkeep, performance).
The pdfs are built by hand. After each edit the tests save and reload the file,
check the operators and structure, find text with letters-pdf's search (pdfium)
and compare pixels: the column below a reflowed paragraph is the same picture
exactly one line pitch lower and the other column, the footer and the next page
are untouched; a table row grows with its rules; a path moves or changes colour
alone; a crop shows the crop; margins change with the link still on its
heading. Some tests use Windows' installed Arial, Tahoma, Verdana, Georgia and
Courier New and skip with a message where they aren't installed. The tests need
pdfium in `vendor/pdfium`. Run the corporate and scan tests with
`-- --nocapture` to see their measurements.
