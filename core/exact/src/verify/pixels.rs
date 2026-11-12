//! comparing two renderings of a page: mean difference, pixels that moved,
//! and the structural similarity (ssim) as scikit-image computes it by
//! default (7 x 7 uniform window, sample covariance, k1 0.01, k2 0.03).

use serde::Serialize;

/// pixel differences below this are antialiasing noise rather than a moved edge.
const NOISE_LEVEL: u8 = 32;

/// one page's comparison.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageComparison {
    pub page: usize,
    pub ssim: f64,
    pub mean_abs_diff: f64,
    pub pixels_over_noise: usize,
    pub ink_pixels: usize,
}

/// a grayscale image, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Gray {
    /// from rgba, composited on white (luma as pil converts it).
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        let pixels = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let alpha = u32::from(p[3]);
                let on_white = |c: u8| (u32::from(c) * alpha + 255 * (255 - alpha)) / 255;
                ((299 * on_white(p[0]) + 587 * on_white(p[1]) + 114 * on_white(p[2])) / 1000) as u8
            })
            .collect();
        Self {
            width: width as usize,
            height: height as usize,
            pixels,
        }
    }

    fn padded(&self, width: usize, height: usize) -> Vec<f64> {
        let mut out = vec![255.0; width * height];
        for y in 0..self.height.min(height) {
            for x in 0..self.width.min(width) {
                out[y * width + x] = f64::from(self.pixels[y * self.width + x]);
            }
        }
        out
    }
}

/// window sums of a w x h image over `size` x `size` windows, via an integral image.
fn window_means(values: &[f64], width: usize, height: usize, size: usize) -> Vec<f64> {
    let mut integral = vec![0.0; (width + 1) * (height + 1)];
    for y in 0..height {
        let mut row = 0.0;
        for x in 0..width {
            row += values[y * width + x];
            integral[(y + 1) * (width + 1) + x + 1] = integral[y * (width + 1) + x + 1] + row;
        }
    }
    let (out_w, out_h) = (width + 1 - size, height + 1 - size);
    let area = (size * size) as f64;
    let mut means = vec![0.0; out_w * out_h];
    for y in 0..out_h {
        for x in 0..out_w {
            let at = |yy: usize, xx: usize| integral[yy * (width + 1) + xx];
            let sum = at(y + size, x + size) - at(y, x + size) - at(y + size, x) + at(y, x);
            means[y * out_w + x] = sum / area;
        }
    }
    means
}

/// mean ssim over every full 7 x 7 window.
pub fn ssim(a: &[f64], b: &[f64], width: usize, height: usize) -> f64 {
    const SIZE: usize = 7;
    if width < SIZE || height < SIZE {
        return 1.0;
    }
    let products = |p: &[f64], q: &[f64]| p.iter().zip(q).map(|(x, y)| x * y).collect::<Vec<f64>>();
    let mean_a = window_means(a, width, height, SIZE);
    let mean_b = window_means(b, width, height, SIZE);
    let mean_aa = window_means(&products(a, a), width, height, SIZE);
    let mean_bb = window_means(&products(b, b), width, height, SIZE);
    let mean_ab = window_means(&products(a, b), width, height, SIZE);
    let count = (SIZE * SIZE) as f64;
    let sample = count / (count - 1.0);
    let (c1, c2) = ((0.01 * 255.0f64).powi(2), (0.03 * 255.0f64).powi(2));
    let mut total = 0.0;
    for i in 0..mean_a.len() {
        let (ua, ub) = (mean_a[i], mean_b[i]);
        let va = sample * (mean_aa[i] - ua * ua);
        let vb = sample * (mean_bb[i] - ub * ub);
        let cov = sample * (mean_ab[i] - ua * ub);
        total +=
            ((2.0 * ua * ub + c1) * (2.0 * cov + c2)) / ((ua * ua + ub * ub + c1) * (va + vb + c2));
    }
    total / mean_a.len() as f64
}

/// compares two renderings of one page.
pub fn compare_pages(page: usize, source: &Gray, candidate: &Gray) -> PageComparison {
    let (width, height) = (
        source.width.max(candidate.width),
        source.height.max(candidate.height),
    );
    let a = source.padded(width, height);
    let b = candidate.padded(width, height);
    let mut total = 0.0;
    let (mut moved, mut ink) = (0, 0);
    for (x, y) in a.iter().zip(&b) {
        let diff = (x - y).abs();
        total += diff;
        moved += usize::from(diff > f64::from(NOISE_LEVEL));
        ink += usize::from(*x < f64::from(255 - NOISE_LEVEL) || *y < f64::from(255 - NOISE_LEVEL));
    }
    PageComparison {
        page,
        ssim: ssim(&a, &b, width, height),
        mean_abs_diff: total / a.len().max(1) as f64,
        pixels_over_noise: moved,
        ink_pixels: ink,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_pages_have_ssim_one() {
        let pixels: Vec<u8> = (0..400).map(|i| (i * 7 % 256) as u8).collect();
        let page = Gray {
            width: 20,
            height: 20,
            pixels,
        };
        let result = compare_pages(0, &page, &page);
        assert!((result.ssim - 1.0).abs() < 1e-12);
        assert_eq!(result.pixels_over_noise, 0);
    }

    #[test]
    fn a_shifted_page_scores_lower() {
        let make = |offset: usize| Gray {
            width: 30,
            height: 30,
            pixels: (0..900)
                .map(|i| if (i % 30 + offset) % 6 < 3 { 0 } else { 255 })
                .collect(),
        };
        let result = compare_pages(0, &make(0), &make(1));
        assert!(result.ssim < 0.9);
        assert!(result.pixels_over_noise > 0);
    }

    #[test]
    fn transparent_is_white() {
        let gray = Gray::from_rgba(1, 1, &[0, 0, 0, 0]);
        assert_eq!(gray.pixels, [255]);
    }
}
