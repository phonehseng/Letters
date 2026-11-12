//! working out which typeface a document's font really is, by comparing
//! the glyphs it has with the same characters in fonts on this computer.
//!
//! the shortlist comes from stored facts (widths, x-height, stroke), then
//! each shortlisted font is read and compared glyph by glyph: outline
//! distance with both glyphs at their origin (so side bearings count),
//! and advance widths. a variable font is first set to the axis values
//! (weight, width...) that bring it closest. a font that matches nearly
//! every shared glyph within a tight tolerance is the same design,
//! perhaps another version of it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::distance::{centred_distance, shape_distance, Samples, ShapeDistance};
use super::donors::{shortlist, DonorFile, DonorPool, TargetProfile};
use super::glyphs::{read_glyphs, GlyphSet};
use super::optimise::golden_minimum;
use crate::model::{Embedding, Source};

/// a glyph is "the same" within this mean outline distance (em): a
/// thousandth and a half of the em, invisible at any reading size.
pub const SAME_GLYPH_MEAN: f64 = 0.0015;
/// and within this largest distance anywhere along the outline.
pub const SAME_GLYPH_MAX: f64 = 0.008;
/// and with advances this close (em).
pub const SAME_ADVANCE: f64 = 0.002;
/// the share of shared glyphs that must be the same for the same design.
pub const SAME_DESIGN_SHARE: f64 = 0.9;
/// fewer shared glyphs than this can't prove a design.
pub const MIN_SHARED: usize = 3;

/// one font that might be the document's typeface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub family: String,
    pub style: String,
    pub source: Source,
    pub key: String,
    /// true when nearly every shared glyph is the same: the same design
    pub same_design: bool,
    /// glyphs both fonts have
    pub shared: usize,
    /// shared glyphs that are the same within tolerance
    pub matching: usize,
    /// mean outline distance over the shared glyphs, em (origins aligned)
    pub mean_distance: f64,
    /// the same with each glyph centred on the other: the look alone
    pub shape_distance: f64,
    pub max_distance: f64,
    /// mean advance difference, em
    pub advance_error: f64,
    /// variation axis values used, for variable fonts
    pub axes: Vec<(String, f32)>,
    pub embedding: Embedding,
    /// "100 × (1 − mean distance / 0.03 em)", the figure shown to people
    pub similarity: f64,
}

/// the mean outline distance that counts as 0% alike, em. unrelated
/// designs of one kind measure 14 to 20 ‰ apart on the same letters, and
/// a mean of 1 ‰ is already hard to see at 11 pt (COMPLETION_REPORT.md),
/// so 30 ‰ is "nothing alike" and every thousandth costs over 3 points.
const NOTHING_ALIKE: f64 = 0.03;

/// the figure people see: 100% for identical outlines, 0% at
/// `NOTHING_ALIKE`.
pub fn similarity_percent(mean_distance: f64) -> f64 {
    ((1.0 - mean_distance / NOTHING_ALIKE) * 100.0).clamp(0.0, 100.0)
}

/// how two glyph sets compare over the characters both have.
#[derive(Clone, Debug, Default)]
pub struct SetComparison {
    pub shared: usize,
    pub matching: usize,
    pub mean: f64,
    pub shape_mean: f64,
    pub max: f64,
    pub advance_error: f64,
    pub per_glyph: Vec<(char, ShapeDistance, f64)>,
}

/// the target's glyphs sampled once, for comparing with many donors.
pub fn sample_set(set: &GlyphSet) -> BTreeMap<char, Samples> {
    set.glyphs
        .iter()
        .map(|(c, glyph)| (*c, Samples::of(&glyph.shape)))
        .collect()
}

/// compares a donor's glyphs with the target's, glyph by glyph.
pub fn compare_sets(
    target: &GlyphSet,
    target_samples: &BTreeMap<char, Samples>,
    donor: &GlyphSet,
) -> SetComparison {
    let mut comparison = SetComparison::default();
    let (mut total, mut shape_total, mut advance_total) = (0.0, 0.0, 0.0);
    for (c, glyph) in &target.glyphs {
        let (Some(theirs), Some(ours)) = (donor.get(*c), target_samples.get(c)) else {
            continue;
        };
        if glyph.shape.is_empty() && theirs.shape.is_empty() && c.is_whitespace() {
            // spaces say nothing about the design, only their width
            advance_total += (glyph.advance - theirs.advance).abs();
            comparison.shared += 1;
            comparison.matching +=
                usize::from((glyph.advance - theirs.advance).abs() <= SAME_ADVANCE);
            continue;
        }
        let their_samples = Samples::of(&theirs.shape);
        let distance = shape_distance(ours, &their_samples);
        let centred = centred_distance(ours, &their_samples);
        let advance = (glyph.advance - theirs.advance).abs();
        comparison.shared += 1;
        if distance.mean <= SAME_GLYPH_MEAN
            && distance.max <= SAME_GLYPH_MAX
            && advance <= SAME_ADVANCE
        {
            comparison.matching += 1;
        }
        total += distance.mean;
        shape_total += centred.mean;
        advance_total += advance;
        comparison.max = comparison.max.max(distance.max);
        comparison.per_glyph.push((*c, distance, advance));
    }
    let drawn = comparison.per_glyph.len().max(1) as f64;
    comparison.mean = total / drawn;
    comparison.shape_mean = shape_total / drawn;
    comparison.advance_error = advance_total / comparison.shared.max(1) as f64;
    comparison
}

