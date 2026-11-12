# letters-compare

The Compare tool of Letters: it takes two versions of a document and finds
what changed in the text, the formatting, the layout and the pictures. The
result is shaped for the two ways the app shows a comparison (side by side,
and one page laid over the other) and can be saved as a report in PDF, plain
text or HTML.

Pages are counted from 0 in the API; the sentences meant for people count
from 1. Every rectangle is in points on the page as the person sees it,
measured from the top left corner with y going down (the same as
`letters-pdf`'s search rectangles).

## How the shell uses it

```rust
use std::sync::atomic::AtomicBool;
use letters_compare::*;

// 1. wrap each file. the renderer draws page `page` at `scale` pixels per
//    point as an RgbaImage; the shell builds it on RenderWorker::tile
//    (see tests/support/mod.rs, `render_page`, for one that stitches tiles)
let old = LopdfSource::open(&old_path)?.with_renderer(move |page, scale| draw(old_id, page, scale));
let new = LopdfSource::open(&new_path)?.with_renderer(move |page, scale| draw(new_id, page, scale));

// 2. compare, on a background thread. progress comes after every page;
//    storing true in `cancel` stops before the next page with CompareError::Cancelled
let cancel = AtomicBool::new(false);
let result: ComparisonResult =
    compare(&old, &new, &CompareOptions::default(), &mut |progress| send(progress), &cancel)?;
// (compare_documents(&old, &new) does the same with the defaults and no progress)

// 3. send `result` to the ui as json (camelCase). for the pair on screen:
let overlay: RgbaImage = overlay_image(&old, &new, &result.page_pairs[i], scale)?;
let both: AlignedImages = aligned_images(&old, &new, &result.page_pairs[i], scale)?;
let right_y = map_scroll(&result.page_pairs[i].scroll_map, left_y);

// 4. the report
let pdf_bytes: Vec<u8> = report_pdf(&result, &old, &new, &ReportOptions { old_name, new_name, ..Default::default() })?;
let text: String = report_text(&result, &options);
let html: String = report_html(&result, &options);
```

`LopdfSource` refuses a locked file with `CompareError::Locked` ("This PDF is
locked with a password. Unlock it first, then compare again."). Any other
document can be compared by implementing `PageSource` (page count, page
sizes, the characters of a page with their rectangles and style, and a
picture of a page).

## What the result holds

`ComparisonResult` has the page counts, `pagePairs`, a `summary` (counts by
category, `ignored`, pairs with changes) and `notes`. Each `PagePair` names
its old page and new page (either may be missing), their sizes, how alike
their text is, its `changes`, a `scrollMap` (matching heights on the two
pages, for scrolling both sides together), an `offset` that lines the new
page's content up with the old page's, and its own `notes` (for example
"compared as a picture").

Each `Change` has an `id` unique in the result, a `kind`, a `category`, the
pages and rectangles on each side (so the ui can draw a connector between
them), the old and new text where there is any, `detail` (one plain
sentence), and for formatting changes the `style` before and after.

## What each kind of change means

| Kind | Category | In plain words | Example sentence |
|---|---|---|---|
| `inserted` | text | words only the new version has | Added “It was warm.” |
| `deleted` | text | words only the old version has | Removed “very” |
| `replaced` | text | words rewritten in place | Changed “fox” to “cat” |
| `moved` | text | the same passage, found somewhere else (on the same page or another one); small edits made while moving are allowed | Moved “This travelling paragraph…” from old page 1 to new page 2 |
| `restyled` | formatting | the same words in a different font, size, weight, slant or colour; neighbouring words with the same change are one change | Font size changed from 14 to 16 in “Introduction” |
| `shifted` | layout | the same text in a different place, when nothing before it explains the move | Line moved down 12 pt (“Third line…”), Left margin changed from 72 to 108 pt |
| `reflowed` | layout | the same paragraph broken into lines differently, with no edit inside it | Paragraph reflowed with different line breaks (“Words of this…”) |
| `resized` | layout | a page of a different size, or turned | Page turned from portrait to landscape (612 × 792 pt to 792 × 612 pt) |
| `redrawn` | image | an area of a picture or drawing that looks different | Picture or drawing changed in this area |
| `pageAdded` | page | a page only the new version has | Page 3 was added (it begins “…”) |
| `pageRemoved` | page | a page only the old version has | Old page 2 was removed (it begins “…”) |
| `pageMoved` | page | a page that is in a different position | Old page 4 is now page 2 |

Not reported by default: differences only in spacing or hyphenation ("data
base" and "database", "compar-ison" broken over two lines and "comparison").
They are counted in `summary.ignored`; set `ignoreSpacing: false` to list
them ("Spacing or hyphenation changed in …").

Not reported at all, because they follow from another change: text that
moves because something above it was added, removed, restyled or reflowed;
a paragraph that reflows because it was edited; everything moving when a
page is cropped or turned.

## How it works

| Module | Job |
|---|---|
| `source.rs` | the `PageSource` trait, `Glyph`, `RgbaImage` |
| `lopdf_source/` | `LopdfSource`: walks content streams (graphics and text state, `TJ` spacing, form xobjects, colours), reads fonts (widths, encodings, `/ToUnicode`, two-byte fonts) and places letters on the visible page (crop box, `/Rotate`) |
| `standard_fonts.rs` | widths of Helvetica, Times and Courier for files that don't list them, and for the report |
| `words.rs` | letters into words, lines and blocks; reading order (columns read one after the other); joining hyphenated words; turned pages are read upright |
| `fingerprint.rs`, `page_align.rs` | matching pages first: added, removed and reordered pages are found by the runs of words pages share, so one added page doesn't shift every comparison after it |
| `diff.rs` | Myers' shortest edit, falling back to unique-word anchors ("patience") for very different pages |
| `text_diff.rs`, `moves.rs` | changed passages and their rectangles; removed and added passages with the same words become one move |
| `formatting.rs` | font, size, bold, italic and colour of words that didn't change |
| `layout.rs` | shifted lines, margins, reflowed paragraphs, page sizes, the scroll map and the offset |
| `pixel_diff.rs` | drawn pages compared outside the text, in small cells, so anti-aliasing doesn't count |
| `overlay.rs` | the overlay picture (shared grey, removed red, added green) and the lined-up pair for a blink view or opacity slider |
| `compare.rs` | the whole comparison, one page pair at a time |
| `report/` | the report as text, html and a tagged pdf |

## Speed and memory

Each page of both documents is read once to make a small fingerprint (a few
kilobytes), and its letters are dropped. Then page pairs are compared one at
a time; a pair's words and pictures exist only while it is compared. Pages
whose drawing is identical (same content and resources, compared by value)
are not drawn at all.

Measured on this machine (Windows 11, `cargo test --test large -- --nocapture`):

| Case | Optimised build | Test (debug) build | Peak working memory |
|---|---|---|---|
| 500 pages against 500, three words changed, one page added, one removed | 1.1 s | 10 s | 16 MB |
| 100 pages against 100 written differently, so every pair is drawn and compared as pictures | 1.6 s | 14 s | 11 MB |

Working memory is what the comparison allocates on top of the two open
documents (11 MB for the two 500 page files; lopdf holds a whole document in
memory). Drawing is done by the caller's renderer, whose own memory isn't
counted.

## Limits

- Text is read from the file's text, not from pictures. Scanned pages without
  a text layer are compared as pictures only, and the pair's notes say so.
- Bold and italic come from the font's name and descriptor; a font that
  only looks bold is not detected. Formatting is compared per word: a change
  to part of a word shows as the word's main style.
- Without `/ToUnicode`, two-byte fonts are read as if their codes were
  unicode, which is often wrong; such text still compares with itself.
  Type 3 fonts and fonts with a vertical writing mode are placed roughly.
- Reading order is worked out from positions. Unusual layouts (text wrapped
  around pictures, tables read across) may be read in an order a person
  wouldn't choose; changes are still found, but may be split differently.
- A page with more than 1,000 changed words between unique shared words is
  reported as rewritten in larger pieces.
- Moves are looked for between passages of at least 4 words, and not at all
  when more than 2,000 passages were removed or added.
- Pictures are compared at 1 pixel per point by default
  (`pictureScale`): a change smaller than about 2 points can be missed.
- The report is in English and uses Helvetica, so letters outside western
  European languages show as "?". The report pdf is tagged (headings, a
  table with header cells, lists, figures with text alternatives, captions;
  decorations and page numbers as artifacts) with a title, language and
  bookmarks; it has not been checked with a PDF/UA validator or a screen
  reader.

## Libraries and licences

| Library | Used for | Licence |
|---|---|---|
| [lopdf](https://crates.io/crates/lopdf) | reading pdfs and writing the report | MIT |
| [serde](https://serde.rs) | the json shape of the result | MIT or Apache-2.0 |
| [thiserror](https://crates.io/crates/thiserror) | error messages | MIT or Apache-2.0 |
| letters-pdf, png, serde_json (tests only) | drawing pages in tests, reading tiles | see the workspace README |

The widths of the standard fonts come from Adobe's Core 14 font metrics
files, which may be used and copied freely. The diff is written here, so no
diff library is used. Nothing here restricts distributing the app.
