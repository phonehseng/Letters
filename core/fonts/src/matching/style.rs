//! the measured style of a font, from whatever glyphs it has: how thick
//! its stems are, how tall its lowercase and capitals are, how much it
//! leans, its stroke contrast, whether it has serifs, and how loosely it
//! is spaced. callers use `StyleFit` to keep new text consistent with the
//! text around it; the fitting and scoring code uses it as the target.

use serde::{Deserialize, Serialize};

use super::glyphs::GlyphSet;
use super::shape::Shape;

/// the measured style of a font, lengths in em.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleFit {
    /// the width of a lowercase vertical stem ("l", "n", "i"...)
    pub stem: f64,
    /// the width of a capital's vertical stem ("I", "H", "L"...)
    pub cap_stem: f64,
    /// the top of flat lowercase letters ("x", "z", "u"...)
    pub x_height: f64,
    /// the top of flat capitals ("H", "E", "I"...)
    pub cap_height: f64,
    /// the top of "b", "d", "h", "k", "l"
    pub ascender_height: f64,
    /// the bottom of "p", "q", "g", "y" (negative)
    pub descender_depth: f64,
    /// the lean in degrees, positive leaning right
    pub slant_degrees: f64,
    /// thick strokes divided by thin ones, on "o" (1 for even strokes)
    pub contrast: f64,
    /// the thinnest horizontal stroke (hairline, bar or serif), which
    /// limits how far a weight change may thin a font
    #[serde(default)]
    pub hairline: f64,
    pub serif: bool,
    /// the average space beside a lowercase letter's ink (left and right
    /// side bearings averaged), em; looser fonts have more
    pub side_bearing: f64,
    /// the average advance of the lowercase letters present, em
    pub average_advance: f64,
    /// how many glyphs the measurements came from
    pub glyphs_measured: usize,
}

/// the filled stretches of a horizontal line through a shape at
/// height `y`, as (left, right) pairs, using the nonzero rule.
pub fn horizontal_spans(shape: &Shape, y: f64) -> Vec<(f64, f64)> {
    spans(shape, y, false)
}

/// the filled stretches of a vertical line at `x`, as (bottom, top).
pub fn vertical_spans(shape: &Shape, x: f64) -> Vec<(f64, f64)> {
    spans(shape, x, true)
}

fn spans(shape: &Shape, at: f64, vertical: bool) -> Vec<(f64, f64)> {
    // crossings of the line, each with the direction of the edge
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    for polygon in shape.polygons(0.004) {
        for (index, a) in polygon.iter().enumerate() {
            let b = polygon[(index + 1) % polygon.len()];
            let (a_across, a_along, b_across, b_along) = if vertical {
                (a.0, a.1, b.0, b.1)
            } else {
                (a.1, a.0, b.1, b.0)
            };
            if (a_across <= at) == (b_across <= at) {
                continue;
            }
            let t = (at - a_across) / (b_across - a_across);
            let position = a_along + (b_along - a_along) * t;
            crossings.push((position, if b_across > a_across { 1 } else { -1 }));
        }
    }
    crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut result = Vec::new();
    let mut winding = 0;
    let mut start = 0.0;
    for (position, direction) in crossings {
        let before = winding;
        winding += direction;
        if before == 0 && winding != 0 {
            start = position;
        } else if before != 0 && winding == 0 && position > start {
            result.push((start, position));
        }
    }
    result
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    values.retain(|v| v.is_finite());
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[middle]
    } else {
        (values[middle - 1] + values[middle]) / 2.0
    })
}

/// lowercase letters whose top is flat at the x-height.
const FLAT_LOWERCASE: &str = "xzvwuynmri";
/// round lowercase letters: they overshoot the x-height a little.
const ROUND_LOWERCASE: &str = "oecsa";
const FLAT_CAPITALS: &str = "HEFIZTLKNMXBDPRU";
const ASCENDERS: &str = "bdhkl";
const DESCENDERS: &str = "pqgjy";
/// letters with a straight vertical stem near their left side.
const LOWERCASE_STEMS: &str = "lindhmkbpru";
const CAPITAL_STEMS: &str = "IHLEFDBPNKR";

fn tops(set: &GlyphSet, letters: &str) -> Vec<f64> {
    letters
        .chars()
        .filter_map(|c| set.get(c)?.shape.bounds().map(|b| b.y_max))
        .collect()
}

fn bottoms(set: &GlyphSet, letters: &str) -> Vec<f64> {
    letters
        .chars()
        .filter_map(|c| set.get(c)?.shape.bounds().map(|b| b.y_min))
        .collect()
}

