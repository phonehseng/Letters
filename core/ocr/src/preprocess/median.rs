//! a 3 by 3 median filter: each pixel becomes the middle value of itself
//! and its eight neighbours.
//!
//! it removes isolated specks and scanner grain but, unlike blurring,
//! keeps the edges of letters sharp, because an edge pixel's middle value
//! is still either ink or paper.

use image::GrayImage;

/// how different a pixel must be from all four neighbours to count as a
/// lone speck, in gray levels.
const SPECK_CONTRAST: i16 = 96;

/// the share of lone specks above which a picture counts as noisy.
const NOISY_SHARE: f64 = 0.002;

/// whether the picture is grainy enough to be worth filtering.
///
/// the filter has a cost: a stroke one pixel thin (a hairline serif, or
/// small print scanned at low resolution) is wiped out by it. so it only
/// runs when many pixels stand out alone from all their neighbours, which
/// clean scans and rendered pages hardly ever have.
pub fn is_noisy(image: &GrayImage) -> bool {
    let (width, height) = image.dimensions();
    if width < 3 || height < 3 {
        return false;
    }
    let source = image.as_raw();
    let at = |x: u32, y: u32| i16::from(source[(y * width + x) as usize]);
    let mut specks = 0u64;
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let value = at(x, y);
            let stands_out = [at(x - 1, y), at(x + 1, y), at(x, y - 1), at(x, y + 1)]
                .iter()
                .all(|neighbour| (neighbour - value).abs() >= SPECK_CONTRAST);
            if stands_out {
                specks += 1;
            }
        }
    }
    specks as f64 / (f64::from(width - 2) * f64::from(height - 2)) > NOISY_SHARE
}

pub fn median3(image: &GrayImage) -> GrayImage {
    let (width, height) = image.dimensions();
    let mut output = GrayImage::new(width, height);
    let source = image.as_raw();
    let at = |x: i64, y: i64| {
        // the edge pixels are repeated outward
        let x = x.clamp(0, width as i64 - 1) as usize;
        let y = y.clamp(0, height as i64 - 1) as usize;
        source[y * width as usize + x]
    };
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let mut window = [0u8; 9];
            let mut slot = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    window[slot] = at(x + dx, y + dy);
                    slot += 1;
                }
            }
            let (_, middle, _) = window.select_nth_unstable(4);
            output.put_pixel(x as u32, y as u32, image::Luma([*middle]));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn a_lone_speck_disappears() {
        let mut image = GrayImage::from_pixel(7, 7, Luma([255]));
        image.put_pixel(3, 3, Luma([0]));
        let filtered = median3(&image);
        assert!(filtered.pixels().all(|pixel| pixel.0[0] == 255));
    }

    #[test]
    fn only_grainy_pictures_count_as_noisy() {
        let mut clean = GrayImage::from_pixel(50, 50, Luma([255]));
        for x in 5..45 {
            clean.put_pixel(x, 20, Luma([0]));
        }
        assert!(!is_noisy(&clean));
        let mut grainy = clean.clone();
        for index in 0..100u32 {
            // scattered specks, never next to each other
            grainy.put_pixel((index * 7) % 47 + 1, (index * 13) % 47 + 1, Luma([0]));
        }
        assert!(is_noisy(&grainy));
    }

    #[test]
    fn a_thick_stroke_survives_with_a_sharp_edge() {
        let mut image = GrayImage::from_pixel(9, 9, Luma([255]));
        for y in 0..9 {
            for x in 3..6 {
                image.put_pixel(x, y, Luma([0]));
            }
        }
        let filtered = median3(&image);
        assert_eq!(filtered.get_pixel(3, 4).0[0], 0);
        assert_eq!(filtered.get_pixel(5, 4).0[0], 0);
        assert_eq!(filtered.get_pixel(2, 4).0[0], 255);
        assert_eq!(filtered.get_pixel(6, 4).0[0], 255);
    }
}
