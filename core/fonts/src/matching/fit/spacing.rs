//! the target's spacing as straight lines of the donor's: advances and
//! side bearings fitted by least squares, and the advance rule that
//! predicts the shared glyphs best.

use super::*;

/// how a new glyph's advance is worked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdvanceRule {
    /// a straight line of the donor's advance: best when the two fonts
    /// share their widths (metric twins) or their proportions
    DonorAdvance,
    /// the changed glyph's ink plus modelled side bearings: best when the
    /// donor's letters are proportioned differently, since the fitted
    /// width already brought the ink to the target's widths
    InkAndBearings,
}

/// the target's spacing as straight lines of the donor's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpacingModel {
    /// target advance ≈ slope × donor advance + offset
    pub advance: (f64, f64),
    /// target left side bearing ≈ slope × changed donor's + offset
    pub left: (f64, f64),
    /// target right side bearing ≈ slope × changed donor's + offset (the
    /// donor's right bearing measured against its advance times the width
    /// scale)
    pub right: (f64, f64),
    /// which of the two ways gives the advance (the one with the smaller
    /// error on the shared glyphs)
    pub rule: AdvanceRule,
}

impl SpacingModel {
    pub fn identity() -> SpacingModel {
        SpacingModel {
            advance: (1.0, 0.0),
            left: (1.0, 0.0),
            right: (1.0, 0.0),
            rule: AdvanceRule::DonorAdvance,
        }
    }

    /// the advance the model gives a changed glyph.
    pub fn advance_for(&self, changed: &Shape, donor_advance: f64, x_scale: f64) -> f64 {
        let from_advance = self.advance.0 * donor_advance + self.advance.1;
        let Some(bounds) = changed.bounds() else {
            return from_advance;
        };
        match self.rule {
            AdvanceRule::DonorAdvance => from_advance,
            AdvanceRule::InkAndBearings => {
                let left = self.left.0 * bounds.x_min + self.left.1;
                let right = self.right.0 * (donor_advance * x_scale - bounds.x_max) + self.right.1;
                left + bounds.width() + right
            }
        }
    }

    /// places a changed glyph: its advance from the model (or the one
    /// given, such as the pdf's width), its ink moved to the modelled
    /// left side bearing, the room left over shared the same way when
    /// the advance is fixed.
    pub fn place(
        &self,
        changed: &Shape,
        donor_advance: f64,
        x_scale: f64,
        fixed_advance: Option<f64>,
    ) -> GlyphData {
        let modelled = self.advance_for(changed, donor_advance, x_scale);
        let Some(bounds) = changed.bounds() else {
            return GlyphData {
                shape: changed.clone(),
                advance: fixed_advance.unwrap_or(modelled).max(0.0),
            };
        };
        let left = self.left.0 * bounds.x_min + self.left.1;
        let advance = fixed_advance.unwrap_or(modelled).max(bounds.width() * 0.5);
        let left = match fixed_advance {
            Some(fixed) => {
                let right = modelled - left - bounds.width();
                let room = fixed - bounds.width();
                let sum = left + right;
                if sum.abs() > 1e-6 {
                    room * (left / sum)
                } else {
                    room / 2.0
                }
            }
            None => left,
        };
        GlyphData {
            shape: changed.translated(left - bounds.x_min, 0.0),
            advance,
        }
    }
}

/// the advance and side bearing lines, by least squares, and whichever
/// advance rule has the smaller error on the shared glyphs.
pub(crate) fn fit_spacing(
    target: &GlyphSet,
    donor: &GlyphSet,
    chars: &[char],
    transform: &Transform,
) -> SpacingModel {
    let mut advances = Vec::new();
    let mut lefts = Vec::new();
    let mut rights = Vec::new();
    let mut changed_glyphs = Vec::new();
    for c in chars {
        let (Some(t), Some(d)) = (target.get(*c), donor.get(*c)) else {
            continue;
        };
        advances.push((d.advance, t.advance));
        let changed = transform.apply(*c, &d.shape);
        if let (Some(tb), Some(db)) = (t.shape.bounds(), changed.bounds()) {
            lefts.push((db.x_min, tb.x_min));
            rights.push((
                d.advance * transform.x_scale - db.x_max,
                t.advance - tb.x_max,
            ));
        }
        changed_glyphs.push((changed, d.advance, t.advance));
    }
    let mut model = SpacingModel {
        advance: sane_line(fit_line(&advances), &advances),
        left: sane_line(fit_line(&lefts), &lefts),
        right: sane_line(fit_line(&rights), &rights),
        rule: AdvanceRule::DonorAdvance,
    };
    let error = |model: &SpacingModel| -> f64 {
        changed_glyphs
            .iter()
            .map(|(shape, donor_advance, target_advance)| {
                (model.advance_for(shape, *donor_advance, transform.x_scale) - target_advance).abs()
            })
            .sum::<f64>()
    };
    let by_ink = SpacingModel {
        rule: AdvanceRule::InkAndBearings,
        ..model.clone()
    };
    if error(&by_ink) < error(&model) {
        model = by_ink;
    }
    model
}

/// a fitted line is kept only when its slope is believable; otherwise a
/// line of slope 1 through the mean difference is safer.
pub(crate) fn sane_line(line: (f64, f64), points: &[(f64, f64)]) -> (f64, f64) {
    if (0.5..=2.0).contains(&line.0) && points.len() >= 3 {
        return line;
    }
    let n = points.len().max(1) as f64;
    let offset = points.iter().map(|p| p.1 - p.0).sum::<f64>() / n;
    (1.0, offset)
}
