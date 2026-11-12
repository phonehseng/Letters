//! a small scanline rasteriser: fills outlines into an alpha bitmap with
//! exact area coverage (the signed area accumulation method), so a
//! preview looks antialiased without any graphics library.
//!
//! how it works: each edge adds, to the pixels it crosses, the share of
//! the pixel that lies to its right, signed by the edge's direction. a
//! running sum along each row then gives every pixel's coverage. the
//! nonzero fill rule falls out of the signs.

use crate::outline::{flatten, Segment};

/// a picture with one byte of coverage per pixel (0 empty, 255 full).
#[derive(Clone, Debug, PartialEq)]
pub struct AlphaBitmap {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl AlphaBitmap {
    /// how much ink the picture has, from 0 to width × height.
    pub fn ink(&self) -> f64 {
        self.pixels
            .iter()
            .map(|value| f64::from(*value) / 255.0)
            .sum()
    }

    pub fn at(&self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x]
        } else {
            0
        }
    }
}

/// previews larger than this are refused; a line of text never needs it.
pub const MAX_PIXELS: usize = 4096 * 1024;

/// fills the outlines into a bitmap of the given size.
pub fn fill(segments: &[Segment], width: usize, height: usize) -> AlphaBitmap {
    let (width, height) = if width.saturating_mul(height) > MAX_PIXELS || width == 0 || height == 0
    {
        (0, 0)
    } else {
        (width, height)
    };
    let mut area = vec![0.0f32; width * height + 2];
    if width > 0 {
        for (from, to) in flatten(segments) {
            add_edge(&mut area, width, height, from, to);
        }
    }
    let mut sum = 0.0f32;
    let pixels = area[..width * height]
        .iter()
        .map(|value| {
            sum += value;
            (sum.abs().min(1.0) * 255.0).round() as u8
        })
        .collect();
    AlphaBitmap {
        width,
        height,
        pixels,
    }
}

fn add(area: &mut [f32], index: isize, value: f32) {
    if index >= 0 {
        if let Some(cell) = area.get_mut(index as usize) {
            *cell += value;
        }
    }
}

/// adds one straight edge. x is kept inside the picture, so parts of a
/// glyph outside it pile up at the edge instead of wrapping around.
fn add_edge(area: &mut [f32], width: usize, height: usize, from: (f32, f32), to: (f32, f32)) {
    let max_x = width as f32 - 0.001;
    let clamp = |point: (f32, f32)| (point.0.clamp(0.0, max_x), point.1);
    let (from, to) = (clamp(from), clamp(to));
    if !(from.0.is_finite() && from.1.is_finite() && to.0.is_finite() && to.1.is_finite()) {
        return;
    }
    if from.1 == to.1 {
        return;
    }
    let (direction, top, bottom) = if from.1 < to.1 {
        (1.0, from, to)
    } else {
        (-1.0, to, from)
    };
    let slope = (bottom.0 - top.0) / (bottom.1 - top.1);
    let mut x = top.0;
    if top.1 < 0.0 {
        x -= top.1 * slope;
    }
    let first_row = top.1.max(0.0) as usize;
    let last_row = (bottom.1.ceil().max(0.0) as usize).min(height);
    for row in first_row..last_row {
        let row_start = (row * width) as isize;
        let dy = (row as f32 + 1.0).min(bottom.1) - (row as f32).max(top.1);
        let x_next = (x + slope * dy).clamp(0.0, max_x);
        let d = dy * direction;
        let (x0, x1) = if x < x_next { (x, x_next) } else { (x_next, x) };
        let x0_floor = x0.floor();
        let x0_index = x0_floor as isize;
        let x1_ceil = x1.ceil();
        let x1_index = x1_ceil as isize;
        if x1_index <= x0_index + 1 {
            // the edge stays within one pixel column on this row
            let middle = 0.5 * (x + x_next) - x0_floor;
            add(area, row_start + x0_index, d - d * middle);
            add(area, row_start + x0_index + 1, d * middle);
        } else {
            let s = 1.0 / (x1 - x0);
            let x0_fraction = x0 - x0_floor;
            let a0 = 0.5 * s * (1.0 - x0_fraction).powi(2);
            let x1_fraction = x1 - x1_ceil + 1.0;
            let am = 0.5 * s * x1_fraction * x1_fraction;
            add(area, row_start + x0_index, d * a0);
            if x1_index == x0_index + 2 {
                add(area, row_start + x0_index + 1, d * (1.0 - a0 - am));
            } else {
                let a1 = s * (1.5 - x0_fraction);
                add(area, row_start + x0_index + 1, d * (a1 - a0));
                for column in x0_index + 2..x1_index - 1 {
                    add(area, row_start + column, d * s);
                }
                let a2 = a1 + (x1_index - x0_index - 3) as f32 * s;
                add(area, row_start + x1_index - 1, d * (1.0 - a2 - am));
            }
            add(area, row_start + x1_index, d * am);
        }
        x = x_next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Segment> {
        vec![
            Segment::Move(x0, y0),
            Segment::Line(x1, y0),
            Segment::Line(x1, y1),
            Segment::Line(x0, y1),
            Segment::Close,
        ]
    }

    #[test]
    fn a_square_fills_exactly_its_pixels() {
        let bitmap = fill(&square(2.0, 2.0, 6.0, 6.0), 8, 8);
        assert_eq!(bitmap.at(3, 3), 255);
        assert_eq!(bitmap.at(1, 1), 0);
        assert_eq!(bitmap.at(7, 3), 0);
        assert!((bitmap.ink() - 16.0).abs() < 0.1);
    }

    #[test]
    fn half_pixels_are_half_covered() {
        let bitmap = fill(&square(0.5, 0.0, 2.0, 2.0), 3, 2);
        assert!((i32::from(bitmap.at(0, 0)) - 128).abs() <= 1);
        assert_eq!(bitmap.at(1, 0), 255);
    }

    #[test]
    fn a_hole_stays_empty_with_reversed_winding() {
        let mut shape = square(0.0, 0.0, 10.0, 10.0);
        shape.extend([
            Segment::Move(3.0, 3.0),
            Segment::Line(3.0, 7.0),
            Segment::Line(7.0, 7.0),
            Segment::Line(7.0, 3.0),
            Segment::Close,
        ]);
        let bitmap = fill(&shape, 10, 10);
        assert_eq!(bitmap.at(5, 5), 0);
        assert_eq!(bitmap.at(1, 5), 255);
    }

    #[test]
    fn shapes_outside_the_picture_never_panic() {
        let bitmap = fill(&square(-50.0, -50.0, 500.0, 500.0), 4, 4);
        assert_eq!(bitmap.at(2, 2), 255);
        let nothing = fill(&square(f32::NAN, 0.0, 1.0, 1.0), 4, 4);
        assert_eq!(nothing.width, 4);
        assert_eq!(fill(&square(0.0, 0.0, 1.0, 1.0), 100_000, 100_000).width, 0);
    }
}
