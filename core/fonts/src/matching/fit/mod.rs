//! bending a near design so its glyphs imitate the target's.
//!
//! when no font on the computer is the same design, the closest one is
//! fitted to the glyphs the document's font does have, in this order:
//!
//! 1. vertical zones: separate scales below the baseline, up to the
//!    x-height, up to the capital height and (for lowercase) up to the
//!    ascender, from the tops and bottoms of the shared glyphs;
//! 2. width and stem weight together, by least squares: every shared
//!    glyph's ink width and the widths of its stems at a few heights give
//!    equations "target = scale × donor + 2 × offset"; the weight of the
//!    horizontal strokes separately, from their thicknesses;
//! 3. slant, by searching the lean that brings the outlines closest;
//! 4. a last polish of width and both weights on the outline distance;
//! 5. spacing: the target's advances and side bearings as straight lines
//!    of the donor's (least squares), so new letters follow the target's
//!    rhythm rather than the donor's.
//!
//! | part | job |
//! |---|---|
//! | `transform` | the change itself, on outlines and on samples |
//! | `measure` | the starting measurements and the shape objective |
//! | `spacing` | advances and side bearings |
//!
//! a variable donor is first set to the axis values that bring it
//! closest (identify.rs). every fourth shared glyph is held out of the
//! fit and used to report how well the fit predicts glyphs it never saw.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::distance::{centred_distance, shape_distance, Samples};
use super::donors::DonorFile;
use super::glyphs::{read_glyphs, GlyphData, GlyphSet};
use super::identify::similarity_percent;
use super::offset::{offset_shape_xy, thinning_limit};
use super::optimise::{fit_line, golden_minimum, least_squares_2};
use super::shape::Shape;
use super::style::{horizontal_spans, measure_style, vertical_spans, StyleFit};
use crate::model::{Embedding, Source};

mod measure;
mod spacing;
#[cfg(test)]
mod tests;
mod transform;

// the parts share this module's imports through `use super::*`; the
// transform and spacing types are part of the public api
use measure::*;
pub use spacing::*;
pub use transform::*;

/// fewer shared drawn glyphs than this can't support a fit.
pub const MIN_FIT_GLYPHS: usize = 3;

/// how well a fit predicts the target, em.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitQuality {
    /// glyphs the fit was made from
    pub fitted: usize,
    /// glyphs held out and used only to check it
    pub held_out: usize,
    /// mean outline distance of the checked glyphs, placed as new text
    /// would be (origins aligned)
    pub mean: f64,
    pub max: f64,
    /// mean advance difference of the checked glyphs
    pub advance_error: f64,
    /// the mean distance before any change, for comparison
    pub unchanged_mean: f64,
    pub similarity: f64,
}

/// a donor fitted to the target, ready to make glyphs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FittedDonor {
    pub family: String,
    pub style: String,
    pub source: Source,
    pub key: String,
    pub embedding: Embedding,
    pub transform: Transform,
    pub spacing: SpacingModel,
    pub quality: FitQuality,
}

impl FittedDonor {
    /// the donor's glyph for `c` changed and placed for the target.
    pub fn make(&self, c: char, glyph: &GlyphData, fixed_advance: Option<f64>) -> GlyphData {
        self.spacing.place(
            &self.transform.apply(c, &glyph.shape),
            glyph.advance,
            self.transform.x_scale,
            fixed_advance,
        )
    }
}

/// the letters read from a donor for measuring its style, whatever the
/// target has.
pub const STYLE_LETTERS: &str = "xzvwuynmriHEFIZTLKNMXBDPRUbdhklpqgjyolOecsa";

/// fits a donor to the target. None when they share too few glyphs, the
/// donor can't be read, or the fit is worse than leaving it alone.
pub fn fit_donor(
    target: &GlyphSet,
    target_style: &StyleFit,
    donor: &DonorFile,
    axes: &[(String, f32)],
) -> Option<FittedDonor> {
    let bytes = donor.bytes().ok()?;
    let wanted = target.glyphs.keys().copied().chain(STYLE_LETTERS.chars());
    let donor_set = read_glyphs(&bytes, donor.index, axes, wanted)?;
    let shared = shared_drawn_chars(target, &donor_set);
    if shared.len() < MIN_FIT_GLYPHS {
        return None;
    }
    let (fit_chars, check_chars) = split_held_out(&shared);
    let target_samples = sample_chars(target, &shared);
    let problem = FitProblem {
        target,
        donor: &donor_set,
        donor_style: measure_style(&donor_set),
        fit_chars: &fit_chars,
        target_samples: &target_samples,
        donor_samples: sample_chars(&donor_set, &shared),
    };
    let transform = problem.fit_transform(target_style, axes);
    let spacing = fit_spacing(target, &donor_set, &fit_chars, &transform);
    let fitted = FittedDonor {
        family: donor.family.clone(),
        style: donor.style.clone(),
        source: donor.source,
        key: donor.key.clone(),
        embedding: donor.embedding.clone(),
        transform,
        spacing,
        quality: FitQuality::default(),
    };
    let checked = if check_chars.is_empty() {
        &fit_chars
    } else {
        &check_chars
    };
    let quality = check_fit(
        &fitted,
        target,
        &donor_set,
        &target_samples,
        checked,
        fit_chars.len(),
        check_chars.len(),
    );
    Some(FittedDonor { quality, ..fitted })
}

