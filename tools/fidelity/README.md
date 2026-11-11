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
| `test_units.py` | unit tests for the parts that do not need Word (`python -m unittest test_units -v`) | under 1 s |

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

## conclusion so far

None of the methods got all glyphs within 0.025 pt. The best measured result is frames with
calibrated baseline offsets: line starts within 0.086 pt (mean 0.030), but glyphs inside a
line drift by up to 0.21 pt. Even the natural advances of the font, with no spacing changes,
drift by up to 0.25 pt at 12 pt. That error comes from Word's layout, not from the DOCX.
Frames are recommended over text boxes, which come close but no better, and over paragraphs,
whose dy bias is 0.09 pt.

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
- Whether the 1/600 in behaviour depends on the default printer.
- Other fonts, sizes, Word builds, or a machine without "Microsoft Print to PDF" as the
  default printer.
- The CFF case has no CFF source PDF, because reportlab cannot write CFF. Its glyph deltas
  are meaningless; only the font identification counts.
