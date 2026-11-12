# letters-a11y

The accessibility engine of Letters: a checker and a fix wizard for tags,
reading order, alt text, contrast, language and table headers, aimed at
PDF/UA (ISO 14289-1). Everything is a pure function on an open
`lopdf::Document`, so the shell runs the fixes through its usual edit
pipeline (`AppState::edit`) and gets undo for free.

Every finding is explained in plain English: what it is, why it matters to
someone using a screen reader, and what the fix does to the file. The
wording lives in one table (`src/rules/catalog.rs`), which the ui can also
read through `rule_catalog()`.

**Words the ui reuses** (constants in the crate root):

- `TAGS_EXPLAINED`: tags are a hidden outline of a PDF that tells screen
  readers what each part is and in what order to read them.
- `ALT_TEXT_EXPLAINED`: alt text is a short written description of a
  picture that a screen reader reads aloud in its place.
- `READING_ORDER_EXPLAINED`: reading order is the order in which a screen
  reader reads the parts of a page, which comes from the tags.
- `PDF_UA_EXPLAINED`: PDF/UA is the international standard for accessible
  PDFs.

## Conventions

- Pages are 0-based; messages count from 1.
- Rectangles are in points, measured on the page **as shown** (after
  `/CropBox` and `/Rotate`) from its visible top left corner, y going down.
- Structs cross to the ui as JSON with camelCase names. A finding's
  location is serialised as `where`.
