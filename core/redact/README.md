# letters-redact

Redaction, hidden information removal and passwords for Letters.

This is the part of the app where a mistake hurts a real person: a student
shares a form with their ID number "blacked out", and someone copies the
number out from under the box. So this crate does **true redaction**: what
is under a box is taken out of the file, not covered.

## What you can call

All functions work on a `lopdf::Document` (lopdf 0.47, re-exported as
`letters_redact::lopdf`). Pages count from 0. Positions are points from the
top left corner of the page **as it is shown** (after `/CropBox` and
`/Rotate`), y growing down. Every parameter and result type is serde with
camelCase names, so the shell can pass them straight to and from the UI.

| Function | What it does |
|---|---|
| `redact(&mut doc, &[RedactionArea], &RedactOptions) -> RedactionReport` | removes everything under the boxes, then draws the boxes |
| `preview(&doc, &[RedactionArea]) -> RedactionPreview` | what `redact` (default options) would remove, per box, with exactly the report and warnings `redact` gives; changes nothing |
| `find_text_areas(&doc, &TextQuery, &FindOptions) -> Vec<FoundText>` | boxes for a phrase, a regular expression, email addresses, phone numbers or long numbers |
| `scrub(&mut doc, &ScrubOptions) -> ScrubReport` | removes the chosen kinds of hidden information |
| `scrub_preview(&doc) -> ScrubReport` | lists what every kind would remove; changes nothing |
| `encrypt(&mut doc, &LockOptions)` | locks the file with AES-256 (PDF 2.0 security handler, revision 6) |
| `open(&[u8]) -> Document` | opens a file; a locked one gives `RedactError::Locked` |
| `unlock(&[u8], password) -> Document` | opens a locked file with its open or owner password |
| `lock_info(&doc) -> Option<LockInfo>` | how the file was locked when it was opened |
| `save_clean(&mut doc) -> Vec<u8>` | writes the whole file from scratch. **Always** save with this after redacting or scrubbing |

Errors are `RedactError`, worded for the person using the app. When an
operation returns an error, the document has not been changed: every
operation plans all its changes against the unchanged document first and
only then applies them.

## What is guaranteed

After `redact` and `save_clean`:

- **Text**: every glyph whose box overlaps a box is removed from the string
  that shows it (`Tj`, `TJ`, `'`, `"`). Glyph boxes come from the font's own
  widths (`/Widths`, `/W`, `/MissingWidth`, the standard 14 font metrics),
  the text state (`Tf`, `Tc`, `Tw`, `Tz`, `TL`, `Ts`, text and line
  matrices) and the current matrix. The gap a removed glyph leaves is filled
  with a `TJ` number of exactly its width, so **the text that stays does not
  move**. Simple fonts, Type 0 (CID) fonts with two byte codes (Identity-H/V
  or an embedded CMap) and Type 3 fonts are handled. Text that is invisible
  (render mode 3, the text layer of scanned pages) is removed the same way.
- **Images**: an image fully under a box is removed. One partly under a box
  is replaced by a copy whose covered pixels are the fill colour; the copy
  is saved with lossless Flate. Flate, LZW, RunLength, ASCIIHex and ASCII85
  (with PNG and TIFF predictors) and JPEG (gray and RGB) are decoded. Soft
  masks (`/SMask`) and stencil masks (`/Mask`) of an image are edited the
  same way. Inline images are rewritten too.
- **Vector paths**: a painted path whose box crosses a redaction box is
  **removed whole by default**, because a clipped path keeps its
  coordinates in the file and a path can be handwriting, a signature or
  outlined text. Filled rectangles (backgrounds, table cells, highlights)
  are cut exactly instead. `clipCrossingPaths` keeps crossing paths and
  clips them, for when looks matter more (see the limits below).
- **Forms (XObjects)** are walked with their own matrix. A form that
  changes is copied for this page, so other pages that use the same form
  keep it unchanged.
- **Annotations** whose `/Rect` touches a box are removed with their
  popups and replies; the objects themselves are emptied, so nothing that
  still points at them keeps their text. Links are annotations too. A form
  field widget takes the field's value (`/V`, `/DV`, `/RV`) with it, and the
  field itself once no widget is left. A form's XFA copy is removed.
- **Copies of the content**: text alternatives (`/ActualText`, `/Alt`,
  `/E`) are removed from marked content around removed content and from the
  structure tree elements that point at it. The document's XMP metadata and
  `/PieceInfo`, and the redacted pages' thumbnails, metadata and
  `/PieceInfo`, are removed.
- **Nothing old survives the save**: `save_clean` drops every object nothing
  uses (including the originals of replaced images, forms and content
  streams), numbers the rest again, writes a classic cross reference table
  and no object streams, and keeps nothing of earlier revisions. Never save
  a redacted document incrementally: an incremental save keeps the old
  content in the file, before the new.
