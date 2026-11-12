// shared by the fidelity and back-and-forth tests, which each use part of it
#![allow(dead_code)]

//! scores how close a candidate pdf page is to a reference page: lines
//! broken at the same words, baseline and start differences of those
//! lines, how far shape and picture boxes are from the reference's, and
//! the structural similarity (ssim) of the rendered pixels.

use super::pdf_reader::{Glyph, Page};

/// one text line: its characters without spaces, baseline and start.
#[derive(Clone, Debug)]
pub struct Line {
    pub text: String,
    pub baseline: f64,
    pub left: f64,
}

/// glyphs on the same baseline that follow each other left to right
/// form a line (content order keeps each shape's text together).
pub fn lines(glyphs: &[Glyph]) -> Vec<Line> {
    let mut found: Vec<Line> = Vec::new();
    let mut last: Option<&Glyph> = None;
    for glyph in glyphs {
        let continues = last.is_some_and(|previous| {
            (previous.y - glyph.y).abs() < 0.6
                && glyph.x > previous.x - 0.5
                && glyph.x - previous.x < previous.size * 3.0
        });
        if continues {
            if let Some(line) = found.last_mut() {
                line.text.push(glyph.ch);
            }
        } else {
            found.push(Line {
                text: glyph.ch.to_string(),
                baseline: glyph.y,
                left: glyph.x,
            });
        }
        last = Some(glyph);
    }
    found
}

#[derive(Clone, Debug, Default)]
pub struct Score {
    pub reference_lines: usize,
    /// reference lines found with the same text in the candidate
    pub matched_lines: usize,
    pub baseline_difference: f64,
    pub left_difference: f64,
    pub reference_boxes: usize,
    pub box_difference: f64,
    pub ssim: f64,
}

impl Score {
    pub fn line_agreement(&self) -> f64 {
        if self.reference_lines == 0 {
            1.0
        } else {
            self.matched_lines as f64 / self.reference_lines as f64
        }
    }
}

/// errors larger than this mean "not found" and are counted as this
const BOX_ERROR_CAP: f64 = 50.0;

pub fn score(reference: &Page, candidate: &Page) -> Score {
    let reference_lines = lines(&reference.glyphs);
    let candidate_lines = lines(&candidate.glyphs);
    let mut used = vec![false; candidate_lines.len()];
    let mut matched = 0;
    let mut baseline = 0.0;
    let mut left = 0.0;
    for line in &reference_lines {
        let found = candidate_lines
            .iter()
            .enumerate()
            .filter(|(index, other)| !used[*index] && other.text == line.text)
            .min_by(|a, b| {
                (a.1.baseline - line.baseline)
                    .abs()
                    .total_cmp(&(b.1.baseline - line.baseline).abs())
            });
        if let Some((index, other)) = found {
            used[index] = true;
            matched += 1;
            baseline += (other.baseline - line.baseline).abs();
            left += (other.left - line.left).abs();
        }
    }
    let average = |total: f64| {
        if matched == 0 {
            0.0
        } else {
            total / matched as f64
        }
    };
    Score {
        reference_lines: reference_lines.len(),
        matched_lines: matched,
        baseline_difference: average(baseline),
        left_difference: average(left),
        reference_boxes: reference.boxes.len(),
        box_difference: box_difference(&reference.boxes, &candidate.boxes),
        ssim: ssim(reference, candidate),
    }
}

/// for each reference box, the mean edge distance to the nearest
/// candidate box; averaged.
fn box_difference(reference: &[[f64; 4]], candidate: &[[f64; 4]]) -> f64 {
    if reference.is_empty() {
        return 0.0;
    }
    let total: f64 = reference
        .iter()
        .map(|a| {
            candidate
                .iter()
                .map(|b| (0..4).map(|i| (a[i] - b[i]).abs()).sum::<f64>() / 4.0)
                .fold(BOX_ERROR_CAP, f64::min)
        })
        .sum();
    total / reference.len() as f64
}

/// mean ssim over 8 by 8 windows (wang et al. 2004, the usual constants).
pub fn ssim(a: &Page, b: &Page) -> f64 {
    let width = a.pixel_width.min(b.pixel_width) as usize;
    let height = a.pixel_height.min(b.pixel_height) as usize;
    if width < 8 || height < 8 {
        return 0.0;
    }
    let c1 = (0.01f64 * 255.0).powi(2);
    let c2 = (0.03f64 * 255.0).powi(2);
    let mut total = 0.0;
    let mut windows = 0;
    for top in (0..height - 7).step_by(8) {
        for left in (0..width - 7).step_by(8) {
            let window = |page: &Page| {
                let mut values = [0.0f64; 64];
                for (index, value) in values.iter_mut().enumerate() {
                    let (x, y) = (left + index % 8, top + index / 8);
                    *value = f64::from(page.pixels[y * page.pixel_width as usize + x]);
                }
                values
            };
            let (x, y) = (window(a), window(b));
            let mean_x = x.iter().sum::<f64>() / 64.0;
            let mean_y = y.iter().sum::<f64>() / 64.0;
            let variance_x = x.iter().map(|v| (v - mean_x).powi(2)).sum::<f64>() / 63.0;
            let variance_y = y.iter().map(|v| (v - mean_y).powi(2)).sum::<f64>() / 63.0;
            let covariance = x
                .iter()
                .zip(&y)
                .map(|(p, q)| (p - mean_x) * (q - mean_y))
                .sum::<f64>()
                / 63.0;
            total += ((2.0 * mean_x * mean_y + c1) * (2.0 * covariance + c2))
                / ((mean_x * mean_x + mean_y * mean_y + c1) * (variance_x + variance_y + c2));
            windows += 1;
        }
    }
    total / f64::from(windows.max(1))
}
