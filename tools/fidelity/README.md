# fidelity experiments: can a DOCX render in Word within 1/40 pt of a PDF?

Small Python scripts that build test PDFs and DOCX files and have Microsoft Word export
each DOCX back to PDF. Then they measure how far every glyph lands from where the
source PDF puts it. The target is 0.025 pt (1/40 pt) for every glyph.

Everything here is a throwaway measurement tool. Nothing in `core/` or `app/` depends on it.

## setup (once)

```powershell
cd C:\Letters\tools\fidelity
py -3.12 -m venv .venv
.venv\Scripts\python.exe -m pip install fonttools pywin32 pypdfium2 pillow numpy scikit-image reportlab ruff
```

All of these are permissively licensed (MIT, BSD, Apache-2.0, PSF). PyMuPDF is not used
because it is AGPL. Microsoft Word for Windows must be installed. Fonts are downloaded on
the first run from their official repositories:

- Crimson Text (TrueType), google/fonts, SIL OFL 1.1. The license is in `fonts/CrimsonText-OFL.txt`.
- Source Serif 4 (CFF OpenType), adobe-fonts/source-serif, SIL OFL 1.1. The license is in
  `fonts/SourceSerif-LICENSE.md`.

Both fonts are renamed ("Fidelity Probe ...") before use, for two reasons. Word must not
be able to find an installed copy. And the OFL reserved font name "Source" must not be kept
on a modified font. The TrueType font is also rescaled to 1000 units per em; see "measurement
pitfalls" below.

## scripts

Run all of them from `C:\Letters\tools\fidelity`. Output goes to `out\` (ignored by git).

| script | what it does | time |
|---|---|---|
| `harness.py source.pdf candidate.docx [--out DIR] [--dpi 300] [--font f.ttf]` | Word exports the docx to PDF (private hidden instance). Then it compares both PDFs. | about 6-10 s |
| `e1_font_embedding.py` | E1: is the obfuscated embedded font really used? | about 35-45 s |
| `e2_positioning.py` | E2: frames vs text boxes vs paragraphs, 300 lines on 3 pages | about 45 s |
| `e3_advance_widths.py` | E3: char spacing via per-char `w:spacing` vs rewritten `hmtx` | about 45 s |
| `e3_spacing_probe.py` | how Word turns a font advance plus `w:spacing` into pen positions | about 6 s |
| `word_pdf_grid.py some.word.pdf ...` | the coordinate grid in a PDF that Word exported | under 1 s |
| `e5_model_probe.py` | does Word follow `grid_model.py`'s floor rule? (random whole-pixel targets at 9, 10.5 and 12 pt) | about 20 s |
| `e5_grid_snap.py` | E5: grid fonts, calibrated frames and predicted spacing on the E2 and E3 test sets | about 60 s |
| `e6_grid_source.py` | E6: does any document-side setting change the grid? (about 30 variants) | about 3 min |
| `e7_odd_sizes.py` | E7: PDF sizes a docx cannot store (10.3, 11.27, 7.9 pt) | about 20 s |
| `e8_vertical.py` | E8: the vertical grid, and whether font sizes move baselines | about 6 s |
| `e9_word_frames.py` | E9: one frame per word on the E2 set (needs an `e5_grid_snap.py` run first) | about 35 s |
| `test_units.py` | unit tests for the parts that do not need Word (`python -m unittest test_units -v`) | under 1 s |

The round 2 helper modules are:

- `grid_model.py`: the pixel model and the spacing calculation. It is pure and unit tested.
- `frame_calibration.py`: picks frame twips by measurement.
- `grid_probe.py`: the grid-detection page used by E6.
- `font_prep.make_grid_font`: builds whole-pixel advances, with optional outline scaling.

From round 2 on, the scripts live in the worktree `C:\Letters-fidelity\tools\fidelity`. They reuse the
existing venv: `C:\Letters\tools\fidelity\.venv\Scripts\python.exe <script>`.

A harness run prints a block like this and writes `<name>.report.json` and `<name>.heatmap_pageN.png`:

```
== e2_frames.docx
   word 16.0 build 16.0.20527, pages source/word: 3/3
   chars source 8396  word 8396  matched 8396  wrong page 0
   origin delta pt   max 0.2662  mean 0.0698  p99 0.1870
   |dx| pt           max 0.2660  mean 0.0496  (signed mean -0.0141)
   |dy| pt           max 0.1100  mean 0.0389  (signed mean -0.0156)
   within 0.025 pt: 8.9%  font size delta max 0.0600
   page 1: ssim 0.9751  mean|diff| 2.480  px>noise 153774/491663 ink
   word font: ___WRD_EMBED_SUB_45  embedded=True  program=truetype  ...
      outlines found in FidelityProbeSerif-Regular.ttf: 100.0% of its outlines
   seconds: word 5.2  raster 2.7  glyphs 0.4  total 8.2
