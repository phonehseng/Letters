//! straightening a crooked page.
//!
//! finding the tilt: imagine casting the ink of the page sideways onto a
//! ruler. when the ruler is lined up with the text lines, the ink piles up
//! into sharp peaks (the lines) with empty gaps between them; at any other
//! angle the peaks smear into each other. so we try angles and keep the
//! one whose pile is "peakiest", measured as the sum of the squared heights.
//! this is the projection-profile method, done on a small black and white
//! copy of the page because the answer doesn't need every pixel.

use image::{GrayImage, Luma};

use super::binarize::otsu_threshold;
use crate::geometry::{Affine, Point};

/// the steepest tilt looked for, in degrees either way. scans are rarely
/// off by more than a few degrees; beyond this a page is probably meant
/// to be at an angle (or is sideways, which orientation handles).
const LARGEST_SKEW: f32 = 15.0;
const COARSE_STEP: f32 = 0.5;
const FINE_STEP: f32 = 0.05;

/// the working copy is reduced to about this resolution. a text line is
/// still several pixels tall, and the search runs in a few milliseconds.
const WORKING_DPI: f32 = 100.0;

/// at most this many ink pixels are cast per angle; more only cost time.
const MOST_SAMPLES: usize = 250_000;

/// a page with fewer ink pixels than this has no text to line up with.
const FEWEST_SAMPLES: usize = 200;

/// how far the text leans, in degrees: positive when lines rise to the
/// right (turned anticlockwise), negative when they fall.
pub fn estimate_skew(image: &GrayImage, dpi: f32) -> f32 {
    let samples = ink_samples(image, dpi);
    if samples.len() < FEWEST_SAMPLES {
        return 0.0;
    }
    let coarse = best_angle(&samples, -LARGEST_SKEW, LARGEST_SKEW, COARSE_STEP);
    let fine = best_angle(
        &samples,
        coarse - COARSE_STEP,
        coarse + COARSE_STEP,
        FINE_STEP,
    );
    (fine * 100.0).round() / 100.0
}

/// the ink pixels of a reduced black and white copy, as centre positions.
fn ink_samples(image: &GrayImage, dpi: f32) -> Vec<Point> {
    let factor = (dpi / WORKING_DPI).round().max(1.0) as u32;
    let small = shrink(image, factor);
    let threshold = otsu_threshold(&small);
    let mut samples: Vec<Point> = small
        .enumerate_pixels()
        .filter(|(_, _, pixel)| pixel.0[0] < threshold)
        .map(|(x, y, _)| Point::new(x as f32 + 0.5, y as f32 + 0.5))
        .collect();
    // a page that is mostly dark is a photo or a cover, not text on paper
    if samples.len() > (small.width() * small.height()) as usize / 2 {
        return Vec::new();
    }
    if samples.len() > MOST_SAMPLES {
        let keep_every = samples.len().div_ceil(MOST_SAMPLES);
        samples = samples.into_iter().step_by(keep_every).collect();
    }
    samples
}

/// averages each `factor` by `factor` block of pixels into one.
pub fn shrink(image: &GrayImage, factor: u32) -> GrayImage {
    if factor <= 1 {
        return image.clone();
    }
    let width = (image.width() / factor).max(1);
    let height = (image.height() / factor).max(1);
    GrayImage::from_fn(width, height, |x, y| {
        let mut sum = 0u32;
        let mut count = 0u32;
        for dy in 0..factor {
            for dx in 0..factor {
                let (sx, sy) = (x * factor + dx, y * factor + dy);
                if sx < image.width() && sy < image.height() {
                    sum += u32::from(image.get_pixel(sx, sy).0[0]);
                    count += 1;
                }
            }
        }
        Luma([(sum / count.max(1)) as u8])
    })
}

/// tries every angle from `from` to `to` in `step`s and returns the one
/// with the peakiest profile.
fn best_angle(samples: &[Point], from: f32, to: f32, step: f32) -> f32 {
    let steps = ((to - from) / step).round() as i32;
    let mut best = (f64::MIN, 0.0);
    for index in 0..=steps {
        let angle = from + index as f32 * step;
        let score = profile_peakiness(samples, angle);
        if score > best.0 {
            best = (score, angle);
        }
    }
    best.1
}

/// casts every sample onto the ruler at right angles to text leaning by
/// `degrees` and sums the squares of the pile heights.
fn profile_peakiness(samples: &[Point], degrees: f32) -> f64 {
    let (sin, cos) = degrees.to_radians().sin_cos();
    // along a line leaning anticlockwise by `degrees` (on a screen, where y
    // grows downward) this quantity stays the same, so a line lands in one bin
    let across = |point: &Point| point.x * sin + point.y * cos;
    let lowest = samples.iter().map(across).fold(f32::INFINITY, f32::min);
    let highest = samples.iter().map(across).fold(f32::NEG_INFINITY, f32::max);
    let mut bins = vec![0u32; (highest - lowest) as usize + 2];
    for point in samples {
        bins[(across(point) - lowest) as usize] += 1;
    }
    bins.iter().map(|&count| f64::from(count).powi(2)).sum()
}

