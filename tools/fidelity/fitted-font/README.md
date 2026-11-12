# fitted document font: an experiment

Can a converted DOCX reach 1/40 pt (0.025 pt) in Word if each document gets its own font,
whose glyphs are pre-composed so the ink lands where the source PDF puts it? Yes: on the
13-document corpus every glyph landed within 0.0066 pt (see `WORKLOG.md` for the results
and decisions; `report_tables.py` regenerates the corpus tables into `out	ables.md`).

The idea in one paragraph: every run of text in a line (or every word) becomes one composite
TrueType glyph whose components are the source's own glyphs, placed at their exact source
offsets in font units. A GSUB rule replaces the typed characters with that glyph, so the
DOCX keeps real, searchable text. Word snaps the glyph's origin to its grid; the converter
measures where the origin landed and moves the components by the leftover error. Outlines
never change layout, so the second export puts the ink exactly where the source has it.

Everything here is a measurement tool. Nothing in `core/` or `app/` depends on it.

## setup

It uses the fidelity venv one folder up, plus `uharfbuzz` (Apache-2.0) for the unit tests:

```powershell
C:\Letters\tools\fidelity\.venv\Scripts\python.exe -m pip install uharfbuzz
```

The round 4 modules (`tools/fidelity`, `tools/fidelity/prototype`) are imported through
`fidelity_path.py`. The open fonts and the 13-document corpus were built in the fidelity
worktree; they are read from `C:\Letters-fidelity\tools\fidelity\fonts` and copied into
`out\corpus` (see `WORKLOG.md`). PyMuPDF was not used: it is AGPL. pdfium (pypdfium2),
pikepdf and the prototype's own content-stream interpreter do all the reading.

Run everything from this folder with `..\.venv\Scripts\python.exe <script>`. Output goes
to `out\` (ignored by git). Word is always a private, invisible instance (`word_session.py`)
whose process id is logged in `out\word_pids.log`.

## modules

| file | what it is for |
|---|---|
| `fidelity_path.py` | puts the round 4 modules on `sys.path`; points their font and cache paths at data this folder may read |
| `word_session.py` | one private Word instance per batch: unique caption, process id read from its window, Quit in `finally` |
| `fitted_font.py` | composite glyphs with exact offsets, GSUB rules (one chaining lookup per feature), outline signatures |
| `fitted_docx.py` | run XML with the `w14` properties that switch features on; a plain probe package |
| `fitted_pieces.py` | a line's characters -> runs -> pieces (whole run, or each word), cut to Word's glyph-size limits |
| `fitted_plan.py` | slots (font copy and feature) per run, piece advances, component offsets, the fitted fonts |
| `fitted_package.py` | rewrites the prototype's DOCX: per-character runs become one run per piece; fonts are replaced |
| `fitted_convert.py` | the converter: prototype DOCX, then measure, bake and check exports |
| `pdf_glyphs.py` | every glyph in Word's PDF: printed origin, size, direction and its outline signature |
| `fitted_measure.py` | finds each piece's glyph in Word's PDF and computes every source glyph's ink error |
| `raster_check.py` | an independent check: ink blob centroids of both PDFs rendered at 1200 dpi, text only |
| `corpus_checks.py` | fonts drawn in Word's PDF, text in the DOCX and in Word's PDF |
| `edit_check.py` | replaces one word through Word and compares every glyph with the unedited export |
| `corpus_fitted.py` | runs converter and checks on the 13 corpus documents (`--mode line` or `word`) |
| `report_tables.py` | markdown tables (`out	ables.md`) from both modes and the round 4 report |
| `predict_check.py` | whether the round 4 model alone predicts each piece's printed origin (no Word) |
| `test_fitted_font.py` | unit tests (`-m unittest test_fitted_font -v`), shaping checked with harfbuzz |

## experiments

| script | question | time |
|---|---|---|
| `probe_ligatures.py` | c1: which features Word applies from which run settings; spaces, length, split runs | 30 s |
| `probe_glyph_limits.py [--em-width]` | c2: components, width and em size a glyph may have before export fails | 15 s each |
| `probe_space_ligatures.py` | c3: why a ligature across spaces failed (ink-less components) | 20 s |
| `probe_private_use.py` | c4: GSUB on the prototype's U+F0000 characters | 10 s |
| `make_one_line.py` | b0 sources: one line, a spaced line, three repeated lines (whole widths) | 1 s |
| `fitted_convert.py in.pdf out_dir [--mode line\|word]` | b0 and any pdf: convert, bake, measure | 10-90 s |
| `probe_embedding.py` | a1: embedded fitted font with fsType 0, 2, 4, 8 | 25 s |
| `probe_output_paths.py` | d1: export for print, screen, PDF/A, SaveAs2, usePrinterMetrics, PrintOut | 5 min |
| `probe_drawingml.py` | d2: are shapes, pictures and text boxes placed off the text grid? | 10 s |
| `corpus_fitted.py [--mode line\|word] [--only name ...]` | the corpus, with fonts, text and edit checks | about 12 min a mode |
