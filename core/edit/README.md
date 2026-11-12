# letters-edit

Edits page content directly: text in place, and images. These are real
changes to the page's drawing instructions, not annotations laid on top.

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
- Errors are sentences for the person using the app (`src/error.rs`).
- Parameters and results use camelCase in JSON.

## Operations

| Function | What it does to the file |
|---|---|
| `page_objects(doc, page)` | reads the page's text as blocks, lines and runs (with font, size, colour, character boxes, and which characters each font can still draw) and its images; changes nothing |
| `replace_text(doc, page, line_or_block_id, text, &options)` | a line: re-sets it from its original start and baseline, growing or shrinking. A block: re-sets the text over the block, wrapped to its width with its alignment and line spacing (or line by line with `reflow: false`), and reports overflow |
| `insert_text(doc, page, line_id, index, text, &options)` | inserts characters; the rest of the line keeps its own glyphs and spacing and moves along |
| `delete_chars(doc, page, line_id, start, end)` | removes characters; the rest of the line closes up |
| `delete_text(doc, page, id)` | removes a block or line (its space stays empty) or a run (its line closes up) |
| `set_text_style(doc, page, id, range, &style, &options)` | changes the size, colour or (to a standard font) font of a run, line, block or character range; text after it on the line moves to fit |
| `add_text(doc, page, &NewText)` | adds new page text, upright on screen, in a standard font or a caller's TrueType file (subset to the glyphs used and written as a composite font with `/ToUnicode`) |
| `add_image(doc, page, rect, bytes)` | adds a PNG (transparency as a soft mask) or JPEG (stored unchanged) on top of the page |
| `replace_image(doc, page, id, bytes, fit)` | draws a new picture in that one image's place: stretched, fitted or filled (cropped to the place) |
| `move_image` / `resize_image` / `rotate_image(doc, page, id, ...)` | changes the placement of exactly that drawing by wrapping its operator in `q <matrix> cm ... Q` |
| `delete_image(doc, page, id)` | removes that drawing; the image object stays if anything else draws it |
| `extract_image(doc, page, id)` | returns the original JPEG bytes, or a PNG (with alpha from a soft mask); changes nothing |
| `bring_image_to_front` / `send_image_to_back(doc, page, id)` | moves that drawing to the end or start of the page content with the matrix it had |

## How an edit is written

Only the operators involved are rewritten; every other byte of the content
stream stays as it was.

- Each removed glyph is taken out of its show operator (`Tj`, `TJ`, `'`, `"`)
  and replaced by a move of the same length (a `TJ` number). The operator still
  ends exactly where it did, so nothing after it shifts.
- The new glyphs are written right after the operator where the edit starts,
  each group placed with an absolute text matrix. Positions come from the font's
  widths with the run's character spacing, word spacing and horizontal scaling.
- Then the text state is set back exactly as the original operator left it:
  font, colour and rise, the line matrix with `Tm`, and the text matrix with a
  `TJ` of numbers only. Later operators find the state they always found.
- Text after the edit point on the same line keeps its own glyphs, fonts and
  kerning, shifted by however much wider or narrower the change is.
- A form XObject being edited is always copied first, and only the drawing being
  edited is pointed at the copy, so other drawings of the same form don't
  change. Its `/BBox` grows to hold the new content. A page content stream
  shared with another page is copied too.
- New fonts and images are added to the right resource dictionary under new
  names. Inherited page resources are copied onto the page, so other pages don't
  change.

## Keeping the original font: what it can and can't do

The original font is used for every character it can really draw:

- **Not embedded** (the reader supplies the glyphs): any character its encoding
  holds, from `/Encoding`, `/Differences` and `/ToUnicode`.
- **Embedded TrueType** (`FontFile2`), simple or composite: only characters
  whose glyph is really in the embedded program and has an outline. The cmaps
  are looked up the way readers do. A subset font usually holds only what the
  page already uses.
- **Embedded Type1** (`FontFile`): only glyph names in the program's
  `/CharStrings`, read after decrypting it.
