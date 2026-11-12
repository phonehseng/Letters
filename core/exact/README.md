# letters-exact: "exact match" pdf to docx

Converts a PDF into a DOCX that Microsoft Word renders like the source,
glyph for glyph. It ports the measured recipe of the fidelity experiments
(`tools/fidelity/RECIPE.md` on the `fidelity` branch, round 4) from its
Python prototype to Rust. The sections of that recipe are cited in the code
as "recipe 6.3" and so on.

## What it promises

The target, measured through Word's own PDF export of the DOCX:

- every glyph within 0.125 pt of the source, mean distance under 0.05 pt;
- the same embedded fonts.

It only promises this for Word 16.0.20527 on Windows, which is where every
rule was measured (see "How it depends on Word" below). Without Word the
conversion still works, but nobody has checked the result, and the report
says so ("Not checked against Word on this computer.").

## Using it

```rust
use letters_exact::{convert, ConvertOptions};

let options = ConvertOptions {
    cache_dir: Some(app_data.join("exact-cache")), // measured facts about this computer's Word
    verify: true,                                  // export through Word and measure every glyph
    ..ConvertOptions::default()
};
let report = convert(pdf_path, docx_path, &options)?;
for warning in &report.warnings {
    println!("{warning}"); // plain sentences for the person
}
if let Some(check) = &report.verification {
    println!("{}", check.summary); // "Checked in Word: every glyph is within 0.074 pt ..."
}
```

`ConvertOptions` also takes a render callback (draws a one-page PDF at a
resolution, used for vector content Word can't draw and for the pixel
comparison), a progress callback, a `CancelFlag`, `use_word: false` (never
start Word), `check_passes` and a per-document Word time limit.

Errors (`ExactError`) are sentences for the person, in the style of
`core/pdf`: a locked PDF gives "This PDF is locked with a password. Unlock
it first, then convert it again."

`ConversionReport` says, per page, how many glyphs, lines, rotated and
hidden lines, pictures (and JPEGs kept as they were), shapes and rasterised
vector items there were; per font face whether it was rebuilt or replaced,
and why; whether Word was there; what was measured; and, with `verify`, the
same numbers as the prototype's harness.

## The pipeline

| module | what it does | recipe |
|---|---|---|
| `pdf` | content-stream interpreter: glyph origins, advances, fonts, sizes, colour, render mode, angle; paths, pictures | 2 |
| `fonts` | rebuilds every face as TrueType (TrueType, OpenType, bare CFF, Type 1, Type 3, CID), merges subsets, cmap from ToUnicode or the encoding, private use from U+F0000, fsType, stand-ins; grid fonts per (face, size); `.odttf` obfuscation | 3, 4, 10 |
| `word` | the measured rules: twips to pixels, the writer's print lattice, spacing, baselines, font sizes | 6 |
| `plan` | lines, frame placement, per-glyph targets and spacing, rotated boxes, vector shapes, rasterising | 5, 7, 8, 10 |
| `docx` | the package, written by hand | 7.1, 7.5, 8, 10 |
| `bridge` | a private, invisible copy of Word, driven by `word-export.ps1` | |
| `calibration` | the measured facts per Word build: built in, cached, probed | 9 |
| `verify` | exports through Word, measures every glyph, the lattice floor, SSIM | |
| `convert` | the pipeline, the checking export and re-plan, the report | 1, 7.6 |

## Measured results

