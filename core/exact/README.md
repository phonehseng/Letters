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

## Three modes

`ConvertOptions::mode` picks one; `OutputMode::title` and `description` are
the export dialog's words.

| mode | for the export dialog |
|---|---|
| **Exact, Fitted Fonts** (`FittedLayout`, the default) | Every line is pinned in place and its words are drawn with fonts fitted to the PDF, the closest match of all; needs Microsoft Word to check it. |
| **Exact, Easy to Edit** (`EasyToEdit`) | Looks exactly like the PDF in Word, with real paragraphs that re-wrap when you type; lines Word can't place exactly stay fixed in place. |
| **Exact, Locked Layout** (`LockedLayout`) | Every line is pinned exactly where the PDF has it, the closest match, but typing doesn't re-wrap the text. |

The structure-first "Easy to Edit" mode lives in `core/convert`. Locked
Layout is round one, byte for byte (`tests/locked.rs`). Fitted Fonts is
round one's frames with the fitted document font (below); without Word it
writes round one's lines and says so. Easy to Edit is described in "Exact,
Easy to Edit: promotion with proof". The default follows the coordinator's
direction of 2026-11-12 (the fitted font as the default text path); Easy to
Edit is the mode whose text re-wraps.

## Exact, Fitted Fonts

Ported from `tools/fidelity/fitted-font` (REPORT.md, "Adopting it in
core/exact"), in `src/fitted`, with `fonts/composite.rs` and
`fonts/gsub.rs`:

- Round one's frames, baselines, grid fonts, pictures, shapes and rotated
  boxes stay. Every horizontal line's chars are cut into runs (same grid
  font, colour and visibility; a char outside the basic plane is a run of
  its own, probe c4) and runs into word pieces (a word and its trailing
  spaces, cut at 31.5 em and 30,000 units).
- Each piece is a composite glyph of the grid font: the source glyphs as
  components at exact offsets in font units (spaces left out, probe c3), a
  whole-pixel advance to the next piece's source start.
- Per feature one chaining lookup; each piece backtracks on the glyph of the
  piece before it; a run's first piece also needs the first char of the next
  piece (the known flaw of the experiment, fixed and tested in
  `gsub::tests::after_an_edit_a_later_copy_of_the_first_word_keeps_its_own_glyphs`).
- Slots: liga, then ss01 to ss20 (`w14:ligatures`, `w14:stylisticSets`),
  then a copy of the font under another family name; no two runs in a slot
  start with the same piece.
- Every piece is its own `w:r` with alternating `w:lang w:eastAsia`, no
  `w:spacing`.
- One measuring export, each piece's glyph found by its decomposed outline
  (Word renumbers glyphs), the residual baked into the composites' outlines,
  a second export is the result. `verify` reads each piece's glyph as its
  components (origin plus offset times size over units per em).
- Lines in a font with fsType 2 or 4 keep round one's runs (2 is ignored by
  Word, 4 makes it wait on a prompt under automation).

Measured 2026-11-12 through Word 16.0.20527.20150 (`tests/word_fitted.rs`),
next to the locked layout (round one, table above):

| document | glyphs | fitted max / mean | locked max / mean | ≤0.025 | pieces found | exports | seconds | one-word edit: glyphs on other lines that moved |
|---|---|---|---|---|---|---|---|---|
| syllabus | 2567 | 0.0050 / 0.0023 | 0.074 / 0.0278 | 100% | 497 of 497 | 2 | 26 | 0 of 491 |
| research paper | 5747 | 0.0050 / 0.0021 | 0.063 / 0.0312 | 100% | 1122 of 1131 | 2 | 19 | 0 of 1116 |
| lab report | 1628 | 0.0050 / 0.0024 | 0.063 / 0.0285 | 100% | 307 of 311 | 2 | 17 | 0 of 302 |
| slides | 389 | 0.1024 / 0.0114 | 0.102 / 0.0477 | 89.2% | 56 of 56 | 2 | 51 | 0 of 95 |
| form | 517 | 0.0064 / 0.0033 | 0.085 / 0.0447 | 100% | 97 of 97 | 2 | 20 | 0 of 94 |
| journal page | 5109 | 0.0066 / 0.0026 | 0.086 / 0.0496 | 100% | 923 of 925 | 2 | 35 | 0 of 915 |
| cid subset | 972 | 0.0050 / 0.0026 | 0.078 / 0.0427 | 100% | 146 of 146 | 2 | 20 | 0 of 130 |
| Type 3 font | 972 | 0.0050 / 0.0025 | 0.078 / 0.0416 | 100% | 225 of 226 | 2 | 21 | 0 of 221 |
| mixed styles | 387 | 0.0063 / 0.0029 | 0.077 / 0.0403 | 100% | 102 of 108 | 2 | 18 | 0 of 99 |
| justified TJ kerning | 1230 | 0.0050 / 0.0025 | 0.084 / 0.0479 | 100% | 240 of 240 | 2 | 17 | 0 of 234 |
| CFF font | 1338 | 0.0050 / 0.0026 | 0.085 / 0.0485 | 100% | 245 of 245 | 2 | 17 | 0 of 231 |
| Type 1 font | 1255 | 0.0050 / 0.0025 | 0.085 / 0.0490 | 100% | 251 of 251 | 2 | 18 | 0 of 233 |
| restricted fsType | 785 | 0.0050 / 0.0024 | 0.085 / 0.0471 | 100% | 149 of 149 | 2 | 17 | 0 of 135 |

