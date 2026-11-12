# letters-pages

The page tools of Letters: pure operations on an open `lopdf::Document`.
Each takes the document (or documents) and one small settings struct, and
either succeeds completely or returns an error and leaves the document as it
was. Page numbers are 0-based positions in display order; error messages
count from 1. The settings structs read camelCase JSON, so the shell can pass
them straight from the interface. Undo is the app's job: it keeps a working
copy and applies each operation to it.

## Operations

| Function | What it does to the file |
|---|---|
| `reorder` | rewrites the page list in the new order (inherited settings are copied onto each page first) |
| `move_pages` | the same, moving a block of pages to before another page, as dragging thumbnails does |
| `rotate` | adds quarter turns to each page's `/Rotate`, on top of any turn inherited from the page tree |
| `delete` | takes pages out of the page list, removes bookmarks, links, named destinations and form widgets that led to them, then drops objects nothing uses any more; refuses to delete every page |
| `insert_blank` | adds empty pages of one size (a `/MediaBox`, empty resources, no content) |
| `insert_from` | copies pages from another document with everything they use (content, fonts, pictures, annotations, form fields, inherited settings) under fresh object ids, and nothing they don't |
| `extract` | a new document holding copies of some pages, with the bookmarks that lead to them |
| `merge` | a new document with each document's pages in turn, each under a top-level bookmark named after it that holds its own bookmarks |
| `split` | new documents by page ranges, every N pages, top-level bookmarks (one per chapter) or a maximum file size, each with a suggested name suffix |
| `crop` | sets `/CropBox` from a rectangle measured on the page as seen; the rest of the page is hidden, not removed (use redaction to remove content) |
| `uncrop` | removes `/CropBox`, or sets it to the full paper when one is inherited |
| `add_page_numbers` | appends a tagged content stream drawing the number in a standard font, plus the font in the page's own resources |
| `add_header_footer` | the same, for up to three texts along the top or bottom with `{page}`, `{pages}`, `{date}` and `{file}` filled in |
| `add_text_watermark` | the same, for text across the middle, with an opacity graphics state, in front of or behind the page |
| `add_image_watermark` | the same, for a PNG (decoded, transparency kept as a soft mask) or JPEG (stored as it is) |
| `remove_stamps` | removes every stamp of one kind that Letters added, and the save/restore wrapping once no stamp is left |

## How stamps work

A stamp is its own content stream, `/LettersStamp <</Kind /PageNumbers>> BDC
q … Q EMC`, added to the page's `/Contents`. The page's original streams are
never edited (other pages may share them); when a stamp goes in front, the
original content is wrapped in a `q`/`Q` pair so a coordinate change it leaves
behind can't move the stamp. Positions are worked out on the page as the
person sees it, so they are right on turned pages, cropped pages and pages
whose paper doesn't start at 0,0. Text uses the standard fonts (Helvetica,
Helvetica Bold, Courier) with WinAnsi encoding, so nothing is embedded;
characters outside WinAnsi are refused with a message naming the character.

## Modules

| File | Job |
|---|---|
| `tree.rs` | the page tree: pages in order, inherited settings, boxes, rotation, the visible frame and its coordinate conversions, flattening the tree to a new order |
| `order.rs`, `rotate.rs`, `crop.rs`, `insert.rs` | operations that change one document |
| `combine.rs`, `split.rs` | operations that make new documents |
| `stamp/` | page numbers, headers and footers, watermarks (`text.rs` fonts and encoding, `image.rs` PNG and JPEG, `page_content.rs` adding and removing stamp streams) |
| `copy.rs` | copying pages between documents without dragging the rest of the source along |
| `links.rs`, `outline.rs`, `forms.rs`, `cleanup.rs` | keeping destinations, bookmarks and form fields valid |
| `changes.rs` | collecting changes so they are applied all at once, after every check has passed |
| `error.rs` | every failure, worded for the person using the app |

## Known limits

- A page's tagged structure (for screen readers) is not rewritten: after a
  delete, references to the deleted pages are removed from it, and copied
  pages don't bring their structure along.
- Two documents merged with form fields of the same name keep both fields;
  the copy is renamed `name_2`.
- `remove_stamps` leaves the stamp's font or picture in the page's resources
  (a few hundred bytes, unused).
- Original content that saves the graphics state more often than it restores
  it isn't fully isolated by the wrapping pair.
- Text width for centring uses exact metrics for ASCII and close estimates
  for accented letters.
- Split by size assumes a longer run of pages never saves smaller, which holds
  because each page only adds objects.

## Tests

`cargo test -p letters-pages`. The integration tests in `tests/` build small
PDFs with lopdf, run an operation, save and reload, and check the structure;
anything visual is drawn with pdfium (through `letters-pdf`, from
`vendor/pdfium`) and checked pixel by pixel.
