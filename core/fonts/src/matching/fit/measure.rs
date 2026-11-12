//! the measurements a fit starts from: zone heights, width and stem
//! weight by least squares, bar weight, slant, and the shape objective the
//! searches minimise.

use super::*;

/// every fourth shared glyph is held out once there are enough to spare.
pub(crate) fn split_held_out(shared: &[char]) -> (Vec<char>, Vec<char>) {
    if shared.len() < 8 {
        return (shared.to_vec(), Vec::new());
    }
    let mut fit = Vec::new();
    let mut check = Vec::new();
    for (index, c) in shared.iter().enumerate() {
        if index % 4 == 3 {
            check.push(*c);
        } else {
            fit.push(*c);
        }
    }
    (fit, check)
}

pub(crate) fn search(
    objective: &impl Fn(&Transform) -> f64,
    transform: &Transform,
    low: f64,
    high: f64,
    set: impl Fn(&mut Transform, f64),
) -> f64 {
    golden_minimum(
        |value| {
            let mut trial = transform.clone();
            set(&mut trial, value);
            objective(&trial)
        },
        low,
        high,
        16,
    )
}

/// the mean distance between the target's glyphs and the changed donor's,
/// each centred on the other (spacing is fitted separately).
pub(crate) fn shape_objective(
    transform: &Transform,
    chars: &[char],
    target: &BTreeMap<char, Samples>,
    donor: &BTreeMap<char, Samples>,
) -> f64 {
    let mut total = 0.0;
    for c in chars {
        let changed = transform.apply_samples(*c, &donor[c]);
        total += centred_distance(&target[c], &changed).mean;
    }
    total / chars.len().max(1) as f64
}

/// the zone heights from the tops and bottoms of shared glyphs: the
/// median ratio of target to donor in each zone.
pub(crate) fn fit_zones(
    target: &GlyphSet,
    donor: &GlyphSet,
    chars: &[char],
    target_style: &StyleFit,
    donor_style: &StyleFit,
) -> VerticalZones {
    let ratio = |letters: &str, use_bottom: bool| -> Option<f64> {
        let mut ratios: Vec<f64> = chars
            .iter()
            .filter(|c| letters.contains(**c))
            .filter_map(|c| {
                let t = target.get(*c)?.shape.bounds()?;
                let d = donor.get(*c)?.shape.bounds()?;
                let (t, d) = if use_bottom {
                    (t.y_min, d.y_min)
                } else {
                    (t.y_max, d.y_max)
                };
                (d.abs() > 0.02).then_some(t / d)
            })
            .filter(|r| (0.5..2.0).contains(r))
            .collect();
        ratios.sort_by(f64::total_cmp);
        ratios.get(ratios.len() / 2).copied()
    };
    let fallback = |t: f64, d: f64| if d.abs() > 0.02 { t / d } else { 1.0 };
    let x = ratio("acemnorsuvwxz", false)
        .unwrap_or_else(|| fallback(target_style.x_height, donor_style.x_height));
    let cap = ratio("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", false);
    let ascender = ratio("bdhkl", false);
    // a zone with no shared letters borrows the other top zone's ratio
    let (cap, ascender) = match (cap, ascender) {
        (Some(cap), Some(ascender)) => (cap, ascender),
        (Some(cap), None) => (cap, cap),
        (None, Some(ascender)) => (ascender, ascender),
        (None, None) => (x, x),
    };
    let descender = ratio("gjpqy", true).unwrap_or(x);
    VerticalZones {
        donor: [
            donor_style.descender_depth,
            donor_style.x_height,
            donor_style.cap_height,
            donor_style.ascender_height,
        ],
        target: [
            donor_style.descender_depth * descender,
            donor_style.x_height * x,
            donor_style.cap_height * cap,
            donor_style.ascender_height * ascender,
        ],
    }
}

pub(crate) fn initial_shear(target: &StyleFit, donor: &StyleFit) -> f64 {
    (target.slant_degrees.to_radians().tan() - donor.slant_degrees.to_radians().tan())
        .clamp(-0.4, 0.4)
}

/// solves "target width = scale × donor width + 2 × weight" by least
/// squares over every shared glyph's ink width and stem widths, with the
/// donor already set to the target's heights and lean.
pub(crate) fn width_and_weight(
    target: &GlyphSet,
    donor: &GlyphSet,
    chars: &[char],
    transform: &Transform,
) -> Option<(f64, f64)> {
    let unweighted = Transform {
        x_scale: 1.0,
        weight: 0.0,
        weight_y: 0.0,
        ..transform.clone()
    };
    let mut rows = Vec::new();
    for c in chars {
        let target_shape = &target.get(*c)?.shape;
        let donor_shape = unweighted.apply(*c, &donor.get(*c)?.shape);
        let (Some(t), Some(d)) = (target_shape.bounds(), donor_shape.bounds()) else {
            continue;
        };
        rows.push((d.width(), 2.0, t.width()));
        for share in [0.3, 0.5, 0.7] {
            let y = t.y_min + t.height() * share;
            let ours = horizontal_spans(target_shape, y);
            let theirs = horizontal_spans(&donor_shape, y);
            if ours.len() == theirs.len() && (1..=4).contains(&ours.len()) {
                for (a, b) in ours.iter().zip(&theirs) {
                    rows.push((b.1 - b.0, 2.0, a.1 - a.0));
                }
            }
        }
    }
    least_squares_2(&rows)
}

/// how much thicker (per side) the target's horizontal strokes are than
/// the donor's once both are at the target's heights: the median over
/// vertical lines through each shared glyph at 30, 50 and 70% of its
/// width, where both cross the same number of strokes.
pub(crate) fn bar_weight(
    target: &GlyphSet,
    donor: &GlyphSet,
    chars: &[char],
    transform: &Transform,
) -> Option<f64> {
    let unweighted = Transform {
        weight: 0.0,
        weight_y: 0.0,
        ..transform.clone()
    };
    let mut differences = Vec::new();
    for c in chars {
        let target_shape = &target.get(*c)?.shape;
        let donor_shape = unweighted.apply(*c, &donor.get(*c)?.shape);
        let (Some(t), Some(d)) = (target_shape.bounds(), donor_shape.bounds()) else {
            continue;
        };
        for share in [0.3, 0.5, 0.7] {
            let ours = vertical_spans(target_shape, t.x_min + t.width() * share);
            let theirs = vertical_spans(&donor_shape, d.x_min + d.width() * share);
            if ours.len() != theirs.len() || !(1..=4).contains(&ours.len()) {
                continue;
            }
            for (a, b) in ours.iter().zip(&theirs) {
                differences.push(((a.1 - a.0) - (b.1 - b.0)) / 2.0);
            }
        }
    }
    if differences.is_empty() {
        return None;
    }
    differences.sort_by(f64::total_cmp);
    Some(differences[differences.len() / 2])
}