/// characters both fonts draw with ink (spaces say nothing of shape).
fn shared_drawn_chars(target: &GlyphSet, donor: &GlyphSet) -> Vec<char> {
    target
        .glyphs
        .iter()
        .filter(|(c, glyph)| {
            let donor_draws = donor.get(**c).is_some_and(|g| !g.shape.is_empty());
            !glyph.shape.is_empty() && donor_draws
        })
        .map(|(c, _)| *c)
        .collect()
}

fn sample_chars(set: &GlyphSet, chars: &[char]) -> BTreeMap<char, Samples> {
    chars
        .iter()
        .map(|c| (*c, Samples::of(&set.glyphs[c].shape)))
        .collect()
}

/// everything one fit works from.
struct FitProblem<'a> {
    target: &'a GlyphSet,
    donor: &'a GlyphSet,
    donor_style: StyleFit,
    fit_chars: &'a [char],
    target_samples: &'a BTreeMap<char, Samples>,
    donor_samples: BTreeMap<char, Samples>,
}

/// how far the polish may move the measured width (5%) and weights
/// (12 ‰ of the em): the measurements are close, the polish only refines.
const POLISH_SCALE: f64 = 0.05;

const POLISH_WEIGHT: f64 = 0.012;

impl FitProblem<'_> {
    fn objective(&self, transform: &Transform) -> f64 {
        shape_objective(
            transform,
            self.fit_chars,
            self.target_samples,
            &self.donor_samples,
        )
    }

    /// zones, then width and weights, then slant, then a polish; never
    /// worse than leaving the donor as it is.
    fn fit_transform(&self, target_style: &StyleFit, axes: &[(String, f32)]) -> Transform {
        let unchanged = Transform {
            axes: axes.to_vec(),
            ..Transform::identity(&self.donor_style)
        };
        let mut transform = Transform {
            zones: fit_zones(
                self.target,
                self.donor,
                self.fit_chars,
                target_style,
                &self.donor_style,
            ),
            shear: initial_shear(target_style, &self.donor_style),
            ..unchanged.clone()
        };
        // twice, so the zones' weight compensation settles
        for _ in 0..2 {
            self.measure_weights(&mut transform);
        }
        self.polish(&mut transform);
        if self.objective(&transform) > self.objective(&unchanged) {
            return unchanged;
        }
        transform
    }

    /// width and stem weight by least squares, bar weight from the
    /// horizontal strokes, each capped so nothing turns inside out.
    fn measure_weights(&self, transform: &mut Transform) {
        if let Some((scale, weight)) =
            width_and_weight(self.target, self.donor, self.fit_chars, transform)
        {
            transform.x_scale = scale.clamp(0.6, 1.6);
            transform.weight = weight.clamp(-self.stem_limit(transform), 0.08);
        }
        if let Some(weight_y) = bar_weight(self.target, self.donor, self.fit_chars, transform) {
            transform.weight_y = weight_y.clamp(-thinning_limit(self.donor_style.hairline), 0.08);
        }
    }

    fn stem_limit(&self, transform: &Transform) -> f64 {
        thinning_limit(self.donor_style.stem * transform.x_scale)
    }

    /// slant by search, then each of width and the two weights in turn.
    fn polish(&self, transform: &mut Transform) {
        let objective = |t: &Transform| self.objective(t);
        let shear = transform.shear;
        transform.shear = search(&objective, transform, shear - 0.15, shear + 0.15, |t, v| {
            t.shear = v
        });
        let scale = transform.x_scale;
        let (low, high) = (scale * (1.0 - POLISH_SCALE), scale * (1.0 + POLISH_SCALE));
        transform.x_scale = search(&objective, transform, low, high, |t, v| t.x_scale = v);
        let weight = transform.weight;
        let low = (weight - POLISH_WEIGHT).max(-self.stem_limit(transform));
        transform.weight = search(
            &objective,
            transform,
            low,
            weight + POLISH_WEIGHT,
            |t, v| t.weight = v,
        );
        let weight_y = transform.weight_y;
        let low = (weight_y - POLISH_WEIGHT).max(-thinning_limit(self.donor_style.hairline));
        transform.weight_y = search(
            &objective,
            transform,
            low,
            weight_y + POLISH_WEIGHT,
            |t, v| t.weight_y = v,
        );
    }
}

/// measures the fit on the chosen glyphs, placed as new text would be.
fn check_fit(
    fitted: &FittedDonor,
    target: &GlyphSet,
    donor: &GlyphSet,
    target_samples: &BTreeMap<char, Samples>,
    chars: &[char],
    fit_count: usize,
    held_out: usize,
) -> FitQuality {
    let (mut total, mut max, mut advance_total, mut unchanged_total) = (0.0, 0.0f64, 0.0, 0.0);
    for c in chars {
        let made = fitted.make(*c, &donor.glyphs[c], None);
        let made_samples = Samples::of(&made.shape);
        let distance = shape_distance(&target_samples[c], &made_samples);
        total += distance.mean;
        max = max.max(distance.max);
        advance_total += (made.advance - target.glyphs[c].advance).abs();
        unchanged_total +=
            shape_distance(&target_samples[c], &Samples::of(&donor.glyphs[c].shape)).mean;
    }
    let n = chars.len().max(1) as f64;
    FitQuality {
        fitted: fit_count,
        held_out,
        mean: total / n,
        max,
        advance_error: advance_total / n,
        unchanged_mean: unchanged_total / n,
        similarity: similarity_percent(total / n),
    }
}
