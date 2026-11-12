# Round-trip report

What happens to a document's look when it goes through Letters, between
docx and odt, and between Word and LibreOffice. Every number here comes
from `tests/freeform_apps.rs` (ignored tests; they need Word and
LibreOffice installed) run on 2026-11-12 with Microsoft Word
16.0.20527.20150 and LibreOffice 26.8.2.2.

## How it is measured

Each application renders a file to PDF itself (Word through a private
hidden instance, LibreOffice headless with its own profile). The two PDFs
are read with `letters-exact`'s PDF reader and compared glyph by glyph
(each character's position on its page) and object by object (the boxes
of drawn paths and pictures). A glyph or object "matches" when the same
character or box is found on the same page; the max and mean distances
are in points. "Identical" means every glyph and object matched at
0.000 pt with the same page count.

The corpus is twelve documents, each written by the application itself:

| document | made by | what it holds |
|---|---|---|
| word_shapes | Word (COM) | preset shapes, a free path, gradients, transparency, a picture fill, arrowheads, dashes, a rotated and a flipped shape |
| word_groups | Word | a group of shapes and a text box, a nested group |
| word_floating_table | Word | a floating table with exact rows, cell margins, turned text, merges |
| word_linked_boxes | Word | one story through three linked text boxes |
| word_columns | Word | unequal columns with a line, a column break, a continuous section |
| word_flyer | Word | a poster page: picture, shapes, text boxes, all positioned |
| lo_shapes, lo_groups, lo_floating_table, lo_linked_boxes, lo_columns, lo_flyer | LibreOffice (from flat ODT) | the same six, as LibreOffice builds them |

## Nothing moves: open and save in Letters

Each document is read by Letters and written back in its own format
without any edit, then rendered by both applications next to the
original.

| document | Word | LibreOffice |
|---|---|---|
| word_shapes | identical (331 glyphs, 19 objects) | identical (335, 100) |
| word_groups | identical (198, 7) | identical (200, 6) |
| word_floating_table | identical (372, 67) | identical (377, 19) |
| word_linked_boxes | identical (697, 3) | identical (532, 3) |
| word_columns | identical (1179, 2; 2 pages) | identical (1193, 2) |
| word_flyer | identical (115, 7) | identical (115, 7) |
| lo_shapes | identical (277, 14) | identical (277, 92) |
| lo_groups | identical (214, 7) | identical (214, 4) |
| lo_floating_table | identical (351, 1) | identical (351, 1) |
| lo_linked_boxes | identical (714, 3) | identical (714, 3) |
| lo_columns | identical (1291, 2) | identical (1291, 2) |
| lo_flyer | identical (71, 6) | identical (71, 6) |

24 of 24 renderings identical. Editing one object (moving one shape)
leaves every other drawing's XML byte for byte as it was
(`tests/freeform.rs`, both formats), and a second save of a saved file is
byte identical.

The exact PDF converter's docx files (a syllabus, slides and a form,
made by `letters-exact` without Word) survive an untouched save:
Word renders all three identically (2567, 389 and 517 glyphs; 167, 13 and
21 objects; 0.000 pt), and `document.xml` differs only in root namespace
declarations the writer adds.

## Back and forth: Letters against the applications' own conversion

Each Word document goes docx to odt to docx through Letters and is
rendered by Word; the baseline is Word itself saving the document as odt
and Word rendering that odt. Each LibreOffice document goes odt to docx
to odt through Letters and is rendered by LibreOffice; the baseline is
LibreOffice saving it as docx and rendering that. Distances are against
the original's rendering in the same application.

