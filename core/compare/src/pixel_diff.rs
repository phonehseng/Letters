//! finding pictures and drawings that changed, by comparing pixels.
//!
//! both pages are drawn, turned upright and lined up by the offset the text
//! comparison found. text areas are masked out: text changes are already
//! reported word by word, and anti-aliased letters that moved half a pixel
//! would otherwise light up everywhere. what is left is compared in small
//! cells; a cell counts as changed only when several of its pixels differ
//! clearly, which ignores the speckle of anti-aliasing. neighbouring
//! changed cells are joined into regions and reported as rectangles.

use std::collections::VecDeque;

use crate::geometry::Rect;
use crate::source::RgbaImage;

/// pixels are compared in square cells this many pixels wide.
const CELL_PIXELS: u32 = 6;

/// a pixel differs when a colour channel differs by more than this (0 to 255).
const PIXEL_THRESHOLD: u8 = 96;

/// a cell is changed when at least this many of its pixels differ.
const CHANGED_PIXELS_PER_CELL: u32 = 4;

/// changed cells this many cells apart still form one region.
const JOIN_CELLS: i64 = 2;

/// regions smaller than this many points on both sides are dropped as noise.
const MIN_REGION_POINTS: f32 = 2.0;

/// what to compare: two upright pictures at the same scale, the text areas
/// to leave out of each (upright points), and the offset of the new page's
/// content from the old page's (upright points).
pub(crate) struct PictureInput<'a> {
    pub left: &'a RgbaImage,
    pub right: &'a RgbaImage,
    pub left_mask: &'a [Rect],
    pub right_mask: &'a [Rect],
    pub offset: (f32, f32),
    pub scale: f32,
}

/// the areas that look different, as rectangles on the old page (upright
/// points). the same area on the new page is the rectangle moved by the offset.
pub(crate) fn changed_regions(input: &PictureInput) -> Vec<Rect> {
    let shift = (
        (input.offset.0 * input.scale).round() as i64,
        (input.offset.1 * input.scale).round() as i64,
    );
    let left_mask = Mask::new(input.left, input.left_mask, input.scale);
    let right_mask = Mask::new(input.right, input.right_mask, input.scale);
    let width = input.left.width.max(input.right.width) as i64;
    let height = input.left.height.max(input.right.height) as i64;
    let columns = (width as usize).div_ceil(CELL_PIXELS as usize);
    let rows = (height as usize).div_ceil(CELL_PIXELS as usize);
    let mut changed_cells = vec![false; columns * rows];
    let mut counts = vec![0u32; columns * rows];

    for y in 0..height {
        for x in 0..width {
            let (rx, ry) = (x + shift.0, y + shift.1);
            if left_mask.covers(x, y) || right_mask.covers(rx, ry) {
                continue;
            }
            let old = colour_at(input.left, x, y);
            let new = colour_at(input.right, rx, ry);
            let differs = old
                .iter()
                .zip(&new)
                .any(|(a, b)| a.abs_diff(*b) > PIXEL_THRESHOLD);
            if differs {
                let cell = (y as usize / CELL_PIXELS as usize) * columns
                    + x as usize / CELL_PIXELS as usize;
                counts[cell] += 1;
                if counts[cell] >= CHANGED_PIXELS_PER_CELL {
                    changed_cells[cell] = true;
                }
            }
        }
    }
    regions(&changed_cells, columns, rows)
        .into_iter()
        .map(|(left, top, right, bottom)| {
            let to_points = |cells: usize| (cells as u32 * CELL_PIXELS) as f32 / input.scale;
            Rect::from_edges(
                to_points(left),
                to_points(top),
                to_points(right + 1).min(width as f32 / input.scale),
                to_points(bottom + 1).min(height as f32 / input.scale),
            )
        })
        .filter(|rect| rect.width >= MIN_REGION_POINTS || rect.height >= MIN_REGION_POINTS)
        .collect()
}

/// a pixel's colour on white paper; outside the picture is paper too.
fn colour_at(image: &RgbaImage, x: i64, y: i64) -> [u8; 3] {
    if x < 0 || y < 0 || x >= image.width as i64 || y >= image.height as i64 {
        return [255, 255, 255];
    }
    image.rgb_on_white(x as u32, y as u32)
}

/// which pixels of a picture are text, and so not compared.
struct Mask {
    width: u32,
    height: u32,
    covered: Vec<bool>,
}

