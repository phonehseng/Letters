//! proof by export: where every glyph of a candidate landed in word's own
//! pdf, and whether the candidate is accepted.
//!
//! the measurement is the verifier's (`verify::compare_glyphs`): glyphs
//! with text are paired in reading order and their distance combines x and
//! y. a candidate is accepted only if every one of its glyphs was found on
//! its page within `THRESHOLD_MAX_PT`. a line that broke at another word
//! than the source moves its words by a line pitch or a line width, far
//! past the threshold, so "every glyph within the threshold" also proves
//! that every line broke at the same word.

use std::collections::HashMap;

use serde::Serialize;

use crate::pdf::PageContent;
use crate::verify::matching::matched_pairs;
use crate::verify::{placed_glyphs, THRESHOLD_MAX_PT};

/// how much further on average a promoted paragraph may be than round
/// one's lines: a tenth of a pixel. measured on the syllabus, word's export
/// of a promotion at the natural level converted again drifted by up to
/// 0.181 pt from the pdf in the second generation, against 0.124 in the
/// first: promotions only as close as round one keep repeated conversion
/// from piling up errors.
pub const MEAN_SLACK_PT: f64 = 0.012;

/// a source glyph: its page and its order on the page.
pub type GlyphKey = (usize, usize);

/// the glyphs the verifier measures (visible text that isn't white space).
pub fn measured(glyph: &crate::pdf::Glyph) -> bool {
    !glyph.text.is_empty() && !glyph.text.chars().all(char::is_whitespace)
}

/// every measured source glyph's distance to where word put it.
#[derive(Clone, Debug, Default)]
pub struct Distances {
    pub by_glyph: HashMap<GlyphKey, f64>,
    /// word's position minus the source's, x and y, for the report
    pub offsets: HashMap<GlyphKey, (f64, f64)>,
}

impl Distances {
    /// pairs the source with word's pdf exactly as the verifier does.
    pub fn measure(source: &[PageContent], word: &[PageContent]) -> Self {
        let keys: Vec<GlyphKey> = source
            .iter()
            .flat_map(|page| page.glyphs.iter())
            .filter(|g| measured(g))
            .map(|g| (g.page, g.sequence))
            .collect();
        let a = placed_glyphs(source);
        let b = placed_glyphs(word);
        let texts_a: Vec<&str> = a.iter().map(|g| g.text.as_str()).collect();
        let texts_b: Vec<&str> = b.iter().map(|g| g.text.as_str()).collect();
        let mut by_glyph = HashMap::new();
        let mut offsets = HashMap::new();
        for (i, j) in matched_pairs(&texts_a, &texts_b) {
            if a[i].page == b[j].page {
                let (dx, dy) = (b[j].x - a[i].x, b[j].y - a[i].y);
                by_glyph.insert(keys[i], dx.hypot(dy));
                offsets.insert(keys[i], (dx, dy));
            }
        }
        Self { by_glyph, offsets }
    }

    /// the mean over every measured glyph found (the verifier's mean).
    pub fn mean(&self) -> f64 {
        if self.by_glyph.is_empty() {
            return 0.0;
        }
        self.by_glyph.values().sum::<f64>() / self.by_glyph.len() as f64
    }
}

/// what the export says about one candidate.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub glyphs: usize,
    /// glyphs word didn't print on their page (or printed as other text)
    pub missing: usize,
    /// glyphs the verifier can't pair in round one's export either
    pub unpaired_anyway: usize,
    pub max_pt: f64,
    pub mean_pt: f64,
    /// positions within the candidate of the lines with a glyph off or missing
    pub failing_lines: Vec<usize>,
    /// x and y offset of the furthest glyph, points
    pub worst_offset: (f64, f64),
    /// the mean of the same glyphs in round one's positioned lines
    pub locked_mean_pt: Option<f64>,
}

impl Verdict {
    /// every glyph found within the threshold, and on average no further
    /// from the source than round one's lines put them (so converting
    /// word's own export again reproduces it instead of drifting).
    pub fn accepted(&self) -> bool {
        let as_close = self
            .locked_mean_pt
            .is_none_or(|locked| self.mean_pt <= locked + MEAN_SLACK_PT);
        self.glyphs > 0 && self.missing == 0 && self.max_pt <= THRESHOLD_MAX_PT && as_close
    }
}

/// judges a candidate from the measured glyphs of each of its lines.
pub fn judge(
    lines: &[Vec<GlyphKey>],
    distances: &Distances,
    locked: Option<&Distances>,
) -> Verdict {
    let mut verdict = Verdict::default();
    let mut sum = 0.0;
    for (position, keys) in lines.iter().enumerate() {
        let mut failing = false;
        for key in keys {
            verdict.glyphs += 1;
            match distances.by_glyph.get(key) {
                Some(&d) => {
                    sum += d;
                    if d > verdict.max_pt {
                        verdict.max_pt = d;
                        verdict.worst_offset =
                            distances.offsets.get(key).copied().unwrap_or_default();
                    }
                    failing |= d > THRESHOLD_MAX_PT;
                }
                // a glyph round one's lines lose too (a ligature written as a
                // private use char reads back as other text) is no proof
                // against the candidate
                None if unpaired_anyway(key, locked) => verdict.unpaired_anyway += 1,
                None => {
                    verdict.missing += 1;
                    failing = true;
                }
            }
        }
        if failing {
            verdict.failing_lines.push(position);
        }
    }
    let found = verdict.glyphs - verdict.missing - verdict.unpaired_anyway;
    verdict.mean_pt = if found > 0 { sum / found as f64 } else { 0.0 };
    verdict.locked_mean_pt = locked.map(|locked| mean_of(lines, locked));
    verdict
}

