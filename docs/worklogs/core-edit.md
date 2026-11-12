# letters-edit worklog

What changed in this crate and why, one entry per piece of work, newest last.

## 2026-11-12 round two begins: what round one left

Round one (text blocks, lines and runs; replace, insert and delete text in
the original font with fallback notices; reflow inside a block without moving
what is below; add text; image add, replace, move, resize, rotate, delete,
extract and restack) is the base. The app's Edit tab is built against its API,
so everything in round two is additive: new functions, new optional fields,
no changed signatures or JSON shapes.

## 2026-11-12 the interpreter records paths, stroke state, clips and marked content

`interpret.rs` now follows path construction (m l c v y h re), painting
(S s f F f* B B* b b* n), clipping (W W*), shadings (sh), the stroke colour,
line width, dash, caps and joins, ExtGState opacity, q-block numbers and
BMC/BDC ... EMC sequences. Vector graphics can't be offered as objects
without knowing where each path's operators start and end and how it was
painted; reflow needs the same to push drawings down a column; structure
upkeep needs to know which marked-content sequence holds what. Colours now
keep their space name and numbers, so moved or new content can reuse a spot
or ICC colour exactly instead of converting it to RGB.

## 2026-11-12 paths in page_objects, with suggested groups

`PageObjects` gains `paths` and `pathGroups` (with `#[serde(default)]`, so
older JSON still reads). Each path reports its shape (rectangle, line,
ellipse, free, shading) on screen, fill and stroke exactly as set, opacity,
whether a clip cuts it, and the editable points of rectangles and lines.
Groups are suggested from a shared q block, a shared marked-content sequence,
or runs of touching paths (how logos are drawn), so the ui can select a logo
as one object; grouping stays a ui notion and nothing in the file changes.
`ImageObject` gains `cropRect`, `pixelsPerInch` and `opacity` so the ui can
show crops and warn about low-resolution replacements.

## 2026-11-12 path editing: move, resize, turn, restyle, delete, restack, duplicate, points

`paths.rs` edits only the chosen drawings: a move wraps the path's own
operator range in `q <matrix> cm ... Q`; a restyle wraps it with colour,
width, dash or ExtGState operators and swaps the painting operator when a
fill or outline is added or removed; points of rectangles and lines are
rewritten in place so line widths don't scale (table rules need that). A
pattern fill is given a moved copy of its pattern, since pattern space
doesn't follow `cm`. Paths in forms go through the existing copy-on-write.
`moving.rs` holds the shared movers (text show operators, images, paths,
annotations), `color_ops.rs` writes a colour in its original space in any
stream, `edit_run.rs` holds the steps every new edit shares, `summary.rs`
the change summary and consent types.

## 2026-11-12 signed documents need consent

`structure.rs` finds signed signature fields; every new edit takes an
`EditConsent` and refuses with `EditError::SignaturesWouldBreak` until the
person accepts that the signatures become invalid. `content_edit_check`
lets the ui ask before the first edit. A new error variant is additive for
the shell, which matches errors with a catch-all arm.

## 2026-11-12 paragraph reflow that pushes the column down

Starting `reflow/`: re-setting a paragraph that grows or shrinks moves the
content below it in the same column by the height change, stops at the next
column, the footer or the bottom margin, and reports overflow with choices
instead of doing something silent. Round one's `replace_text` is left as it
was, because the Edit tab relies on its behaviour.

## 2026-11-12 interpret.rs split into interpret/ along its seams

`interpret.rs` had grown to 1,178 lines. It is now `interpret/mod.rs` (the
runner and its operator families: state, text, colour, objects),
`records.rs` (what running content finds), `text.rs` (show operators and
glyphs), `paths.rs` (path construction and painting) and `state.rs`
(marked content, ExtGState, colour spaces). The long `run` loop became one
small dispatcher and one function per family of operators, so each fits on
a screen. No behaviour changed: all 74 tests passed before and after.

## 2026-11-12 reflow: what landed and how it decides

