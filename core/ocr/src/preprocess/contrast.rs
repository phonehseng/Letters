//! stretching a faint scan so its darkest ink is black and its paper white.

use image::GrayImage;

/// the share of pixels allowed to end up fully black or fully white.
/// ignoring the extreme half percent at each end keeps a single speck of
/// pure black or a shiny spot from deciding the stretch.
const CLIP_SHARE: f32 = 0.005;

/// a picture whose ink and paper are closer than this is left alone: it is
/// blank, or a photo of something that isn't text, and stretching would
/// only amplify noise.
const SMALLEST_RANGE: u8 = 32;

/// stretches the gray levels in place. returns whether anything changed.
pub fn normalize_contrast(image: &mut GrayImage) -> bool {
    let (dark, light) = clipped_range(image);
    if light <= dark || light - dark < SMALLEST_RANGE || (dark == 0 && light == 255) {
        return false;
    }
    let scale = 255.0 / f32::from(light - dark);
    let table: Vec<u8> = (0..=255u8)
        .map(|value| {
            let stretched = (f32::from(value.saturating_sub(dark))) * scale;
            stretched.round().min(255.0) as u8
        })
        .collect();
    for pixel in image.pixels_mut() {
        pixel.0[0] = table[pixel.0[0] as usize];
    }
    true
}

/// the gray levels below which and above which `CLIP_SHARE` of pixels lie.
fn clipped_range(image: &GrayImage) -> (u8, u8) {
    let mut histogram = [0u64; 256];
    for pixel in image.pixels() {
        histogram[pixel.0[0] as usize] += 1;
    }
    let total = (image.width() as u64 * image.height() as u64) as f32;
    let limit = (total * CLIP_SHARE) as u64;
    let level_after = |levels: &mut dyn Iterator<Item = usize>| {
        let mut seen = 0;
        for level in levels {
            seen += histogram[level];
            if seen > limit {
                return level as u8;
            }
        }
        0
    };
    let dark = level_after(&mut (0..256));
    let light = level_after(&mut (0..256).rev());
    (dark, light)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn a_faint_scan_is_stretched_to_black_and_white() {
        // gray "ink" at 100 on gray "paper" at 180
        let mut image = GrayImage::from_pixel(10, 10, Luma([180]));
        for x in 0..10 {
            image.put_pixel(x, 5, Luma([100]));
        }
        assert!(normalize_contrast(&mut image));
        assert_eq!(image.get_pixel(0, 5).0[0], 0);
        assert_eq!(image.get_pixel(0, 0).0[0], 255);
    }

    #[test]
    fn a_blank_page_is_left_alone() {
        let mut image = GrayImage::from_pixel(10, 10, Luma([240]));
        image.put_pixel(2, 2, Luma([230]));
        assert!(!normalize_contrast(&mut image));
        assert_eq!(image.get_pixel(0, 0).0[0], 240);
    }

    #[test]
    fn a_clean_page_needs_no_stretch() {
        let mut image = GrayImage::from_pixel(10, 10, Luma([255]));
        for x in 0..10 {
            image.put_pixel(x, 5, Luma([0]));
        }
        assert!(!normalize_contrast(&mut image));
    }
}
