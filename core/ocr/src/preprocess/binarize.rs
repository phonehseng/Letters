//! deciding for every pixel whether it is ink (black) or paper (white).
//!
//! one threshold for the whole page fails on photos and on scans with a
//! shadow: the dark corner turns black and faint text in the light part
//! turns white. sauvola's method instead compares each pixel with its own
//! neighbourhood: a pixel is ink when it is clearly darker than the average
//! around it, where "clearly" depends on how much the neighbourhood varies.

use image::GrayImage;

/// how strongly local variation lowers the threshold. 0.2 to 0.5 are the
/// usual choices; lower keeps more faint ink.
const SENSITIVITY: f64 = 0.25;

/// the largest spread gray levels can have, which the method divides by.
const DYNAMIC_RANGE: f64 = 128.0;

/// below this spread a neighbourhood is flat: all paper, or all inside a
/// thick stroke. sauvola cannot tell those apart, so a flat pixel is ink
/// only when it is darker than both the page-wide threshold and
/// `DARK_ENOUGH`: paper in a shadow can be quite gray, ink is darker still.
const FLAT_SPREAD: f64 = 12.0;
const DARK_ENOUGH: f64 = 96.0;

/// the neighbourhood is about 1/6 inch across: wider than a thick stroke
/// of body text, narrower than a shadow's gradual change.
fn window_radius(dpi: f32) -> u32 {
    ((dpi / 12.0).round() as u32).max(3)
}

/// returns a picture with only black (0) and white (255) pixels.
pub fn sauvola(image: &GrayImage, dpi: f32) -> GrayImage {
    let (width, height) = image.dimensions();
    let radius = window_radius(dpi) as i64;
    let flat_threshold = f64::from(otsu_threshold(image)).min(DARK_ENOUGH);
    let source = image.as_raw();
    let mut output = GrayImage::new(width, height);
    let (w, h) = (width as i64, height as i64);

    // sums of the values, and of their squares, down each column within
    // the window rows. moving to the next row adds one row and drops one,
    // so memory stays at one row's worth however tall the page is.
    let mut column_sum = vec![0u64; width as usize];
    let mut column_squares = vec![0u64; width as usize];
    let add_row = |row: i64, sign: i64, sums: &mut [u64], squares: &mut [u64]| {
        if row < 0 || row >= h {
            return;
        }
        let start = (row * w) as usize;
        for (x, &value) in source[start..start + width as usize].iter().enumerate() {
            let value = u64::from(value);
            if sign > 0 {
                sums[x] += value;
                squares[x] += value * value;
            } else {
                sums[x] -= value;
                squares[x] -= value * value;
            }
        }
    };
    for row in 0..radius.min(h) {
        add_row(row, 1, &mut column_sum, &mut column_squares);
    }

    for y in 0..h {
        add_row(y + radius, 1, &mut column_sum, &mut column_squares);
        add_row(y - radius - 1, -1, &mut column_sum, &mut column_squares);
        let rows = (y + radius).min(h - 1) - (y - radius).max(0) + 1;

        // the same sliding trick across the row
        let mut sum: u64 = 0;
        let mut squares: u64 = 0;
        for x in 0..radius.min(w) {
            sum += column_sum[x as usize];
            squares += column_squares[x as usize];
        }
        for x in 0..w {
            let entering = x + radius;
            if entering < w {
                sum += column_sum[entering as usize];
                squares += column_squares[entering as usize];
            }
            let leaving = x - radius - 1;
            if leaving >= 0 {
                sum -= column_sum[leaving as usize];
                squares -= column_squares[leaving as usize];
            }
            let columns = (x + radius).min(w - 1) - (x - radius).max(0) + 1;
            let count = (rows * columns) as f64;
            let mean = sum as f64 / count;
            let spread = (squares as f64 / count - mean * mean).max(0.0).sqrt();
            let value = f64::from(source[(y * w + x) as usize]);
            let ink = if spread < FLAT_SPREAD {
                value < flat_threshold
            } else {
                value < mean * (1.0 + SENSITIVITY * (spread / DYNAMIC_RANGE - 1.0))
            };
            output.put_pixel(x as u32, y as u32, image::Luma([if ink { 0 } else { 255 }]));
        }
    }
    output
}

/// otsu's threshold: the gray level that best splits the picture's
/// histogram into two groups (ink and paper).
pub(crate) fn otsu_threshold(image: &GrayImage) -> u8 {
    let mut histogram = [0u64; 256];
    for pixel in image.pixels() {
        histogram[pixel.0[0] as usize] += 1;
    }
    let total: u64 = histogram.iter().sum();
    let weighted_total: f64 = histogram
        .iter()
        .enumerate()
        .map(|(level, &count)| level as f64 * count as f64)
        .sum();
    let mut best = (0.0, 128u8);
    let mut below = 0u64;
    let mut weighted_below = 0.0;
    for (level, &count) in histogram.iter().enumerate() {
        below += count;
        weighted_below += level as f64 * count as f64;
        let above = total - below;
        if below == 0 || above == 0 {
            continue;
        }
        let mean_below = weighted_below / below as f64;
        let mean_above = (weighted_total - weighted_below) / above as f64;
        let between = below as f64 * above as f64 * (mean_below - mean_above).powi(2);
        if between > best.0 {
            // the threshold sits just above the darker group
            best = (between, (level + 1).min(255) as u8);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn otsu_splits_two_groups() {
        let mut image = GrayImage::from_pixel(10, 10, Luma([200]));
        for x in 0..10 {
            image.put_pixel(x, 0, Luma([40]));
        }
        let threshold = otsu_threshold(&image);
        assert!(threshold > 40 && threshold <= 200, "{threshold}");
    }

    #[test]
    fn faint_text_in_a_shadow_and_in_the_light_both_survive() {
        // paper fades from light (230) on the left to dark (110) on the
        // right, and "ink" is 60 levels darker than the paper around it
        let (width, height) = (300, 60);
        let mut image = GrayImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let paper = 230.0 - 120.0 * x as f32 / width as f32;
            let in_stroke = (20..26).contains(&y) && x % 20 < 6;
            let value = if in_stroke { paper - 60.0 } else { paper };
            *pixel = Luma([value as u8]);
        }
        let binary = sauvola(&image, 150.0);
        // strokes are black at both ends of the page
        assert_eq!(binary.get_pixel(2, 22).0[0], 0);
        assert_eq!(binary.get_pixel(282, 22).0[0], 0);
        // paper is white at both ends, even the dark right side
        assert_eq!(binary.get_pixel(12, 50).0[0], 255);
        assert_eq!(binary.get_pixel(292, 50).0[0], 255);
    }

    #[test]
    fn the_inside_of_a_thick_stroke_stays_black() {
        let mut image = GrayImage::from_pixel(200, 200, Luma([250]));
        for y in 40..160 {
            for x in 40..160 {
                image.put_pixel(x, y, Luma([10]));
            }
        }
        let binary = sauvola(&image, 300.0);
        assert_eq!(binary.get_pixel(100, 100).0[0], 0);
        assert_eq!(binary.get_pixel(10, 10).0[0], 255);
    }

    #[test]
    fn the_result_is_only_black_and_white() {
        let image = GrayImage::from_fn(64, 64, |x, y| Luma([((x * 4 + y) % 256) as u8]));
        let binary = sauvola(&image, 300.0);
        assert!(binary.pixels().all(|pixel| matches!(pixel.0[0], 0 | 255)));
    }
}
