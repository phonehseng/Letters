# complete

Giving a document's subset font the letters new text needs. The contract
and limits are in the crate's README ("Completing a document's font").

| File | What it is for |
|---|---|
| `mod.rs` | the public calls (`identify`, `complete_font`, `font_for_new_text`), options and `CompletedFont` |
| `subset_font.rs` | the document's font: glyphs, glyph numbers, pdf widths, name, outline hash |
| `pipeline/` | trying the strategies in order: lenders, per-glyph strategies, generation, report sentences |
| `synthesis.rs` | accented letters, dotless i and j, ligatures, superscripts from the font's own glyphs |
| `marks.rs` | which accented letters are a base plus a mark (latin-1 and latin extended-a) |
| `generated/` | the optional generated glyphs: request and disclosure, strict reply checks, the fake generator |
| `truetype/` | writing the completed font: glyphs, cmap, kern and font-wide tables |
| `report.rs` | strategies, provenance, the match report and its words |
| `cache.rs` | remembering fits and completed fonts by a hash of the subset's outlines |
