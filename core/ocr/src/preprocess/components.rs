//! cleaning a black and white picture shape by shape.
//!
//! a "shape" is a group of touching black pixels (diagonal neighbours
//! count as touching): a letter, a speck of dust, the dark band left by a
//! scanner lid. looking at whole shapes rather than single pixels lets us
//! remove dust without nibbling at letters.

use image::GrayImage;

const INK: u8 = 0;
const PAPER: u8 = 255;

/// a shape found by `shapes`: its pixels (as indexes into the picture) and
/// its bounding box.
struct Shape {
    pixels: Vec<usize>,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl Shape {
    fn width(&self) -> u32 {
        self.right - self.left + 1
    }

    fn height(&self) -> u32 {
        self.bottom - self.top + 1
    }
}

/// the largest speck removed, in pixels at 300 dpi. a full stop in 8 point
/// type is about 25 pixels, so 6 leaves every real mark alone.
const LARGEST_SPECK_AT_300_DPI: f32 = 6.0;

/// removes shapes so small they can only be dust or noise. returns how
/// many were removed.
pub fn remove_specks(image: &mut GrayImage, dpi: f32) -> usize {
    let scale = dpi / 300.0;
    let largest = (LARGEST_SPECK_AT_300_DPI * scale * scale).round().max(1.0) as usize;
    let mut removed = 0;
    for_each_shape(image, |shape| shape.pixels.len() <= largest, &mut removed);
    removed
}

/// removes large shapes touching the edge of the picture: the dark band of
/// a scanner lid, the shadow of a book's spine, the edge of the table in a
/// photo. a shape counts as large when it spans more than a tenth of the
/// page either way, which no letter does. returns how many were removed.
pub fn clear_border(image: &mut GrayImage) -> usize {
    let (width, height) = image.dimensions();
    let mut removed = 0;
    for_each_shape(
        image,
        |shape| {
            let touches_edge = shape.left == 0
                || shape.top == 0
                || shape.right == width - 1
                || shape.bottom == height - 1;
            let large = shape.width() > width / 10 || shape.height() > height / 10;
            touches_edge && large
        },
        &mut removed,
    );
    removed
}

/// finds every shape and paints over those `remove` picks.
fn for_each_shape(image: &mut GrayImage, remove: impl Fn(&Shape) -> bool, removed: &mut usize) {
    let (width, height) = image.dimensions();
    let mut seen = vec![false; width as usize * height as usize];
    let mut stack = Vec::new();
    for start in 0..seen.len() {
        if seen[start] || image.as_raw()[start] != INK {
            continue;
        }
        let shape = flood(image, start, &mut seen, &mut stack);
        if remove(&shape) {
            let pixels: &mut [u8] = image;
            for &index in &shape.pixels {
                pixels[index] = PAPER;
            }
            *removed += 1;
        }
    }
}

/// collects the shape that pixel `start` belongs to.
fn flood(image: &GrayImage, start: usize, seen: &mut [bool], stack: &mut Vec<usize>) -> Shape {
    let (width, height) = image.dimensions();
    let raw = image.as_raw();
    let row = width as usize;
    let mut shape = Shape {
        pixels: Vec::new(),
        left: u32::MAX,
        top: u32::MAX,
        right: 0,
        bottom: 0,
    };
    seen[start] = true;
    stack.push(start);
    while let Some(index) = stack.pop() {
        let (x, y) = ((index % row) as u32, (index / row) as u32);
        shape.pixels.push(index);
        shape.left = shape.left.min(x);
        shape.right = shape.right.max(x);
        shape.top = shape.top.min(y);
        shape.bottom = shape.bottom.max(y);
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
                if nx < 0 || ny < 0 || nx >= i64::from(width) || ny >= i64::from(height) {
                    continue;
                }
                let neighbour = ny as usize * row + nx as usize;
                if !seen[neighbour] && raw[neighbour] == INK {
                    seen[neighbour] = true;
                    stack.push(neighbour);
                }
            }
        }
    }
    shape
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    fn page() -> GrayImage {
        GrayImage::from_pixel(100, 100, Luma([PAPER]))
    }

    fn fill(image: &mut GrayImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>) {
        for row in y {
            for column in x.clone() {
                image.put_pixel(column, row, Luma([INK]));
            }
        }
    }

    #[test]
    fn specks_go_and_letters_stay() {
        let mut image = page();
        fill(&mut image, 10..12, 10..12); // a 4 pixel speck
        fill(&mut image, 50..56, 50..60); // a letter-sized shape
        let removed = remove_specks(&mut image, 300.0);
        assert_eq!(removed, 1);
        assert_eq!(image.get_pixel(10, 10).0[0], PAPER);
        assert_eq!(image.get_pixel(52, 55).0[0], INK);
    }

    #[test]
    fn a_dark_band_at_the_edge_goes() {
        let mut image = page();
        fill(&mut image, 0..6, 0..100); // the lid's shadow down the left side
        fill(&mut image, 40..46, 40..50); // a letter
        fill(&mut image, 94..100, 45..50); // a small mark cut by the edge
        assert_eq!(clear_border(&mut image), 1);
        assert_eq!(image.get_pixel(2, 50).0[0], PAPER);
        assert_eq!(image.get_pixel(42, 45).0[0], INK);
        assert_eq!(image.get_pixel(97, 47).0[0], INK);
    }

    #[test]
    fn diagonal_pixels_belong_to_one_shape() {
        let mut image = page();
        for step in 0..20 {
            image.put_pixel(step, step, Luma([INK]));
        }
        // 20 pixels touching corner to corner are one shape, not 20 specks
        assert_eq!(remove_specks(&mut image, 300.0), 0);
    }
}
