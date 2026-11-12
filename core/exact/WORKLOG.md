# letters-exact worklog

Newest entry last. Each entry says what changed and why.

## 2026-11-11 round one: the measured recipe in rust

Ported the fidelity prototype's recipe (RECIPE.md round 4) to this crate:
pdf interpreter, font rebuilding, grid fonts, one positioned frame per line,
the word bridge, calibration, verification. Through Word every glyph of the
13-document corpus lands within 0.125 pt of the source (README, "Measured
results"). The output is exact but hard to edit: one frame per line, one run
per character.

## 2026-11-12 locked layout pinned byte for byte

Added `OutputMode` (easy to edit, locked layout) and `tests/locked.rs`, which
hashes every corpus docx written in locked layout mode without word and
compares it with `tests/corpus/locked-hashes.json`, recorded from the round
one code before any hybrid change. Why: round one's output is the maximum
fidelity fallback and must not drift while the hybrid mode is built on top.
`tests/golden.rs` and `tests/word.rs` now ask for locked layout explicitly.

## 2026-11-12 probe: how word lays out the lines of real paragraphs

Added `tests/hybrid_probe.rs`: 6 documents of 13 three-line paragraphs with
random line heights, spacing and indents, stacked in one frame and in body
flow. Measured (word 16.0.20527): indents become pixels at 5/12 px per twip
rounded half down, left and first-line indent added in twips first (234 of
234 line starts); between paragraphs word keeps the larger of space after
and space before, not the sum; line tops add up in twips and follow the
frame rule, except exactly on a pixel edge, where word goes either way; body
flow put many more lines a pixel off than stacked frames. These rules are in
`src/hybrid/rules.rs` and decided the anchoring (stacked frames, gaps as
space before only).

## 2026-11-12 hybrid mode: paragraphs promoted with proof

Added `src/hybrid`: paragraph detection on round one's planned lines,
candidates at three levels (natural, every glyph, pinned), proof exports that
accept a paragraph only when every glyph is within 0.125 pt, splitting at
failing lines, stacking of accepted paragraphs in shared frames with a final
checking export, demotion when the document mean would pass 0.05 pt, a
decision cache per document, and a prediction-only path without word that
says so. Why: the product goal is a docx a person can edit (type a line into
a paragraph and have it re-wrap) that still matches the pdf.

## 2026-11-12 list items and the column edge in paragraph detection

The first syllabus run merged two numbered list items into one "justified"
paragraph. Detection now treats a line starting with a list marker as a new
paragraph, takes the column's right edge from the widest line starting near
the same left edge (not only the paragraph's own lines), and ignores the gap
after a marker when deciding whether a line was stretched.

## 2026-11-12 vertical placement that survives pixel edges

Most paragraphs that failed their proof were off by exactly one pixel in y
on a later line (worst glyph 0.134 pt = one 0.12 pt pixel plus the lattice
floor). The line height and top search now counts a line top on a pixel edge
as either neighbouring pixel and minimises the worst case first, and tries
line heights 3 twips either side of the measured pitch.

## 2026-11-12 font completion for new text

Added `src/hybrid/completion.rs` and `FontPlan::complete_faces`: in easy to
edit mode every face gets the latin-1, latin extended-a, punctuation and
currency chars it lacks, accented letters composed from the face's own base
letter, the rest borrowed from a bundled stand-in fitted to the face, with a
measured similarity and a plain warning when it is weak; the space takes the
pdf's own width. Why: typed text must not fall back to another font.
`letters-fonts` has no completion api yet, so a small `GlyphDonor` trait
stands in for it. Adds the `unicode-normalization` dependency (MIT or
Apache-2.0) for the decompositions.

## 2026-11-12 the donor is chosen by measured similarity

The first offline run gave serif faces a sans donor (the pdf's serif flag
was off) and reported 0% for faces with no letters to compare. The donor is
now the bundled stand-in whose glyphs overlap the face's best; with no
letters in common the similarity is reported as not measured, not as 0.

## 2026-11-12 edit tests through word automation

`word-export.ps1` can now make one edit to the open document in memory
before exporting it (insert text after a found phrase, delete a phrase, or
insert and undo); nothing is saved. `WordBridge::export_edited` sends the
edits base64-encoded. `src/hybrid/edits.rs` plans four edits per document
(a sentence typed into the middle of a promoted paragraph, a sentence
deleted, text typed at the end, one char typed and undone) and checks that
only the edited paragraph re-wraps, that what is below it in its frame moves
by its line pitch (to the pixel word rounds line tops to), that nothing else
moves and that typed text is drawn in the embedded fonts. Why: the product
goal is editing without disturbing the rest of the page, so it is measured.

## 2026-11-12 baseline tables for paragraph line heights

The probe `the_lines_after_the_first_of_every_line_height` showed that the
closed baseline rule (recipe 6.6) only holds for the 16 line heights the
recipe measured: for heights 200 to 330 twips it put 57 of 393 lines a
pixel off, frame tops included, which is what failed most paragraphs by
0.134 pt. With each height's measured 60-phase table every first and second
line and 390 of 393 third lines landed on the predicted pixel. The hybrid
mode now measures the tables of every line height its paragraphs may get in
one word session before the proof rounds (`calibration::measure_baselines`)
and caches them with the other word facts.

## 2026-11-12 hybrid module split for reading

`src/hybrid/mod.rs` had grown past 800 lines. The proof rounds moved to
`ladder.rs`, writing and the final arrangement to `settle.rs`, the line
summaries and the editability numbers to `summary.rs`, the vertical search
to `vertical.rs`. Unused helpers were removed.

## 2026-11-12 the hybrid pass moved out of convert/mod.rs

`convert/mod.rs` held the pipeline and the hybrid wiring; the wiring (what
the hybrid builder gets, and writing page bodies into round one's package)
is now `convert/hybrid_pass.rs`, so the pipeline file reads as round one's
recipe plus one call. `error.rs` got its opening comment.

## 2026-11-12 promotions must be as close as round one

The second generation of the syllabus (word's export of the converted docx,
converted again) drifted to 0.181 pt from the original pdf although each
generation passed: natural-level promotions were accepted at up to 0.124 pt
where round one's lines sat at about 0.03, and each generation added its own
error. A candidate is now accepted only when its glyphs are on average at
most 0.012 pt (a tenth of a pixel) further from the source than round one's
positioned lines put the same glyphs, measured in word's export of round
one's docx. On a source that word printed itself every glyph is on the
lattice, so a promotion has to reproduce it exactly.

## 2026-11-12 libreoffice as a second target, measured

Added `tests/libreoffice.rs`: the corpus docx files (easy to edit and locked
layout) exported by a headless libreoffice with a private profile and
measured with the same verifier, so the distance to a libreoffice-faithful
recipe is known before one is attempted.

## 2026-11-12 first full corpus run, and two fixes from it

The first full run (13 documents, edit tests, second generation) passed on
positions everywhere and every edit test passed, but left paragraphs
positioned for two reasons. Ligatures written as private use chars are
never paired by the verifier, in round one's export either, and counted as
missing glyphs: a glyph round one's export also loses is no longer held
against a candidate. Paragraphs of the Word-made documents failed by one
pixel in y on some lines: their pitch follows a pixel pattern no single
exact line height reproduces. Such paragraphs are now cut, before the
proofs, into pieces that one placement prints within 0.08 pt
(`vertical::vertical_pieces`); the pieces stack in one frame.

## 2026-11-12 convert/mod.rs split along its steps

`convert/mod.rs` was still 773 lines. Measuring the model facts and the
verifying export moved to `convert/word_steps.rs`, the plain sentences about
fonts to `convert/notes.rs`, and placing rotated lines next to its measuring
exports in `convert/rotated_pass.rs`. No behaviour changed: the locked
layout hashes and the golden plans still match.

## 2026-11-12 measured results of the easy to edit mode in the README

Second full corpus run with all rules above: all 13 classes pass on
positions, every edit test passed where one could be planned, and the second
generation reproduces the first exactly on 12 of 13 documents (the slides'
rotated line moves). LibreOffice measured far off (stacked frames are not
stacked there). README now has the design, the tables, the OOXML emitted
and the limits.

## 2026-11-12 fitted document font: port started

The coordinator reported the fitted-font experiment
(`tools/fidelity/fitted-font`, read only): one composite glyph per word
piece with exact component offsets, chosen by GSUB rules, puts every glyph
within 0.0066 pt in Word's export. Porting it as the text path of
positioned lines, in `src/fitted`, with the known flaw fixed first (a
run's first piece now needs the next piece as lookahead), keeping the
current recipe without Word and for fsType 2 and 4 fonts.

## 2026-11-12 composite glyphs and a gsub writer

`fonts/truetype.rs` can now write composite glyphs after the simple ones
(exact 16-bit offsets, no ROUND_XY_TO_GRID, no USE_MY_METRICS, maxp counting
them) and extra tables, through `write_truetype_with`; `write_truetype` is
unchanged, so the locked layout's bytes are too. `fonts/gsub.rs` writes the
fitted font's gsub: per feature one chaining lookup (format 3 rules, most
specific first) calling ligature or single substitutions, all behind
extension lookups so the table can pass 64 kB. Shaping tests run it through
rustybuzz (a dev dependency, MIT). The known flaw is fixed and tested: a
run's first piece asks for the first char of the piece after it, so after
an edit a later copy of the same word no longer takes the first piece's
glyph; a whole next piece as lookahead made the word before an edit lose its
glyph too, one char does not.

## 2026-11-12 the fitted fonts mode

`src/fitted` ports the experiment: round one's lines are cut into runs
(cut around chars outside the basic plane) and word pieces (a word and its
trailing spaces, cut at 31.5 em), every piece becomes a composite of the
source glyphs at their exact offsets with a whole-pixel advance to the next
piece, runs get slots (liga, ss01 to ss20, then a font copy) so no two runs
of a slot start with the same piece, each piece is its own run with
alternating east asian language and no character spacing. One measuring
export finds every piece's glyph by its outline, the residual is baked into
the outlines, the second export is the result; `verify` reads each piece's
glyph as its components. New mode `OutputMode::FittedLayout`; without Word,
and for lines in fonts with fsType 2 or 4, round one's lines are kept. First
document (justified TJ kerning): max 0.0050 pt, mean 0.0025 pt, 240 of 240
pieces found, and a one-word edit moved no glyph on any other line.

## 2026-11-12 fitted fonts measured on the corpus, and made the default

Through Word on all 13 documents: max 0.0050 to 0.0066 pt (the slides 0.102,
their rotated line still on round one's boxes), every glyph paired, two
exports each, 17 to 51 s, and a one-word edit moved no glyph on any other
line in 13 of 13. `OutputMode::FittedLayout` is now the default, following
the coordinator's direction; without Word it writes round one's lines and
warns. The composite writing moved to `fonts/composite.rs` and the raster
overlap measure of font completion to `hybrid/overlap.rs` to keep files
readable. README has the design, the table and the emitted constructs.