| document | path | glyphs matched | glyph max / mean pt | objects matched | object max / mean pt |
|---|---|---|---|---|---|
| word_shapes | Letters | 327/331 | 2.292 / 0.954 | 18/19 | 0.000 / 0.000 |
| | Word as odt | 327/331 | 0.840 / 0.412 | 18/19 | 3.351 / 0.240 |
| word_groups | Letters | 196/198 | 2.568 / 1.098 | 7/7 | 0.000 / 0.000 |
| | Word as odt | 196/198 | 0.759 / 0.362 | 5/7 | 0.600 / 0.140 |
| word_floating_table | Letters | 367/372 | 439.9 / 8.24 | 12/67 | 31.7 / 26.9 |
| | Word as odt | 218/372 | 237.5 / 70.75 | 2/67 | 31.9 / 29.9 |
| word_linked_boxes | Letters | 687/697 | 2.292 / 0.557 | 3/3 | 0.000 / 0.000 |
| | Word as odt | 687/697 | 2.283 / 0.733 | 3/3 | 0.100 / 0.100 |
| word_columns | Letters | 1165/1179 | 589.5 / 106.6 | 1/2 | 8.04 / 8.04 |
| | Word as odt | 1165/1179 | 585.6 / 105.7 | 1/2 | 6.96 / 6.96 |
| word_flyer | Letters | 115/115 | 1.376 / 0.373 | 7/7 | 0.000 / 0.000 |
| | Word as odt | 115/115 | 0.524 / 0.240 | 7/7 | 0.050 / 0.029 |
| lo_shapes | Letters | 277/277 | 0.750 / 0.035 | 92/92 | 4.90 / 0.163 |
| | LibreOffice as docx | 277/277 | 0.150 / 0.007 | 92/92 | 4.70 / 0.148 |
| lo_groups | Letters | 198/214 | 0.252 / 0.031 | 3/4 | 0.050 / 0.050 |
| | LibreOffice as docx | 214/214 | 0.112 / 0.008 | 4/4 | 0.050 / 0.038 |
| lo_floating_table | Letters | 351/351 | 0.696 / 0.173 | 1/1 | 0.000 / 0.000 |
| | LibreOffice as docx | 351/351 | 0.000 / 0.000 | 1/1 | 0.050 / 0.050 |
| lo_linked_boxes | Letters | 714/714 | 169.1 / 67.2 | 3/3 | 0.000 / 0.000 |
| | LibreOffice as docx | 714/714 | 0.364 / 0.175 | 3/3 | 0.050 / 0.050 |
| lo_columns | Letters | 1291/1291 | 0.252 / 0.018 | 2/2 | 0.000 / 0.000 |
| | LibreOffice as docx | 1291/1291 | 0.000 / 0.000 | 2/2 | 0.250 / 0.250 |
| lo_flyer | Letters | 71/71 | 8.722 / 7.634 | 5/6 | 0.150 / 0.060 |
| | LibreOffice as docx | 71/71 | 2.051 / 0.210 | 6/6 | 5.55 / 1.85 |

Page counts matched in every row. Letters places objects (shapes, groups,
boxes, pictures) better than the application's own conversion in 8 of 12
documents; it does worse in word_columns, lo_shapes (by 0.2 pt), lo_groups
and lo_flyer (one object not matched in each of the last two). Text drifts
more than in the applications' own conversion in most documents, mainly
for reason 6 below.
Timings: Word saving six documents as odt took 16.6 s, LibreOffice saving
six as docx 29.7 s; the "nothing moves" renderings took 19.9 s in one
Word batch and 18.3 s in LibreOffice. The four suites together take
192 s. A second run after the file splits gave the same numbers in every
table here.

## What still changes, worst first

1. **Floating tables through odt** (word_floating_table). Odt has no
   floating table, so Letters writes it in a frame, and after the trip
   back only 12 of 67 cell boxes are where they were. Word's own odt
   conversion is worse (2 of 67). Docx to docx and odt to odt are
   unaffected.
2. **Word's columns through odt** (word_columns). A continuous section of
   unequal columns becomes a `text:section`; after the trip most of the
   text moves by up to 590 pt. Word's own conversion moves it the same
   way (585 pt).
