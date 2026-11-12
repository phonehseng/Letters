# letters-standards

Archival and print standards for Letters: checking and converting to
PDF/A, PDF/X print checks, a general "Check This PDF", and making files
smaller without changing how they look. Everything runs on this computer.

**Conformance claims come from Letters' own checker.** Every PDF/A report
carries `CHECKER_NOTE`. The tests also run veraPDF, the reference validator,
on every conversion they make (see "Checking with veraPDF" below), but the
app itself never runs it.

## How to call it

Functions take a `lopdf::Document` (lopdf 0.47) or the file's bytes. Pages
are counted from 0. Results are serde with camelCase names. Errors are
`StandardsError`, whose `to_string()` is a sentence for the person.

| Call | What it does |
|---|---|
| `check_pdfa(&doc, PdfaLevel) -> PdfaReport` | Checks a loaded document against PDF/A-1b (`a1b`), 2b (`a2b`), 2u (`a2u`) or 3b (`a3b`). Changes nothing. |
| `check_pdfa_file(&bytes, PdfaLevel)` | The same plus the rules about the file's bytes (header, binary comment, end of file, stream keywords, part 1 structure). |
| `convert_to_pdfa(&mut doc, PdfaLevel, &ConversionOptions, &ConversionHelpers) -> ConversionReport` | Fixes everything it can, then checks again. |
| `save_pdfa(&mut doc) -> bytes` | Saves a converted document the way PDF/A needs it. Always save converted documents with this. |
| `check_pdfx(&doc, PdfxLevel, &PdfxOptions) -> PdfxReport` | PDF/X-1a (`x1a`) or PDF/X-4 (`x4`) print checks. Report only. |
| `preflight(&bytes) -> PreflightReport` | "Check This PDF". Report only. |
| `optimize(&mut doc, &OptimizeOptions) -> OptimizeReport` | Makes the file smaller; `report.bytes` is the new file. |

Each finding (`Finding`) has a `rule` id (`fonts.notEmbedded`), the
`clause` (`ISO 19005-2 6.2.11.4.1`), a `message`, the `page` and `object`
(`12 0 R`) when known, and `fix`: `automatic`, `needsFont`,
`needsRendering` (PDF/A-1 transparency) or `notPossible`.

`ConversionHelpers` lends the conversion what this crate can't do alone:
- `fonts: &dyn FontSource`: `find(&FontRequest) -> Option<FontFile>`. The
  request has the font's name (without a subset tag), bold, italic, and
  metric-compatible substitutes from letters-fonts' table (Helvetica:
  Liberation Sans). In the app, answer from letters-fonts' library of fonts
  on this computer; set `same_widths` when the file is the same font or
  metric compatible.
- `renderer: &dyn PageRenderer`: `render(page, pixels_per_point) ->
  RenderedPage` (RGB, rows from the top, showing the crop box) of the
  **original** document. In the app, save the document before converting
  and render that copy with letters-pdf.

`ConversionReport` lists every change in a sentence (`changes`), each
font's outcome (`fonts`: embedded as which family, whether its widths
differ, or why not), each area turned into a picture (`rasterised`),
whether an output intent was added, what still blocks compliance
(`blockers`) with why, the reports before and after, and a `summary`.

## PDF/A: what is checked