Measured on 2026-11-11 through Word 16.0.20527.20150 (`tests/word.rs`,
the prototype's 13-document corpus). Distances combine x and y, in pt;
glyph origins of both PDFs are read with this crate's interpreter;
"prototype" is `corpus_report.md` (round 4); "floor" is the lattice floor.

| class | glyphs | max | mean | p99 | ≤0.025 | ≤0.06 | ≤0.125 | positions | fonts | overall | prototype max / mean | floor max / mean | off plan after check |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| syllabus | 2567 | 0.074 | 0.0278 | 0.059 | 46.5% | 100.0% | 100.0% | pass | pass | pass | 0.074 / 0.0278 | 0.074 / 0.0278 | 0 |
| research paper | 5747 | 0.063 | 0.0312 | 0.059 | 37.4% | 99.9% | 100.0% | pass | pass | pass | 0.063 / 0.0312 | 0.063 / 0.0312 | 0 |
| lab report | 1628 | 0.063 | 0.0285 | 0.060 | 45.1% | 99.9% | 100.0% | pass | pass | pass | 0.063 / 0.0285 | 0.063 / 0.0285 | 0 |
| slides | 389 | 0.102 | 0.0477 | 0.095 | 13.9% | 72.8% | 100.0% | pass | pass | pass | 0.102 / 0.0494 | 0.083 / 0.0430 | 0 |
| form | 517 | 0.085 | 0.0447 | 0.083 | 18.0% | 71.2% | 100.0% | pass | FAIL (stand-ins) | FAIL | 0.085 / 0.0447 | 0.085 / 0.0447 | 0 |
| journal page | 5109 | 0.086 | 0.0496 | 0.083 | 13.4% | 63.8% | 100.0% | pass | pass | pass | 0.086 / 0.0496 | 0.086 / 0.0496 | 0 (24, 17, 0) |
| cid subset > 256 glyphs | 972 | 0.078 | 0.0427 | 0.077 | 17.9% | 84.6% | 100.0% | pass | pass | pass | 0.078 / 0.0427 | 0.078 / 0.0427 | 0 |
| Type 3 font | 972 | 0.078 | 0.0416 | 0.076 | 21.0% | 86.5% | 100.0% | pass | pass | pass | 0.078 / 0.0416 | 0.078 / 0.0416 | 0 |
| mixed styles | 387 | 0.077 | 0.0403 | 0.073 | 21.2% | 89.1% | 100.0% | pass | pass | pass | 0.077 / 0.0403 | 0.077 / 0.0403 | 0 |
| justified TJ kerning | 1230 | 0.084 | 0.0479 | 0.082 | 13.9% | 67.8% | 100.0% | pass | pass | pass | 0.084 / 0.0479 | 0.084 / 0.0479 | 0 |
| CFF font | 1338 | 0.085 | 0.0485 | 0.082 | 13.2% | 68.8% | 100.0% | pass | pass | pass | 0.085 / 0.0485 | 0.085 / 0.0485 | 0 |
| Type 1 font | 1255 | 0.085 | 0.0490 | 0.083 | 11.9% | 66.5% | 100.0% | pass | pass | pass | 0.085 / 0.0490 | 0.085 / 0.0490 | 0 |
| restricted fsType | 785 | 0.085 | 0.0471 | 0.084 | 18.9% | 64.1% | 100.0% | pass | FAIL (fsType 2) | FAIL | 0.085 / 0.0471 | 0.085 / 0.0471 | 0 |

11 of 13 classes pass; all 13 pass on positions. Every number equals the
prototype's except the slides' mean, which is lower because landscape pages
now use their own measured tables (see "departures"). The form's fonts are
not embedded in its PDF and the restricted font forbids embedding, so both
get stand-ins and fail "same fonts" by definition. A conversion takes 10 to
25 s with Word (one checking export), 55 s for the journal (two re-plans)
and 86 s for the slides (rotated measuring exports).

## How it depends on Word

Every rule below was measured on **Word 16.0 build 16.0.20527 (file version
16.0.20527.20150) on Windows**, default printer "Microsoft Print to PDF" at
600 dpi (recipe 12):

1. Layout positions are whole 1/600 in pixels, for frames and inside a line.
2. Frame twips map to pixels by `twips_pixel` (recipe 6.1).
3. The PDF media box is the page size rounded to whole pixels.
4. The writer prints `Tm` on a 1/3000 in lattice with a step table per page
   size, 5 significant digits, at most 3 decimals.
5. Inside one text object positions come from `Tc`/`TJ`, off that lattice,
   so every glyph gets its own run, and runs alternate `w:lang w:eastAsia`
   so Word never merges them.
6. Each glyph's `w:spacing` becomes whole pixels on its own, rounded half
   away from zero; Word re-syncs when the exact sum drifts about a pixel.
7. Exact line height puts the baseline by recipe 6.6.
8. Sizes are stored in half points; glyphs are drawn at whole pixels per
   em; layout uses the stored size.
9. Embedded TrueType is used; embedded CFF is ignored (Calibri is drawn).
10. A font that claims no OS/2 code page is replaced by Times New Roman.
11. Text in U+E000..U+F8FF is drawn in Calibri; U+F0000 and up comes from
    the embedded font.

What is measured per build, and how:

- **Built in** (`calibration/word-16.0.20527.20150-model.json`): writer step
  tables for letter (x 5100, y 6600), A4 (x 4961, y 7016), 1008 x 612 pt
  (x 8400, y 5100) and 720 x 405 pt (x 6000, y 3375), all measured with
  portrait-marked sections by the prototype; landscape-marked 720 x 405 pt
  (`x:6000:landscape`, `y:3375:landscape`), measured by this crate; and
  baseline tables for 16 exact line heights. They are used when Word's
  file version is 16.0.20527.20150, and when Word is not installed at all.
- **Cached** in the folder the caller gives (`cache_dir`), as
  `word-<build>-model.json` in the prototype's format.
- **Probed** automatically when Word is there and a page size or line
  height (or the whole build) is unknown: frames with a 12 pt probe glyph,
  a coarse pass every 5 px and a fine pass near each step (recipe 9). Each
  page size costs two to four exports; each line height one.
- Without Word, an unknown page size gets an estimated step table (up to 3
  px off per step on letter) and the report says positions there were
  estimated. Unknown line heights use the closed baseline rule, which
  matched all 960 measured frames.

The checking export (recipe 7.6) runs whenever Word is there: one export,
the lines Word laid out unexpectedly are re-planned, at most twice.

## Word safety

Word is only driven through `src/bridge/word-export.ps1`:

- `New-Object -ComObject Word.Application` must start a new process with no
  documents in it; otherwise the script leaves that copy alone and stops.
- The copy is identified twice: it is one of the WINWORD processes that
  were not running before, and its main window (class `OpusApp`) carries a
  unique title the script gives it. Its process id is printed at once.
- Alerts off, invisible, documents opened read only, not added to recent
  files, with a made-up password so locked files fail at once.
- Word's application options are never changed.
- Every call has a time limit (30 s plus `word_seconds_per_document` per
  document). On a timeout or cancel the script is stopped, and if it had
  not named its Word copy yet the bridge waits up to 30 s for the name
  first, so the copy is never left running unseen.
- After every call, even a failed one, exactly that process is ended if it
  is still there (after checking it is still WINWORD.EXE). No other Word
  process is ever touched. If ending it fails, the error names its process
  id.

## Known limits

- **Grid floor.** Positions can only be lattice points: 0.12 pt apart
  (0.144 pt across a step). A source off the lattice can't do better than
  max 0.072 pt and mean about 0.03 pt per axis; every corpus class sits on
  its floor (table above). A class whose floor mean is at or above 0.05 pt
  cannot pass, whatever the converter does; the journal page sits at
  0.0496.
- **Fonts.** Fonts the PDF doesn't embed only get stand-ins (Arimo, Tinos,
  Cousine; positions stay exact, shapes differ), so the "same fonts"
  criterion fails by definition. A restricted fsType (2) forces a stand-in
  and a notice. fsType 4 fonts are embedded; whether Word then opens the
  file read-only is not verified. Type 3 glyphs keep only their filled
  paths; strokes, pictures and text inside a glyph are reported, not
  rebuilt. Italic and oblique standard fonts get upright stand-ins.