/// round one's export lost this glyph as well.
fn unpaired_anyway(key: &GlyphKey, locked: Option<&Distances>) -> bool {
    locked.is_some_and(|locked| !locked.by_glyph.contains_key(key))
}

/// the mean distance of these glyphs in another export (missing ones left out).
fn mean_of(lines: &[Vec<GlyphKey>], distances: &Distances) -> f64 {
    let found: Vec<f64> = lines
        .iter()
        .flatten()
        .filter_map(|key| distances.by_glyph.get(key).copied())
        .collect();
    if found.is_empty() {
        return f64::INFINITY;
    }
    found.iter().sum::<f64>() / found.len() as f64
}

/// the glyphs that would bring the document mean back under the target
/// if they returned to the positions `fallback` gives them: candidates are
/// demoted, worst first, until the estimated mean is under `target`.
/// returns the indexes of the candidates to demote.
pub fn demote_for_mean(
    candidates: &[Vec<GlyphKey>],
    current: &Distances,
    fallback: &Distances,
    target: f64,
) -> Vec<usize> {
    let count = current.by_glyph.len().max(1) as f64;
    let mut total: f64 = current.by_glyph.values().sum();
    // what each candidate adds over its fallback positions
    let mut excess: Vec<(f64, usize)> = candidates
        .iter()
        .enumerate()
        .map(|(index, keys)| {
            let extra: f64 = keys
                .iter()
                .map(|k| {
                    current.by_glyph.get(k).copied().unwrap_or(0.0)
                        - fallback.by_glyph.get(k).copied().unwrap_or(0.0)
                })
                .sum();
            (extra, index)
        })
        .collect();
    excess.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut demoted = Vec::new();
    for (extra, index) in excess {
        if total / count < target || extra <= 0.0 {
            break;
        }
        total -= extra;
        demoted.push(index);
    }
    demoted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distances(values: &[(GlyphKey, f64)]) -> Distances {
        Distances {
            by_glyph: values.iter().copied().collect(),
            offsets: HashMap::new(),
        }
    }

    #[test]
    fn a_candidate_within_the_threshold_is_accepted() {
        // a two-line paragraph on the lattice floor (max 0.074 pt, the syllabus' floor)
        let found = distances(&[
            ((0, 1), 0.031),
            ((0, 2), 0.059),
            ((0, 3), 0.074),
            ((0, 4), 0.012),
        ]);
        let verdict = judge(&[vec![(0, 1), (0, 2)], vec![(0, 3), (0, 4)]], &found, None);
        assert!(verdict.accepted());
        assert_eq!(verdict.failing_lines, Vec::<usize>::new());
        assert!((verdict.max_pt - 0.074).abs() < 1e-12);
    }

    #[test]
    fn a_wrong_break_fails_the_lines_it_moved() {
        // the last word of line 0 went to line 1: a line pitch off
        let found = distances(&[((0, 1), 0.03), ((0, 2), 13.8), ((0, 3), 0.04)]);
        let verdict = judge(&[vec![(0, 1), (0, 2)], vec![(0, 3)]], &found, None);
        assert!(!verdict.accepted());
        assert_eq!(verdict.failing_lines, [0]);
    }

    #[test]
    fn a_promotion_further_off_than_round_one_is_refused() {
        // within 0.125 pt, but twice as far as round one's lines on average
        let found = distances(&[((0, 1), 0.10), ((0, 2), 0.06)]);
        let locked = distances(&[((0, 1), 0.04), ((0, 2), 0.02)]);
        let lines = [vec![(0, 1), (0, 2)]];
        assert!(!judge(&lines, &found, Some(&locked)).accepted());
        assert!(judge(&lines, &found, None).accepted());
        let close = distances(&[((0, 1), 0.04), ((0, 2), 0.03)]);
        assert!(judge(&lines, &close, Some(&locked)).accepted());
    }

    #[test]
    fn a_glyph_round_one_also_loses_is_not_held_against_a_candidate() {
        // (0, 2) is a ligature: the verifier pairs it in neither export
        let found = distances(&[((0, 1), 0.03)]);
        let locked = distances(&[((0, 1), 0.03)]);
        let verdict = judge(&[vec![(0, 1), (0, 2)]], &found, Some(&locked));
        assert_eq!((verdict.missing, verdict.unpaired_anyway), (0, 1));
        assert!(verdict.accepted());
    }

    #[test]
    fn a_missing_glyph_fails() {
        let found = distances(&[((0, 1), 0.03)]);
        let verdict = judge(&[vec![(0, 1), (0, 2)]], &found, None);
        assert_eq!(verdict.missing, 1);
        assert!(!verdict.accepted());
    }

    #[test]
    fn the_worst_candidates_are_demoted_until_the_mean_passes() {
        let keys = |page: usize| -> Vec<GlyphKey> { (0..10).map(|i| (page, i)).collect() };
        let candidates = [keys(0), keys(1)];
        let mut current = HashMap::new();
        let mut fallback = HashMap::new();
        for i in 0..10 {
            current.insert((0, i), 0.08);
            current.insert((1, i), 0.05);
            fallback.insert((0, i), 0.03);
            fallback.insert((1, i), 0.03);
        }
        let current = Distances {
            by_glyph: current,
            offsets: HashMap::new(),
        };
        let fallback = Distances {
            by_glyph: fallback,
            offsets: HashMap::new(),
        };
        // mean 0.065: demoting candidate 0 brings it to 0.04
        assert_eq!(demote_for_mean(&candidates, &current, &fallback, 0.05), [0]);
        assert!(demote_for_mean(&candidates, &current, &fallback, 0.07).is_empty());
    }
}
