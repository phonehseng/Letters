//! the hand-written part of ORACLE_REPORT.md: the rules fitted on the
//! measurements and the ranked list of what still disagrees. kept here so
//! the report is always written whole by `score_corpus`.

pub const ANALYSIS: &str = r#"## Rules fitted on the measurements

Each rule was changed only when a simple, explainable rule matched what the oracle did;
the code comment next to each cites the measurement. No rule is a per-document fudge.
Rules 1-16 are from round one (30 documents); 17-25 from round two, fitted on the fit set
and probes only.

| # | rule | Word | LibreOffice | evidence |
|---|---|---|---|---|
| 1 | line height of a font | os/2 typo if USE_TYPO_METRICS, else win ascent + descent + GDI external leading above the ascent | hhea ascent + line gap above, hhea descent, each in whole twips | font probes, 7 families x 4 sizes |
| 2 | first baseline | top + ascent (Word's 1/600 in grid not copied for drawing) | twips | font probes |
| 3 | multiple line spacing | (factor - 1) x the text's height below the text; pictures not multiplied | same | spacing_modes, images_inline |
| 4 | space between paragraphs | the larger of after and the next before | same | spacing_rules |
| 5 | space before at a page top | kept on the first page, dropped elsewhere | kept after a hard break | headings, spacing_rules |
| 6 | the foot of a page | a multiple's extra may hang into the margin, not into notes | same | headings, footnotes |
| 7 | paragraph mark size | does not grow a line with text | - | font probes |
| 8 | justified text | spaces squeezed by up to 25 % of their width | **the same (round two)** | justify probes: both applications took in the same words |
| 9 | tables (round one) | no table style: zero cell margins; a row reserves its top border | 5.4 pt margins | tables_fixed, tables_auto |
| 10 | footnotes | number added, widow control on notes, shared area in columns | - | footnotes |
| 11 | continuous sections | new region below max(after, before), level columns | - | two_columns |
| 12 | tab leaders | drawn as characters | same | indents_tabs |
| 13 | fonts | embedded first, then Office cloud fonts, then system | no cloud fonts | Word-made documents, font_aptos |
| 14 | kerning and ligatures | off unless asked | same for docx | corpus |
| 15 | glyph advances | fractional | fractional | corpus |
| 16 | pictures alone in a line | no descent | - | real_lab_report |
| 17 | autofit share | narrowest + share of (natural - narrowest), natural widths capped at 1584 pt (22 in) | **keeps the file's grid** | table probes: 19 of 19 over-wide probes within 0.1 pt in Word; all 23 probes on the grid in LibreOffice |
| 18 | column side space | larger of cell margin and border per side | same | probes: +1.0 pt with no margins and half-point borders, +10.8 pt with 5.4 pt margins |
| 19 | autofit room | between the outer borders' centres (468 - 0.5) | - | probes: 467.6 pt |
| 20 | table position and insets | left rule centre half its width right of the margin; text at the larger of margin and border right of the rule | same | probes: 72.26 / 72.74 pt (Word), 72.3 / 77.75 pt (LibreOffice) |
| 21 | table style borders | read from the style chain when the table has none | same | w_tables_aptos drew no rule Word drew |
| 22 | split rows | widow and orphan control in each cell | none | w_tables_aptos; LibreOffice tables 58 % -> 32 % page breaks with it |
| 23 | square wrap | text on both sides of a float inside the column, a piece only if its first word fits | (same assumed) | w_floating_aptos; images 54.5 % -> 99.3 % lines |
| 24 | fields | page and page count in body text live (two passes); DATE shows today in its picture | same | ECMA-376 17.16; no corpus document yet |
| 25 | missing fonts | metric twin (Carlito, Arimo, Tinos, Cousine), else look-alike (Caladea, Inter), else open fallbacks; variable fonts at the asked weight | same | font library measurements (core/fonts README) |

## What still disagrees, ranked by measured impact

1. **ODT files LibreOffice made: columns and page borders.** The office crate's ODT reader
   drops `text:section` (where LibreOffice keeps docx columns) and does not add a page
   border's padding and width to the margins, so we lay those pages out in one column and
   too wide (columns_two_whole, page_borders_box: 0 % of lines). Both are reader gaps in
   core/office, not layout rules; this crate cannot see the information.
2. **Word's page flow on long Word-made tables** (w_tables_mla, w_tables_calibri): lines
   match (93-97 %) but rows land on other pages; next suspects are Word's row height
   rounding and "at least" heights from the Normal style's spacing inside cells.
3. **Justified squeeze.** Both applications refuse some lines needing only 12-20 % of the
   spaces while taking others needing 25 %; the share alone is not the rule.
4. **Two- and three-column pages with footnotes**: Word re-fits earlier columns when a later
   column's notes grow the shared area (real_research_paper, columns_*_notes); not done.
5. **Floats**: wrap distances are Word's defaults (the file's distL/distR are not read),
   tight and through wrap use the box, objects that push to the next page and text boxes
   that grow with their text are approximate (floats_text_boxes, w_floating_*).
6. **LibreOffice flavor**: header room, widow decisions and its table row splitting are
   fitted only coarsely (headers_* on ODT, lo_tables).
7. **Fonts without Microsoft's**: Cambria, Georgia, Aptos, Segoe UI and Verdana have no metric
   twin among the open fonts, so documents in them break lines differently (see the open
   fonts rows).
8. **Word's 1/600 inch grid**: drawing stays exact by design (0.02-0.12 pt differences);
   `LayoutOptions::word_grid_drawing` snaps for side-by-side checks only.
9. **Hyphenation** stays off: no pattern set with a clearly permissive licence for every
   language was found, and both applications leave automatic hyphenation off by default.
10. **Right-to-left, East Asian and complex scripts** are detected and reported, laid out left
    to right; shapes and groups are reported and skipped.
"#;