`reflow_paragraph` re-sets a block to its width and alignment (a one-line
block wraps at its column's right edge, since it has no width of its own),
then moves text blocks, images, paths and annotations inside the column
band below it by the change in height. The band is the block's extent
widened by blocks stacked with it (at least half their own width inside),
so a short paragraph still finds its whole column. The bottom limit is the
caller's, else the top of an obstacle crossing the band edge, else the
footer (text in the lowest fifth repeating on the neighbouring pages, digits
ignored), else a bottom margin taken equal to the top margin (at least half
an inch) but never above content already there. Overflow is reported with
four choices and, by default, nothing changes. Tightening tries 64 settings
gentlest first (word spaces to 85%, letter spacing to -0.015 em, leading to
97%, leading counted double so it is used last). Carrying moves the lines
that don't fit to the top of the same column on the next page, written in
the same font objects, spacing and colour space, and pushes that column
down; it is offered only when nothing lies below the paragraph, and a page
pushed past its own limit is reported rather than carried again (one level
of cascade). The kerning the page already uses between letter pairs in the
paragraph's font is repeated in the new text. Tests: 10, with pixel checks
that the column moved by exactly the line pitch and nothing else changed.

## 2026-11-12 tables: editing a cell grows its row

Starting `reflow/table.rs`: ruled grids are found from straight line paths
and thin rectangles; editing a cell re-sets its text to the cell's width
and, when it needs more height, grows the row: rows, rules and content
below move down, and vertical rules crossing the row's bottom edge are
lengthened by rewriting their coordinates, so line widths stay exact.

## 2026-11-12 tables landed; rules now keep table rows out of one block

`edit_table_cell` finds the cell around a text block from straight line
paths and thin rectangles (the nearest enclosing rules on all four sides),
re-sets the text to the cell's width keeping its inset on both sides, and
grows the row by what the text needs beyond its old bottom padding. Content
below the row inside the table's width moves down, and vertical rules that
cross the row's bottom edge get their lower ends rewritten, so a 1 pt rule
stays 1 pt. To make cells editable at all, `layout.rs` no longer joins two
lines into one block when a thin horizontal rule runs between them (lower
than 0.3 of the size below the upper baseline, so underlines don't split
paragraphs); before, evenly spaced rows of a table read as one paragraph.
This changes `page_objects` grouping for ruled tables only. Tests: 3.

## 2026-11-12 new text in the same font: appending look-alike glyphs

`letters_fonts::complete` doesn't exist yet (core/fonts has `matching`,
still being written). So glyph donors sit behind a small `GlyphSource`
trait in `fonts/donor.rs`, with two sources now: the same font installed
on this computer, and a font file the app passes in (a look-alike chosen
with the font library). `fonts/extend.rs` appends the donor's outlines to
the embedded TrueType program of the font already on the page, under new
glyph ids, and updates the widths, /ToUnicode, the descriptor's font file
and, for composite fonts, /W and /CIDToGIDMap. Existing glyph ids, widths
and the bytes of existing text are untouched, so the new text is drawn by
the same font object as the old text around it.

## 2026-11-12 the corporate-letter case, measured

`tests/corporate.rs` builds the product owner's case: a logo of four paths,
a justified paragraph in an embedded subset of Windows' Arial set in a
spot colour (Separation), and a footer repeated on page two. Adding "From
1 July, the café opens at 7 am — ask Quentin for the €5 voucher." needs 9
characters the subset lacks (1 5 7 F J Q é — €). With the installed Arial
as donor they are appended to the same font object: no fallback notice,
every run of the paragraph in one font id, no rgb operator written, the
text found by pdfium's search. Measured by drawing the edited page again
with the subset swapped for the full Arial (same content, so only glyphs
can differ): 0 of 2131 inked pixels of the added lines differ by more
than a quarter of the range. With Tahoma as a look-alike donor (scaled to
Arial's cap height) 101 of 2151 differ (4.7%). A first attempt measured
against a separately typeset reference line; an untouched line already
differed by 9.9% that way (pdfium snaps glyphs differently when each glyph
has its own Tm), so that measure was dropped as too noisy. Separation and
DeviceN colours now show as ink (tint 1 looks dark), not as grey.

## 2026-11-12 Match Style, and image crop, cut, flip and opacity

`add_text_matching` adds a text box in the style of the nearest paragraph
(or a chosen one): the same font object (extended when it lacks a
character), size, character and word spacing, leading, alignment and the
fill colour in its own colour space; the test checks a Separation colour is
written as `/CS0 cs 1 scn`, not converted. The set lines are written by
`reflow/standalone.rs`, which carrying to the next page now shares.

`image_tools.rs`: `crop_image` wraps the drawing in `q x y w h re W n ...
Q` in the image's unit square, so every pixel stays and round one's
`replace_image` keeps the crop (it replaces only the Do operator inside the
clip). `crop_image_pixels` cuts the pixels away (decoding jpeg or png with
the `image` crate, MIT or Apache-2.0, and storing the cut as a lossless
png-style image) and draws it exactly where the kept part was; the test
checks it renders pixel-identical to the clipped version. `flip_image`
mirrors in the unit square; `set_image_opacity` adds an ExtGState. Tests: 5.

## 2026-11-12 page margins, page size and page boxes

Starting `page_boxes.rs`: report each page's content bounds and margins;
change margins by scaling and shifting all content with one matrix around
the page's content streams (annotations get the same matrix, so links and
widgets stay on what they mark); resize the page with fit, fill or keep
size; add or remove white space on one side (only the boxes change); set
MediaBox, CropBox, BleedBox, TrimBox and ArtBox to exact numbers.
core/pages' crop hides part of a page by setting /CropBox from a box seen
on screen; nothing here duplicates it.

## 2026-11-12 page boxes landed