Every glyph of every document is paired. The slides' maximum and p99 are
its rotated line, still on round one's rotated text boxes (the fitted path
does horizontal lines only). Exports are the fitted path's two; round one's
checking export runs before them. The one-word edit types one char after
the first word of a line through Word and compares Word's export with the
unedited one: on every other line no glyph moved (to 0.0001 pt). The form
and the restricted font still fail "same fonts" (stand-ins), as in round
one. Not done: fitted pieces inside the easy to edit mode's paragraphs,
rotated lines, and finding the 1 to 9 pieces per document the outline
search misses (their glyphs are still paired by the verifier).
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
| `convert` | the pipeline, the checking export and re-plan, the report; `hybrid_pass` hands the planned lines to `hybrid` | 1, 7.6 |
| `fitted` | Exact, Fitted Fonts: pieces, composites, slots, rules, the measuring export and the bake | below |
| `hybrid` | Exact, Easy to Edit: paragraphs, candidates, proof, stacks, font completion, cache, edit tests | below |

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

## Exact, Easy to Edit: promotion with proof

Round one's output looks right but edits badly: typing into a line pushes
only that line. The easy to edit mode starts from round one's plan (every
line, every glyph's target pixel) and replaces positioned lines by real
paragraphs wherever Word is shown to put every glyph where round one would.