```

- "origin delta" is the distance between pen origins (from pdfium's text page API) of
  characters matched in reading order. Matching is a text alignment, so one dropped
  character does not shift every later pair.
- In the heatmap, blue marks ink that is only in the source and red marks ink that is
  only in Word's output.
- The fonts section lists every font that draws text in Word's PDF. With `--font`, it also
  checks glyph outlines against a font file. Word renames embedded fonts to
  `___WRD_EMBED_SUB_nn`, so the name proves nothing, but the outlines do.

### how Word is driven

`word_export.py` uses `DispatchEx("Word.Application")`, which always starts a new Word
process. It refuses to continue if that instance already has documents open. It opens the
docx read-only and invisible, with alerts off, then calls `ExportAsFixedFormat`. A `finally`
block closes the document without saving and quits that instance. The script only force-ends
a process if exactly one new `WINWORD.EXE` appeared and it ignores Quit. On this machine,
Word instances that were already open were never touched: their process ids were the same
before and after every run.

## units Word uses, and what they mean for 0.025 pt

| where | unit | step | worst rounding |
|---|---|---|---|
| frame x/y (`w:framePr`), indents, spacing, tab stops, `w:spacing` | twip = 1/20 pt | 0.05 pt | 0.025 pt (exactly the target) |
| font size `w:sz` | half point | 0.5 pt | a PDF at 10.3 pt cannot be expressed at all |
| DrawingML offsets (`wp:posOffset`) | EMU = 1/12700 pt | 0.00008 pt | negligible, but see below |
| Word's PDF export: coordinates (`Tm`) | 1/3000 in = 0.024 pt (measured) | 0.024 pt | 0.012 pt |
| Word's PDF export: font size (`Tf`) | 1/600 in = 0.12 pt (measured) | 0.12 pt | 10.5 pt is written as 10.56, 14 as 14.04, while 9 and 12 stay exact |
| Word's layout along a line | about 1/600 in (measured at 12 pt) | about 0.12 pt | see E3 probe |

The default printer on this machine is "Microsoft Print to PDF" at 600 dpi. Word is known
to lay out with printer metrics. Nobody has checked whether a different printer changes the
1/600 in behaviour.

## results on this machine (Word 16.0 build 16.0.20527, 2026-11-11)

### E1 font embedding

| case | font in Word's PDF | outlines match our font | outlines match Calibri |
|---|---|---|---|
| full TrueType, obfuscated `.odttf`, `w:embedTrueTypeFonts` on | `___WRD_EMBED_SUB_45` (TrueType) | 100% | 0% |
| same, but `w:embedTrueTypeFonts` missing from settings | `___WRD_EMBED_SUB_45` | 100% | 0% |
| fontTools subset (only used chars, no layout tables) | `___WRD_EMBED_SUB_45` | 100% | 0% |
| CFF-flavoured `.otf`, obfuscated the same way | **Calibri** (substituted silently) | 0% | 100% |
| control: family named, nothing embedded | **Cambria** (substituted silently) | 0% | 0% |

Word reads embedded TrueType fonts (whole or subset) whether or not the settings flag is
present. It ignores an embedded CFF font without any warning and falls back to Calibri.
(When nothing is embedded at all, it falls back to Cambria instead, probably because of
`w:family roman`.) **A converter has to convert CFF/Type1 fonts to TrueType outlines before embedding.**

### E2 positioning (300 lines, 8396 glyphs, 3 pages, sizes 9 and 10.5 pt, x = 72.37 + k*3.17, y step 13.61)

The top-to-baseline offset in a 13.6 pt exact line was measured on a separate calibration page:
frames 10.91, text boxes 10.94, paragraphs 10.87 pt (about 0.8 of the line height). The same
value applies to both sizes.

| method | all glyphs max / mean / p99 (pt) | within 0.025 | line starts dx max / mean | line starts dy max / mean | drift along line max / mean |
|---|---|---|---|---|---|
| frames | 0.266 / 0.070 / 0.187 | 8.9% | 0.086 / 0.030 | 0.110 / 0.039 | 0.207 / 0.045 |
| text boxes | 0.247 / 0.082 / 0.203 | 6.3% | 0.100 / 0.037 | 0.110 / 0.040 | 0.207 / 0.045 |
| paragraphs | 0.304 / 0.135 / 0.250 | 0.3% | 0.110 / 0.058 | 0.200 / 0.096 | 0.207 / 0.045 |

- Error does not build up from page to page: pages 1-3 give identical statistics.
- There is a small trend down each page, -0.012 to -0.015 pt per 100 pt, for all three
  methods.
- Paragraphs: the spacing is computed against absolute targets, so rounding does not build
  up. But the baseline offset measured on the calibration page did not carry over: there is a
  constant bias of -0.09 pt.
- Paragraphs: Word drops "spacing before" on the first paragraph of a page that follows a
  page break. Page 2 moved up by about 50 pt until a 1 pt spacer paragraph was added.
- Text boxes are positioned in EMU, yet they are no more accurate than frames positioned in
  twips.

### E3 advance widths (12 pt, frames, 8 lines per group, max / mean |dx| in pt)

| variant | natural | charSpace 0.37 | widths x1.043 | per-glyph jitter |
|---|---|---|---|---|
| plain (font advances) | 0.250 / 0.085 | 23.7 / 10.3 | 12.2 / 5.56 | 2.78 / 0.75 |
| per-char `w:spacing`, error diffusion | 0.250 / 0.085 | 0.186 / 0.060 | 0.267 / 0.071 | 0.294 / 0.082 |
| same, corrected once with Word's measured errors | 0.292 / 0.115 | 0.308 / 0.087 | 0.290 / 0.077 | 0.348 / 0.085 |
| same, corrected twice | 0.236 / 0.072 | 0.224 / 0.077 | 0.274 / 0.077 | 0.321 / 0.085 |
| rewritten `hmtx` (jitter falls back to spacing) | 0.250 / 0.085 | 0.276 / 0.063 | 0.248 / 0.055 | 0.294 / 0.082 |
| unhinted font, plain / with spacing | identical to the hinted rows | | | |

- The probe (`e3_spacing_probe.py`): when the ideal advance is a whole multiple of 0.12 pt,
  Word steps by exactly that amount. Otherwise the steps alternate between two values about
  0.12 pt apart.
- The average advance stays within -0.017 to +0.006 pt per character of the ideal.
- So Word places glyphs on roughly a 1/600 inch grid along a line and does not land exactly
  on fractional advances. Feedback correction does not converge, which fits a quantised
  response.
- TrueType hinting makes no difference.

### round 1 conclusion (superseded by round 2 below)

None of the methods got all glyphs within 0.025 pt. The best measured result is frames with
calibrated baseline offsets: line starts within 0.086 pt (mean 0.030), but glyphs inside a
line drift by up to 0.21 pt. Even the natural advances of the font, with no spacing changes,
drift by up to 0.25 pt at 12 pt. That error comes from Word's layout, not from the DOCX.
Frames are recommended over text boxes, which come close but no better, and over paragraphs,
whose dy bias is 0.09 pt.

## round 2: working with the 1/600 inch grid (2026-11-11, same machine and Word build)

### the model of Word, as measured

- **Inside a line:** pen positions are whole 1/600 inch pixels (0.12 pt).
  - If every glyph advance is a whole number of pixels (a grid font), then after n glyphs the
    pen sits at `floor(sum of advances + sum of w:spacing twips x 5/12)` pixels from the line start.
  - `e5_model_probe.py` sets random whole-pixel targets at 9, 10.5 and 12 pt. When the spacing
    aims a quarter pixel above each target, Word's layout lands on every target pixel.
  - The first attempt aimed at the pixel centre. It broke the floor rule once, at an exact
    half pixel. Aiming at a quarter pixel above keeps floor and round in agreement.
  - What remains is the PDF writer: Tc with 2 decimals, and TJ in 1/1000 em. Measured residual
    from the whole pixel: at most 0.026 pt at 12 pt and 0.056 pt at 9/10.5 pt (from a one-off
    decomposition of the `e5_grid_snap` output, not a committed script). Over 9,669 glyphs checked there were 0 layout pixel misses.
- **Twips and pixels:** a twip is 5/12 pixel. Twips and pixels coincide only every 12 twips
  (0.6 pt = 5 pixels). Spacing is not rounded to the grid per character; Word accumulates it
  exactly and floors the pen.
- **Frame x and y:** both land on the pixel grid, by a deterministic rule that depends only
  on the twip value (not on the other axis). The rule is not a plain floor or round of
  `twips x 5/12`; the same remainder mod 12 gives different results at different positions.
  So frames are placed by calibration: `frame_calibration.py` tries 9 neighbouring twip
  values per line on a separate document and keeps the best one.
- **Pixel phase:** PDF coordinates land at a pixel phase of 0 or +0.024 pt depending on the
  region of the page (0 below about 50 pt, +0.024 beyond). Above 100 pt the PDF prints only 5
  significant digits, which adds up to 0.005 pt of rounding.
- **Font size:** Word stores half points but draws glyphs at whole pixels per em:
  10.5 -> 10.56, 11.5 -> 11.52, 8 -> 8.04, 14 -> 14.04. Layout still uses the stored size.

### E5 grid-snapped advances: per-line frames, pt

Recipe, applied per line:

1. Use one grid font per font size. Its `hmtx` advances are whole pixels at that size: units
   per em = 50 x half points, 12 units per pixel.
2. Choose the frame x/y twips by calibration.
3. Give each glyph a target: the pixel nearest its source x, counted from the line start
   Word really produced.
4. Set per-character `w:spacing` from `grid_model.spacing_for_pixel_targets`.

The "floor" row is arithmetic: the distance from each source glyph to the nearest pixel of its
line. No grid-bound method can do better.

| set | variant | max | mean | p99 | <= 0.025 | <= 0.06 | \|dx\| max / mean | \|dy\| max / mean |
|---|---|---|---|---|---|---|---|---|
| E3 (12 pt, 4 groups) | floor (x only) | 0.060 | 0.031 | 0.060 | | | | |
| E3 | calibrated frames, plain font | 23.7 (charSpace lines) | 4.17 | 20.5 | 2.6% | 9.5% | 23.7 / 4.16 | 0.060 / 0.030 |
| E3 | **grid snap** | **0.110** | **0.050** | **0.098** | 12.0% | 68.6% | 0.104 / 0.035 | 0.060 / 0.030 |
| E2 (9 + 10.5 pt, 3 pages) | floor (x only) | 0.060 | 0.030 | 0.059 | | | | |
| E2 | calibrated frames, plain font | 0.227 | 0.064 | 0.171 | 8.8% | 52.3% | 0.227 / 0.050 | 0.060 / 0.030 |
| E2 | **grid snap** | **0.119** | **0.048** | **0.090** | 13.1% | 73.0% | 0.108 / 0.032 | 0.060 / 0.030 |

E3 grid snap, max / mean |dx| per group:

| group | max / mean \|dx\| |
|---|---|
| natural | 0.096 / 0.034 |
| charSpace | 0.104 / 0.034 |
| scaled | 0.099 / 0.036 |
| jitter | 0.087 / 0.035 |

Error no longer builds up along a line. Drift along a line is at most 0.082â€“0.090 pt at 12 pt,
compared with 0.26 pt before.

Cost:

- **Fonts:** one embedded font variant per size, about 96 KB each for this font, uncompressed
  before zip. Subsetting it was not tried.
- **Runs:** one run per character.
- **docx size, E2 set:** 150 KB with grid snap, 65 KB plain.
- **Word time:** export time is unchanged (about 5â€“6 s per harness run).

### E6 where the grid comes from

Every variant below gave the same result: line starts on the 0.12 pt grid, pen steps on
0.12 pt, the same x errors (-0.086..+0.034 pt for the sweep) and the same baselines.

- **compat settings:** compatibilityMode 11, 12, 14 and 15; `w:usePrinterMetrics` in mode 15
  and in mode 11; `w:doNotUseHTMLParagraphAutoSpacing`.
- **ExportAsFixedFormat:** OptimizeFor screen; UseISO19005_1 (PDF/A); BitmapMissingFonts off.
- **Save path:** SaveAs2 with wdFormatPDF instead of ExportAsFixedFormat.
- **`Document.Compatibility(...)` toggled on the open, read-only document:**
  - `wdUsePrinterMetrics`, `wdTruncateFontHeight`, `wdNoLeading`, `wdSubFontBySize`: Word
    refuses them in mode 15 ("out of range for this file format"). In mode 11 they are
    accepted but change nothing.
  - `wdSpacingInWholePoints` (mode 11 only): only drops `w:spacing` below 1 pt; the grid stays.
  - `wdExactOnTop`: only moves the baseline (9.6 -> 11.4 pt below the top of a 12 pt line); the
    grid stays.
  - `wdDontUseHTMLParagraphAutoSpacing`: no change.
  - `wdWPSpace` does not exist in this build's enum.
- **Font size 6, 9, 10.5, 12, 24, 48 pt:** the grid is constant in points (0.12 pt). Only the
  PDF writer's own noise grows with size, because it works in 1/1000 em.
- **Read only, not changed:**
  - Default printer: "Microsoft Print to PDF", 600 x 600 dpi.
  - `ActivePrinter`: "Microsoft Print to PDF".
  - `Options.UseSubPixelPositioning`: True. `UseCharacterUnit`: False.

So nothing on the document side moves the grid. Whether the printer driver's resolution
causes it is untested: changing the default printer is a system setting. Changing Word's
application options was not tried either, because Word saves them to the user's profile.

### E7 sizes a docx cannot store

The test set has 40 lines at 10.3, 11.27, 7.9 and 10.5 pt, with calibrated frames.

The grid font was built at the nearest half-point size. Its outlines and metrics were scaled
by pdf size / drawn size (for example 10.3 / 10.56).

| variant | max | mean | p99 | glyph size drawn, 7.9 / 10.3 / 10.5 / 11.27 pt |
|---|---|---|---|---|
| nearest half point, plain font | 4.79 | 1.25 | 4.24 | 8.04 / 10.56 / 10.56 / 11.52 (up to +0.26 pt) |
| grid font scaled to pdf size + grid snap | **0.120** | **0.048** | **0.094** | 7.901 / 10.292 / 10.509 / 11.267 (within 0.01 pt) |

Glyph size is measured from the median of matched ink-box heights. Odd sizes behave exactly
like storable ones once the font is scaled. The scaling also removes the 10.56-for-10.5 size
error that plain fonts have.

### E8 vertical

- **The grid:**
  - Baselines also land on the 0.12 pt pixel grid, with the same 0 / +0.024 pt phase effect.
  - A sweep of frame y in 1-twip sub-steps gave top-to-baseline distances of 10.82â€“11.02 pt:
    a 0.20 pt spread from rounding alone.
- **Calibrated frame y:** |dy| at most 0.060 pt (exactly half a pixel), mean 0.030 pt, on all
  three sets (E3, E2, E7). Uncalibrated it was 0.110 / 0.039 in round 1.
- **Font sizes, same frame top:** with exact line height, the 9 pt baseline was 10.94 pt
  below the top in all three cases:
  - alone;
  - next to a 14 pt frame;
  - sharing its line with 14 pt runs.

  Exact line spacing puts the baseline at a fixed share of the line height, whatever the size.
- **"At least" line height:** a 14 pt run in the same line pushes the 9 pt baseline from
  10.49 to 13.27 pt. Use exact line height.

### E9 one frame per word (fallback, not needed)

Test conditions: the E2 set, word frames at the word's source x (rounded twips, no
calibration and no grid font), on the line's calibrated y.

- **Accuracy:** max 0.249, mean 0.066, p99 0.185 pt. That is worse than per-line grid snapping
  (0.119 / 0.048), because rounding each word's start to twips without calibration costs more
  than drift inside a word.
- **docx size:** 82 KB (1,500 frames).
- **Word timings:** opening takes 0.66 s; export takes 2.27 s against 1.18 s for the per-line
  E5 document.
- **Editing:** Word sees 1,502 paragraphs, one word each, joined by paragraph marks. A
  three-word phrase search fails. The per-line document (302 paragraphs) finds it.

## what this means for the converter

- **Best measured glyph error through Word on this machine:** max 0.110â€“0.120 pt,
  mean 0.048â€“0.050 pt, p99 0.090â€“0.098 pt, combining x and y.
  - x alone: max 0.104â€“0.109, mean 0.032â€“0.035.
  - y alone: max 0.060, mean 0.030.
  - The arithmetic floor on the 1/600 inch grid is 0.06 max / 0.03 mean per axis. The rest of
    the gap is the PDF writer's rounding.
- **0.025 pt for every glyph is not reachable through Word's PDF export.** Only 12â€“13% of
  glyphs are within it. 69â€“73% are within 0.06 pt.
- **Recipe:**
  - One absolutely positioned frame per line, with exact line height.
  - An embedded TrueType grid font per (pdf size): stored at the nearest half point, outlines
    scaled to the pdf size, every advance a whole 1/600 inch pixel at the stored size.
  - One run per character, with `w:spacing` from `grid_model.spacing_for_pixel_targets`:
    each glyph's pixel target counted from the real line start, aimed a quarter pixel above.
  - Frame x/y twips from a calibration table of Word's twips-to-pixel mapping. It depends only
    on the twip value, so it could be built once per Word build instead of per document. That
    has not been tried.
  - Kerning and ligatures off; CFF fonts converted to TrueType first (E1).
- **Still unknown:**
  - Whether the grid follows the printer or the display. It was not tested, because that
    would need a system setting changed.
  - Whether what Word shows on screen or prints matches its PDF (E4).
  - The exact twips-to-pixel rule.
  - Whether the calibration table holds across Word builds and machines.
  - Other fonts, and rotated text.
  - Whether phrase search, editing and reflow still feel sane with one run per character.
    Search across runs worked; reflow when a user edits a line was not tested.

## measurement pitfalls found on the way

- pdfium stores `/Widths` as whole 1/1000 em values. reportlab writes fractional widths for
  a 1024-unit font, so pdfium's reading of the *source* PDF was off by up to 0.23 pt. Rescaling
  the test font to 1000 units per em fixed it. Every experiment now checks the source PDF
  against the line definitions (`source check ... within 0.00005 pt`) and stops if it is off
  by more than 0.001 pt.
- `w:kern` switches kerning *on* for sizes at or above its value. `w:kern w:val="0"` means
  "kern everything". It is omitted here. With this GPOS-only font, Word gave identical results
  with and without it.

## not verified

- Whether Word's PDF export matches what Word shows on screen or prints (E4, out of scope).
- Whether the 1/600 in behaviour depends on the default printer. E6 shows that no
  document-side setting changes it. The printer itself was not changed.
- Other fonts, sizes, Word builds, or a machine without "Microsoft Print to PDF" as the
  default printer.
- The CFF case has no CFF source PDF, because reportlab cannot write CFF. Its glyph deltas
  are meaningless; only the font identification counts.

## round 3: an end-to-end pdf to docx prototype, measured on a student corpus

Target: every glyph within 0.125 pt, mean under 0.05 pt, the same embedded fonts. All
numbers were measured through Word's PDF export on 2026-11-11 with the same Word build.
Full tables: `corpus_report.md` and `corpus_report.json`. Worst-page heatmaps:
`corpus_report_assets/`.

### how to run

```powershell
cd C:\Letters-fidelity\tools\fidelity
$py = "C:\Letters\tools\fidelity\.venv\Scripts\python.exe"
& $py -m pip install pikepdf                                   # MPL-2.0, used as a dev tool only
& $py make_corpus.py                                           # corpus\*.pdf + corpus\manifest.json (about 40 s)
& $py corpus_run.py                                            # convert + measure all six (about 4 min, first run)
& $py -m prototype.convert in.pdf out.docx                     # any text pdf
& $py run_prototype.py syllabus                                # one class, with harness
& $py -m prototype.diagnose out\prototype\NAME\NAME.report.json   # worst lines
& $py -m prototype.plan_check out\prototype\NAME                # planned vs produced pixels
& $py -m unittest test_prototype test_units                     # 30 tests, no word needed
```

Licenses:

- New dependency: pikepdf 10.18 (MPL-2.0; file-level copyleft, so acceptable as an unmodified
  dev tool here). It brings lxml (BSD-3).
- New fonts, all SIL OFL 1.1 from google/fonts: Lato, Tinos, Cousine, Arimo, Noto Sans Math,
  and Crimson Text Bold/Italic. Their license texts are in `fonts/`.
- Microsoft fonts: the corpus does not use them on purpose. Word itself still wrote ArialMT
  for the list tab spaces in the syllabus; the converter never embeds that font, because
  spaces take the font of their neighbours.
- No LibreOffice or LaTeX is installed, so neither was used. Scanned handouts belong to the
  OCR stage and are not part of this corpus.

### the prototype (`prototype/`)

The steps follow the module names:

1. **`pdf_content`** reads glyphs. Each glyph's text, code, font, size, advance, colour,
   rendering mode and angle come from its own content-stream interpreter (pikepdf parsing).
   That interpreter is used instead of pdfium's text page because only it gives char codes
   and the font program behind each glyph.
   - Its pen origins match pdfium's within 0.00006 pt on PDFs with whole-number widths.
   - On reportlab PDFs with fractional `/Widths`, pdfium is off by up to 0.5 pt (see below).
2. **`font_rebuild`** rebuilds every face as clean TrueType with fontTools:
   - only the glyphs that are used, from all subsets of a face (merging seen in the syllabus
     and the research paper);
   - a cmap from ToUnicode or the encoding, with private-use code points for glyphs whose
     text is missing, a ligature or ambiguous (0 of them in this corpus);
   - the PDF's widths as advances, and a unique family name;
   - CFF converted to quadratic outlines, with a bound of 1 unit per 1000 em;
   - fsType read and reported. A restricted font is replaced by a stand-in, and the report
     says so.

   Non-embedded standard fonts get metric-compatible OFL stand-ins (Arimo, Tinos, Cousine),
   carrying the PDF's own widths, so positions stay exact while glyph shapes differ.
3. **Grid fonts:** `font_prep.make_grid_font` builds one per face and size (round 2 recipe).
4. **`lines`** puts one frame per line. Superscripts and subscripts become their own lines
   at their own baseline.
5. **`calibration`** places frames by measured twips. The table is cached per Word build in
   `out/calibration_cache/`. Probe lines closer than 12 pt × 4 pt go on separate probe
   pages, because a subscript once matched its neighbour and poisoned the cache.
6. **Spacing:** per-character `w:spacing` comes from the floor model.
7. **`rotated`:** rotated lines become DrawingML text boxes. They are placed after one
   measuring export, because geometry alone missed by 19.8 pt.
8. **`vectors`:** rectangles and straight strokes become DrawingML shapes. Everything else
   (curves, dashes, clips that cut, shadings, transparency, patterns) is rendered from a
   page that holds only those items. The result is a transparent PNG behind the text, so
   text is never rasterised.
9. **`docx_out`:** pictures keep their JPEG bytes; one section per page size; invisible
   (render mode 3) text becomes `w:vanish`.

Fixes found on the way:

- Word ignored the first rebuilt fonts and drew Times New Roman. A font that claims no code
  page in OS/2 is replaced, so Latin-1 is always claimed now.
- Word did not honour a docx font that holds nothing but a space glyph.

### results (from `corpus_report.md`)

| class | producer | max | mean | p99 | <=0.125 | positions | fonts | SSIM min |
|---|---|---|---|---|---|---|---|---|
| syllabus | Word | 0.136 | 0.0374 | 0.125 | 98.9% | FAIL (max) | pass | 0.979 |
| research paper | Word | 0.171 | 0.0416 | 0.109 | 99.7% | FAIL (max) | pass | 0.968 |
| lab report | Word | 0.154 | 0.0395 | 0.126 | 98.8% | FAIL (max) | pass | 0.990 |
| slides | reportlab | 0.210 | 0.0630 | 0.207 | 90.5% | FAIL | pass | 0.986 |
| form | reportlab, standard 14 fonts | 0.156 | 0.0550 | 0.153 | 96.5% | FAIL | FAIL (4 stand-ins) | 0.990 |
| journal page | reportlab, 6 fonts, odd sizes | 0.222 | 0.0541 | 0.116 | 99.3% | FAIL | pass | 0.907 |

No class passes. The three Word-made classes miss only on the maximum: mean 0.037–0.042 pt,
y exactly 0, 99% of glyphs within 0.125 pt. The reportlab classes also miss the mean.

Pictures:

- Corner error is 0.000–0.067 pt.
- Every JPEG kept its original bytes, including reportlab's ASCII85-wrapped ones. The PNG
  in the lab report stays PNG.

Vectors:

- 284 vector items became shapes.
- 11 were rasterised (circles and a rounded rectangle on the slides, a dashed rule on the
  form).

Editability (Word opened each docx):

- **Find:** the known phrase was found in all six. A known sentence was found in 5 of 6; on
  the slides it spans two lines, and every line is its own paragraph.
- **Reading order:** similarity 1.000 for the five unrotated classes, 0.882 for the slides,
  where the rotated labels sit in text boxes.
- **Paragraphs:** one per line, plus one anchor paragraph per page.

Time:

- Conversion takes 9.0–18.6 s per document with a cold calibration cache, most of it Word
  probe exports. A repeat conversion with a warm cache took 0.3 s (syllabus).
- Word's export of the docx takes 0.7–1.3 s.

Calibration cache check: 4,212 cached twip values were probed again across all six
documents.

- x: no disagreements.
- y: 36 disagreements (0.9%), so y is not purely a function of (line height, twips). The
  cache is safe for x and nearly safe for y.

### what stands between the prototype and the threshold, by impact

1. **Word's PDF writer does not print the pixel its layout chose.** `plan_check` on the
   journal shows the offset from the whole pixel reaching 0.497 px (0.06 pt), mean
   0.142 px. 76 of 5,109 glyphs landed a whole pixel off the plan. This sits on top of the
   0.06 pt grid floor. Looks fundamental: it happens inside Word's export.
2. **Word-made sources carry the same writer offset.** Their coordinates are Word pixels
   plus up to about 0.05 pt of writer offset. Rounding a noisy source to the nearest pixel
   sometimes picks the wrong pixel, which is why syllabus, paper and lab report reach
   0.136–0.171 with y exactly 0.
   - Untried fix: recover Word's layout pixel from the source by modelling the writer's
     TJ/Tc arithmetic.
3. **Large text (20 pt and more) follows the floor model less well.** A probe at 20 pt with
   no `w:spacing` at all was off by up to 0.79 px. The 20 and 36 pt slide lines reach
   0.11–0.13 pt.
4. **Rotated text in text boxes:** after the measured correction, 0.21 pt remains (slides).
5. **Glyph size for odd sizes.** Positions are fine, but word-level heatmaps of the journal
   (SSIM 0.907) show outline differences on many words. That is partly sub-pixel shifts
   and partly the drawn-size scaling.
6. **Fonts that are not embedded in the source** (the form) can only get stand-ins. Their
   positions come from the PDF's widths, but the "same fonts" criterion cannot pass by
   definition.
7. **Mean on non-Word sources:** reportlab positions are not on Word's grid, so the 0.03 pt
   mean floor plus writer offset gives 0.054–0.063.

Word limits that look fundamental:

- the 1/600 inch layout grid (0.06 pt floor);
- the PDF writer's own rounding;
- half-point font sizes (worked around with scaled outlines);
- frames that cannot rotate;
- embedded CFF ignored (worked around by conversion);
- silent font replacement when a font claims no code page.

### round 3: not verified

- **CFF, Type1 and Type3 font paths:** implemented except Type3, which is reported. No
  producer on this machine writes them, so the corpus never exercises them and the CFF
  outline error was never measured on a document.
- **Restricted fsType fallback:** never exercised end to end (the policy is unit tested).
- **Right-to-left and complex-script detection:** only unit tested.
- **The >256-glyph subset split:** reportlab kept all 239 Crimson glyphs in one subset, so
  merging was only seen on Word's own split subsets.
- **Cache portability:** whether the cache holds across Word builds or machines.
- **Screen vs PDF:** whether Word's screen layout matches its PDF (E4).

Round 4 below closes most of these.

## round 4: Word's PDF writer modelled exactly (2026-11-11, same machine and Word build)

`RECIPE.md` is the algorithm as it now stands, written to be ported without the Python.
Full tables are in `corpus_report.md` and `corpus_report.json`; round 3's report is kept as
`corpus_report_round3.json` for the before and after.

### how to run (new in round 4)

```powershell
$py = "C:\Letters\tools\fidelity\.venv\Scripts\python.exe"
& $py e10_writer_probe.py x y --dense [--page a4|wide]       # writer probes (11-20k frames, about 20 s each)
& $py e10_writer_probe.py --analyse e10_x_dense e10_y_dense  # check word_writer_model on every saved probe
& $py e11_run_split.py        # which run property splits text objects
& $py e12_exported_plan.py    # plan on the print lattice, 9-36 pt
& $py e15_spacing_rule.py     # how w:spacing becomes pixels
& $py e16_spacing_phase.py    # long random spacings vs the balanced planner
& $py e13_standin_shapes.py   # ofl vs windows stand-in shapes
& $py e14_standin_ssim.py     # the form class with both stand-in sets
& $py make_corpus_extra.py    # 7 more corpus documents (edge headless + pikepdf), corpus\extra
& $py corpus_run.py --extra   # all 13 classes (about 25 min with the cache check)
& $py -m prototype.plan_check out\prototype\NAME     # planned vs produced layout pixels
& $py -m prototype.floor corpus\NAME.pdf             # the lattice floor of a source
& $py -m prototype.source_pixels corpus\NAME.pdf     # layout pixels recovered from a word-made pdf
& $py -m unittest test_writer_model test_prototype test_units   # 53 tests, no word needed
```

New modules:

- `word_writer_model.py` (pure, unit tested): the twips rule, step tables, print format,
  baseline rule, encode and decode.
- `word_pdf_raw.py`: the raw Tm/Tf/TJ operands as printed.
- `prototype/verify_pass.py`, `prototype/floor.py`, `prototype/source_pixels.py`,
  `prototype/decompose.py`.

### 1. Word's PDF export, modelled (e10)

Three probe documents, using a grid font so that pen offsets inside a line are known
exactly:

- frame x swept over every twip;
- frame y swept over every twip;
- lines at 9 to 36 pt.

They ran on letter, A4 and a 1008 x 612 pt page: 85,571 frames in all.

- **Frame twips to pixel**: `floor((5t+1)/12)`, minus one when 5t+1 is a multiple of 12 and
  t mod 60 is 19 or 55. It held for all 85,571 frames, x and y.
- **Pixel to printed number**:
  - Tm is printed in 1/3000 in units: `5p + k(p)`, where k counts the writer's "step
    pixels" up to p.
  - Steps are roughly page size / 4.73 apart, but not exactly, and not a function of the
    axis length alone (x 5100 px and y 5100 px differ). So they are measured per page size.
  - The value is printed with 5 significant digits and at most 3 decimals.
  - The media box is the page size rounded to whole pixels.
  - The model reproduces every printed Tm in all 85,571 probes. That needed one fix: the
    fifth step lands on `5(p+1)`, so it reads one pixel late if k < 5 is assumed.
- **Inside a text object** Word does not print the lattice:
  - It writes Tc and TJ in 1/1000 em against integer /Widths, correcting only past about
    3/1000 em.
  - Where drawn size = stored size (9, 36 pt), the rule "adjust when the error passes
    3/1000 em, by the rounded error" reproduces 99.5% and 99.6% of the TJ numbers.
  - Where they differ (10.5 drawn 10.56, 20 drawn 20.04) it reproduces only 45% and 81%;
    this part is not modelled.
  - This in-object rounding is the round 3 "writer residue" (up to 0.497 px). It also
    explains why 20 and 36 pt text looked off: the layout pixels themselves were right
    (e12).
- **Fix: every glyph in its own text object.** Word splits text objects wherever
  neighbouring runs differ in any property (e11: language, East Asian language, bidi
  language, noProof, colour, spacing, character style). The converter alternates
  `w:lang w:eastAsia` en-US / en-GB, which does nothing to Latin text.
- **e12 result**: random fractional targets planned on the lattice. 1,992 of 1,992 glyphs
  at 9, 10.5, 11, 12, 14, 20 and 36 pt printed exactly the planned number, with max error
  0.060 pt.

**(a) Recovering Word's layout pixels from a Word-made source** (`prototype/source_pixels.py`):

- Every glyph that starts a text object decodes exactly: 492 of 492 across syllabus,
  research paper and lab report.
- The 11,372 glyphs inside text objects carry the in-object rounding. Their distance to the
  nearest pixel is spread evenly (mean 0.25 px, max 0.50 px), so rounding cannot recover
  their pixel.
- This turned out not to be needed: the measurement compares printed positions, so the
  plan aims at the source's printed position, not its layout pixel.

**(b) Planning on the exported position**:

- Each glyph gets the layout pixel whose *printed* x is nearest its source x.
- The frame twips come from the closed rule instead of a probe.
- The baseline twips are the ones whose printed y is nearest.

On the three Word-made classes the result is exactly the lattice floor (table below).

### 2. The whole-pixel misses (e15, e16, journal)

The rule round 2 fitted, "floor of the cumulative spacing", is wrong.

- e15 (562 pens) found the real rule: **each glyph's `w:spacing` becomes whole pixels on its
  own**, `twips * 5/12` rounded half away from zero. A floor of the sum fitted only 66 of 121
  single values.
- e16 (1,980 steps of random spacings) found a second effect: Word also keeps the exact sum
  and moves a glyph by a pixel when the two drift about a pixel apart (47 of 1,980 steps).
  The exact trigger could not be fitted.
- The planner now picks, among the 2 or 3 twip values that give the wanted pixels, the one
  that keeps the exact sum within 5/24 px. That gave 0 of 1,980 steps off the rule
  (`grid_model.spacing_for_pixel_steps`).
- The round 3 plan had made each glyph's spacing the difference of cumulative values,
  which is how its 76 journal glyphs ended up a pixel away.
- One glyph is still not explained: the journal's italic "W" in "C. Writer". Word laid it
  out 1 px wider than its grid advance and moved it back 23 glyphs later.
- `prototype/verify_pass.py` handles this case. It exports once, decodes every glyph's
  layout pixel, re-plans each line from its first wrong glyph, and keeps the best plan per
  line, in at most 2 more exports. On the journal: 24 glyphs off, then 17, then 0, and the
  line keeps its best pass. That run is the one in the table. Every other class: 0 glyphs
  off plan after the first export.

### 3. Large text (20 pt and up)

Not a layout effect. With every glyph in its own text object, 20 pt and 36 pt lines print
exactly as planned (e12). The 0.79 px of round 3 is the in-object TJ threshold: 3/1000 em is
0.5 px at 20 pt and 0.9 px at 36 pt. The slides' 20 and 36 pt lines now sit on the floor.

### 4. Rotated text

- With `wrap="none"`, Word moved a rotated box's line whenever its content width changed:
  the 19.8 pt round 3 saw, and a new shift after every re-plan. With `wrap="square"` (box
  20 pt wider than the line) the start no longer depends on the content.
- The box itself moves in steps of about 0.36 pt (3 px). Successive corrections gave
  -0.184, +0.176, -0.184 and +0.104 pt along the line.
- So the converter:
  - moves the box up to 4 times;
  - measures where Word starts the line;
  - plans the glyphs against that start, as frames are planned.
- Slides: rotated glyphs within 0.064 pt along the line (was 0.21).
- This is a systematic measuring pass, not a prediction. It costs 5 exports per document
  with rotated text and is not cached. The rotated print lattice (a 0.12 pt lattice along
  the line, its step table unknown) is not modelled.

### 5. The lattice floor per class

`prototype/floor.py` takes each source glyph's distance to the nearest printable position
(x and y). No plan through Word can beat it. The achieved numbers equal the floor on every
class to within 0.001 pt (max) and 0.0004 pt (mean), apart from the slides' rotated line.
So the remaining error is Word's lattice, not the converter. Numbers are in the table below
and in `corpus_report.md`.

### 6. y calibration: what else y depends on

Round 3's y cache disagreed on 0.9% of values because **printed y depends on the page
height**: the writer's step table for that page size. Round 3's probe pages were not the
document's page size (and their height varied with the document). x had the same flaw
hidden: probe width was `max(612, max x + 60)`.

The cache now stores only page-size-independent facts:

- step tables keyed by axis and page pixels;
- baseline offset tables keyed by exact line height, 60 entries indexed by y twips mod 60.

The baseline rule turned out to be closed form (960 of 960 measured frames):

- For line twips mod 60 of 0 or 20: `top pixel + round(0.8 * line px)`.
- For mod 60 = 40: `bottom pixel - round(0.2 * line px)`.

Re-measuring every cached fact the 13 documents use gave 21 compared, 0 disagreements.

### 7. Non-embedded standard fonts (form)

- **fsType.** Windows ships Arial, Arial Bold, Times New Roman (regular, bold, italic) and
  Courier New. All have fsType 0x0008 (editable embedding), so a document may embed them.
  They are Monotype fonts licensed with Windows: never ship them; use them only if present
  on the user's machine. They are still not Helvetica or Times, so the "same fonts"
  criterion fails either way.
- **Glyph shape, open stand-ins vs Windows fonts** (e13). Printable ASCII, freetype at
  200 px/em, ink masks aligned on the pen origin:

  | pdf font | open stand-in | windows font | IoU mean / min | pixels differing |
  |---|---|---|---|---|
  | Helvetica | Arimo | Arial | 0.773 / 0.281 ('*') | 22.7% |
  | Helvetica-Bold | Arimo Bold | Arial Bold | 0.804 / 0.083 ('_') | 19.6% |
  | Times-Roman | Tinos | Times New Roman | 0.679 / 0.000 ('_') | 32.1% |
  | Times-Bold | Tinos Bold | Times New Roman Bold | 0.724 / 0.000 ('_') | 27.6% |
  | Courier | Cousine | Courier New | 0.301 / 0.000 ('(') | 69.9% |

- **Against what a viewer shows** (e14). Text-line SSIM against pdfium's rendering of the
  source, 28 lines, form class:
  - open stand-ins: mean 0.795, min 0.485;
  - Windows fonts: mean 0.868, min 0.623 (and real italics);
  - positions identical: max 0.085, mean 0.0447 pt.

  pdfium probably renders the missing standard fonts with Windows' own fonts here, which
  would favour the Windows set; that was not checked.

### 8. New corpus classes (`make_corpus_extra.py`, `corpus\extra\`)

No producer on this machine writes bare CFF or Type 1, so those pages are hand-built with
pikepdf, operator by operator.

Microsoft Edge 157 headless (Skia/PDF) prints the rest. It runs with a private profile in
`out\edge_profile`; the user's browser is not touched.

- Skia writes TrueType web fonts as Type0 CIDFontType2 Identity-H, and CFF web fonts as
  Type 3.
- It places every glyph with its own Tj and a fractional Td.

Fonts (all OFL): Crimson Text, Lato, Source Serif 4. Modified fonts are renamed: the Type 1
conversion is "Letters One Serif", the fsType 2 copy of Lato is "Letters Locked Sans".

Paths that broke the first time they ran, now fixed:

- Type 1 programs failed to rebuild: only CFF was converted to quadratic.
- Type 3 had no rebuild at all. It is now rebuilt from the glyph procedures' filled paths.
- A browser ToUnicode mapped a glyph to U+0000, and Word refused the DOCX.
- Ligature glyphs on private-use code points U+E000.. were drawn by Word in Calibri. They
  now use U+F0000.. (`w:sym` and a symbol cmap were tried; the latter broke the whole font).

### results, round 3 -> round 4 (from `corpus_report.md`)

Target: max 0.125 pt, mean under 0.05 pt, same embedded fonts. All numbers are combined x/y
distances in pt.

| class | producer | round 3 max / mean | round 4 max / mean | lattice floor max / mean | positions | fonts | overall |
|---|---|---|---|---|---|---|---|
| syllabus | Word | 0.136 / 0.0374 | 0.074 / 0.0278 | 0.074 / 0.0278 | pass | pass | **pass** |
| research paper | Word | 0.171 / 0.0416 | 0.063 / 0.0312 | 0.063 / 0.0312 | pass | pass | **pass** |
| lab report | Word | 0.154 / 0.0395 | 0.063 / 0.0285 | 0.063 / 0.0285 | pass | pass | **pass** |
| slides | reportlab | 0.210 / 0.0630 | 0.102 / 0.0494 | 0.083 / 0.0447 (rotated excluded) | pass | pass | **pass** |
| form | reportlab, std 14 | 0.156 / 0.0550 | 0.085 / 0.0447 | 0.085 / 0.0447 | pass | FAIL (stand-ins) | FAIL |
| journal page | reportlab | 0.222 / 0.0541 | 0.086 / 0.0496 | 0.086 / 0.0496 | pass | pass | **pass** |
| cid subset > 256 glyphs | Edge (Skia) | new | 0.078 / 0.0427 | 0.078 / 0.0427 | pass | pass | **pass** |
| Type 3 font | Edge (Skia) | new | 0.078 / 0.0416 | 0.078 / 0.0416 | pass | pass | **pass** |
| mixed styles (5 sizes, colour, sup/sub, small caps) | Edge (Skia) | new | 0.077 / 0.0403 | 0.077 / 0.0403 | pass | pass | **pass** |
| justified TJ kerning | pikepdf | new | 0.084 / 0.0479 | 0.084 / 0.0479 | pass | pass | **pass** |
| CFF font (FontFile3) | pikepdf | new | 0.085 / 0.0485 | 0.085 / 0.0485 | pass | pass | **pass** |
| Type 1 font (FontFile) | pikepdf | new | 0.085 / 0.0490 | 0.085 / 0.0490 | pass | pass | **pass** |
| restricted fsType | pikepdf | new | 0.085 / 0.0471 | 0.085 / 0.0471 | pass | FAIL (fsType 2) | FAIL |

- 11 of 13 classes pass; all 13 pass on positions.
- Every non-rotated class sits on its lattice floor. The slides' extra 0.019 pt max comes
  from the rotated line.
- Cache check: 21 cached facts measured again, 0 disagreements.
- Conversion with a warm cache takes 6.4 to 9.4 s per document, mostly the checking export.
  The journal took 28.4 s (two re-plan exports) and the slides 49.6 s (rotated measuring
  exports).

### what still blocks

- **form**: positions pass. Its fonts are not embedded in the source, so the "same fonts"
  criterion fails by definition, with either stand-in set.
- **restricted_font**: positions pass. fsType 2 forbids embedding, so a stand-in is used
  and the user must be told.
- Any class whose lattice floor mean is at or above 0.05 pt cannot pass, whatever the
  converter does. That happens to a two-axis off-lattice source whose baselines fall badly
  against the lattice. The journal page sits at 0.0497 (floor) / 0.0496 (achieved). This is
  Word's 1/600 in layout grid and its 1/3000 in print lattice, measured as above.

### round 4: not verified

- Other Word builds, machines, and default printers.
- Page sizes beyond the four measured (letter, A4, 1008 x 612, 720 x 405). New sizes are
  measured automatically by a probe export.
- The exact trigger of Word's spacing re-sync (e16), and why the journal's italic W was
  laid out 1 px wide.
- TJ numbers inside one text object where drawn size differs from stored size.
- A model of the rotated print lattice; rotated angles other than 90 degrees.
- Whether fsType 4 fonts make Word open the DOCX read-only.
- Right-to-left and complex scripts: still only unit tested.
- Whether pdfium uses Windows fonts for the form's missing fonts (e14).