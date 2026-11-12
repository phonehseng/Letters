//! composite glyphs for the fitted fonts: a glyph made of other glyphs of
//! the font at exact offsets, written after the simple glyphs.

use super::outline::{ContourPoint, Contours};
use super::truetype::{
    bounds_of, clamp16, glyph_bytes, maxp_table, write_tables, Bounds, FontGlyph, FontSpec, Writer,
};

/// one component of a composite glyph: a glyph of the font at an exact offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Component {
    pub glyph: u16,
    pub dx: i32,
    pub dy: i32,
}

/// a composite glyph (the fitted font's pieces): components and its own advance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompositeGlyph {
    pub components: Vec<Component>,
    pub advance: u16,
}

/// writes a font with composite glyphs after the simple ones (glyph ids
/// `spec.glyphs.len()` and up) and extra tables such as gsub.
pub fn write_truetype_with(
    spec: &FontSpec,
    composites: &[CompositeGlyph],
    extra_tables: Vec<([u8; 4], Vec<u8>)>,
) -> Vec<u8> {
    let mut glyphs: Vec<FontGlyph> = spec.glyphs.to_vec();
    let mut glyph_data: Vec<Vec<u8>> = spec
        .glyphs
        .iter()
        .map(|g| glyph_bytes(&g.contours))
        .collect();
    for composite in composites {
        // the decomposed outline gives the metrics tables their bounds
        let decomposed = decompose(spec.glyphs, composite);
        glyph_data.push(composite_bytes(composite, bounds_of(&decomposed)));
        glyphs.push(FontGlyph {
            contours: decomposed,
            advance: composite.advance,
        });
    }
    let extended = FontSpec {
        glyphs: &glyphs,
        ..*spec
    };
    let mut tables = extra_tables;
    let simple_maxp = maxp_table(spec);
    tables.push((
        *b"maxp",
        with_composites(simple_maxp, composites, &glyphs[spec.glyphs.len()..]),
    ));
    write_tables(&extended, glyph_data, tables)
}

/// a composite's components' contours moved to their offsets.
fn decompose(glyphs: &[FontGlyph], composite: &CompositeGlyph) -> Contours {
    let mut contours = Vec::new();
    for component in &composite.components {
        for contour in &glyphs[usize::from(component.glyph)].contours {
            contours.push(
                contour
                    .iter()
                    .map(|p| ContourPoint {
                        x: p.x + component.dx,
                        y: p.y + component.dy,
                        on_curve: p.on_curve,
                    })
                    .collect(),
            );
        }
    }
    contours
}

/// glyf composite flags (opentype glyf): 16-bit offsets, offsets are x and
/// y values, more components follow. no ROUND_XY_TO_GRID and no
/// USE_MY_METRICS: offsets stay exact and the composite keeps its advance.
const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const MORE_COMPONENTS: u16 = 0x0020;

fn composite_bytes(composite: &CompositeGlyph, bounds: Option<Bounds>) -> Vec<u8> {
    if composite.components.is_empty() {
        return Vec::new();
    }
    let bounds = bounds.unwrap_or_default();
    let mut w = Writer(Vec::new());
    w.i16(-1);
    for v in [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max] {
        w.i16(clamp16(v));
    }
    let last = composite.components.len() - 1;
    for (index, component) in composite.components.iter().enumerate() {
        let more = if index < last { MORE_COMPONENTS } else { 0 };
        w.u16(ARG_1_AND_2_ARE_WORDS | ARGS_ARE_XY_VALUES | more);
        w.u16(component.glyph);
        w.i16(clamp16(component.dx));
        w.i16(clamp16(component.dy));
    }
    w.0
}

/// maxp with the composites counted (opentype maxp version 1.0: glyph
/// count at byte 4, composite points at 10, contours at 12, component
/// elements at 28, depth at 30).
fn with_composites(
    mut maxp: Vec<u8>,
    composites: &[CompositeGlyph],
    decomposed: &[FontGlyph],
) -> Vec<u8> {
    if composites.is_empty() {
        return maxp;
    }
    let points = decomposed
        .iter()
        .map(|g| g.contours.iter().map(Vec::len).sum::<usize>())
        .max()
        .unwrap_or(0);
    let contours = decomposed
        .iter()
        .map(|g| g.contours.len())
        .max()
        .unwrap_or(0);
    let elements = composites
        .iter()
        .map(|c| c.components.len())
        .max()
        .unwrap_or(0);
    let glyph_count = u16::from_be_bytes([maxp[4], maxp[5]]) as usize + composites.len();
    for (at, value) in [
        (4, glyph_count),
        (10, points),
        (12, contours),
        (28, elements),
        (30, 1),
    ] {
        let value = value.min(usize::from(u16::MAX)) as u16;
        maxp[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    maxp
}