/// the x-height: the flat lowercase tops, else the round ones less their
/// usual overshoot, else what the font says, else a common proportion.
pub fn x_height_of(set: &GlyphSet) -> f64 {
    // "i" has a dot above its stem; its top is no x-height
    let flat: Vec<f64> = FLAT_LOWERCASE
        .chars()
        .filter(|c| *c != 'i')
        .filter_map(|c| set.get(c)?.shape.bounds().map(|b| b.y_max))
        .collect();
    median(flat)
        .or_else(|| median(tops(set, ROUND_LOWERCASE)).map(|top| top / 1.015))
        .or(set.metrics.x_height)
        .unwrap_or(0.52)
}

pub fn cap_height_of(set: &GlyphSet) -> f64 {
    median(tops(set, FLAT_CAPITALS))
        .or(set.metrics.cap_height)
        .or_else(|| median(tops(set, "OCGQS")).map(|top| top / 1.012))
        .unwrap_or(x_height_of(set) * 1.4)
}

/// the width of the first stem a horizontal line meets at `height`, for
/// the letters given.
fn stems(set: &GlyphSet, letters: &str, height: f64) -> Vec<f64> {
    letters
        .chars()
        .filter_map(|c| {
            let shape = &set.get(c)?.shape;
            let first = *horizontal_spans(shape, height).first()?;
            Some(first.1 - first.0)
        })
        .filter(|width| *width > 0.004 && *width < 0.4)
        .collect()
}

/// the lean of the stems: how far the middle of the first stem moves
/// sideways from low to high, in degrees, positive leaning right.
fn slant_of(set: &GlyphSet, x_height: f64, cap_height: f64) -> f64 {
    let lean = |c: char, top: f64| -> Option<f64> {
        let shape = &set.get(c)?.shape;
        let (low, high) = (top * 0.25, top * 0.75);
        let a = *horizontal_spans(shape, low).first()?;
        let b = *horizontal_spans(shape, high).first()?;
        let shift = (b.0 + b.1) / 2.0 - (a.0 + a.1) / 2.0;
        Some((shift / (high - low)).atan().to_degrees())
    };
    let leans: Vec<f64> = "lI1"
        .chars()
        .filter_map(|c| lean(c, if c == 'l' { x_height * 1.3 } else { cap_height }))
        .chain("nmr".chars().filter_map(|c| lean(c, x_height)))
        .filter(|degrees| degrees.abs() < 30.0)
        .collect();
    median(leans).unwrap_or(-set.metrics.italic_angle)
}

/// the contrast on "o" (or "O"): the side stroke over the top stroke.
fn contrast_of(set: &GlyphSet) -> f64 {
    for c in ['o', 'O', 'e', 'c'] {
        let Some(glyph) = set.get(c) else { continue };
        let Some(bounds) = glyph.shape.bounds() else {
            continue;
        };
        let middle_y = (bounds.y_min + bounds.y_max) / 2.0;
        let middle_x = bounds.centre_x();
        let side = horizontal_spans(&glyph.shape, middle_y);
        let top = vertical_spans(&glyph.shape, middle_x);
        if let (Some(side), Some(top)) = (side.first(), top.last()) {
            let thin = top.1 - top.0;
            if thin > 0.002 {
                return ((side.1 - side.0) / thin).clamp(0.2, 20.0);
            }
        }
    }
    1.0
}

/// the median, over a few letters, of the thinnest horizontal stroke a
/// vertical line through the letter crosses (its bars, hairlines and
/// serifs all count), em.
fn hairline_of(set: &GlyphSet) -> Option<f64> {
    let thinnest = |c: char| -> Option<f64> {
        let shape = &set.get(c)?.shape;
        let bounds = shape.bounds()?;
        [0.3, 0.5, 0.7]
            .into_iter()
            .flat_map(|share| vertical_spans(shape, bounds.x_min + bounds.width() * share))
            .map(|span| span.1 - span.0)
            .filter(|thickness| *thickness > 0.002)
            .reduce(f64::min)
    };
    median("oenHEBxzu".chars().filter_map(thinnest).collect())
}

/// serifs show as feet: near the baseline "l", "i" or "I" is much wider
/// than its stem.
fn has_serifs(set: &GlyphSet, stem: f64, cap_stem: f64) -> bool {
    let foot = |c: char, stem: f64| -> Option<bool> {
        let shape = &set.get(c)?.shape;
        let low = horizontal_spans(shape, 0.012);
        let width = low.first().map(|span| span.1 - span.0)?;
        Some(width > stem * 1.8)
    };
    ['l', 'i', 'I', 'n']
        .into_iter()
        .filter_map(|c| foot(c, if c == 'I' { cap_stem } else { stem }))
        .next()
        .unwrap_or(false)
}

