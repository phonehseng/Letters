//! setting a paragraph a little tighter so it fits, only within limits
//! that keep it looking the same: word spaces first, then letter
//! spacing, then leading by a hair. the gentlest setting that fits wins.

use super::paragraph::Spacing;

/// word spaces may shrink to 85% of their width: justified text in books
/// routinely varies spaces by more than that, so the eye doesn't notice.
const WORD_SCALES: [f32; 4] = [1.0, 0.95, 0.9, 0.85];
/// letter spacing may tighten by up to 0.015 em (15 thousandths), less
/// than the tracking change most typesetters treat as visible.
const LETTER_EMS: [f32; 4] = [0.0, -0.005, -0.01, -0.015];
/// leading may shrink by up to 3%: at 12 points that is 0.4 points a
/// line, a hair, and less than the rounding many writers apply.
const LEADING_SCALES: [f32; 4] = [1.0, 0.99, 0.98, 0.97];

/// every allowed setting, gentlest first. the gentleness of a setting
/// is how far it goes into each allowance, added up.
pub(crate) fn candidates() -> Vec<Spacing> {
    let mut all = Vec::new();
    for word_scale in WORD_SCALES {
        for letter_em in LETTER_EMS {
            for leading_scale in LEADING_SCALES {
                all.push(Spacing {
                    letter_em,
                    word_scale,
                    leading_scale,
                });
            }
        }
    }
    all.sort_by(|a, b| strength(a).total_cmp(&strength(b)));
    all
}

/// how much of the allowances a setting uses, from 0 (normal) to 3 (all).
/// leading counts double so it is only used when spacing alone can't fit.
fn strength(spacing: &Spacing) -> f32 {
    let word = (1.0 - spacing.word_scale) / (1.0 - WORD_SCALES[3]);
    let letter = -spacing.letter_em / -LETTER_EMS[3];
    let leading = 2.0 * (1.0 - spacing.leading_scale) / (1.0 - LEADING_SCALES[3]);
    word + letter + leading
}

/// the gentlest setting for which `fits` is true, or none.
pub(crate) fn gentlest_fit(fits: impl Fn(&Spacing) -> bool) -> Option<Spacing> {
    candidates()
        .into_iter()
        .skip(1)
        .find(|spacing| fits(spacing))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_spacing_comes_first_and_leading_last() {
        let all = candidates();
        assert_eq!(all[0], Spacing::NORMAL);
        assert_eq!(all.len(), 64);
        let first_with_leading = all.iter().position(|s| s.leading_scale < 1.0).unwrap();
        let first_with_letters = all.iter().position(|s| s.letter_em < 0.0).unwrap();
        assert!(first_with_letters < first_with_leading);
    }
}