- An encrypted document is refused with `A11yError::Locked` ("Unlock it
  first"). Fixes also refuse a document that was decrypted on loading,
  because saving it would silently drop its protection.
- Every fix checks what it needs before it changes anything and answers a
  `FixResult { changes: [sentences] }`.

## Calling it from the shell

```rust
use letters_a11y::*;

// check; the renderer is optional and only needed for contrast
let report: Report = check(&doc, Some(&mut renderer))?;   // or check(&doc, None)
let report = check_with_options(&doc, None, &CheckOptions { bookmark_page_threshold: 10, ..Default::default() })?;
let summary: String = report_text(&report);
let page: String = report_html(&report);

// the renderer: draw `rect` (visible page points) of `page` at `scale` pixels per point
impl RegionRenderer for MyRenderer {
    fn render_region(&mut self, page: u32, rect: Rect, scale: f32) -> Option<RgbaImage> { … }
}
// closures work too: |page, rect, scale| -> Option<RgbaImage>

// fixes, each `fn(&mut Document, …) -> Result<FixResult, A11yError>` unless noted
set_language(&mut doc, "en-US")?;
set_title(&mut doc, "Essay on rivers")?;            // info + xmp dc:title + DisplayDocTitle
show_title(&mut doc)?;
mark_as_tagged(&mut doc)?;
set_alt_text(&mut doc, target, "A map of the Rhine")?;   // target: Target::Element{element} | Target::Image{image}
mark_as_artifact(&mut doc, target)?;
set_heading_level(&mut doc, element, 2)?;
propose_heading_levels(&doc)? -> Vec<HeadingChange>;     // a preview for the ui
fix_heading_levels(&mut doc)?;
set_table_headers(&mut doc, table, TableHeaders { header_rows: 1, header_columns: 0 })?;
auto_reading_order(&doc, page)? -> Vec<ObjectRef>;       // a proposal
set_reading_order(&mut doc, page, &order)?;
set_field_tooltip(&mut doc, field, "First name")?;
set_tab_order(&mut doc)?;
add_link_text(&mut doc, annotation, "Library opening hours")?;
set_annotation_contents(&mut doc, annotation, "Box around the key figure")?;
tag_annotations(&mut doc)?;
add_bookmarks_from_headings(&mut doc)?;
apply_text_colour(&mut doc, &runs, suggested)?;          // runs and colour come from the finding's fix
tag_untagged_content(&mut doc, page)? -> AutoTagResult;
auto_tag(&mut doc, &AutoTagOptions { replace_existing: false, language: Some("en".into()) })? -> AutoTagResult;
mark_pdfua(&mut doc, renderer)?;                         // refuses unless every must-fix is gone
```

Each finding's `fix` field (`FixKind`) says which of these applies and
carries what the call needs (the element, the annotation, a suggested
title or tooltip, a proposed reading order, the text runs and the nearest
passing colour). The ui asks the person for the one answer where needed
and calls the function.

## What PDF/UA needs that this tool can't check

`mark_pdfua` writes `pdfuaid:part 1` into the XMP metadata **only** when no
"must fix" finding is left. That is necessary but not enough: PDF/UA also
needs a person to confirm what no program can judge. The report lists
these as **check by hand** items, and `PDFUA_HUMAN_CHECKS` has the list for
a confirmation screen:

- is each picture's alt text meaningful (not just present)?
- is the reading order logical when read aloud?
- are headings used for structure, not for looks?
- is colour never the only way meaning is shown?
- are phrases in other languages marked?
- does nothing flash, and does nothing only work with scripts?

The wizard should walk the person through those before calling
`mark_pdfua`, and the mark is only honest once they have.

## The score

`report.score` counts machine-checkable rules: passed out of passed plus
failed. Rules that could not be run (contrast without a renderer, tag rules
on an untagged file) are **not** counted as passes; they are listed with a
reason, and `score.statement` says so in words, along with how many checks
need a person.

## Rules

| Rule id | What it checks | Fix | Limits |
|---|---|---|---|
| `not-tagged` | the document has a structure tree with elements | automatic proposal: `auto_tag` | when it fails, all other tag rules are reported as not checked |
| `tagged-flag-missing` | `/MarkInfo /Marked true` | automatic: `mark_as_tagged` | |
| `structure-tree-damaged` | loops and references to missing objects in the tags | manual | the readable part is still checked |
| `missing-language` | catalog `/Lang` present | one answer: `set_language` | |
| `invalid-language` | `/Lang` on the catalog, tags and marked content is a BCP 47 tag | one answer: `set_language` (catalog only) | shape of the tag only, not the registry |
| `language-change-unmarked` | tagged text in another alphabet (Greek, Cyrillic, Arabic, Hebrew, CJK, Devanagari, Thai) has a `/Lang` | manual | same-alphabet changes are a check by hand; only fonts with reliable Unicode are judged |
| `missing-title` | `dc:title` in the XMP metadata | one answer: `set_title` (suggests the first H1 or the biggest text) | |
| `title-not-displayed` | `/ViewerPreferences /DisplayDocTitle true` | automatic: `show_title` | |
| `missing-alt-text` | Figure and Formula tags (and tags holding pictures) have `/Alt` or `/ActualText` | one answer: `set_alt_text`, or `mark_as_artifact` | |
| `alt-text-is-file-name` | alt text isn't a file name or placeholder ("image", "IMG_2041.jpg") | one answer: `set_alt_text` | obvious cases only |
| `image-not-tagged` | pictures in the page content are tagged or artifacts | one answer: `set_alt_text` on the picture (wraps it in a new Figure) or `mark_as_artifact` | pictures inside form xobjects are not reported separately |
| `no-headings` | documents of 2+ pages or 300+ words have headings | manual (`set_heading_level`, or `auto_tag` with replace) | |
| `first-heading-not-h1` | numbered headings start at H1 | automatic: `fix_heading_levels` | |
| `heading-level-skipped` | no heading is more than one level below the one before | automatic: `fix_heading_levels` | |
| `reading-order-mismatch` | the tags' block order roughly follows the column-aware layout order | proposal: `auto_reading_order` then `set_reading_order` | reported when more than a third of steps between blocks jump; pages with fewer than 3 blocks are skipped |
| `table-missing-headers` | tables have TH cells | one answer: `set_table_headers` | |
| `table-header-missing-scope` | TH cells have a Scope (unless cells use Headers) | one answer: `set_table_headers` | |
| `table-irregular-rows` | rows cover the same number of columns, counting RowSpan and ColSpan | manual | |
| `table-structure-invalid` | Table › (THead/TBody/TFoot) › TR › TH/TD | manual | |
| `list-structure-invalid` | L › LI › Lbl/LBody | manual | |
| `list-not-tagged-as-list` | runs of 2+ paragraphs starting with a bullet or number | manual | common bullets and `1.`, `a)`, `iv.` only |
| `link-not-tagged` | link annotations are in a Link tag (OBJR) | automatic: `tag_annotations` | |
| `link-missing-description` | link annotations have `/Contents` | one answer: `add_link_text` (suggests the link's words or address) | |
| `link-text-not-descriptive` | link text isn't "click here", "more" or a bare address | one answer: `add_link_text` (also sets the Link tag's `/Alt`) | the words on the page stay; only the description changes |
| `annotation-not-tagged` | other annotations are in the tags | automatic: `tag_annotations` | pop-ups, hidden and printer marks are skipped |
| `annotation-missing-contents` | other annotations have `/Contents` | one answer: `set_annotation_contents` | |
| `field-missing-tooltip` | form fields have `/TU` | one answer: `set_field_tooltip` (suggests a readable form of the field name) | |
| `field-not-tagged` | widgets are in a Form tag | automatic: `tag_annotations` | |
| `tab-order-not-structure` | pages with annotations have `/Tabs /S` | automatic: `set_tab_order` | |
| `untagged-content` | in a tagged document, text and shapes are in marked content with an MCID or are artifacts | proposal: `tag_untagged_content` (paragraphs and figures at the end of the tags, shapes as artifacts) | content inside form xobjects counts as tagged when the form is |
| `font-not-embedded` | fonts used for visible text are embedded | manual | invisible text (render mode 3) is exempt |
| `text-not-unicode` | fonts have ToUnicode or a standard encoding with known glyph names | manual (export again or OCR) | the glyph name list is the common names plus `uniXXXX`, so unusual standard names can be reported |
| `low-contrast` | WCAG 2 contrast of text against the drawn background: 4.5:1, or 3:1 for 18 pt / 14 pt bold | one answer: `apply_text_colour` with the nearest passing colour (same hue) | needs a renderer; background is the most common colour under the text that isn't the text colour; text in form xobjects can't be recoloured |
| `no-bookmarks` | documents of 10+ pages (adjustable) have an outline | automatic: `add_bookmarks_from_headings` (needs headings) | |
| `page-labels-invalid` | page labels start at page 1, stay inside the document, use known styles | manual | |
| `security-blocks-assistive-tech` | the "copy for accessibility" permission isn't off | manual | only visible for files lopdf could open (empty user password) |
| `image-only-page` | pages with no text whose pictures cover half the page or more | points to the OCR tool (`RecogniseText`) | |
| `scripted-or-moving-content` | JavaScript, document or page actions, video, sound, 3D | check by hand | Letters doesn't run scripts or watch media |
| `check-alt-text-meaningful` | | check by hand | |
| `check-reading-order-logical` | | check by hand | |
| `check-headings-for-structure` | | check by hand | |
| `check-colour-not-only-meaning` | | check by hand | |
| `check-language-changes` | | check by hand | |

## How `auto_tag` works

A best effort the wizard asks the person to review. It returns
`AutoTagResult { changes, decisions, counts }`; `decisions` lists what it
was unsure about (headings ranked by size, a grid read as a table with the
first row as headers, a picture behind text treated as a scan, list labels
drawn together with their text), each with a page, rectangle, the new tag
and a confidence.

1. Each page's content is read: text runs with position, size, font and
   colour; pictures; painted shapes.
2. Ruled tables are found first (at least 3 by 3 full-length rules) and
   their text is assigned to cells. Everything else is grouped into lines,
   then put into column-aware reading order (x-y cut) and joined into
   paragraphs.
3. Lines in the top or bottom 8% that are page numbers, or that repeat on
   other pages, become pagination artifacts. Tiny pictures, shapes and
   lines become layout artifacts.
4. Body text is the most common size; short paragraphs at least 1.2 times
   bigger, or bold where body text isn't, are headings, ranked by size into
   H1 to H6. Runs of two or more paragraphs starting with a bullet or
   number become a list.
5. Every unit of content is wrapped in `BDC … EMC` with an MCID, closed
   before any `BT`, `ET`, `q`, `Q` or existing marked content so nesting
   stays valid. Then the tree is written: `/StructTreeRoot` with a
   Document element, the parent tree, `/StructParents` on pages,
   `/MarkInfo /Marked true` and an empty `/RoleMap`.
6. Links, notes and fields are tagged, tab order is set to follow the tags,
   and the language is set when given.

Wrapping only adds marked-content operators, so pages draw exactly the same
pixels; the tests check this with pdfium.

## Modules

| File | Job |
|---|---|
| `lib.rs` | the public api and the plain-language definitions |
| `rules/mod.rs` | reads the document once (`Context`), runs every rule family, builds the report and score |
| `rules/catalog.rs` | every rule's id, severity, wording, standard reference and limits |
| `rules/*.rs` | one family each: tagging, language, title, figures, headings, reading order, tables, lists, annotations, content tags, fonts, contrast, document, hand |
| `fixes/*.rs` | the fixes, by kind: metadata, figures, headings, tables, order, annotations, content |
| `autotag/` | `analyse.rs` (lines, paragraphs, margins), `grid.rs` (ruled tables), `mod.rs` (classifying, planning and writing) |
| `content.rs` | reads page content: text, pictures, shapes, colours, marked content, through form xobjects |
| `rewrite.rs` | wraps content in marked content, turns tags into artifacts, recolours text, replaces page content |
| `structure.rs` | reads the structure tree, loop-safe and depth-safe |
| `tagwriter.rs` | adds elements and keeps the parent tree in step |
| `geometry.rs` | rectangles, matrices, the visible page frame |
| `layout.rs` | column-aware reading order and how far two orders disagree |
| `colour.rs` | WCAG luminance, contrast ratio, nearest passing colour |
| `fonts.rs` | widths, embedding and Unicode mapping of fonts |
| `numtree.rs`, `xmp.rs`, `objects.rs` | number trees, the two XMP entries this crate writes, small lopdf helpers |
| `pdfua.rs` | the PDF/UA identifier |
| `export.rs` | the report as plain text and as accessible HTML |

## Tests

`cargo test -p letters-a11y` (set `CARGO_TARGET_DIR` as the repository
does). The integration tests build every PDF by hand in
`tests/support/mod.rs` (an embedded type 3 "box" font with a ToUnicode map
so documents can pass every rule, standard fonts, a small picture) and draw
pages with pdfium through `letters-pdf`, which needs `vendor/pdfium`.

- `checker.rs`: a passing and a failing case for each rule
- `fixes.rs`: each fix removes its finding and nothing else changes; the
  file reloads; rewritten pages draw the same pixels
- `autotag.rs`: expected element sequences and that MCIDs hold the right
  content, artifacts, tables, two columns, retagging
- `contrast.rs`: measured ratios (#777 fails at 4.48, #767676 passes,
  large text, dark backgrounds) and recolouring
- `locations.rs`: rectangles on turned and cropped pages, confirmed against
  pdfium's pixels
- `robustness.rs`: loops, missing objects, deep trees, nonsense values,
  unreadable content, a form that draws itself
- `export.rs`: the text and HTML reports

## Libraries

lopdf (MIT), serde (MIT or Apache-2.0), thiserror (MIT or Apache-2.0); for
tests letters-pdf, png and serde_json (MIT or Apache-2.0). Nothing here
restricts distribution.
