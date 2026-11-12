# matching

Comparing and fitting glyph outlines, for completing a document's font.

| File | What it is for |
|---|---|
| `shape/` | outlines in em units (TrueType-style quadratic contours), geometry, building from drawing commands |
| `distance.rs` | resampling outlines and their chamfer (mean) and hausdorff (max) distance |
| `glyphs.rs` | reading characters' glyphs, metrics and kerning from a font, variable fonts at chosen axes |
| `style.rs` | `StyleFit`: stem, heights, slant, contrast, hairline, serifs, spacing |
| `offset.rs` | making strokes heavier or lighter, separately across and up |
| `donors.rs` | the fonts that may lend glyphs, their licences, and the quick shortlist |
| `identify.rs` | glyph-by-glyph comparison, same-design test, variable axis search, percent alike |
| `fit/` | the fitted transform, its measurements and searches, the spacing model |
| `optimise.rs` | least squares lines and pairs, golden-section search |