- **Embedded CFF** (`FontFile3` that isn't OpenType): Letters can't look inside
  these yet. Only characters the page already draws in that font, plus any the
  descriptor's `/CharSet` lists, count as drawable. This errs on the safe side.
- **Composite fonts** are written to only with `Identity-H`/`Identity-V` codes.
  Characters the full embedded font has but `/ToUnicode` doesn't list are added
  to a new `/ToUnicode` map, so copied text stays right.
- **Type3** fonts can draw only glyphs in their `/CharProcs`.

When a character is missing, the fallback chain is:
1. The same font installed on this computer (found by PostScript name in the
   standard font folders). If its OS/2 fsType allows embedding, the needed
   glyphs are copied in as a subset; if it allows only whole-font embedding, the
   whole font is copied. A font that forbids embedding is never used.
2. Otherwise a similar standard font (Helvetica, Times or Courier, with bold and
   italic matched).

Every fallback comes back as a `FontNotice` with a sentence such as "The
original font can’t draw “é”, so Helvetica was used for the new text." Only
the missing characters change font. With `allowStandardFonts: false` the edit
is refused instead.

## Known limits

- Re-set text is positioned by the font's widths. Kerning inside the replaced
  part is not kept (text after the edit keeps its kerning). Ligatures are taken
  whole when an edit touches them.
- Word gaps that a pdf makes by moving (with no space glyph) appear as spaces in
  the line text. Character indexes count them.
- Line and block grouping follows geometry (same baseline, small gaps; similar
  size, even spacing and lined-up edges for blocks). Unusual layouts can group
  differently from how a person reads them. Reflow moves only the block's own
  lines; text below the block is not pushed down, and overflow is reported for
  the UI to handle.
- Justified reflow spreads extra width over spaces with `TJ` numbers.
  Vertical writing (`Identity-V`) is read as if horizontal.
- A colour that differs from the edit's starting point and was set in a colour
  space other than gray, RGB or CMYK is written back as RGB.
- When a block is reflowed, all its new text sits in the marked-content sequence
  of its first line. Marked content stays balanced, but `/ActualText`, `/Alt`
  and the structure tree are not updated.
- Copying fonts from the computer works for TrueType outlines (.ttf and .ttc).
  OpenType CFF (.otf) fonts can't be subset and are refused. Each edit that
  copies glyphs adds a new subset font rather than extending an earlier one.
- Inline images (BI ... EI) can be moved, resized, turned, deleted and
  restacked, but not saved out or replaced.
- Images are saved out as PNG only from 8-bit gray, RGB, CMYK, ICC-based and
  indexed RGB data (and 1-bit gray). JPEG 2000, CCITT and JBIG2 images are
  refused with a message.
- Restacking an image drops any clipping path that cut it where it was.
- Form XObjects are always copied when edited. An original that nothing else
  uses stays in the file until the file is cleaned up.

## Libraries

| Library | Used for | License |
|---|---|---|
| lopdf | the pdf object model | MIT |
| ttf-parser | reading TrueType and OpenType fonts | MIT or Apache-2.0 |
| png | reading and writing png pictures | MIT or Apache-2.0 |
| serde, thiserror | small helpers | MIT or Apache-2.0 |

TrueType subsetting is written here; it keeps glyph numbers, so composite fonts
use them directly as CIDs. The standard font widths come from Adobe's freely
usable Core 14 AFM metrics (`src/fonts/metrics.rs`), and the encodings and glyph
names from the pdf standard and the Adobe Glyph List.

## Tests

```
cargo test -p letters-edit
```

The tests build pdfs by hand: standard fonts, a subset TrueType font with a
custom encoding, a composite font, forms drawn twice, several content streams,
marked content, and rotated and cropped pages. Fonts are made of box glyphs by
`tests/support/font.rs`. After each edit the tests save and reload the file,
check the operators and structure, find the text with letters-pdf's search
(pdfium) and check pixels. One test copies a glyph from Windows' installed
Arial, and skips with a message where Arial isn't installed. The tests need
pdfium in `vendor/pdfium`.