| Area | Rules (ISO 19005 parts 1 / 2–3 clauses) | Fix |
|---|---|---|
| File | header and binary comment (6.1.2); nothing after `%%EOF` (6.1.3); stream keyword line breaks (6.1.7 / 6.1.7.1); part 1: no cross-reference or object streams (6.1.4) | automatic (`save_pdfa`) |
| Document | not encrypted (6.1.3); trailer `/ID` (6.1.3) | not possible / automatic |
| Metadata | XMP present, unfiltered in part 1 (6.7.2 / 6.6.2.1); xpacket header (6.7.5 / 6.6.2.2); pdfaid part and conformance (6.7.11 / 6.6.4); info dictionary agrees with XMP: title, author, subject, keywords, creator, producer, dates (6.7.3 / 6.6.2.3) | automatic |
| Colour | PDF/A output intent with a profile, one profile only (6.2.2 / 6.2.3); device RGB needs an RGB intent, CMYK a CMYK intent, gray any intent, followed through indexed spaces and images (6.2.3.3 / 6.2.4.3) | automatic (sRGB intent); CMYK not possible |
| Fonts | every font drawn with embedded, Type 3 excepted (6.3.4 / 6.2.11.4.1); widths listed (6.3.6 / 6.2.11.5); 2u: unicode known (ToUnicode, a standard encoding with glyph list names, or a known CID collection) (6.2.11.7) | needs a font |
| Actions | Launch, Sound, Movie, ResetForm, ImportData, JavaScript, and in parts 2–3 Hide, SetOCGState, Rendition, Trans, GoTo3DView; named actions other than page moves (6.6.1); `/AA` anywhere (6.6.2); document scripts | automatic (removed) |
| Objects | LZW (6.1.10 / 6.1.7.2); streams in other files (6.1.7); PostScript, reference xobjects, OPI (6.2.6–7 / 6.2.9); part 1 JPEG 2000; size limits: names 127 bytes, strings, numbers, arrays and dictionaries in part 1, object count (6.1.12 / 6.1.13) | automatic / not possible |
| Pages | sizes 3 to 14,400 units (6.1.12 / 6.1.13); part 1 page groups (6.4) | not possible / automatic |
| Annotations | types allowed per part (6.5.2 / 6.3.1); print flag set, hidden flags clear (6.5.3 / 6.3.2); part 1 opacity; appearance only `/N`; parts 2–3: appearance stream required except popups and links (6.3.3) | automatic for widgets, square, circle, line, polygon, polyline, ink, text markup, notes and free text; other kinds not possible |
| Transparency | part 1 only: soft masks, opacity, blend modes, transparency groups (6.4) | needs rendering |
| Images | Interpolate, Alternates (6.2.4 / 6.2.8) | automatic |
| Forms | XFA, NeedAppearances (6.9 / 6.4) | automatic |
| Layers | part 1: none allowed (not possible); parts 2–3: configurations named, no `/AS` (6.9) | automatic |
| Attachments | part 1: none, also no `/EF` in any file specification (6.1.11); part 2: only PDF/A files (6.8); part 3: relationship, media type, unicode name and association through `/AF` (6.8) | automatic (removed in parts 1–2; described and associated in part 3) |

## PDF/A: what the conversion does

- Removes forbidden actions, scripts, event actions, XFA, NeedAppearances,
  PostScript, OPI and reference xobjects; turns off picture smoothing;
  re-encodes LZW as Flate; removes forbidden annotation types; sets
  annotations to print and show; drops rollover and pressed looks; in part
  1 makes annotations opaque; names layer configurations. Each removal is a
  sentence in `changes`.
- Parts 1–2: removes attachments (PDF/A-3 keeps them). Part 3: gives
  attachments a relationship (`Unspecified` when unknown), a unicode name,
  a media type (`application/octet-stream` when unknown) and lists them in
  the catalog's `/AF`.
- Embeds fonts the document only names: the caller's font file is checked
  for its licence (fsType, through letters-fonts: a font whose licence
  forbids embedding is never embedded and stays a blocker), subset to the
  WinAnsi characters when the licence allows subsetting, and written as a
  TrueType font with `/Widths` taken from the embedded program itself, a
  font descriptor and a ToUnicode map. A substitute with other widths is
  reported (`widthsDiffer`): lines of text may run slightly longer or
  shorter. Simple fonts in WinAnsi or Standard encoding only; fonts with
  their own `/Differences`, symbolic fonts and Type 0 fonts are reported
  as not embedded.
- Draws missing appearance streams (parts 2–3): form fields through
  letters-forms, simple annotations from their own geometry and colour.