- **Redacting twice is the same as once**: the boxes are marked
  (`/LettersRedaction` marked content), and a second redaction of the same
  boxes writes a byte-for-byte identical file.
- **Warnings**: the removed text is looked for in the document title and
  other info, bookmarks, comments and form fields, and on every page; each
  match is reported, as are images that are also used on another page,
  form fields shown in two places, attachments, and permission settings
  that will not be kept. These are reported, not removed, because guessing
  would be wrong as often as right.

`scrub` items, each chosen on its own: document info, XMP metadata (on every
object), attachments (`/EmbeddedFiles`, file attachment annotations,
`/AF`), JavaScript and actions that open programs or send data (`/OpenAction`,
`/AA`, `/Names /JavaScript`, `/A`, XFA), hidden layers (the content drawn on
layers that are off is removed from every page and form, not just the layer
entry), comments, form field values (with their appearances), thumbnails,
`/PieceInfo`, other private program data, unused objects, external links,
and fingerprints (a new random file `/ID`, and the ids in kept XMP).

**Passwords.** The open password is real encryption: AES-256 with a random
256 bit file key, random IVs for every string and stream, and the revision 6
key derivation (SHA-256/384/512). **Permissions are only a request**: a file
with permissions but no open password can be opened by anyone, and many
programs ignore the request. Locked files can be opened if they use RC4 (40
to 128 bit, revisions 2 to 4), AES-128 (revision 4) or AES-256 (revisions 5
and 6). Certificate (public key) security and custom handlers are not
supported. The AES-256 handler itself is lopdf's; this crate chooses the
settings.

## How it is tested

`cargo test -p letters-redact` (pdfium must be in `vendor/pdfium`; run
`tools/fetch-pdfium` first). The test files are written byte by byte, so no
PDF writing library has to be trusted. A known secret is placed in simple
font text (`Tj`, `TJ` with kerning, `'` and `"`), a CID font, a Type 3
font, a form XObject, text split over two content streams, Flate and JPEG
images with known pixel patterns, an inline image, a soft mask, a vector
path, an annotation, a link, a form field value and its appearance, the
structure tree and `/ActualText`, and (for the scrub test) document info,
XMP, two kinds of attachment, scripts, a hidden layer, private data, a
thumbnail and an unused object. After redaction (and scrubbing) and
`save_clean`, the tests check that:

1. the raw bytes do not hold the secret as plain text, lower case, UTF-16
   (both byte orders) or hex, and neither does any stream once inflated;
2. parsing the file with lopdf and walking every object (strings, names,
   decoded streams) finds no trace of it;
3. pdfium's own text search finds neither the secret nor any four letter
   piece of it;
4. pdfium's rendering is exactly the fill colour inside every box and
   pixel for pixel unchanged outside it (so surviving text did not move),
   and pdfium's position of each surviving word is unchanged;
5. the image data, decoded again, has the fill colour under the box and
   the original pixels elsewhere.

Also covered: all four rotations with a crop box that does not start at
the origin (our boxes are checked against pdfium's own positions),
redacting part of a word, overlapping boxes, a box over nothing, locked
input, an unreadable page (refused, nothing changed), redacting twice, an
image that can't be decoded, an image shared with another page, and the
search patterns. For passwords: the right password opens the file, a wrong
one does not, the bytes do not hold a known plain text, and pdfium (an
independent implementation) opens our AES-256 files with the password and
refuses them without it. qpdf 12.4.2 (through pikepdf, in
`tools/fidelity/.venv`) was also used by hand to open our AES-256 output:
it refuses no password and a wrong one, accepts the open and owner
passwords, reads R6/V5 AESV3 256 bit and the right permissions; it also
opened every file the tests write without a warning. That check is not part
of `cargo test`, because qpdf is not a dependency of the project.

## Known limits

Places where content could survive, or where more is removed than asked:

- **Fonts stay in the file.** An embedded font subset still contains the
  glyph shapes of every letter it was made with, and its `/ToUnicode`
  table lists those letters, so a font can show *which* letters the
  document used (not in which order). Type 3 glyph procedures stay too.
- **Glyphs next to a box**: a glyph whose box does not overlap the box is
  kept even if its ink (an accent, a descender) reaches under it. Glyph
  boxes run from the font's descent to its ascent.
- **Fonts whose widths are missing** (no `/Widths`, not a standard font, or
  a CID font with a predefined CMap other than Identity, such as
  `UniGB-UCS2-H` or `90ms-RKSJ-H`, whose tables Letters does not ship):
  glyph positions can't be known, so a string that could reach the box is
  removed with the rest of its line, and this is reported.
- **Vertical text** (Identity-V and other `-V` CMaps) uses an approximate
  glyph box (one em tall, centred on the pen); it errs towards removing.
- **Images that can't be decoded** (JPEG 2000, JBIG2, CCITT fax, CMYK JPEG,
  images over 100 million pixels) are removed whole when a box touches
  them, and this is reported. Scanned pages often use JBIG2 or CCITT, so
  redacting a word there removes the whole page picture.
