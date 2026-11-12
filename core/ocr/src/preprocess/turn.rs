//! turning a picture by whole quarter turns, for pages scanned sideways
//! or upside down. no pixel is blurred, they only move.

use image::{imageops, GrayImage};

use crate::geometry::Affine;

/// turns the picture clockwise by `quarter_turns` times 90 degrees.
///
/// returns the turned picture and the map from a position in it back to
/// the same spot in the picture given. positions are measured on pixel
/// edges, so (0, 0) is the outer corner of the top left pixel.
pub fn turn_clockwise(image: &GrayImage, quarter_turns: u32) -> (GrayImage, Affine) {
    let (width, height) = (image.width() as f32, image.height() as f32);
    match quarter_turns % 4 {
        // a quarter turn clockwise: the original's left edge becomes the top,
        // so a step right is a step up the old picture and a step down is
        // a step right
        1 => (
            imageops::rotate90(image),
            Affine::new(0.0, -1.0, 1.0, 0.0, 0.0, height),
        ),
        2 => (
            imageops::rotate180(image),
            Affine::new(-1.0, 0.0, 0.0, -1.0, width, height),
        ),
        3 => (
            imageops::rotate270(image),
            Affine::new(0.0, 1.0, -1.0, 0.0, width, 0.0),
        ),
        _ => (image.clone(), Affine::IDENTITY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use image::Luma;

    /// a picture with one black pixel, and where that pixel ends up after
    /// turning must match where the recorded map says it came from.
    fn check(quarter_turns: u32) {
        let mut image = GrayImage::from_pixel(5, 3, Luma([255]));
        image.put_pixel(1, 0, Luma([0]));
        let (turned, back) = turn_clockwise(&image, quarter_turns);
        let (x, y, _) = turned
            .enumerate_pixels()
            .find(|(_, _, pixel)| pixel.0[0] == 0)
            .unwrap();
        // the centre of the black pixel in the turned picture
        let centre = back.apply(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        assert!(
            centre.distance_to(Point::new(1.5, 0.5)) < 1e-4,
            "{quarter_turns} turns: {centre:?}"
        );
    }

    #[test]
    fn every_turn_maps_back_to_the_original_pixel() {
        for turns in 0..4 {
            check(turns);
        }
    }

    #[test]
    fn a_quarter_turn_swaps_width_and_height() {
        let image = GrayImage::new(5, 3);
        assert_eq!(turn_clockwise(&image, 1).0.dimensions(), (3, 5));
        assert_eq!(turn_clockwise(&image, 2).0.dimensions(), (5, 3));
    }
}