1. **Paragraphs** (`hybrid/paragraphs.rs`). Consecutive planned lines join
   when they have the same grid font, a steady pitch (within one 0.12 pt
   pixel, Word sources alternate by a pixel), the same left edge after the
   first line (which may be indented or hanging), no list marker at the
   start, and a greedy break (the next line's first word would not have fit
   on the line, against the widest line starting near the same left edge).
   Justified: every line but the last ends at one right edge and some space
   is stretched. Rotated, invisible and superscript lines are left alone
   (a superscript is a line of its own in round one's plan).
2. **Candidates** (`hybrid/candidates.rs`). One paragraph, real spaces
   between lines, no forced breaks, exact line pitch, indents, `jc both`
   when justified. Three levels, least intrusive first: *natural* (left:
   only word starts corrected, on the space before them; justified: only
   glyphs inside words, Word spreads the spaces), *every glyph* (round
   one's spacing on each glyph, inside one paragraph), *pinned* (left
   aligned, a `w:br` after each line whose break Word would not make by
   itself, then the right edge two pixels wider). The right edge is chosen
   inside the window that keeps every break: at least the longest line,
   less than any line plus its next word (minus Word's justified squeeze of
   a quarter of the spaces). Every glyph keeps its own run: inside one text
   object Word's PDF writer places glyphs by its own rounding (recipe 6.4),
   and runs are invisible to the person.
3. **Vertical** (`hybrid/vertical.rs`). The exact line height (pitch ± 3
   twips) and the first line's top whose printed baselines are nearest the
   source's; a line top on a pixel edge must pass either neighbouring pixel.
   Each line height's 60-phase baseline table is measured first, in one
   Word session (`calibration::measure_baselines`), because the closed rule
   only holds for the 16 heights the recipe measured.
4. **Proof** (`hybrid/proof.rs`, `hybrid/ladder.rs`). All candidates of a
   round go into one export, each in its own frame so none can move
   another. A candidate is accepted when every one of its glyphs is found
   on its page within 0.125 pt (a wrong break moves words by a line, so this
   also proves the breaks) and its glyphs are on average at most 0.012 pt
   further than round one's lines put them. A paragraph that fails every
   level is split at its failing lines; the pieces try the pinned levels
   again, the failing lines stay positioned. At most 6 proof exports.
5. **Anchoring** (`hybrid/stacks.rs`). Accepted paragraphs of a column go
   into one frame: paragraphs with the same `w:framePr` stack, so a
   paragraph that grows pushes those below it in its frame, and nothing
   else. Gaps are `w:spacing w:before` only (Word keeps the larger of space
   after and space before). A final export checks the stacked document: a
   paragraph that moved gets a frame of its own, one that still fails goes
   back to positioned lines, and if the document mean would reach 0.05 pt
   the worst promotions are demoted. The report always comes from the last
   export of the final docx.
6. **Without Word** the same arithmetic predicts: a paragraph is promoted
   only at the every-glyph level, left aligned, with at least two pixels of
   room on either side of every break, and the report says "Predicted, not
   checked against Word on this computer." The layout engine
   (`core/layout`, `Flavor::Word`) could lay out one paragraph through a
   `letters-office` document with `ProvidedFonts`, but it breaks lines in
   fractional points with the same quarter-space squeeze, so for grid fonts
   with per-glyph spacing it decides the same breaks as the pixel window
   used here; it is not wired in (it would add `letters-layout` and
   `letters-office` as dependencies for no new information).
7. **Cache** (`hybrid/cache.rs`). Accepted decisions are kept per pdf hash,
   crate version and Word build in the cache folder; a second conversion
   skips the proof rounds and runs only the final checking export.

### Measured rules behind it (`tests/hybrid_probe.rs`, Word 16.0.20527)

- Indents: 5/12 px per twip, rounded half down, left and first-line indent
  summed in twips first: 234 of 234 line starts (6 documents, 78 paragraphs).
- Space between paragraphs: the larger of space after and space before, not
  the sum (all 144 gaps).
- Line tops inside a paragraph add up in twips and each follows the frame
  rule with its height's measured table: 393 of 393 first and second lines,
  390 of 393 third lines (heights 200 to 330 twips); with the closed rule 57
  of 393 were a pixel off. Exactly on a pixel edge Word went either way
  (25 of 32 edge lines differed from the rule), so placement avoids edges.
- Body flow (the page's text area, margins for the column) put 75 of 234
  lines a pixel off the frame rule against 25 of 234 for stacked frames
  (before the tables); it also holds one column per section only. Stacked
  frames were kept, body flow is not used (`glyphsInBodyFlow` is 0).

### Measured: the corpus in Exact, Easy to Edit

2026-11-12, Word 16.0.20527.20150, `tests/word_hybrid.rs` (one cold run:
hybrid decisions not cached; Word facts cached). Fidelity is measured
exactly as for the locked mode (`verify`, glyph positions in Word's own PDF
export of the final docx). "Locked" is the table above (the locked layout's
bytes are unchanged, `tests/locked.rs`). Text shares count measured
glyphs. Pieces: a paragraph cut into vertical pieces counts once per piece,
so promoted can exceed detected. Exports: proof rounds, the baseline-table
session and the final checks (round one's checking export comes on top).

| class | max (locked) | mean (locked) | p99 | ≤0.125 | positions | text in multi-line paragraphs | single-line paragraphs | positioned lines | multi-line promoted / detected | pins / splits / cuts | shared frames (paragraphs) | exports | seconds |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| syllabus | 0.121 (0.074) | 0.0280 (0.0278) | 0.060 | 100% | pass | 62.3% | 35.5% | 2.1% | 7 / 6 | 0 / 1 / 1 | 7 (27) | 7 | 65 |
| research paper | 0.071 (0.063) | 0.0312 (0.0312) | 0.059 | 100% | pass | 69.0% | 29.6% | 1.4% | 21 / 13 | 0 / 0 / 11 | 8 (19) | 5 | 44 |
| lab report | 0.063 (0.063) | 0.0285 (0.0285) | 0.060 | 100% | pass | 81.6% | 18.4% | 0.0% | 5 / 5 | 0 / 0 / 0 | 3 (7) | 4 | 39 |
| slides | 0.110 (0.102) | 0.0481 (0.0477) | 0.102 | 100% | pass | 59.4% | 40.6% | 0.0% | 4 / 4 | 0 / 0 / 0 | 3 (7) | 4 | 74 |
| form | 0.124 (0.085) | 0.0462 (0.0447) | 0.093 | 100% | pass (fonts FAIL, stand-ins) | 38.4% | 54.8% | 6.8% | 1 / 1 | 0 / 0 / 0 | 2 (11) | 4 | 34 |
| journal page | 0.124 (0.086) | 0.0497 (0.0496) | 0.083 | 100% | pass | 62.9% | 36.3% | 0.8% | 12 / 12 | 0 / 0 / 0 | 7 (21) | 5 | 60 |
| cid subset > 256 glyphs | 0.092 (0.078) | 0.0454 (0.0427) | 0.091 | 100% | pass | 89.8% | 10.2% | 0.0% | 4 / 4 | 0 / 0 / 0 | 1 (5) | 4 | 33 |
| Type 3 font | 0.078 (0.078) | 0.0416 (0.0416) | 0.076 | 100% | pass | 97.1% | 2.9% | 0.0% | 4 / 4 | 0 / 0 / 0 | 1 (5) | 3 | 25 |
| mixed styles | 0.086 (0.077) | 0.0405 (0.0403) | 0.076 | 100% | pass | 47.0% | 53.0% | 0.0% | 1 / 1 | 0 / 0 / 0 | 2 (5) | 4 | 33 |
| justified TJ kerning | 0.092 (0.084) | 0.0490 (0.0479) | 0.088 | 100% | pass | 97.0% | 2.5% | 0.5% | 4 / 4 | 0 / 0 / 1 | 1 (5) | 3 | 25 |
| CFF font | 0.085 (0.085) | 0.0485 (0.0485) | 0.082 | 100% | pass | 97.6% | 2.4% | 0.0% | 5 / 5 | 0 / 0 / 0 | 1 (6) | 3 | 28 |
| Type 1 font | 0.085 (0.085) | 0.0490 (0.0490) | 0.083 | 100% | pass | 98.2% | 1.8% | 0.0% | 5 / 5 | 0 / 0 / 0 | 1 (6) | 3 | 28 |
| restricted fsType | 0.107 (0.085) | 0.0474 (0.0471) | 0.084 | 100% | pass (fonts FAIL, fsType 2) | 95.9% | 4.1% | 0.0% | 3 / 3 | 0 / 0 / 0 | 1 (4) | 3 | 29 |

All 13 classes pass on positions in easy to edit mode; the same two fail
"same fonts" as in locked mode, for the same reasons. Body flow is 0% by
design. The maximum rises where promotions were accepted closer to the
0.125 pt limit than round one's lines; the mean stays within 0.003 pt of
locked. A second conversion of the same pdf (`a_second_conversion_reuses_the_decisions`):
cache hit, 1 export instead of 4, 14 s instead of 33 s.

**Edit tests** (`edits.rs`, through Word automation, every document): a
76-char sentence typed into the middle of the promoted paragraph with the
most paragraphs below it in its frame, a sentence (or five words) deleted
from it, 35 chars typed at the end of the last promoted paragraph, and one
char typed and undone. Pass means: nothing outside the paragraph and the
paragraphs below it in its frame moved (to 0.002 pt), everything below moved
straight down by the change in lines times the line pitch (to the pixel
Word rounds line tops to), the edited paragraph re-wrapped, and typed text
was drawn in the embedded fonts.

| class | insert | delete | type at end | type and undo | glyphs outside that stayed put |
|---|---|---|---|---|---|
| syllabus | pass, +1 line, 67 below moved | pass, -1 line | pass | pass | 2384 of 2384 |
| research paper | pass, +2 lines, 196 below | pass, -1 | pass | pass | 5088 of 5088 |
| lab report | pass, +1, 179 below | pass, -1 | pass | pass | 1071 of 1071 |
| slides | pass, +3, 41 below | pass, 0 | pass | pass | 286 of 286 |
| form | pass, 0 | pass, -1 | pass | pass | 304 of 304 |
| journal page | pass, +3, 1425 below | pass, -1 | pass | pass | 3369 of 3369 |
| cid subset | pass, +1, 648 below | pass, -1 | pass | pass | 100 of 100 |
| Type 3 font | pass, +1, 702 below | pass, -1 | pass | pass | 28 of 28 |
| mixed styles | not run | not run | pass | not run | 206 of 206 |
| justified TJ kerning | pass, +1, 911 below | pass, -1 | pass | pass | 37 of 37 |
| CFF font | pass, +1, 1057 below | pass, -1 | pass | pass | 32 of 32 |
| Type 1 font | pass, +1, 994 below | pass, -1 | pass | pass | 23 of 23 |
| restricted fsType | pass, +1, 513 below | pass, 0 | pass | pass | 32 of 32 |

The anchoring that survives edits in Word is the stacked frame: same
`w:framePr` on consecutive paragraphs, gaps as space before. Mixed styles'
only multi-line paragraph contains ligature chars (private use), so no
unique plain needle was found for the middle edits. Lines left positioned
(and paragraphs in other frames) never move, even when an edited paragraph
grows over them: that is the limit of (iii) here.

**Back and forth** (PDF, Word's export of the docx, that PDF converted
again, Word's export again):

| class | gen 1 vs pdf max / mean | gen 2 vs gen 1 | gen 2 vs pdf | promoted gen 1 / gen 2 |
|---|---|---|---|---|
| syllabus | 0.121 / 0.0280 | 0.000 / 0.0000 | 0.121 / 0.0280 | 7 / 6 |
| research paper | 0.071 / 0.0312 | 0.000 / 0.0000 | 0.071 / 0.0312 | 21 / 21 |
| lab report | 0.063 / 0.0285 | 0.000 / 0.0000 | 0.063 / 0.0285 | 5 / 5 |
| slides | 0.110 / 0.0481 | 0.088 / 0.0084 | 0.110 / 0.0458 | 4 / 4 |
| form | 0.124 / 0.0462 | 0.000 / 0.0000 | 0.124 / 0.0462 | 1 / 1 |
| journal page | 0.124 / 0.0497 | 0.000 / 0.0000 | 0.124 / 0.0497 | 12 / 13 |
| the other 7 | as gen 1 | 0.000 / 0.0000 | as gen 1 | same |

Text survives as text (every glyph matched in both generations) and fonts
stay embedded. Repeated conversion converges: the second generation
reproduces the first exactly everywhere except the slides' rotated line.
Before the "as close as round one" rule (first run, same day) the second
generation drifted to 0.16-0.24 pt from the pdf on 8 classes.

**Fonts for new text** (`completion.rs`): every face covers latin-1, latin
extended-a, punctuation, digits and currency; typed text in every edit test
was drawn from the embedded fonts (`___WRD_EMBED_SUB_nn`). Measured
look-alike overlap of the donor chosen, per face (`tests/hybrid_offline.rs`):
Helvetica, Times and Arimo/Tinos/Cousine faces 98-100%; Lato 70-78%
(Arimo); Source Serif and the Word-made Times/Calibri subsets 63-75%
(Tinos); Crimson Text 63-68% (Tinos), its italic 41% and Lato Italic 53%:
weak (no italic donor is bundled), and the converter says so in a
warning. Faces with no letters (symbol subsets) are not compared. Accented
letters whose base the face has are composed from the face's own base and
the donor's accent (161 per face typically).

**LibreOffice** (`tests/libreoffice.rs`, headless export with a private
profile, same verifier): the distance is large, so no ODT path was
written. Max / mean in pt, share within 0.125 pt, glyphs the verifier could
pair (LibreOffice 26.8, 2026-11-12):

| class | easy to edit docx | locked layout docx |
|---|---|---|
| syllabus | 452 / 11.1 / 17% (1167 of 2567) | 7.2 / 1.05 / 28% (1167) |
| research paper | 229 / 23.1 / 23% (2201 of 5747) | 7.5 / 0.79 / 26% (2201) |
| lab report | 0.53 / 0.15 / 52% (212 of 1628) | 1.7 / 0.40 / 29% (212) |
| slides | 0.32 / 0.14 / 55% (99 of 389) | 0.44 / 0.19 / 23% (99) |
| form | 252 / 5.9 / 30% (517 of 555) | 3.9 / 0.70 / 46% (517) |
| journal page | 406 / 2.3 / 40% (5109) | 2.3 / 0.53 / 16% (5109) |
| cid subset | 337 / 4.3 / 31% (972 of 978) | 3.9 / 0.86 / 11% (972) |
| Type 3 font | 439 / 41.4 / 0.2% (972 of 974) | 4.2 / 1.70 / 8% (972) |
| mixed styles | 446 / 19.4 / 17% (387 of 389) | 2.9 / 0.90 / 20% (387) |
| justified TJ kerning | 454 / 58.0 / 9% (1230) | 5.6 / 0.55 / 21% (1230) |
| CFF font | 1.17 / 0.37 / 31% (1338) | 2.6 / 0.95 / 9% (1338) |
| Type 1 font | 1.77 / 0.55 / 18% (1255) | 4.4 / 1.72 / 9% (1255) |
| restricted fsType | 1.43 / 0.45 / 23% (785) | 1.43 / 0.45 / 23% (785) |

The headless LibreOffice processes ended on their own after each export
(none was left). What it shows: LibreOffice does not stack paragraphs
that share `w:framePr` into one frame (each lands on its own, so stacked
paragraphs are hundreds of points off), and even single-line frames are
0.4 to 7.5 pt off: LibreOffice lays out in its own units with its own font
metrics and none of Word's 1/600 in grid, so nothing of the measured Word
model transfers. On the Word-made documents the verifier paired fewer than
half the glyphs (LibreOffice's PDF of the re-embedded `___WRD_EMBED_SUB`
subsets reads back as other text). An ODT-targeted recipe would need: a
measured LibreOffice model (its twip-to-device rounding, baseline in exact
lines, kerning and justification rules, the PDF writer's precision); text
frames (`draw:frame` with `draw:text-box`) holding consecutive paragraphs
instead of `w:framePr` stacks; its own checking export through a headless
LibreOffice with a private profile (the harness in `tests/libreoffice.rs`);
and a check that it uses embedded TrueType fonts the way Word does.

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
- **Editing.** Locked layout: one frame per line, one run per char,
  alternating East Asian language tags, sizes rounded to half points
  (glyphs scaled to match). Easy to edit: see its section; its limits are
  listed under "Limits of the easy to edit mode".
- **Not verified.** Other Word builds, another default printer, page sizes
  until measured, and screen against PDF. The step probes ran here on
  720 x 405 pt (both markings, see above); the baseline probe for an
  unseen line height was not run, because the built-in tables cover the
  corpus (the closed rule it falls back to matched 960 of 960 frames).
  `RECIPE.md`'s `LETTERS_STANDINS=windows` option is not ported.

## Limits of the easy to edit mode

- Superscripts, subscripts and anything on its own baseline inside a line
  stay positioned lines, and so does the line around them (it is split in
  pieces at the shift). Rotated and invisible text too.
- Only left and justified paragraphs are detected; centred and right
  aligned text stays single-line paragraphs, left aligned in its frame
  (typing into it extends it to the right).
- No tables, text boxes, section columns or body flow: grid-like content
  (forms, schedules) becomes single-line paragraphs in frames of their own.
  Floating tables with exact rows, as the product owner asked, were not
  tried this round.
- Positioned lines and other frames never move when an edited paragraph
  grows over them.
- Every glyph keeps its own run and most carry character spacing of a few
  twips (the every-glyph level): invisible in Word's interface, but typed
  text inherits the spacing of the run it is typed into (at most a pixel per
  char).
- The right indent of a paragraph inside a shared frame and the frame width
  use the indent rule (5/12 px per twip, half down) without their own probe;
  the final checking export catches a paragraph where that is wrong.
- Pitch patterns of Word-made sources that no exact line height reproduces
  cut paragraphs into stacked pieces: text then re-wraps within a piece only.
- The fonts for new text come from three bundled families (Arimo, Tinos,
  Cousine): italic faces get an upright donor and a weak-match warning.
- Without Word nothing is checked and the report says so.
- Not verified: other Word builds, edits beyond the four kinds tested,
  paragraphs crossing a page, and LibreOffice (measured far off, above).

## The OOXML this crate writes

Every part is written by hand (`src/docx`, `src/hybrid/candidates.rs`),
with the standard namespaces only (`w`, `r`, `wp`, `a`, `pic`, `wps`, `mc`,
`w14`; `mc:Ignorable="w14"`) and no private extensions:

- **Package parts**: `[Content_Types].xml`, `_rels/.rels`,
  `word/document.xml` and its rels, `word/styles.xml` (document defaults
  and a `Normal` style only), `word/settings.xml` (`w:embedTrueTypeFonts`,
  `w:defaultTabStop`, `w:autoHyphenation false`,
  `w:characterSpacingControl doNotCompress`, `compatibilityMode` 15),
  `word/fontTable.xml` with `w:embedRegular` pointing at obfuscated
  `word/fonts/fontN.odttf` parts (ECMA-376 17.8.1), `word/media/imageN.png|jpeg`.
- **Sections**: one per page size; `w:pgSz` (with `w:orient="landscape"`
  when wider than tall), all margins 0, `w:cols w:space="0"`,
  `w:docGrid w:linePitch="360"`. Pages of one size are separated by an empty
  1 pt paragraph with `w:pageBreakBefore`; a size change by a 1 pt paragraph
  carrying the section's `w:sectPr`.
- **Positioned lines** (locked layout, and the lines easy to edit leaves in
  place): a paragraph with `w:framePr w:w w:wrap="through" w:vAnchor="page"
  w:hAnchor="page" w:x w:y` (no height), `w:spacing w:line w:lineRule="exact"`,
  zero indents, one run per char.
- **Promoted paragraphs** (easy to edit): consecutive paragraphs with the
  same `w:framePr` (which Word stacks in one frame), each with
  `w:widowControl 0`, `w:suppressAutoHyphens`, `w:autoSpaceDE/DN 0`,
  `w:adjustRightInd 0`, `w:snapToGrid 0` (in schema order),
  `w:spacing w:before w:after="0" w:line w:lineRule="exact"`,
  `w:ind w:left w:right w:firstLine|w:hanging`, `w:jc w:val="both"` when
  justified, and `<w:r><w:br/></w:r>` for a pinned line.
- **Runs**: `w:rFonts` (all four slots the grid family), `w:vanish` for
  invisible text, `w:color`, `w:spacing` (character spacing in twips),
  `w:sz`/`w:szCs`, `w:lang w:eastAsia` alternating en-US/en-GB (no effect
  on latin text, keeps runs apart, recipe 6.4), `w14:ligatures none`; no
  `w:kern`. One `w:t xml:space="preserve"` per run.
- **Drawings**, all in one 1 pt anchor paragraph per page: `wp:anchor`
  relative to the page, `wp:wrapNone`, `behindDoc` 1, holding `pic:pic`
  pictures, `wps:wsp` rectangles and lines (`prst` rect or line), and for
  rotated lines `wps:wsp` text boxes (`wps:txbx`, `bodyPr wrap="square"`,
  zero insets) with one paragraph like a positioned line. Shapes are wrapped
  in `mc:AlternateContent`/`mc:Choice Requires="wps"` **without an
  `mc:Fallback`** (Word reads them without one; a reader that needs VML
  gets nothing for them). Adding VML fallbacks would change the locked
  layout bytes, so it was not done in this round.

- **Fitted fonts**: runs as above without w:spacing, one per piece, with
  w14:ligatures w14:val="standard" (slot liga) or
  w14:stylisticSets/w14:styleSet w14:id="1..20", and
  w14:ligatures w14:val="none" on plain text. The embedded fonts gain
  composite glyphs (glyf composite records, no ROUND_XY_TO_GRID) and a
  GSUB table (DFLT and latn, features liga and ss01..ss20, chaining format 3
  rules calling ligature or single substitutions, inside extension lookups);
  font copies are separate w:font entries named <family> f1 and so on.

Nothing else is emitted: no tables, no text boxes for horizontal text, no
section columns, no fields, no numbering definitions (list markers stay
text with a hanging indent).

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
- `cargo test -p letters-exact`: also the locked layout's byte for byte
  check (`tests/locked.rs`) and the easy to edit mode without Word
  (`tests/hybrid_offline.rs`).
- `cargo test -p letters-exact --release --test word_fitted -- --ignored --nocapture --test-threads 1`:
  the fitted fonts corpus through Word with the one-word edit (about 6 minutes).
- `cargo test -p letters-exact --release --test word_hybrid -- --ignored --nocapture --test-threads 1`:
  the easy to edit corpus through Word with the edit tests and the second
  generation (about 20 minutes), and the cache test.
- `cargo test -p letters-exact --release --test hybrid_probe -- --ignored --nocapture`:
  the paragraph layout probes behind `hybrid/rules.rs` and `vertical.rs`.
- `cargo test -p letters-exact --release --test libreoffice -- --ignored --nocapture`:
  the corpus docx files exported by LibreOffice.
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