/// turns the picture anticlockwise by `degrees` around its centre, onto a
/// canvas just big enough to hold all of it, filling new corners with white.
///
/// returns the turned picture and the map from a position in it back to
/// the same spot in the picture given. pixels are blended from their four
/// nearest neighbours (bilinear), which keeps letter edges smooth.
pub fn rotate(image: &GrayImage, degrees: f32) -> (GrayImage, Affine) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (width, height) = (image.width() as f32, image.height() as f32);
    // the small allowance stops rounding error in sin and cos from adding
    // a whole extra pixel (a quarter turn of 100 pixels is 100, not 101)
    let fit = |size: f32| (size - 1e-3).ceil().max(1.0);
    let new_width = fit(width * cos.abs() + height * sin.abs());
    let new_height = fit(width * sin.abs() + height * cos.abs());

    // turning back clockwise by `degrees` around the centres takes a spot
    // on the new canvas to the spot it came from
    let (new_centre_x, new_centre_y) = (new_width / 2.0, new_height / 2.0);
    let (old_centre_x, old_centre_y) = (width / 2.0, height / 2.0);
    let back = Affine::new(
        cos,
        sin,
        -sin,
        cos,
        old_centre_x - (cos * new_centre_x - sin * new_centre_y),
        old_centre_y - (sin * new_centre_x + cos * new_centre_y),
    );

    let turned = GrayImage::from_fn(new_width as u32, new_height as u32, |x, y| {
        let source = back.apply(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        Luma([bilinear(image, source.x - 0.5, source.y - 0.5)])
    });
    (turned, back)
}

/// the gray level at a position between pixel centres. outside the
/// picture is white paper.
fn bilinear(image: &GrayImage, x: f32, y: f32) -> u8 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |px: f32, py: f32| -> f32 {
        if px < 0.0 || py < 0.0 || px >= image.width() as f32 || py >= image.height() as f32 {
            255.0
        } else {
            f32::from(image.get_pixel(px as u32, py as u32).0[0])
        }
    };
    let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1.0, y0) * fx;
    let bottom = at(x0, y0 + 1.0) * (1.0 - fx) + at(x0 + 1.0, y0 + 1.0) * fx;
    (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a page of "text": dark bars like lines of words, leaning by `degrees`.
    fn striped_page(degrees: f32) -> GrayImage {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let (width, height) = (900.0, 1100.0);
        GrayImage::from_fn(900, 1100, |x, y| {
            // position measured from the centre, then turned back upright
            let (dx, dy) = (x as f32 - width / 2.0, y as f32 - height / 2.0);
            let upright_x = dx * cos - dy * sin;
            let upright_y = dx * sin + dy * cos;
            let in_line = upright_y.rem_euclid(40.0) < 12.0;
            // gaps between "words" so it isn't just solid bars
            let in_word = upright_x.rem_euclid(60.0) < 45.0;
            let inside = upright_x.abs() < 350.0 && upright_y.abs() < 450.0;
            Luma([if in_line && in_word && inside { 0 } else { 255 }])
        })
    }

    #[test]
    fn finds_known_tilts() {
        for degrees in [-7.0, -3.0, -1.0, 0.0, 1.0, 3.0, 7.0] {
            let found = estimate_skew(&striped_page(degrees), 100.0);
            assert!(
                (found - degrees).abs() <= 0.15,
                "made {degrees}, found {found}"
            );
        }
    }

    #[test]
    fn a_blank_page_has_no_tilt() {
        let blank = GrayImage::from_pixel(300, 300, Luma([255]));
        assert_eq!(estimate_skew(&blank, 300.0), 0.0);
    }

    #[test]
    fn rotating_back_straightens_the_page() {
        let page = striped_page(4.0);
        let (straight, _) = rotate(&page, -4.0);
        let left = estimate_skew(&straight, 100.0);
        assert!(left.abs() <= 0.15, "still leaning by {left}");
    }

    #[test]
    fn the_recorded_map_finds_the_original_pixel() {
        let mut page = GrayImage::from_pixel(200, 100, Luma([255]));
        page.put_pixel(150, 20, Luma([0]));
        let (turned, back) = rotate(&page, 10.0);
        // the darkest pixel of the turned picture should map back to (150, 20)
        let (x, y, _) = turned
            .enumerate_pixels()
            .min_by_key(|(_, _, pixel)| pixel.0[0])
            .unwrap();
        let source = back.apply(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        assert!(
            source.distance_to(Point::new(150.5, 20.5)) < 1.0,
            "{source:?}"
        );
    }

    #[test]
    fn the_canvas_grows_to_hold_the_corners() {
        let page = GrayImage::from_pixel(200, 100, Luma([255]));
        let (turned, _) = rotate(&page, 90.0);
        assert_eq!(turned.dimensions(), (100, 200));
    }
}