impl Mask {
    fn new(image: &RgbaImage, rects: &[Rect], scale: f32) -> Mask {
        let mut mask = Mask {
            width: image.width,
            height: image.height,
            covered: vec![false; image.width as usize * image.height as usize],
        };
        for rect in rects {
            // a pixel of margin catches the anti-aliased edges of letters
            let rect = rect.expanded(1.0);
            let left = (rect.x * scale).floor().max(0.0) as u32;
            let top = (rect.y * scale).floor().max(0.0) as u32;
            let right = ((rect.right() * scale).ceil().max(0.0) as u32).min(mask.width);
            let bottom = ((rect.bottom() * scale).ceil().max(0.0) as u32).min(mask.height);
            for y in top..bottom {
                let row = y as usize * mask.width as usize;
                mask.covered[row + left as usize..row + right.max(left) as usize].fill(true);
            }
        }
        mask
    }

    fn covers(&self, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return false;
        }
        self.covered[y as usize * self.width as usize + x as usize]
    }
}

/// joins changed cells that are near each other into regions, returned as
/// (left, top, right, bottom) cell numbers.
fn regions(changed: &[bool], columns: usize, rows: usize) -> Vec<(usize, usize, usize, usize)> {
    let mut seen = vec![false; changed.len()];
    let mut found = Vec::new();
    for start in 0..changed.len() {
        if !changed[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut queue = VecDeque::from([start]);
        let (mut left, mut top, mut right, mut bottom) = (columns, rows, 0, 0);
        while let Some(cell) = queue.pop_front() {
            let (x, y) = (cell % columns, cell / columns);
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
            for dy in -JOIN_CELLS..=JOIN_CELLS {
                for dx in -JOIN_CELLS..=JOIN_CELLS {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= columns as i64 || ny >= rows as i64 {
                        continue;
                    }
                    let neighbour = ny as usize * columns + nx as usize;
                    if changed[neighbour] && !seen[neighbour] {
                        seen[neighbour] = true;
                        queue.push_back(neighbour);
                    }
                }
            }
        }
        found.push((left, top, right, bottom));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_with_square(x: u32, y: u32, size: u32, colour: [u8; 4]) -> RgbaImage {
        let mut image = RgbaImage::white(200, 200);
        for py in y..y + size {
            for px in x..x + size {
                image.set_pixel(px, py, colour);
            }
        }
        image
    }

    fn compare(
        left: &RgbaImage,
        right: &RgbaImage,
        mask: &[Rect],
        offset: (f32, f32),
    ) -> Vec<Rect> {
        changed_regions(&PictureInput {
            left,
            right,
            left_mask: mask,
            right_mask: mask,
            offset,
            scale: 1.0,
        })
    }

    #[test]
    fn the_same_picture_has_no_changes() {
        let page = page_with_square(20, 20, 30, [0, 0, 0, 255]);
        assert!(compare(&page, &page, &[], (0.0, 0.0)).is_empty());
    }

    #[test]
    fn a_recoloured_square_is_one_region_around_it() {
        let old = page_with_square(60, 60, 30, [0, 0, 255, 255]);
        let new = page_with_square(60, 60, 30, [255, 0, 0, 255]);
        let found = compare(&old, &new, &[], (0.0, 0.0));
        assert_eq!(found.len(), 1);
        let region = found[0];
        assert!(region.x <= 60.0 && region.right() >= 90.0, "{region:?}");
        assert!(region.y <= 60.0 && region.bottom() >= 90.0, "{region:?}");
        assert!(region.width < 45.0, "{region:?}");
    }

    #[test]
    fn masked_areas_are_not_compared() {
        let old = page_with_square(60, 60, 30, [0, 0, 255, 255]);
        let new = page_with_square(60, 60, 30, [255, 0, 0, 255]);
        let mask = [Rect::new(50.0, 50.0, 50.0, 50.0)];
        assert!(compare(&old, &new, &mask, (0.0, 0.0)).is_empty());
    }

    #[test]
    fn scattered_single_pixels_are_noise() {
        let old = RgbaImage::white(100, 100);
        let mut new = old.clone();
        for step in 0..10 {
            new.set_pixel(step * 9, step * 7, [0, 0, 0, 255]);
        }
        assert!(compare(&old, &new, &[], (0.0, 0.0)).is_empty());
    }

    #[test]
    fn content_moved_by_the_offset_matches() {
        let old = page_with_square(40, 40, 20, [0, 0, 0, 255]);
        let new = page_with_square(40, 52, 20, [0, 0, 0, 255]);
        assert!(compare(&old, &new, &[], (0.0, 12.0)).is_empty());
        assert!(!compare(&old, &new, &[], (0.0, 0.0)).is_empty());
    }
}
