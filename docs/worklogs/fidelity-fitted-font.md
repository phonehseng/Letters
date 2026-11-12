# worklog: fitted document font

## 2026-11-12 setup

- New folder, owned by this experiment only. The round 4 code is imported, never edited:
  `fidelity_path.py` puts `tools/fidelity` on `sys.path` and points its font folder at the
  fidelity worktree (`C:\Letters-fidelity\tools\fidelity\fonts`, read only), so nothing is
  downloaded into a folder this work does not own. The calibration cache and the 13 corpus
  PDFs were copied into `out\` for the same reason.
- PyMuPDF was suggested for measuring, but it is AGPL, which this project never adds. The
  measuring uses pikepdf (the prototype's interpreter and `word_pdf_raw`) and pdfium.
- `word_session.py` gives each Word a unique caption and reads the process id from its
  window, so the log names exactly our processes even with other people's Word running.

## 2026-11-12 c1-c4: what Word's shaper does with an embedded font

- Ligatures apply only with `w14:ligatures` (standard = liga, contextual = clig,
  historical = hlig, discretional = dlig); calt needs `w14:cntxtAlts`; ss01..ss20 each need
  their own `w14:stylisticSets` entry. rlig fires whenever any of these is set, never
  without. So one font copy gives 21 independent slots (liga plus 20 stylistic sets).
- A ligature may span spaces, two runs of identical formatting, and runs that differ only
  in `w:lang w:eastAsia`. 300 characters still became one glyph. `w:spacing` on the run
  stops ligatures. Chaining context works, also across a space.
- Export failed on a natural composite with several space components: spaces are now left
  out of composites (they carry no ink). Export also failed on any glyph whose ink reaches
  32.7 em from its origin, at 500 and 1000 units per em; 32 em worked. Pieces are cut at
  31.5 em.
- A rule containing a supplementary-plane character (the prototype's U+F0000 ligature
  stand-ins) never applied, though a rule for the character alone did. Runs are now cut
  before and after such characters.

## 2026-11-12 b0: one line

- Baking works: one 11 pt line went from max 0.053 pt (origin on Word's lattice) to max
  0.0050 pt, with the origin unchanged between the two exports. The 0.005 is half a font
  unit at 1100 units per em. The raster check agreed (max blob shift 0.009 pt) once the
  source used whole-number widths: pdfium misplaces glyphs of fractional-width sources,
  the pitfall the fidelity README already records.
- The first raster check (whole-line phase correlation) reported 0.17 pt where glyph edges
  agreed within a 1200 dpi pixel; it was replaced by per-blob centroids.

## 2026-11-12 corpus, first runs

- The form and the journal failed to export: one glyph reached 45.7 em (the em limit
  above). Fixed by the 31.5 em cut.
- Piece advances used the next run of the same font instead of the next run of the line;
  lines mixing fonts were laid out up to 236 pt off. Baking hid it in the error numbers
  but not in the layout. Fixed with `Run.next_in_line`.
- Editing one line moved a glyph on another line by 1.0 pt: Word placed a later piece of a
  multi-glyph text object with a Tc it had carried over from an earlier object. Every piece
  is now its own run with alternating east Asian language, so its own Tm places it.
- The edit check first found its three-word context on another line; the context must now
  be unique in the document.

## 2026-11-12 results

- Corpus, both modes: 22,906 of 22,906 visible glyphs within 0.0066 pt (max), mean 0.0020
  (line mode) and 0.0024 (word mode). Round 4: max 0.063-0.102. The rest is half a font
  unit (0.005 pt per axis at 100 units per stored point).
- No piece's printed origin moved between the measuring and the baked export (787 pieces
  in line mode, 4,406 in word mode). The round 4 model predicted 740 of 776 line-mode
  origins exactly; the rest were whole pixels off, so one export stays necessary.
- Editing one word through Word left every glyph of every other line unchanged, in all 13
  documents and both modes. Word mode keeps the words before the edit exact; line mode
  does not. In word mode a later word of the edited line can take another occurrence's
  glyph, because a run's first piece has no context; giving it a lookahead is the fix.
- Word substitutes Cambria for an embedded fsType 2 font, and blocks on an invisible
  prompt for fsType 4; PrintOut to "Microsoft Print to PDF" also blocked (our Word was
  ended by the watchdog). The other export paths gave identical output.
- DrawingML shapes and pictures are placed on twips (0.05 pt), text in text boxes on the
  1/600 in grid.
- The report file itself could not be written by this session's tools; its content went to
  the hand-back message instead.