/// options for identifying.
#[derive(Clone, Debug)]
pub struct IdentifyOptions {
    /// how many fonts are compared glyph by glyph
    pub candidate_limit: usize,
    /// whether variable fonts are tuned to their closest instance
    pub solve_axes: bool,
}

impl Default for IdentifyOptions {
    fn default() -> Self {
        IdentifyOptions {
            candidate_limit: 10,
            solve_axes: true,
        }
    }
}

/// the fonts most like the target, best first. the first is the same
/// design when its `same_design` is set.
pub fn identify_glyphs(
    target: &GlyphSet,
    profile: &TargetProfile,
    pool: &DonorPool,
    options: &IdentifyOptions,
) -> Vec<Candidate> {
    let samples = sample_set(target);
    let chars: Vec<char> = target.glyphs.keys().copied().collect();
    let mut found: Vec<Candidate> = shortlist(profile, pool, options.candidate_limit)
        .into_iter()
        .filter_map(|donor| compare_donor(target, &samples, &chars, donor, options))
        .collect();
    found.sort_by(|a, b| {
        b.same_design
            .cmp(&a.same_design)
            .then(a.mean_distance.total_cmp(&b.mean_distance))
            .then_with(|| a.key.cmp(&b.key))
    });
    found
}

/// reads one donor and compares it, after tuning a variable font.
pub fn compare_donor(
    target: &GlyphSet,
    samples: &BTreeMap<char, Samples>,
    chars: &[char],
    donor: &DonorFile,
    options: &IdentifyOptions,
) -> Option<Candidate> {
    let bytes = donor.bytes().ok()?;
    let axes = if options.solve_axes && !donor.axes.is_empty() {
        solve_axes(&bytes, donor, chars, |set| {
            let comparison = compare_sets(target, samples, set);
            comparison.mean + comparison.advance_error * 0.5
        })
    } else {
        Vec::new()
    };
    let set = read_glyphs(&bytes, donor.index, &axes, chars.iter().copied())?;
    let comparison = compare_sets(target, samples, &set);
    if comparison.shared == 0 {
        return None;
    }
    let same_design = comparison.shared >= MIN_SHARED
        && comparison.matching as f64 >= comparison.shared as f64 * SAME_DESIGN_SHARE;
    Some(Candidate {
        family: donor.family.clone(),
        style: donor.style.clone(),
        source: donor.source,
        key: donor.key.clone(),
        same_design,
        shared: comparison.shared,
        matching: comparison.matching,
        mean_distance: comparison.mean,
        shape_distance: comparison.shape_mean,
        max_distance: comparison.max,
        advance_error: comparison.advance_error,
        axes,
        embedding: donor.embedding.clone(),
        similarity: similarity_percent(comparison.mean),
    })
}

/// the axis values of a variable font that make `score` lowest: each
/// axis searched in turn (twice round), the others held.
pub fn solve_axes(
    bytes: &[u8],
    donor: &DonorFile,
    chars: &[char],
    mut score: impl FnMut(&GlyphSet) -> f64,
) -> Vec<(String, f32)> {
    // a dozen letters are plenty to find an instance, and quick
    let probe: Vec<char> = chars
        .iter()
        .copied()
        .filter(|c| c.is_alphanumeric())
        .take(16)
        .collect();
    let mut values: Vec<(String, f32)> = donor
        .axes
        .iter()
        .filter(|axis| {
            matches!(
                axis.tag.as_str(),
                "wght" | "wdth" | "opsz" | "slnt" | "ital"
            )
        })
        .map(|axis| (axis.tag.clone(), axis.default))
        .collect();
    for _round in 0..2 {
        for at in 0..values.len() {
            let Some(axis) = donor.axes.iter().find(|axis| axis.tag == values[at].0) else {
                continue;
            };
            let (low, high) = (f64::from(axis.min), f64::from(axis.max));
            if high - low < 1e-3 {
                continue;
            }
            let best = golden_minimum(
                |value| {
                    let mut trial = values.clone();
                    trial[at].1 = value as f32;
                    read_glyphs(bytes, donor.index, &trial, probe.iter().copied())
                        .map(|set| score(&set))
                        .unwrap_or(f64::MAX)
                },
                low,
                high,
                14,
            );
            values[at].1 = best as f32;
        }
    }
    values
}