- **Glyphs without text** (empty, ligatures, duplicates, control chars) are
  written as U+F0000 and up, so copying them gives placeholder chars; the
  report counts them.
- **Rotated text** goes into rotated text boxes placed by 4 to 6 measuring
  exports; the rotated print lattice isn't modelled, so a rotated line's
  start can stay up to about 0.06 pt off. Without Word, rotated boxes are
  placed by geometry alone and can be several points off (the report says
  so). Only the 90 degree lines of the corpus were measured.
- **Vectors.** Only axis-aligned rectangles and straight strokes become
  shapes. Everything else needs the render callback to become a picture;
  without one it is left out and counted (`vectorsLeftOut`).
- **Pictures.** JPX, JBIG2 and CCITT images are skipped with a problem
  note; Lab, separation and DeviceN colours are approximated.
- **Pages.** `/Rotate` is reported but not applied. Page boxes other than
  the media box are ignored.
- **Scripts.** Right-to-left and complex scripts are counted and warned
  about, not shaped.
- **CMaps.** Type 0 fonts with an encoding other than Identity-H/V are
  read as two-byte identity (noted as a problem).
- **Editing.** One frame per line, one run per char, alternating East Asian
  language tags, sizes rounded to half points (glyphs scaled to match).
- **Not verified.** Other Word builds, another default printer, page sizes
  until measured, and screen against PDF. The step probes ran here on
  720 x 405 pt (both markings, see above); the baseline probe for an
  unseen line height was not run, because the built-in tables cover the
  corpus (the closed rule it falls back to matched 960 of 960 frames).
  `RECIPE.md`'s `LETTERS_STANDINS=windows` option is not ported.

