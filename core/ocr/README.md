# letters-ocr

OCR (optical character recognition) reads the letters in a picture of a
page, such as a scan or a photo, and turns them into real text you can
search, select and copy.

This crate does it fully offline. It cleans up the page picture, has
[Tesseract](https://github.com/tesseract-ocr/tesseract) read it, puts every
word back where it is on the page, and can write the words into the PDF as
an invisible layer over the picture. It also turns pictures into PDFs and
tells the app which pages are scans.

## How the engine is run, and why

Tesseract 5 (Apache-2.0) with Leptonica (BSD-2-Clause) is the engine. There
were two ways to use it:

- **(a) run the `tesseract` program as a separate process** and read its
  output, or
- **(b) link its C API into Letters** through a `-sys` crate.

We use **(a)**. (b) was not attempted, for these reasons: Letters builds
with the GNU toolchain on Windows (no Visual Studio, no administrator
rights), so linking would need MinGW builds of Tesseract and every image
library it uses, plus bindgen with libclang and pkg-config set up to find
them. The only MinGW builds available as plain archives are MSYS2's, and
those can't be used (see below). Running a program also has real benefits:
a crash on a strange page fails that page only, a page can be cancelled
by stopping the process, nothing of Tesseract's C++ runtime is linked into
Letters, and the engine can be updated without rebuilding the app.

### Where the program comes from

`tools/fetch-tesseract.ps1` downloads a pinned build into `vendor/tesseract`
(ignored by git, like `vendor/pdfium`). Run it once after cloning:

```powershell
.\tools\fetch-tesseract.ps1
```

It needs no administrator rights and runs no installer. It downloads 24
packages from [conda-forge](https://conda-forge.org) (the community package
collection behind conda), checks each against the SHA-256 that anaconda.org
publishes for it, unpacks them with the `tar` that ships with Windows (it
reads `.conda` zips and zstd), and keeps only the 31 files of `tesseract.exe` and the DLLs it
actually loads (found by following its DLL imports with `objdump -p`), plus
every license file. Language data comes from
[tessdata_fast 4.1.0](https://github.com/tesseract-ocr/tessdata_fast)
(Apache-2.0): English, Spanish, French and German, and `osd`, which finds
which way up a page is. tessdata_fast publishes no checksums, so the script
pins the SHA-256 recorded when it was written.

The result is about 74 MB: 55 MB of program files (32 MB of it is ICU's
data file, which libxml2 needs) and 19 MB of language data (`osd` alone is
10 MB).

On macOS and Linux, `tools/fetch-tesseract.sh` uses the Tesseract 5 the
package manager installed (`brew install tesseract`) and downloads and
checks the same language data. **It has not been run on macOS**; only the
"not installed" path was run here (in Git Bash). A later packaging stage
should bundle a pinned build there too; conda-forge publishes `osx-64` and
`osx-arm64` builds of the same version.

### What was tried first: MSYS2

MSYS2's `mingw-w64-x86_64-tesseract-ocr` 5.5.4 and its dependencies are
plain `.pkg.tar.zst` archives and unpack without installing MSYS2. Following
the DLL imports of `tesseract.exe` gave 39 DLLs from 33 packages. It was
ruled out because MSYS2's libtiff links **jbigkit, which is GPL-2.0**, and
the curl it links pulls in libidn2, libunistring and gettext (GPL/LGPL).
The UB Mannheim build was not tried: it comes as an installer (7-Zip could
unpack it), it is a MinGW build too, so it would need the same check of
every DLL, and conda-forge's archives are easier to pin and verify.

### Licenses of what is fetched

| Files | License |
|---|---|
| tesseract.exe, tesseract55.dll | Apache-2.0 |
| leptonica | BSD-2-Clause |
| tessdata_fast language data | Apache-2.0 |
| libarchive (the "lgpl" build, without the GPL lzo) | BSD-2-Clause |
| libtiff | libtiff (HPND, permissive) |
| libpng | libpng/zlib |
| libjpeg-turbo | IJG, BSD-3-Clause, zlib |
| libwebp, libssh2, zstd | BSD-3-Clause |
| openjpeg, lz4 (library) | BSD-2-Clause |
| giflib, libdeflate, libpsl, libxml2, ICU | MIT or the Unicode license (MIT-style) |
| curl | curl license (MIT-style) |
| openssl, lerc | Apache-2.0 |
| zlib | zlib |
| liblzma, bzip2 | 0BSD, bzip2 license |
| **libiconv** (iconv.dll, charset.dll) | **LGPL-2.1** |
| **Microsoft Visual C++ runtime** (vcruntime140, vcruntime140_1, msvcp140) | **Microsoft software license terms** |

Nothing is GPL. Two items need a decision when installers are built:

- **libiconv is LGPL-2.1.** It is a separate DLL loaded by a separate
  program, which the LGPL allows; the installer must ship its license and
  let people replace the DLL (keeping it as its own file does that).
- **The Visual C++ runtime** is not open source. Its terms (in
  `vendor/tesseract/licenses/vc14_runtime-*`) allow installing and using
  it; whether an installer may carry these DLLs or should run Microsoft's
  own redistributable is for the packaging stage to settle. Most Windows 11
  machines already have them in `System32` (this one does).

## Using it from the shell

```rust
use letters_ocr::*;

// once: where tools/fetch-tesseract put the engine (an installed app
// would look next to the program, like engine_location.rs does for pdfium)
let engine = Engine::locate(&tesseract_folder)?;          // Err(EngineMissing) if absent
let languages = engine.installed_languages();             // ["deu", "eng", "fra", "spa"]

// should the ui offer OCR for this page?
let scan: bool = is_scanned(&document, page)?;            // page is zero based
let makeup: PageMakeup = page_makeup(&document, page)?;   // coverage, text counts, earlier layer

// read one page: the page drawn at 300 dpi by the renderer, as rgba pixels
let picture = PageImage::from_rgba(width, height, &rgba, 300.0)?;
// (or PageImage::from_encoded(&png_bytes, Some(300.0)), or PageImage::open(path))
let options = OcrOptions { languages: vec!["eng".into()], preprocess: PreprocessOptions::default() };
let cancel = CancelFlag::new();                           // keep a clone; cancel() from any thread
let words: OcrPage = recognize(&engine, &picture, &options, &cancel)?;

// make it searchable (inside AppState::edit, so it is one undo step)
state.edit(id, |document| add_text_layer(document, page, &words))?;
// running again replaces the layer; remove_text_layer / has_text_layer exist too

// many pages: one picture in memory at a time, progress after each page
let pages: Vec<OcrPage> = recognize_pages(&engine, count, |index| draw_page(index),
    &options, &mut |progress: Progress| emit(progress), &cancel)?;

// text for the editable-text stages
let text: String = words.plain_text();
let structure: StructuredText = structured_text(&words);  // paragraphs in reading order

// pictures to pdf, optionally read as they go
let document: lopdf::Document = images_to_pdf(&[ImageSource::File(path)],
    &ImagesToPdfOptions { paper: PaperSize::A4, assumed_dpi: 300.0, margin: 36.0 },
    Some((&engine, &options)), &mut |progress| emit(progress), &cancel)?;
```

`OcrPage`, `OcrWord`, `Progress`, `PageMakeup`, `StructuredText` and the
option types serialise with camelCase names for the ui. Every error is an
`OcrError` whose message is a sentence for the person using the app; a
missing language names it ("Text in Korean can’t be read because the
Korean language data is missing. Install Letters again, or choose a
different language.").

All positions are in points from the top left of the page as it is shown,
like the viewer and the other editing tools.

## How a page is read

| File | Job |
|---|---|
| `page_image.rs` | the picture: rgba from the renderer, or png/jpeg/tiff files (camera orientation applied), kept as gray |
| `metadata.rs` | resolution from png, jpeg and tiff headers; a jpeg's size, colours and camera orientation |
| `preprocess/` | the cleanup steps, one file each (below) |
| `engine.rs` | finding and running `tesseract`: the picture goes in on standard input as raw gray pixels, hOCR comes out on standard output; stopped at once when cancelled |
| `hocr.rs` | reading hOCR: words with boxes and confidence, lines with baseline and font size, paragraphs and blocks |
| `geometry.rs` | points, rectangles and the affine maps between spaces |
| `result.rs` | `OcrPage` and `OcrWord`: hOCR mapped back through the cleanup to page points |
| `recognize.rs` | the whole run for one page, and for many with progress and cancelling |
| `structure.rs` | paragraphs in reading order, with a two-column rule |
| `pdf_page.rs` | finding a page and converting between the shown page and pdf user space (crop box, /Rotate) |
| `text_layer/` | the invisible text layer and its two fonts |
| `images_to_pdf.rs` | pictures to pdf pages |
| `scanned.rs` | is this page a scan? |

Cleanup steps, in the order they run (each can be switched off in
`PreprocessOptions`, and `OcrPage::steps` says which ran):

1. **Orientation**: Tesseract's orientation detection (`--psm 0`, the `osd`
   data) on a 150 dpi copy of the middle of the inked area. Only trusted
   above confidence 2. The page is turned by whole quarter turns.
2. **Contrast**: gray levels stretched so the darkest 0.5% is black and the
   lightest 0.5% white. Skipped on blank and already-clean pages.
3. **Median filter** (3 by 3), only when the picture is grainy (more than
   0.2% of pixels stand out alone from all four neighbours), because it wipes
   out hairline strokes.
4. **Deskew**: the tilt is found by projection profiles on a 100 dpi black
   and white copy (every 0.5° from -15° to 15°, then every 0.05° around
   the best), and the page turned back with bilinear blending onto a canvas
   big enough for its corners. Tilts under 0.1° are left alone.
5. **Binarisation**: Sauvola's method with a 1/6 inch window, computed with
   running column sums so memory stays at one row whatever the page size.
   Flat areas (inside thick strokes, or plain paper) use a page-wide
   threshold capped at gray level 96, so gray paper in a shadow stays white.
6. **Specks**: black shapes up to 6 pixels (at 300 dpi) removed.
7. **Borders**: black shapes touching the edge and spanning more than a
   tenth of the page (scanner lids, book spines, table edges) removed.

The turns and the deskew record the way back (`Prepared::to_original`), so
words found in the cleaned picture are placed on the original page. Each
word keeps its upright box, its sloping baseline (start and end) and its
font size.

## The text layer

Each word is written in text render mode 3 (invisible), positioned with a
text matrix whose first column runs along the word's baseline and whose
second points "up" its letters, so words on crooked lines and on turned
pages lie exactly over the picture. `Tz` stretches each word to its box.
The layer is wrapped in a `/LettersOcrText BMC … EMC` marked-content tag and
lives in its own content stream; the page's own content is wrapped in `q …
Q` once so a leftover transformation can't move the words. Running again
finds the layer by a marker in its stream dictionary (or, if another tool
merged the streams, by the tag) and replaces it.

Fonts:

- **Latin text** (everything in the Windows "ANSI" set, which covers
  English, Spanish, French, German and the rest of western Europe) uses
  Helvetica, which every reader has built in, so nothing is embedded. Its
  `/Widths` are Helvetica's real widths, so every letter's selection box
  sits over that letter.
- **Anything else** uses a Type0 font with `Identity-H` encoding: each
  character is written as its UTF-16 code units, and a `ToUnicode` CMap
  maps every code to the same Unicode value, so copying gives exactly the
  characters that were read. The embedded TrueType font (built in
  `glyphless.rs`, under 400 bytes) has one plain box glyph that every
  character uses; it is never drawn, but readers measure it for
  highlights. Characters beyond the first 65536 take two codes each, and
  readers are expected to join the halves; this has not been tested.

## Tests and measurements

`cargo test -p letters-ocr` runs 88 unit tests (no engine needed: every
cleanup step, hOCR reading, coordinate maps, the fonts, the layer writer,
images to PDF, scan detection, reading order) and 20 integration tests.
The integration tests draw known text with our own PDF renderer
(`letters-pdf`, so `vendor/pdfium` is needed too), read it back, and check
the PDF through `letters-pdf`'s search. They fail with "run
tools/fetch-tesseract first" when the engine is missing.

Measured on this machine (Windows 11, release build, one test at a time) for
a full letter page of 11 point Helvetica body text (36 lines, 301 words) at
300 dpi, 2551 × 3301 pixels:

| | time |
|---|---|
| cleanup (all steps but orientation) | 0.08 s (1.2 s in a debug build) |
| read, orientation detection off | 1.6 s |
| read, orientation detection on (default) | 2.8 s |

Orientation detection costs about a second a page, mostly Tesseract loading
its 10 MB `osd` data in each process; the ui could switch it off for
documents that are known to be upright.

Accuracy (characters right, after an edit-distance comparison):

| page | result |
|---|---|
| clean 12 pt text, and the full page above | 100% (the tests require 98%) |
| tilted by ±1°, ±3°, ±7° | tilt found exactly (tests allow 0.2°), 100% read |
| grain of ±40 gray levels and 2% dust | 100% with cleanup, 98.5% without |
| turned a quarter turn | turned back, 100% |
| Spanish, French, German lines read in their own language | exact, accents included |

Word boxes land within 3 points of where the letters were drawn (tests use
Courier, whose letter positions are known exactly), and baselines within
1.5 points. After adding the layer, search through `letters-pdf` finds each
word within 1.5 points of its recognised box, phrases across words and
lines are found, and every rendered pixel is unchanged.

## What is partial or missing

- **Several languages at once** are allowed (`languages: ["spa", "fra"]`),
  but with the fast language data, reading a page in languages it isn't in
  costs accuracy: German read as Spanish gave "Strafie", read with all
  three gave "StraRe" for "Straße". The ui should ask for the document's
  language and default to one.
- Only the first page of a multi-page TIFF is read.
- Mirrored camera orientations (EXIF 2, 4, 5, 7) are shown unmirrored.
- Right-to-left scripts and vertical text are not handled by the layer
  writer (words are written left to right along their baseline).
- The two-column rule handles two columns with full-width headings between
  them; three or more columns, and tables, keep Tesseract's own order.
- `is_scanned` does not look inside annotations or count text drawn as
  vector shapes.
- macOS: the fetch script is untested there, and nothing is bundled yet.
