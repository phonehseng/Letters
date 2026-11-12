//! grid fonts: one per face and pdf size (recipe 4).
//!
//! for each (face, pdf size S):
//! 1. the stored size is `H = round(2S) / 2`, a docx stores half points
//! 2. units per em `U = 50 * 2H`, so one 1/600 inch pixel at H is 12 units
//! 3. word draws glyphs at `D = round(H / 0.12) * 0.12`
//! 4. outlines and metrics are scaled to `round(U * S / D)` units and the
//!    em relabelled as `U`: glyphs then draw at size S while layout runs at H
//! 5. every advance is snapped to a whole pixel, `round(advance / 12) * 12`
//!
//! the layout advance of a glyph in pixels is then `advance / 12`.

use std::collections::HashMap;

use super::outline::ContourPoint;
use super::rebuild::RebuiltFace;
use super::truetype::{write_truetype, FontGlyph, FontSpec};
use crate::units::round_half_up;
use crate::word::sizes::{
    grid_units_per_em, nearest_half_point, outline_units_per_em, snap_advance, GRID_UNITS_PER_PIXEL,
};

/// one grid font, ready to embed.
#[derive(Clone, Debug)]
pub struct GridFont {
    pub family: String,
    pub stored_size: f64,
    pub pdf_size: f64,
    pub units_per_em: i64,
    /// char to its layout advance in pixels
    pub advance_pixels: HashMap<char, i64>,
    pub bytes: Vec<u8>,
}

impl GridFont {
    /// a char's advance in whole pixels; a char the font lacks draws `.notdef`.
    pub fn pixels(&self, char: char) -> i64 {
        self.advance_pixels
            .get(&char)
            .copied()
            .unwrap_or_else(|| self.advance_pixels.get(&'\u{0}').copied().unwrap_or(0))
    }
}

/// the size tag in a grid family name: 10.5 becomes "10h5", 12 stays "12".
pub fn size_tag(size: f64) -> String {
    let text = format!("{size:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.replace('.', "h")
}

/// scales one value the way fonttools' `scale_upem` does: round half up.
fn scaled(value: i64, factor: f64) -> i64 {
    round_half_up(value as f64 * factor) as i64
}

/// builds the grid font of `face` for one pdf size (rounded to 0.01 pt).
pub fn make_grid_font(face: &RebuiltFace, pdf_size: f64) -> Result<GridFont, String> {
    let stored = nearest_half_point(pdf_size);
    let units_per_em = grid_units_per_em(stored)
        .ok_or_else(|| format!("{pdf_size} pt is too large for a docx font size"))?;
    let outline_em = outline_units_per_em(units_per_em, pdf_size, stored);
    let factor = outline_em as f64 / face.units_per_em as f64;
    let glyphs: Vec<FontGlyph> = face
        .glyphs
        .iter()
        .map(|glyph| FontGlyph {
            contours: glyph
                .contours
                .iter()
                .map(|contour| {
                    contour
                        .iter()
                        .map(|p| ContourPoint {
                            x: scaled(i64::from(p.x), factor) as i32,
                            y: scaled(i64::from(p.y), factor) as i32,
                            on_curve: p.on_curve,
                        })
                        .collect()
                })
                .collect(),
            advance: snap_advance(scaled(i64::from(glyph.advance), factor)).clamp(0, 65535) as u16,
        })
        .collect();
    let family = format!("{} {}", face.family, size_tag(pdf_size));
    let mut advance_pixels: HashMap<char, i64> = face
        .cmap
        .iter()
        .filter_map(|(&code, &glyph)| {
            let char = char::from_u32(code)?;
            Some((
                char,
                i64::from(glyphs[glyph as usize].advance) / GRID_UNITS_PER_PIXEL,
            ))
        })
        .collect();
    advance_pixels.insert('\u{0}', i64::from(glyphs[0].advance) / GRID_UNITS_PER_PIXEL);
    let bytes = write_truetype(&FontSpec {
        family: &family,
        units_per_em: units_per_em as u16,
        glyphs: &glyphs,
        cmap: &face.cmap,
        ascent: scaled(face.ascent, factor).clamp(-32768, 32767) as i16,
        descent: scaled(face.descent, factor).clamp(-32768, 32767) as i16,
        fs_type: face.fs_type.unwrap_or(0),
    });
    Ok(GridFont {
        family,
        stored_size: stored,
        pdf_size,
        units_per_em,
        advance_pixels,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn face() -> RebuiltFace {
        RebuiltFace {
            face_key: "Test|truetype".into(),
            family: "Ltr 00000001".into(),
            pdf_font: "Test".into(),
            units_per_em: 1000,
            glyphs: vec![
                FontGlyph {
                    contours: Vec::new(),
                    advance: 500,
                },
                FontGlyph {
                    contours: Vec::new(),
                    advance: 250,
                },
                FontGlyph {
                    contours: Vec::new(),
                    advance: 556,
                },
            ],
            cmap: BTreeMap::from([(0x20, 1), (0x41, 2)]),
            char_for: HashMap::new(),
            ascent: 800,
            descent: -200,
            fs_type: None,
            permission: "",
            cubic_source: false,
            outline_error_em: 0.0,
            private_use: 0,
            substitute_for: None,
            problems: Vec::new(),
        }
    }

    #[test]
    fn size_tags() {
        assert_eq!(size_tag(10.5), "10h5");
        assert_eq!(size_tag(12.0), "12");
        assert_eq!(size_tag(9.96), "9h96");
    }

    #[test]
    fn advances_are_whole_pixels_at_the_stored_size() {
        // 12 pt: U = 1200 = D, so 556/1000 em is 667.2 units, 55.6 px -> 56 px
        let grid = make_grid_font(&face(), 12.0).unwrap();
        assert_eq!(grid.units_per_em, 1200);
        assert_eq!(grid.pixels('A'), 56);
        assert_eq!(grid.pixels(' '), 25); // 300 units
        let parsed = ttf_parser::Face::parse(&grid.bytes, 0).unwrap();
        assert_eq!(parsed.units_per_em(), 1200);
        assert_eq!(
            parsed.glyph_hor_advance(parsed.glyph_index('A').unwrap()),
            Some(672)
        );
    }

    #[test]
    fn odd_sizes_scale_against_the_drawn_size() {
        // 10.3 pt: stored 10.5 (U 1050), drawn 10.56, outlines at 1024 units
        let grid = make_grid_font(&face(), 10.3).unwrap();
        assert_eq!(grid.stored_size, 10.5);
        assert_eq!(grid.units_per_em, 1050);
        // 556 * 1.024 = 569.3 -> 569 units -> 47.4 px -> 47 px
        assert_eq!(grid.pixels('A'), 47);
        assert!(grid.family.ends_with("10h3"));
    }
}
