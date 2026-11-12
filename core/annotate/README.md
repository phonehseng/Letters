# letters-annotate

Adds highlights, notes, text boxes, shapes, freehand ink and pictures to PDF
pages, and lists, changes, deletes and flattens them.

Every operation takes a `lopdf::Document` that is already in memory and changes
it in place. The app applies each one to its working copy and keeps its own
undo history; nothing here reads or writes files.

## Rules every operation follows

- Pages are counted from 0.
- Every position is in points (1/72 inch) from the top left corner of the page
  **as the person sees it**, with y going down. Page rotation (`/Rotate`),
  cropping (`/CropBox`) and media boxes that don't start at zero are converted
  here, for all four rotations.
- Every annotation made here is a standard PDF annotation with its own drawing
  (`/AP /N`), so it looks the same in every PDF reader.
- Every annotation gets a stable id: its `/NM`, a name starting `letters-` that
  survives saving and reopening. Annotations from other apps without one are
  called `ref:<object>-<generation>` until they are first changed, when they get
  a `/NM` (the old id keeps working).
- Every new annotation gets `/M` (changed), `/CreationDate`, `/T` (author, when
  given), `/P` (its page) and the print flag (`/F 4`), so it also prints.
- An encrypted document is refused with "Unlock it first, then try again".
- Errors are sentences the UI can show as they are (see `src/error.rs`).
- Parameter and result structs use camelCase in JSON, so the desktop shell can
  pass them straight between the UI and the engine.

## Operations

| Function | What it does to the file |
|---|---|
| `add_text_markup(doc, page, &TextMarkupParams)` | adds a `/Highlight`, `/Underline`, `/StrikeOut` or `/Squiggly` with one `/QuadPoints` quad per selected line; highlights multiply so text stays readable |
| `add_note(doc, page, &NoteParams)` | adds a sticky note (`/Text`) with its icon drawn in `/AP`, plus a closed `/Popup` linked both ways |
| `add_reply(doc, page, reply_to, &ReplyParams)` | adds a `/Text` reply with `/IRT` pointing at any annotation and `/RT /R`; it draws nothing on the page |
| `add_text_box(doc, page, &TextBoxParams)` | adds a `/FreeText` with the text in `/Contents`, font, size and colours in `/DA`, alignment in `/Q`, border in `/BS`, fill in `/C`, and the text wrapped by measured widths in `/AP` |
| `add_shape(doc, page, &ShapeParams)` | adds a `/Square` or `/Circle` with stroke `/C`, fill `/IC`, width and dash in `/BS` |
| `add_line(doc, page, &LineParams)` | adds a `/Line` with its ends in `/L` and arrowheads in `/LE` |
| `add_polygon(doc, page, &PolygonParams)` | adds a `/Polygon` (closed, may be filled) or `/PolyLine` (open) with corners in `/Vertices` |
| `add_ink(doc, page, &InkParams)` | adds an `/Ink` with the raw points in `/InkList`, drawn as smooth curves |
| `add_image(doc, page, &ImageParams)` | adds a `/Stamp` showing a PNG (with transparency) or JPEG (stored unchanged) picture |
| `create(doc, page, &NewAnnotation)` | any of the above, chosen by the kind of `NewAnnotation` |
| `list(doc, page)` | reads every annotation on the page into `AnnotationInfo` (pop-ups and form fields are left out); changes nothing |
| `get(doc, page, id)` | reads one annotation; changes nothing |
| `update(doc, page, id, &AnnotationChanges)` | moves, resizes, restyles or retypes an annotation, redraws its `/AP` (a pure move keeps the drawing), updates `/M`; a change that fails leaves it exactly as it was |
| `delete(doc, page, id)` | removes the annotation, its pop-up, every reply in its thread and their drawings from the file and from `/Annots` |
| `flatten(doc, &FlattenScope)` | paints the chosen comments and marks into the page content and removes the annotation objects; links, form fields and redaction marks are left alone |

Flattening, in one sentence for the UI (also exported as `FLATTEN_EXPLANATION`):
*flattening makes comments and marks a permanent part of the page: they look
exactly the same, but can no longer be edited, moved or removed.*

## Annotations from other apps

They are listed and can be deleted. Kinds Letters understands (the ones above)
can also be moved, resized and restyled; a move keeps their own drawing, other
changes replace it with Letters' drawing. Other kinds (links, carets, sounds and
so on) are listed as kind `other` and are never changed. Form field widgets
belong to the form tools and are neither listed nor changed here. `/Annots`
lists are kept tidy: entries pointing at missing objects are dropped whenever a
page is changed, and drawings are only removed when nothing else uses them.

## Fonts

Text boxes use the standard PDF fonts (Helvetica, Times and Courier, each with
bold and italic) in WinAnsiEncoding, so nothing is embedded. Text with
characters outside that set (for example Chinese or emoji) is refused with an
error naming those characters. The character widths in `src/font_metrics.rs`
come from Adobe's freely usable Core 14 AFM files; they are also written into
each text box's font so every reader spaces letters exactly as Letters wrapped
them.

## Tests

```
cargo test -p letters-annotate
```

The integration tests in `tests/` build small PDFs by hand, add annotations,
save and reload them, check the dictionaries, and render pages through
`letters-pdf` (PDFium) to check pixels, on all four rotations, cropped pages and
shifted page boxes. They need PDFium in `vendor/pdfium` (run
`tools/fetch-pdfium` first) or a folder named by `LETTERS_PDFIUM_DIR`.
`tests/fixtures/red-blue.jpg` is a 32 by 16 picture made for these tests.