3. **LibreOffice's linked text boxes through docx** (lo_linked_boxes).
   The chain survives, but LibreOffice reading our docx breaks the story
   between the boxes at other lines (up to 169 pt); its own docx does not.
   The cause is not found yet.
4. **LibreOffice's poster page through docx** (lo_flyer): its text moves
   by about 8 pt and one object is not matched. LibreOffice's own docx
   moves the objects more (5.5 pt) but the text less.
5. **Text in a LibreOffice group through docx** (lo_groups): 16 glyphs of
   a text box inside a group are not drawn after the trip. LibreOffice
   does not draw a text frame inside a group, so Letters writes group
   members that are text boxes as rectangles holding text; the text of
   one member still goes missing.
6. **Word's character shaping through odt** (all Word documents, about
   2.3 pt at most): kerning, ligatures and character spacing are kept in
   docx (`w:kern`, `w14:ligatures`, `w:spacing`) but have no place in the
   model's odt output, so lines set slightly differently. Word's own
   conversion keeps more of it (0.8 pt). Fixing it means a new run mark
   carried into `fo:letter-spacing` and `style:letter-kerning`.
7. **Unequal columns in LibreOffice** stretch to fill the width between
   the margins; Word allows narrower columns. Reported by
   `libreoffice_compatibility` as `columns-fill-the-page`.
8. Smaller things that are kept in the same format but not across:
   `w:docGrid`, covered cells' own paragraph ids, connector glue points
   (kept as plain lines), arrowhead sizes, Word's short dashes, gradients
   with more than two stops in odt programs other than LibreOffice.

## Tidy

`tidy::tidy` with the default options (and box merging for the last row)
on our save of each document, both renderings compared with the untidied
save: identical in Word and LibreOffice, all 26 renderings.

| document | saved bytes | tidied bytes | saving | styles removed |
|---|---|---|---|---|
| word_shapes | 29,812 | 29,106 | 2.4% | 26 |
| word_groups | 16,093 | 15,386 | 4.4% | 26 |
| word_floating_table | 12,115 | 11,408 | 5.8% | 26 |
| word_linked_boxes | 16,399 | 15,692 | 4.3% | 26 |
| word_columns | 11,460 | 10,753 | 6.2% | 26 |
| word_flyer | 20,709 | 20,002 | 3.4% | 26 |
| lo_shapes | 17,478 | 17,164 | 1.8% | 5 |
| lo_groups | 12,961 | 12,647 | 2.4% | 5 |
| lo_floating_table | 14,416 | 14,102 | 2.2% | 5 |
| lo_linked_boxes | 20,866 | 20,552 | 1.5% | 5 |
| lo_columns | 29,846 | 29,532 | 1.1% | 5 |
| lo_flyer | 13,854 | 13,540 | 2.3% | 5 |
| stacked_lines (eleven one-line boxes) | 4,341 | 3,867 | 10.9% | 0 (11 boxes merged) |

The application-made files have little to merge: their runs and
properties are already tidy, and drawings with original XML are left
alone. The savings are unused styles.

## Choices made on the way

- A drawing nobody changed is written from its original XML, not from
  the model; that is what makes "nothing moves" exact.
- Odt has no floating table: it is written in a frame, the way
  LibreOffice itself does it.
- LibreOffice does not draw text frames inside groups, so group members
  that are text boxes are written as rectangles with text in odt.
- A drawing inside another drawing's text is written as plain paragraphs
  in docx, because Word's PDF export hangs on nested drawings.
- Theme fonts keep the explicit font names only when the file had them;
  adding them changed LibreOffice's line breaks.
- Odt files are written with the ODF version their carried parts already
  use, because LibreOffice refuses a package whose manifest and content
  disagree.

## Not checked

- Other odt programs than LibreOffice (Calligra, Google Docs, OnlyOffice).
- Word 2007, which uses the VML copies the writer adds.
- Rotated text boxes and pictures in odt in Word.
- Whether LibreOffice draws `ooxml-*` custom shape types that come
  without an enhanced path (Letters always writes the path).