`page_margins`, `set_page_margins` (scale to fit or shift), `resize_page`
(fit, fill, keep size; the other boxes are removed since they described
the old paper), `add_page_space` (a binding margin: only MediaBox and
CropBox move on the side seen on screen, worked out through the page's
rotation) and `page_boxes` / `set_page_boxes` (exact numbers, every box
checked to lie inside the MediaBox). Tests: 6, including the link staying
over its heading after the content is scaled and the second page's
objects unchanged.

## 2026-11-12 scan to editable text

Starting `scan/`: `convert_scan_to_text` turns a scanned page into real,
visible text using the caller's recognition result (letters-ocr's
`OcrPage`, a dependency for the types only). The scan's pixels give each
line's look: ink colour, stroke weight, slant and serif or monospace
character, matched to the closest standard face; each word is fitted to
its box with horizontal scaling. Two modes: text over the scan with the
text areas painted out in the local background colour, or text only with
the non-text regions kept as pictures. Every line reports its confidence.

## 2026-11-12 scan conversion landed, with its measurements

`scan/` converts a scanned page with the caller's `OcrPage`. Each line's
ink and paper colours, stem width, stroke contrast and slant are measured
from the scan (strokes are measured after the slant is taken out, or
italics read as low-contrast); monospace is read from how evenly the words'
widths per letter run. The closest standard face is chosen (Helvetica,
Times or Courier, bold and italic), and each word is placed at its box's
left on its baseline and scaled across (Tz) to the box's width. Text over
the scan paints each word's box (1 pt larger) with the paper colour
measured around its line; text only drops the scan and keeps inked
regions no word covers (touching or overlapping regions merged, specks
under 10 pt dropped) as pictures. The serif threshold was first 0.7;
Georgia measured 0.75 and was read as sans, so it is now 0.85, between the
measured serif faces (Times 0.50, Times Italic 0.67, Georgia 0.75) and
the sans and typewriter faces (all 1.00). Measured on generated 216 dpi
jpeg scans with perfect recognition boxes: faces in the standard set 6 of
6 right, letter positions exact, ssim 0.999 to 1.000; faces outside it
(Arial, Verdana, Georgia, Courier New) all given the right family, mean
letter error 0 to 0.31 pt, ssim 0.80 to 1.00 (Verdana 0.80 and Georgia
0.81 are the honest gap between a real typeface and its closest standard
one). Tests: 4.

## 2026-11-12 structure upkeep: ActualText follows changed text

A reflowed or re-set paragraph inside a marked-content sequence that
carries /ActualText (or /Alt) and holds nothing but that paragraph gets
the new text in it, since the person changed that text; nothing else's
ActualText is touched. Tests check MCIDs stay in order and sequences stay
balanced after reflow, and that signed and locked files are refused.

## 2026-11-12 structure checks landed; files split to stay readable

ActualText upkeep, the signature gate and the lock check are tested (4
tests, including MCIDs keeping their order after a reflow pushes a tagged
paragraph down). The performance test edits page 150 of a 300-page file in
about the time page 2 of a 3-page file takes (3.8 ms against 4.0 ms, debug
build) and checks four other pages' objects are unchanged.

For the readability rules, `paths.rs` (687 lines) became `paths/` (moving,
`style.rs`, `restack.rs`, `points.rs`); round one's `text_edit.rs` (844)
became `text_edit/` (the api, `line.rs`, `block.rs`); `layout.rs` (649)
became `layout/` (ids and finding, `grouping.rs`, `objects.rs`);
`fonts/pdf_font.rs` (813) became `fonts/pdf_font/` (the font, `dictionary.rs`,
`program.rs`), and `PdfFont::load` gave its ascent and descent reading to a
helper. No behaviour changed; all 117 tests passed after each split. Dead
fields found by the compiler (marked-content ids and parents nothing read,
an annotation subtype, two unused helpers) were removed. The one remaining
long function is round one's test font builder `tests/support/font.rs`
`box_font` (152 lines of table bytes), left as it is.

## 2026-11-12 README for round two

The README lists every operation with its exact effect on the file, the
overflow choices and how the bottom limit is found, what keeping the
original font and matching a scan's typeface can and can't do (with the
measured numbers), the limits, the source files and the tests.

## 2026-11-12 glyphs for missing letters come from letters-fonts' completion

The small `GlyphSource` stand-in (the same font found by name, or a file
scaled to the target's cap height) is replaced by
`letters_fonts::complete::complete_font`, so a subset font gains missing
letters from the same font on this computer, else a near font fitted to it,
else letters built from its own (accents), else the closest font as it is.
Generated glyphs are never asked for. The app lends its font library for one
edit with `with_font_donors` (a thread-local, so parallel edits and tests
can't see each other's donors); without it the same font installed here is
the only donor, as before. `FontExtension` gained `matchMethod`,
`similarity` and a one-sentence `sentence` for the person (new fields with
defaults). Measured on the corporate test: the installed Arial still draws
the added lines with 0 of 2131 pixels different; Tahoma alone, now fitted,
gives 97 of 2170 (4.5%) against 101 of 2151 with cap-height scaling.
