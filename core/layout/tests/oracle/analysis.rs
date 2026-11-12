//! the hand-written part of ORACLE_REPORT.md: the rules fitted on the
//! measurements and the ranked list of what still disagrees. kept here so
//! the report is always written whole by `score_corpus`.

pub const ANALYSIS: &str = r#"## Rules fitted on the measurements

Each rule below was changed only when a simple, explainable rule matched what the oracle
did on a corpus document; the code comment next to each cites the document. No rule is a
per-document fudge factor.

| # | rule | Word | LibreOffice | evidence |
|---|---|---|---|---|
| 1 | line height of a font | os/2 typo if USE_TYPO_METRICS, else win ascent + descent + GDI external leading (hhea line gap beyond the win metrics) above the ascent | hhea ascent + line gap above, hhea descent, each rounded to whole twips | font probes: 7 families x 4 sizes, mean pitch within 0.02 pt of the rule (Times 12: 13.80 = 2355/2048 x 12; Aptos 11: 13.44 = typo) |
| 2 | first baseline | top + ascent. Word's own baselines also land on its 1/600 in grid (ascent px = round(height px) - round(descent px), all 7 probe fonts), which we deliberately do not copy for drawing | twips | font probes |
| 3 | multiple line spacing | (factor - 1) x the text's height added below the text; inline pictures are not multiplied; superscripts use up that extra space before they grow the line | same assumed | spacing_modes (10 modes), images_inline (0.03 pt mean), footnotes |
| 4 | space between paragraphs | the larger of space after and the next space before (no sum) | same | spacing_rules: 12 + 12 leaves 12, 18 + 6 leaves 18 |
| 5 | space before at a page top | kept on the document's first page, dropped at every other page top (natural or hard break) | same | headings, spacing_rules |
| 6 | the foot of a page | the extra of a multiple may hang into the bottom margin, but not into a footnote area; keep-with-next counts the same way | same assumed | headings (1.08 line kept; 75 % -> 100 % page breaks), footnotes (double-spaced line moved) |
| 7 | paragraph mark size | does not grow a line that has text | - | font probes (9 pt paragraph, 11 pt mark) |
| 8 | justified text | spaces may be squeezed by up to 25 % of their width to keep the last word | no squeezing | justified: 48 % -> 92 % lines; sharp optimum at 0.22-0.25 |
| 9 | tables | no table style: zero cell margins; a row reserves its top border's width; columns refitted to content when any cell lacks a width (min + k (max - min)); table styles sit between document defaults and paragraph styles (ECMA-376 17.7.2) | 5.4 pt margins | tables_fixed 59 % -> 92 %, tables_auto 48 % -> 90 % |
| 10 | footnotes | the note's own number added (the model drops w:footnoteRef); widow and orphan control on notes; the whole line, spacing included, must fit above the note area; in columns notes sit under their column with one shared area height | - | footnotes: page breaks 14 % -> 100 %, mean baseline 0.11 pt |
| 11 | continuous sections | the new region starts after max(previous space after, first space before), level across columns; balancing respects widow and orphan control | - | two_columns, real_research_paper |
| 12 | tab leaders | drawn as characters (dots), like both oracles | same | indents_tabs 89 % -> 100 % |
| 13 | fonts | document-embedded fonts first (obfuscated .odttf), then Office's cloud fonts (Aptos), then system fonts | no Office cloud fonts: Aptos becomes Times New Roman | Word-made documents 56-60 % -> 91-100 %; font_aptos 0 % -> 100 % against LibreOffice |
| 14 | kerning and ligatures | off unless the run asks (w:kern, w14:ligatures) | the same for DOCX: both off breaks 79 % of lines like LibreOffice, both on 54 % | fitted over the corpus |
| 15 | glyph advances | fractional (recipe e3) | fractional: twip-rounded advances broke 75 % of lines like LibreOffice, exact 79 % | fitted over the corpus |
| 16 | pictures alone in a line | no descent below the picture | - | real_lab_report: figure paragraph 3.8 pt shorter |

## What still disagrees, ranked by impact

1. **Table column widths (autofit).** Word's autofit is approximated by the proportional
   min/max rule; tables_long shows columns off by up to 7 pt, which then breaks cell lines
   differently and moves rows across pages. LibreOffice's table widths and margins (text
   0.35 pt further in, its own autofit) are barely modelled.
2. **LibreOffice page flow.** Two columns, headers and some widow decisions differ in ways
   not yet fitted (see the per-document table): its column balancing and the room a header
   takes are the first suspects.
3. **Two-column footnotes in Word.** Word shortens a column already filled when a later
   column's notes enlarge the shared note area; this model does not go back
   (real_research_paper page breaks).
4. **Justified squeeze.** 25 % of the space width is the best single ratio, but Word declined
   a few lines that needed only 20 %: the real rule depends on something else (perhaps its
   pixel grid). 5-8 % of justified lines still break differently.
5. **Word's 1/600 inch grid.** Our baselines and x positions are exact; Word's sit on its
   pixel grid and its PDF writer's lattice (RECIPE.md), so 0.02-0.12 pt differences remain by
   design. Where the grid decides a break (a line exactly at the margin) we can still differ.
6. **Floating objects.** Square wrap uses the wider side only (Word flows text on both sides);
   tight and through wrap use the box; wrap distances are Word's defaults, not the file's.
7. **Table cell x offset.** Word places cell text 0.74 pt right of the column edge with
   half-point borders and its columns 0.1 pt narrower than the grid; not modelled.
8. **Line breaking classes.** UAX #14 with Word's slash rule; other differences (dashes,
   percent signs, East Asian rules) are untested.
9. **Fields in body text** keep their stored value (only headers and footers get live page
   numbers).
10. **Right-to-left, East Asian and complex scripts** are detected and reported, laid out left
    to right.
"#;