/// measures the style of a set of glyphs.
pub fn measure_style(set: &GlyphSet) -> StyleFit {
    let x_height = x_height_of(set);
    let cap_height = cap_height_of(set);
    let ascender_height = median(tops(set, ASCENDERS)).unwrap_or(cap_height * 1.03);
    let descender_depth =
        median(bottoms(set, DESCENDERS)).unwrap_or(set.metrics.descender.max(-0.4) * 0.9);
    let stem = median(stems(set, LOWERCASE_STEMS, x_height * 0.45))
        .or_else(|| median(stems(set, "o", x_height * 0.5)))
        .unwrap_or(0.0);
    let cap_stem = median(stems(set, CAPITAL_STEMS, cap_height * 0.45)).unwrap_or(stem * 1.08);
    let stem = if stem > 0.0 { stem } else { cap_stem / 1.08 };
    let mut bearings = Vec::new();
    let mut advances = Vec::new();
    for (c, glyph) in &set.glyphs {
        if !c.is_lowercase() {
            continue;
        }
        if let Some(bounds) = glyph.shape.bounds() {
            bearings.push(bounds.x_min);
            bearings.push(glyph.advance - bounds.x_max);
            advances.push(glyph.advance);
        }
    }
    StyleFit {
        stem,
        cap_stem,
        x_height,
        cap_height,
        ascender_height,
        descender_depth,
        slant_degrees: slant_of(set, x_height, cap_height),
        contrast: contrast_of(set),
        hairline: hairline_of(set).unwrap_or(stem),
        serif: has_serifs(set, stem, cap_stem),
        side_bearing: median(bearings).unwrap_or(0.04),
        average_advance: if advances.is_empty() {
            0.5
        } else {
            advances.iter().sum::<f64>() / advances.len() as f64
        },
        glyphs_measured: set.glyphs.len(),
    }
}

/// where a character's ink should start and end vertically in a font of
/// this style: (bottom, top). used to judge generated glyphs.
pub fn expected_extent(style: &StyleFit, c: char) -> Option<(f64, f64)> {
    let bottom = if DESCENDERS.contains(c) {
        style.descender_depth
    } else {
        0.0
    };
    let top = if c.is_ascii_digit() || c.is_ascii_uppercase() {
        style.cap_height
    } else if ASCENDERS.contains(c) || c == 'f' || c == 't' {
        if c == 't' {
            (style.x_height + style.ascender_height) / 2.0
        } else {
            style.ascender_height
        }
    } else if c.is_ascii_lowercase() {
        if c == 'i' || c == 'j' {
            style.ascender_height * 0.95
        } else {
            style.x_height
        }
    } else {
        return None;
    };
    Some((bottom, top))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::glyphs::{GlyphData, GlyphSet};
    use crate::matching::shape::tests::{ring, square};
    use crate::matching::shape::Shape;

    fn bar(x0: f64, x1: f64, top: f64) -> GlyphData {
        GlyphData {
            shape: Shape {
                contours: vec![square(x0, 0.0, x1, top)],
            },
            advance: x1 + x0,
        }
    }

    #[test]
    fn spans_follow_the_nonzero_rule() {
        let spans = horizontal_spans(&ring(), 0.3);
        assert_eq!(spans.len(), 2);
        assert!((spans[0].0 - 0.1).abs() < 1e-9 && (spans[0].1 - 0.2).abs() < 1e-9);
        assert!(vertical_spans(&ring(), 0.3).len() == 2);
        assert!(horizontal_spans(&ring(), 0.7).is_empty());
    }

    #[test]
    fn stems_and_heights_are_measured() {
        let mut set = GlyphSet::default();
        set.glyphs.insert('l', bar(0.05, 0.13, 0.72));
        set.glyphs.insert('x', bar(0.05, 0.45, 0.5));
        set.glyphs.insert('H', bar(0.08, 0.18, 0.7));
        let style = measure_style(&set);
        assert!((style.x_height - 0.5).abs() < 1e-9);
        assert!((style.cap_height - 0.7).abs() < 1e-9);
        assert!((style.stem - 0.08).abs() < 1e-6, "{style:?}");
        assert!((style.cap_stem - 0.10).abs() < 1e-6, "{style:?}");
        assert!(style.slant_degrees.abs() < 0.1);
        assert!(!style.serif);
    }

    #[test]
    fn a_lean_is_seen() {
        let mut set = GlyphSet::default();
        set.glyphs.insert('x', bar(0.05, 0.45, 0.5));
        let leaning = bar(0.05, 0.13, 0.72).shape.sheared(0.2);
        set.glyphs.insert(
            'l',
            GlyphData {
                shape: leaning,
                advance: 0.2,
            },
        );
        let style = measure_style(&set);
        assert!(
            (style.slant_degrees - 0.2f64.atan().to_degrees()).abs() < 0.5,
            "{style:?}"
        );
    }
}