- **Indexed images** get the palette colour nearest the fill colour; Lab,
  Separation and DeviceN images get zero samples. The box drawn on top shows
  the fill colour either way; the image data no longer holds the original.
- **Clipping paths are kept**: a path used only as a clip stays, because
  removing it would change what is drawn outside the box. Nothing is
  painted through it inside the box afterwards, but its outline stays in
  the file.
- **`clipCrossingPaths`** keeps the coordinates of crossing paths in the
  file (hidden by a clip). Shadings (`sh`) are always clipped, not cut:
  a mesh shading's points under the box stay in the file.
- **Patterns and soft mask groups** (`/Pattern` cells, `/SMask` in
  `ExtGState`) are not edited. A shape filled with a pattern that crosses a
  box is removed or cut; the pattern cell itself stays.
- **Annotations are judged by `/Rect`**. An annotation whose appearance is
  drawn outside its `/Rect` and into a box is kept.
- **The structure tree** is cleaned for marked content on the page itself;
  marked content inside forms (`/Stm` references) is not matched, so
  alternative text tied to it stays. Structure elements written directly
  inside other elements (not as their own objects) are not changed.
- **Other copies are reported, not removed**: the document title and info,
  bookmark titles, other pages, comments elsewhere, and images or form
  fields shared with other pages. A form field shown in two places loses its
  value, but the other place keeps its old appearance until the form is
  filled again.
- **Hidden layers** in `scrub`: content is removed for layers that are off
  in the default configuration (`/D`); other configurations and usage
  dictionaries are not consulted. A `q` left open inside removed hidden
  content is kept open so the stream stays balanced, but the state changes
  inside it are lost.
- **Encrypted files** must be unlocked (`unlock`) before redaction or
  scrubbing. A file that only had permissions (no open password) is opened
  without asking and saved without its permissions unless locked again;
  this is reported as a warning.
- `preview` and `scrub_preview` work on a copy of the document, so they
  briefly need twice the memory of the open document, and `preview` takes
  as long as a redaction (images are really rewritten).
- A Flate stream that is damaged is decoded by lopdf as far as it goes
  (often to nothing), so a damaged image under a box is rewritten from
  that, padded with zeros, rather than reported as unreadable. Its original
  data is still gone from the file.
- A page whose content can't be read is refused rather than redacted.

## Where things are

| File | Job |
|---|---|
| `content/lexer.rs`, `content/mod.rs` | reading and writing content streams, including inline images |
| `fonts/` | font widths and code splitting: simple, CID (CMaps), Type 3, the standard 14 fonts, glyph names |
| `geometry.rs`, `page.rs` | matrices and rectangles; the visible page frame for every rotation and crop box |
| `redact/walker.rs` | the content interpreter: graphics state, marked content, the operator loop |
| `redact/text.rs` | text state and glyph removal |
| `redact/images.rs`, `redact/xobjects.rs` | pixel coverage and rewriting; images, forms and shadings |
| `redact/paths.rs` | what happens to vector paths |
| `redact/annotations.rs`, `redact/structure.rs` | annotations, form fields, and the structure tree |
| `redact/marks.rs` | the drawn boxes and labels, and recognising them again |
| `redact/mod.rs` | planning and applying a redaction, the preview |
| `find.rs`, `text_layout.rs` | finding text and patterns; joining glyphs into text |
| `scrub.rs` | hidden information |
| `lock.rs` | passwords and permissions |
| `cleanup.rs` | the clean save |
| `warnings.rs` | places that may still show what was removed |

## Libraries

lopdf, flate2, jpeg-decoder, regex, getrandom, serde, thiserror (all MIT or
Apache-2.0; lopdf brings in a few BSD-3-Clause crates for Brotli and text
encodings). Tests also use png and serde_json (MIT or Apache-2.0),
jpeg-encoder (MIT or Apache-2.0, plus the permissive IJG license; tests
only, never shipped) and `letters-pdf` (pdfium, Apache-2.0). The standard font
widths are Adobe's published core 14 font metrics and the glyph names are
from the Adobe Glyph List, both free to use and redistribute; the tables in
`fonts/standard_metrics.rs` and `fonts/glyph_names.rs` were generated from
the copies in reportlab (BSD) and fontTools (MIT). Nothing here restricts
distributing the app.