## Departures from the recipe text, and why

- **Landscape pages have their own step tables** (measured here, the one
  change to the recipe's facts). The recipe's 720 x 405 pt tables were
  probed with sections that carry no `w:orient`, but the converter (like
  the prototype's) marks wider-than-tall sections `w:orient="landscape"`,
  and Word then prints on another lattice. Measured with this crate's
  probes on 16.0.20527.20150, 720 x 405 pt: marked portrait, x 633, 1982,
  3331, 4680 and y 331, 1036, 1741, 2446, 3151 (exactly the recipe's
  tables, which also checks the probe code against the prototype's);
  marked landscape, x 513, 1608, 2702, 3796, 4891 and y 326, 1020, 1713,
  2407, 3101. With the portrait tables, the checking export could not
  decode 14 of the slides' 15 horizontal lines (the prototype skipped them
  silently). Tables are now keyed `x:<px>:landscape` for landscape
  sections; the measured landscape tables for 720 x 405 are built in. The
  built-in 1008 x 612 pt tables were also measured portrait-marked, so a
  landscape page of that size is probed (or, without Word, estimated, with
  a warning). Effect on the slides through Word: mean 0.0494 to 0.0477 pt,
  within 0.06 pt 67.6% to 72.8%, every line checked; the maximum (0.102,
  the rotated line) is unchanged.
- Rounding ties follow the prototype, which was measured: Python's
  `round` (half to even) where it used it (advances snapped to pixels,
  widths to font units, page pixels, twips) and fontTools' `otRound`
  (half up) where fontTools scaled fonts. The recipe text only says
  "round"; an advance of exactly half a pixel does occur.
- The cubic to quadratic conversion is not cu2qu: each cubic is split into
  n equal pieces, each approximated by the quadratic with the midpoint
  control `(3(c1 + c2) - (p0 + p3)) / 4`, with n chosen from the exact
  error formula `√3/36 |p3 - 3c2 + 3c1 - p0| / n³ ≤ 1/1000 em`. The bound
  is the same as the recipe's; the curves differ slightly from cu2qu's.
  Glyph shapes don't affect positions.
- Stand-ins use the descriptor's serif flag as well as a `Times` name (the
  recipe says "serif flag"; the prototype only looked at the name).
- Type 1 programs fall back to their own built-in encoding after the PDF's
  encoding (the recipe's order; the prototype skipped that step).
- Unique family names use a 64-bit FNV hash of the PDF's bytes and the face
  instead of SHA-1 of the file name and the face, so two different files
  with the same name never share a family.
- The verification reuses the last checking export when no line was
  re-planned after it, instead of exporting the same DOCX again.
- The pixel comparison renders at 150 dpi through the caller's callback
  (the prototype used pdfium at 300 dpi), so SSIM values are not directly
  comparable with the prototype's.

## Tests

- `cargo test -p letters-exact`: unit tests for every pure rule with the
  recipe's worked examples and the prototype's test cases; the golden
  comparison (`tests/golden.rs`): for all 13 corpus documents the plan
  (every frame's twips, every glyph's target, advance and spacing, and the
  counts of pictures, shapes and fonts) must equal the prototype's; failure
  paths (`tests/failures.rs`).
- `cargo test -p letters-exact --release --test word -- --ignored
  --nocapture --test-threads 1`: through Word: the bridge, a timeout, a
  cancel, and the whole corpus against the prototype's numbers. Each test
  checks that the Word processes running before are untouched and none is
  left behind.

The corpus in `tests/corpus` is the prototype's: generated test documents
set in open-licensed fonts (Crimson Text, Lato, Source Serif 4, Tinos,
Arimo, Cousine, Noto Sans Math; licenses in `tests/corpus/licenses`). The
golden plans in `tests/corpus/golden` were extracted from the prototype's
output of the same run that produced `corpus_report.md`. The stand-in fonts
in `fonts/` are Arimo, Tinos and Cousine (SIL OFL 1.1, licenses alongside).
`data/make_tables.py` regenerates `src/pdf/tables.rs` (the Adobe glyph
list, the PDF encodings and the standard 14 widths) with fontTools and
reportlab.
