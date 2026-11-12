# letters-fonts work log

What changed in this crate and why, newest last. Earlier history (the font
library: index, search, previews, embedding) is in the commits and README.md.

## 2026-11-12 outline geometry and distance for font matching

Added `src/matching/shape.rs` and `src/matching/distance.rs`. Text added to a
converted pdf must match fonts the pdf embeds only as subsets, so glyphs of
different fonts have to be compared. Outlines are kept TrueType-style
(quadratic, em units) so transforms, offsetting and writing all handle one
kind of curve; cubic curves are converted on the way in. Distance is the
chamfer (mean) and hausdorff (max) distance between evenly resampled
contours, so contour start points and point counts don't matter.

## 2026-11-12 glyph reading, measured style and outline offsetting

Added `glyphs.rs` (characters' glyphs and metrics from any font, variable
fonts at chosen axis values, kern and GPOS pair kerning), `style.rs`
(`StyleFit`: stem, heights, slant, contrast, serifs, spacing, which callers
use to keep spacing consistent) and `offset.rs` (emboldening and thinning by
moving points along the miter, capped so strokes can't turn inside out).
Contour direction is fixed for the whole glyph at once, because variable
fonts keep overlapping contours that a per-contour rule would turn into holes.

## 2026-11-12 donor pool, licence gating and identification

Added `donors.rs` and `identify.rs`. A donor may lend glyphs only when its
fsType allows editable embedding and subsetting; every refusal is a sentence.
Identification shortlists fonts by stored facts and then compares every
shared glyph; a font matching at least 90% of shared glyphs within 1.5 ‰ of
the em (8 ‰ at worst) and 2 ‰ in advance is the same design. Variable donors
are first set to their closest instance by golden-section search per axis.

## 2026-11-12 near-donor fit

Added `fit.rs` and `optimise.rs`: vertical zones, width and weight by least
squares over ink widths and stem widths, slant by search, a final polish,
and a spacing model, with every fourth shared glyph held out to report how
well the fit predicts glyphs it never saw.

## 2026-11-12 completion pipeline, writer, accents, generated glyphs, cache

Added `src/complete/`: `SubsetFont` (the document's font), the TrueType
writer (old glyphs byte for byte and number for number, new glyphs appended,
neutral family name, correct checksum adjustment), accent, ligature and
superscript synthesis, the generated-glyph request with its disclosure,
strict reply checks and style scoring (tested only against a fake
generator), the match report and its sentences, and the cache keyed by a
hash of the subset's outlines. `subset::read_loca` became `pub(crate)` and
`FontLibrary::preview_font_file` was added so the pipeline can reuse them.

## 2026-11-12 evaluation harness

Added `examples/completion_report.rs`, which cuts test fonts down to the
letters of one sentence, completes a second sentence under scenarios a to f
and compares with the true glyphs, writing `COMPLETION_REPORT.md`. First run:
exact donors and accents are invisible, near donors were plainly visible.

## 2026-11-12 separate ascender zone and a second spacing rule

The first run showed Arimo's ascenders landing 45 ‰ too high for Arial:
Arial's ascenders equal its capitals, Arimo's are taller, and one top zone
stretched one or the other. Lowercase letters now map through their own
ascender zone. Spacing gained a second rule (changed ink plus modelled side
bearings), chosen when it predicts the shared glyphs' advances better than a
line of the donor's advances.

## 2026-11-12 separate stem and bar weight, fairer generated-glyph scoring

Times New Roman Bold's serifs are 18 ‰ of the em thick; thinning it to the
regular weight by 15 ‰ a side turned the serifs inside out and pushed every
letter 30 ‰ below the baseline. Weight is now two amounts, one for vertical
stems (least squares on stem widths) and one for horizontal bars, hairlines
and serifs (median of their thickness differences), each capped by the
thinnest stroke it moves. The fit was split into `FitProblem` steps so each
fits on a screen. Generated glyphs were scored against the fitted glyph's own
advance, which always favoured the fitted glyph; the advance now counts only
when the pdf gives the true width. The harness now reports shapes (new glyphs
at the true advances) apart from line drift, and judges the seam on the worse
of shapes and spacing, because a wrong width shifts every later letter.

## 2026-11-12 split large files, tests, report and documentation

The readability check listed six new files over 500 lines and four long
functions. `truetype`, `fit`, `generated`, `pipeline` and `shape` became
folders split along their seams (each part says what it is for), the
harness became a multi-file example with one function per scenario, and
`complete_uncached`, `write_on_base` and the harness `main` were broken into
named steps. Added `tests/completion.rs` (the strategies end to end on the
Atkinson fixtures: exact, fitted, licence refusal, accents, cache, consent
for generated glyphs), `COMPLETION_REPORT.md` from the harness, the README
section on the contract, licences and limits, and folder READMEs. The
percent alike now reaches 0% at 30 ‰ mean distance instead of 100 ‰: with
the old scale a plainly visible look-alike read as 96% alike. `error.rs`
gained its opening comment.
