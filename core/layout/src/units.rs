//! lengths: the model's twips and half points, the result's points, and
//! word's 1/600 inch layout pixels.

/// points in one twip (a twentieth of a point).
pub const POINTS_PER_TWIP: f64 = 1.0 / 20.0;

/// word lays text out on 1/600 inch pixels: 0.12 pt (recipe 6, measured
/// on word 16.0.20527 with the default 600 dpi printer).
pub const WORD_PIXEL: f64 = 0.12;

pub fn twips_to_points(twips: i32) -> f64 {
    f64::from(twips) * POINTS_PER_TWIP
}

pub fn half_points_to_points(half_points: u32) -> f64 {
    f64::from(half_points) / 2.0
}

/// word's rule for turning a twip position into a layout pixel (recipe
/// 6.1, held for 85,571 measured frames): `floor((5t + 1) / 12)`, one
/// lower when `5t + 1` is a multiple of 12 and `t mod 60` is 19 or 55.
pub fn word_twips_to_pixel(twips: i64) -> i64 {
    let value = 5 * twips + 1;
    let mut pixel = value.div_euclid(12);
    if value.rem_euclid(12) == 0 && matches!(twips.rem_euclid(60), 19 | 55) {
        pixel -= 1;
    }
    pixel
}

/// a length in points rounded to the nearest word pixel, in points.
pub fn snap_to_word_pixel(points: f64) -> f64 {
    (points / WORD_PIXEL).round() * WORD_PIXEL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twips_pixels_follow_the_recipe() {
        // 60 twips are exactly 25 pixels
        assert_eq!(word_twips_to_pixel(60), 25);
        assert_eq!(word_twips_to_pixel(1440), 600);
        // the two edge cases that round down
        assert_eq!(word_twips_to_pixel(19), 7);
        assert_eq!(word_twips_to_pixel(55), 22);
        // and one that rounds up (t mod 60 = 7)
        assert_eq!(word_twips_to_pixel(7), 3);
    }

    #[test]
    fn units_convert() {
        assert_eq!(twips_to_points(1440), 72.0);
        assert_eq!(half_points_to_points(23), 11.5);
        assert!((snap_to_word_pixel(80.57) - 80.52).abs() < 1e-9);
    }
}
