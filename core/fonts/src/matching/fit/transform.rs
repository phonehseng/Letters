//! the fitted change of a donor's glyphs: vertical zones, width, stem and
//! bar weight, and lean, applied to outlines or to sampled points.

use super::*;

/// the vertical zones: where the donor's descender, x-height, capital
/// height and ascender go in the target, em.
///
/// capitals and lowercase get separate top zones: fonts that share
/// widths can still differ there (Arial's ascenders are as tall as its
/// capitals, those of its metric twin Arimo are taller), and one mapping
/// for both would stretch one or the other.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerticalZones {
    /// donor heights: descender depth, x-height, capital height, ascender
    pub donor: [f64; 4],
    /// where each lands in the target
    pub target: [f64; 4],
}

impl VerticalZones {
    pub fn identity(style: &StyleFit) -> VerticalZones {
        let heights = [
            style.descender_depth,
            style.x_height,
            style.cap_height,
            style.ascender_height,
        ];
        VerticalZones {
            donor: heights,
            target: heights,
        }
    }
}

/// lowercase letters reach up to the ascender; capitals, figures and
/// everything else to the capital height.
pub fn uses_ascender_zone(c: char) -> bool {
    c.is_lowercase()
}

/// a fitted change of a donor's glyphs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub x_scale: f64,
    /// how far each side of every vertical stroke moves, em (positive is
    /// bolder)
    pub weight: f64,
    /// the same for horizontal strokes (bars, hairlines, serifs), which
    /// change far less between weights than stems do
    #[serde(default)]
    pub weight_y: f64,
    /// added lean: x moves by shear × y
    pub shear: f64,
    pub zones: VerticalZones,
    /// variation axis values of a variable donor
    pub axes: Vec<(String, f32)>,
}

impl Transform {
    pub fn identity(style: &StyleFit) -> Transform {
        Transform {
            x_scale: 1.0,
            weight: 0.0,
            weight_y: 0.0,
            shear: 0.0,
            zones: VerticalZones::identity(style),
            axes: Vec::new(),
        }
    }

    /// the knots of the vertical mapping, before offsetting: chosen so
    /// that after horizontal edges move by `weight_y` the baseline stays
    /// at 0 and the tops land on the target heights, as real bold fonts do.
    pub(crate) fn knots(&self, c: char) -> Vec<(f64, f64)> {
        let w = self.weight_y;
        let [d_desc, d_x, d_cap, d_asc] = self.zones.donor;
        let [t_desc, t_x, t_cap, t_asc] = self.zones.target;
        let (d_top, t_top) = if uses_ascender_zone(c) {
            (d_asc, t_asc)
        } else {
            (d_cap, t_cap)
        };
        let mut knots = vec![(0.0, w)];
        if d_desc < -0.02 && t_desc < -0.02 {
            knots.insert(0, (d_desc, t_desc + w));
        }
        if d_x > 0.05 && t_x - w > w {
            knots.push((d_x, t_x - w));
        }
        if d_top > d_x * 1.05 && t_top - w > knots.last().map_or(0.0, |k| k.1) {
            knots.push((d_top, t_top - w));
        }
        knots
    }

    /// where a donor height of character `c` goes, before offsetting.
    pub fn map_y(&self, c: char, y: f64) -> f64 {
        piecewise(&self.knots(c), y)
    }

    /// the donor glyph of `c` changed: zones and width, then lean, then
    /// weight.
    pub fn apply(&self, c: char, shape: &Shape) -> Shape {
        let knots = self.knots(c);
        let (scale, shear) = (self.x_scale, self.shear);
        let mapped = shape.map(|x, y| {
            let y2 = piecewise(&knots, y);
            (x * scale + shear * y2, y2)
        });
        offset_shape_xy(&mapped, self.weight, self.weight_y)
    }

    /// the same change on sampled points, which is much quicker and is
    /// what the fitting searches use.
    pub fn apply_samples(&self, c: char, samples: &Samples) -> Samples {
        let knots = self.knots(c);
        let (scale, shear) = (self.x_scale, self.shear);
        samples
            .mapped(|x, y| {
                let y2 = piecewise(&knots, y);
                (x * scale + shear * y2, y2)
            })
            .offset_xy(self.weight, self.weight_y)
    }
}

/// a straight-line-between-knots function, continued past the ends.
pub(crate) fn piecewise(knots: &[(f64, f64)], y: f64) -> f64 {
    match knots.len() {
        0 => y,
        1 => y + knots[0].1 - knots[0].0,
        _ => {
            let at = knots
                .windows(2)
                .position(|pair| y <= pair[1].0)
                .unwrap_or(knots.len() - 2);
            let (a, b) = (knots[at], knots[at + 1]);
            let slope = (b.1 - a.1) / (b.0 - a.0);
            a.1 + (y - a.0) * slope
        }
    }
}