- Adds an sRGB output intent when there is no PDF/A one, with a profile
  generated by `icc.rs`: ICC version 2, display class, the sRGB primaries
  adapted to D50 and the sRGB tone curve as a 1024 point table. Letters
  ships no third-party profile, so no licence needs tracking.
- PDF/A-1 transparency (only with a renderer, and
  `rasteriseTransparency`, on by default): a picture with a soft mask
  painted directly on the page is replaced by a picture of its area as it
  looks (`wholePage: false`); any other transparency (opacity, blend modes,
  groups) turns the whole page into a picture at `pixelsPerPoint` (300 ppi
  by default) with the page's text kept on top as invisible text (render
  mode 3), so it can still be searched and copied. Every area is reported.
  Turned (rotated) pages aren't rasterised and stay blockers.
- Writes XMP with pdfaid and the document information, makes the info
  dictionary agree (dates written as XMP reads them back, producer
  "Letters", modification date now), adds a file identifier, sets the
  version (1.4 for part 1, 1.7 otherwise), removes objects nothing refers
  to, and checks again.
- `save_pdfa` writes a classic cross-reference table (no object or
  cross-reference streams), the binary comment `%âãÏÓ`, and nothing after
  `%%EOF`.
- When something can't be fixed (CMYK colour without a CMYK profile,
  streams in other files, size limits, page sizes, fonts without a source,
  annotation kinds Letters can't draw, part 1 layers, turned pages with
  transparency), `compliant` is false, the summary says so ("The document
  can't be made PDF/A-2b yet: …"), and `blockers` lists each one.

## Check This PDF, optimise, PDF/X

- `preflight`: each in-use entry of a classic cross-reference table is
  checked against the object it points at (files with cross-reference
  streams are checked by loading); references to objects not in the file;
  fonts and whether they are embedded; every picture's resolution as
  placed (following `cm` through page content and form xobjects); page
  boxes; what the bytes go to (pictures, fonts, page content, other
  streams, objects); and a portability checklist of things viewers and
  printers show differently: fonts not embedded, pictures under 150 ppi,
  XFA, NeedAppearances, scripts, layers, encryption, overprint, blend
  modes, video, sound and 3D, Type 3 fonts, JBIG2 and JPEG 2000, spot
  colours. "Looks identical everywhere" means none of these.
- `optimize`: removes unused objects, stores identical streams once
  (dictionary apart from `/Length` and bytes equal), Flate-compresses
  streams stored plain (XMP stays plain), optionally downsamples 8-bit gray
  and RGB pictures (plain or Flate, no masks or decode arrays) drawn sharper
  than `maxPixelsPerInch` by averaging whole blocks of pixels (a picture
  drawn twice keeps what its smaller drawing needs), and packs objects into
  object streams when that is smaller (`objectStreams`, on by default;
  PDF/A-1 forbids them, so don't use it on PDF/A-1 files). Each measure's
  bytes saved is found by saving after it. Not done, and said in
  `notDone`: subsetting fonts embedded whole, linearisation (fast web
  view), recompressing JPEG pictures.
- `check_pdfx`: PDF/X output intent (`/S /GTS_PDFX`; X-4 also needs its
  profile); the version named (`GTS_PDFXVersion` for X-1a, XMP for X-4);
  `/Trapped` True or False; a trim (or art) box on every page, inside the
  bleed box and the page; fonts embedded; picture resolution against
  `minColourPpi` (300) and `minLineArtPpi` (1200); RGB colour (X-1a always;
  X-4 unless the printing condition is RGB); transparency in X-1a; spot
  colours listed by name. Conversion to PDF/X is out of scope: it needs
  colour conversion to the press's profile.

## How it is tested

`cargo test -p letters-standards` (set `CARGO_TARGET_DIR`): 6 unit tests and
14 integration tests, plus 2 veraPDF tests ignored by default.

- `tests/pdfa.rs`: for 24 rules a clean converted document that passes, the
  same document broken in one way that fails with that rule, and the broken
  document converted again: fixable ones become compliant by both
  `check_pdfa_file` and the conversion's own report; unfixable ones stay
  blocked with the right rule. Plain documents convert to all four levels
  with the look unchanged (rendered before and after with PDFium, at most
  20 pixels differ). Fonts embed with their own widths and unicode; a font
  whose licence forbids embedding is a blocker; PDF/A-1 transparency
  becomes a picture of the area (looking the same) or of the page (text
  still extractable). qpdf's `--check` (through pikepdf in
  `tools/fidelity/.venv`) finds nothing wrong in every output.
- `tests/preflight.rs`: a sound file; a broken cross-reference entry found
  (and qpdf notices it too); missing objects; fonts, resolution and the
  checklist; optimising a wasteful file to under a quarter of its size with
  every measure saving bytes, the sharp picture downsampled, qpdf clean,
  and the look unchanged; PDF/X passes a print-ready file and names each
  problem of a plain one, X-1a refusing transparency that X-4 allows.

## Checking with veraPDF

veraPDF is dual licensed (GPLv3+ / MPLv2+). Here it is a **development
tool only**: it is never bundled with, shipped in, or linked into the app.

Installed on the development machine (per user, no administrator rights,
no PATH or system changes): veraPDF 1.30.4 (greenfield), from
`https://software.verapdf.org/releases/verapdf-installer.zip`, its GPG
signature checked (key 13DD 102B 4DD6 9354 D12D E5A8 3184 8632 78B1 7FE7,
"Carl Wilson <techlead@verapdf.org>"), installed headless with an IzPack
auto-install file into `%LOCALAPPDATA%\Programs\verapdf`, running on the
Java 17 already installed.

`cargo test -p letters-standards --test verapdf -- --ignored` runs it
(found through `LETTERS_VERAPDF`, else the folder above) on every
conversion at every level: plain boxes, embedded fonts, a soft-masked
picture, and a document needing repairs (a square comment without an
appearance, a link with a script, an attachment); all 16 pass, and an
unconverted file is rejected. veraPDF found two gaps in our checker that
are now fixed: an `/EF` in any file specification, even an unused one, in
PDF/A-1, and attachments not associated through `/AF` in PDF/A-3.

For the owner: to check any file yourself, run
`%LOCALAPPDATA%\Programs\verapdf\verapdf.bat --flavour 2b --format text -v file.pdf`
(flavours `1b`, `2b`, `2u`, `3b`) and send back the output, or
`--format mrr` for the full machine-readable report.

## Known limits

- The checker covers the rules above; veraPDF checks many more (font
  program internals such as CIDSet and CharSet accuracy, glyph width
  agreement in existing fonts, every colour space in shadings and
  patterns, inline images, ICC profile internals, XMP schema validity).
  Our checker passing is not proof of conformance; veraPDF passing on the
  test files is evidence for those files only.
- Part 1 subset fonts without CIDSet or CharSet, inline images' colour and
  colours inside patterns and shadings aren't checked.
- PDF/A-2u: fonts are only made unicode-readable when Letters embeds them.
- Level a (accessible PDF/A) isn't offered; letters-a11y does tagging.
- Rasterising needs the caller's renderer and ignores turned pages; text
  in a rasterised whole page is kept invisible, other vector art becomes
  pixels.
- Cross-reference checking reads classic tables only; stream `/Length`
  values are checked by lopdf when loading rather than by this crate.
- No linearisation, font subsetting of whole fonts, or JPEG recompression.

## Libraries and licenses

Direct dependencies: lopdf, serde, thiserror, sha2, ttf-parser (MIT or
Apache-2.0), letters-fonts and letters-forms (this workspace). Test-only:
letters-pdf, png, serde_json. Nothing restricts distributing the app.
veraPDF, qpdf/pikepdf and PDFium are used by tests only.
