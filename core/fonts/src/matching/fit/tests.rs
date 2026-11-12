//! tests for fitting, on letters made of rectangles whose changes are known.

use super::*;
use crate::matching::glyphs::{GlyphData, GlyphSet};
use crate::matching::shape::tests::{ring, square};

/// letters made of rectangles: every rectangle's sides are moved to
/// `x × width ∓ extra`, so stems get `2 × extra` heavier and the whole
/// letter `width` times wider, while heights stay put (as in a real
/// family's weights).
fn letters_with(width: f64, extra: f64) -> Vec<(char, Shape, f64)> {
    let rect =
        |x0: f64, y0: f64, x1: f64, y1: f64| square(x0 * width - extra, y0, x1 * width + extra, y1);
    let letter = |rects: &[(f64, f64, f64, f64)]| Shape {
        contours: rects.iter().map(|r| rect(r.0, r.1, r.2, r.3)).collect(),
    };
    vec![
        ('l', letter(&[(0.06, 0.0, 0.14, 0.72)]), 0.2 * width),
        ('i', letter(&[(0.06, 0.0, 0.14, 0.5)]), 0.2 * width),
        (
            'n',
            letter(&[
                (0.06, 0.0, 0.14, 0.5),
                (0.36, 0.0, 0.44, 0.5),
                (0.06, 0.42, 0.44, 0.5),
            ]),
            0.5 * width,
        ),
        (
            'h',
            letter(&[
                (0.06, 0.0, 0.14, 0.72),
                (0.36, 0.0, 0.44, 0.5),
                (0.06, 0.42, 0.44, 0.5),
            ]),
            0.5 * width,
        ),
        (
            'H',
            letter(&[
                (0.08, 0.0, 0.17, 0.7),
                (0.48, 0.0, 0.57, 0.7),
                (0.08, 0.32, 0.57, 0.4),
            ]),
            0.65 * width,
        ),
        ('I', letter(&[(0.08, 0.0, 0.17, 0.7)]), 0.25 * width),
        ('x', letter(&[(0.05, 0.0, 0.45, 0.5)]), 0.5 * width),
        (
            'u',
            letter(&[
                (0.06, 0.0, 0.14, 0.5),
                (0.36, 0.0, 0.44, 0.5),
                (0.06, 0.0, 0.44, 0.08),
            ]),
            0.5 * width,
        ),
    ]
}

fn letter_shapes() -> Vec<(char, Shape, f64)> {
    let mut letters = letters_with(1.0, 0.0);
    letters.push(('o', ring().scaled(1.0, 1.25, 0.0, -0.125), 0.6));
    letters
}

fn set_from(letters: &[(char, Shape, f64)]) -> GlyphSet {
    let mut set = GlyphSet::default();
    for (c, shape, advance) in letters {
        set.glyphs.insert(
            *c,
            GlyphData {
                shape: shape.with_truetype_direction(),
                advance: *advance,
            },
        );
    }
    set
}

#[test]
fn piecewise_zones_hit_their_knots() {
    let style = StyleFit {
        x_height: 0.5,
        cap_height: 0.7,
        ascender_height: 0.74,
        descender_depth: -0.2,
        ..StyleFit::default()
    };
    let mut transform = Transform::identity(&style);
    transform.zones.target = [-0.22, 0.55, 0.72, 0.72];
    assert!((transform.map_y('x', 0.5) - 0.55).abs() < 1e-12);
    assert!((transform.map_y('H', 0.7) - 0.72).abs() < 1e-12);
    // lowercase ascenders have their own zone: 0.74 lands on 0.72 too
    assert!((transform.map_y('l', 0.74) - 0.72).abs() < 1e-12);
    assert!((transform.map_y('H', 0.74) - 0.72).abs() > 0.01);
    assert!((transform.map_y('p', -0.2) + 0.22).abs() < 1e-12);
    assert!(transform.map_y('x', 0.0).abs() < 1e-12);
    // with bar weight, the knots move in so the offset lands on them
    transform.weight_y = 0.01;
    assert!((transform.map_y('x', 0.5) - 0.54).abs() < 1e-12);
    assert!((transform.map_y('x', 0.0) - 0.01).abs() < 1e-12);
}

#[test]
fn a_known_change_is_recovered() {
    // the "donor" is the target made 8% narrower and lighter by
    // 0.012 em a side; the fit must find both
    let target = set_from(&letters_with(1.0, 0.0));
    let donor = set_from(&letters_with(1.0 / 1.08, -0.012));
    let target_style = measure_style(&target);
    let donor_style = measure_style(&donor);
    let chars: Vec<char> = target.glyphs.keys().copied().collect();
    let mut transform = Transform::identity(&donor_style);
    transform.zones = fit_zones(&target, &donor, &chars, &target_style, &donor_style);
    let (scale, weight) = width_and_weight(&target, &donor, &chars, &transform).unwrap();
    assert!((scale - 1.08).abs() < 0.03, "scale {scale}");
    assert!((weight - 0.012 * 1.08).abs() < 0.004, "weight {weight}");
    transform.x_scale = scale;
    transform.weight = weight;
    let spacing = fit_spacing(&target, &donor, &chars, &transform);
    assert!((spacing.advance.0 - 1.08).abs() < 0.02, "{spacing:?}");
}

#[test]
fn a_lean_is_fitted_by_search() {
    let upright = set_from(&letter_shapes());
    let leaning_letters: Vec<(char, Shape, f64)> = letter_shapes()
        .into_iter()
        .map(|(c, shape, advance)| (c, shape.sheared(0.18), advance))
        .collect();
    let target = set_from(&leaning_letters);
    let chars: Vec<char> = target.glyphs.keys().copied().collect();
    let samples = |set: &GlyphSet| -> BTreeMap<char, Samples> {
        set.glyphs
            .iter()
            .map(|(c, g)| (*c, Samples::of(&g.shape)))
            .collect()
    };
    let (target_samples, donor_samples) = (samples(&target), samples(&upright));
    let objective = |t: &Transform| shape_objective(t, &chars, &target_samples, &donor_samples);
    let start = Transform::identity(&measure_style(&upright));
    let shear = search(&objective, &start, -0.3, 0.3, |t, v| t.shear = v);
    assert!((shear - 0.18).abs() < 0.01, "shear {shear}");
}

#[test]
fn placing_follows_the_spacing_model() {
    let model = SpacingModel {
        advance: (1.1, 0.0),
        left: (1.0, 0.02),
        right: (1.0, 0.01),
        rule: AdvanceRule::DonorAdvance,
    };
    let shape = Shape {
        contours: vec![square(0.05, 0.0, 0.35, 0.5)],
    };
    let placed = model.place(&shape, 0.4, 1.0, None);
    assert!((placed.advance - 0.44).abs() < 1e-12);
    assert!((placed.shape.bounds().unwrap().x_min - 0.07).abs() < 1e-9);
    // a fixed advance shares the room in the modelled proportion
    let fixed = model.place(&shape, 0.4, 1.0, Some(0.5));
    assert_eq!(fixed.advance, 0.5);
    let bounds = fixed.shape.bounds().unwrap();
    assert!(bounds.x_min > 0.07 && 0.5 - bounds.x_max > 0.0);
    // by ink: left 0.07, ink 0.3, right (0.4 - 0.35) + 0.01
    let by_ink = SpacingModel {
        rule: AdvanceRule::InkAndBearings,
        ..model
    };
    assert!((by_ink.place(&shape, 0.4, 1.0, None).advance - 0.43).abs() < 1e-12);
}
