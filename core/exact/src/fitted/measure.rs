//! where word drew each piece, found by the glyph's outline, and every
//! source glyph's ink position from it.
//!
//! word subsets the fonts it writes into its pdf and renumbers every glyph,
//! so a piece's glyph is found by its decomposed outline, on its page and
//! nearest where it was expected. its ink is exact by construction:
//! component k is drawn at the printed origin plus the offset written,
//! scaled by the size word draws at (fitted-font report, "fitted_measure").

use std::collections::HashMap;

use crate::fonts::outline::Recorder;
use crate::fonts::source::GlyphSource;
use crate::pdf::PageContent;
use crate::verify::PlacedGlyph;

/// a piece is matched to a drawn glyph only this close to where it was
/// expected: word may print a later piece up to 1.7 pt from its planned pen
/// (fitted-font report, the syllabus), and other lines are 8 pt away or more.
pub const MATCH_RADIUS_PT: f64 = 4.0;

/// an outline, comparable across fonts that renumber glyphs.
pub type Signature = Vec<i64>;

/// the decomposed outline of a glyph of a font file.
pub fn font_signature(font: &[u8], glyph: u16) -> Option<Signature> {
    let face = ttf_parser::Face::parse(font, 0).ok()?;
    let mut recorder = Recorder::default();
    face.outline_glyph(ttf_parser::GlyphId(glyph), &mut recorder)?;
    Some(recorder.outline.signature())
}

/// one glyph of word's pdf with its outline.
pub struct DrawnGlyph {
    pub index: usize,
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub signature: Option<Signature>,
}

/// every glyph word drew, with the outline its font program gives it.
pub fn drawn_glyphs(pages: &[PageContent]) -> Vec<DrawnGlyph> {
    let mut sources: HashMap<String, Option<GlyphSource>> = HashMap::new();
    let mut drawn = Vec::new();
    let mut problems = Vec::new();
    for (index, glyph) in pages.iter().flat_map(|p| p.glyphs.iter()).enumerate() {
        let source = sources
            .entry(glyph.font.key.clone())
            .or_insert_with(|| GlyphSource::new(&glyph.font).ok());
        let signature = source.as_ref().and_then(|s| {
            let found = s.glyph_for(glyph.code)?;
            Some(s.outline(&found, &mut problems).signature())
        });
        drawn.push(DrawnGlyph {
            index,
            page: glyph.page,
            x: glyph.x,
            y: glyph.y,
            size: glyph.size,
            signature,
        });
    }
    drawn
}

/// the drawn glyph with this outline on this page nearest `expected`.
pub fn locate<'a>(
    drawn: &'a [DrawnGlyph],
    by_signature: &HashMap<&Signature, Vec<usize>>,
    signature: &Signature,
    page: usize,
    expected: (f64, f64),
) -> Option<&'a DrawnGlyph> {
    by_signature
        .get(signature)?
        .iter()
        .map(|&i| &drawn[i])
        .filter(|g| g.page == page)
        .map(|g| ((g.x - expected.0).hypot(g.y - expected.1), g))
        .filter(|(distance, _)| *distance <= MATCH_RADIUS_PT)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, g)| g)
}

/// drawn glyphs indexed by outline.
pub fn index_by_signature(drawn: &[DrawnGlyph]) -> HashMap<&Signature, Vec<usize>> {
    let mut index: HashMap<&Signature, Vec<usize>> = HashMap::new();
    for (position, glyph) in drawn.iter().enumerate() {
        if let Some(signature) = &glyph.signature {
            index.entry(signature).or_default().push(position);
        }
    }
    index
}

/// a piece found in word's pdf: which drawn glyph, and its chars' ink positions.
pub struct FoundPiece {
    pub drawn_index: usize,
    /// (char, x, y) of each inked component
    pub components: Vec<(String, f64, f64)>,
}

/// the glyphs of word's pdf as the verifier reads them, with every found
/// piece's glyph replaced by its components at their ink positions, in the
/// same reading order.
pub fn expanded_glyphs(pages: &[PageContent], found: &[FoundPiece]) -> Vec<PlacedGlyph> {
    let by_drawn: HashMap<usize, &FoundPiece> = found.iter().map(|f| (f.drawn_index, f)).collect();
    let mut placed = Vec::new();
    for (index, glyph) in pages.iter().flat_map(|p| p.glyphs.iter()).enumerate() {
        if let Some(piece) = by_drawn.get(&index) {
            for (text, x, y) in &piece.components {
                placed.push(PlacedGlyph {
                    page: glyph.page,
                    text: text.clone(),
                    x: *x,
                    y: *y,
                    font: glyph.font.base_name.clone(),
                });
            }
            continue;
        }
        if glyph.text.is_empty() || glyph.text.chars().all(char::is_whitespace) {
            continue;
        }
        placed.push(PlacedGlyph {
            page: glyph.page,
            text: glyph.text.clone(),
            x: glyph.x,
            y: glyph.y,
            font: glyph.font.base_name.clone(),
        });
    }
    placed
}
