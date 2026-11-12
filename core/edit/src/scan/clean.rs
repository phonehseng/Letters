//! what happens to the scan's own pixels: under real text the old letters
//! are painted out with the paper colour around them (a simple local
//! inpainting), and in text-only mode the regions with ink that no word
//! explains (figures, signatures, stamps) are found so they can be kept
//! as pictures.

use image::{Rgb, RgbImage};

use crate::geometry::Rect;

use super::pixels::{luminance, Scan};

/// words are painted out a little beyond their boxes (points), since
/// recognition boxes hug the ink and anti-aliased edges reach further.
const PAINT_MARGIN: f32 = 1.0;
/// figures are found on a grid of cells this many pixels wide.
const CELL: u32 = 8;
/// a cell holds ink when this many of its pixels are dark.
const INK_PIXELS: usize = 3;
/// pixels darker than this (0 to 255) are ink, for finding figures.
const INK_LEVEL: f32 = 128.0;
/// regions smaller than this (points each way) are specks, not figures.
const MIN_FIGURE: f32 = 10.0;

fn grown(rect: &Rect, by: f32) -> Rect {
    Rect::new(
        rect.x - by,
        rect.y - by,
        rect.width + 2.0 * by,
        rect.height + 2.0 * by,
    )
}

/// paints each word's box (a little larger) with its line's paper colour.
pub(crate) fn paint_out(scan: &Scan, pixels: &mut RgbImage, words: &[(Rect, [u8; 3])]) {
    for (rect, paper) in words {
        let (left, top, right, bottom) = scan.pixel_box(&grown(rect, PAINT_MARGIN));
        for y in top..bottom.min(pixels.height()) {
            for x in left..right.min(pixels.width()) {
                pixels.put_pixel(x, y, Rgb(*paper));
            }
        }
    }
}

/// the regions of ink outside every word: boxes of pixels (left, top,
/// right, bottom).
pub(crate) fn figures(scan: &Scan, words: &[Rect]) -> Vec<(u32, u32, u32, u32)> {
    let columns = scan.pixels.width().div_ceil(CELL) as usize;
    let rows = scan.pixels.height().div_ceil(CELL) as usize;
    let near_word = words_mask(scan, words, columns, rows);
    let mut inked = vec![vec![false; columns]; rows];
    for (row, line) in inked.iter_mut().enumerate() {
        for (column, cell) in line.iter_mut().enumerate() {
            *cell =
                !near_word[row][column] && cell_has_ink(&scan.pixels, column as u32, row as u32);
        }
    }
    let (sx, sy) = scan.scale();
    merged(regions(&mut inked))
        .into_iter()
        .map(|(c0, r0, c1, r1)| {
            let right = ((c1 + 1) as u32 * CELL).min(scan.pixels.width());
            let bottom = ((r1 + 1) as u32 * CELL).min(scan.pixels.height());
            (c0 as u32 * CELL, r0 as u32 * CELL, right, bottom)
        })
        .filter(|(left, top, right, bottom)| {
            (right - left) as f32 / sx >= MIN_FIGURE && (bottom - top) as f32 / sy >= MIN_FIGURE
        })
        .collect()
}

/// cells covered by a word's box (grown a little).
fn words_mask(scan: &Scan, words: &[Rect], columns: usize, rows: usize) -> Vec<Vec<bool>> {
    let mut mask = vec![vec![false; columns]; rows];
    for word in words {
        let (left, top, right, bottom) = scan.pixel_box(&grown(word, 2.0 * PAINT_MARGIN));
        let (first_row, last_row) = (
            (top / CELL) as usize,
            ((bottom / CELL) as usize).min(rows - 1),
        );
        let (first_column, last_column) = (
            (left / CELL) as usize,
            ((right / CELL) as usize).min(columns - 1),
        );
        for line in &mut mask[first_row..=last_row] {
            for cell in &mut line[first_column..=last_column] {
                *cell = true;
            }
        }
    }
    mask
}

fn cell_has_ink(pixels: &RgbImage, column: u32, row: u32) -> bool {
    let mut dark = 0;
    for y in row * CELL..((row + 1) * CELL).min(pixels.height()) {
        for x in column * CELL..((column + 1) * CELL).min(pixels.width()) {
            if luminance(pixels.get_pixel(x, y)) < INK_LEVEL {
                dark += 1;
            }
        }
    }
    dark >= INK_PIXELS
}

/// the bounding boxes (first column, first row, last column, last row) of
/// groups of touching inked cells. cells are cleared as they are taken.
fn regions(inked: &mut [Vec<bool>]) -> Vec<(usize, usize, usize, usize)> {
    let mut found = Vec::new();
    for row in 0..inked.len() {
        for column in 0..inked[row].len() {
            if inked[row][column] {
                found.push(flood(inked, column, row));
            }
        }
    }
    found
}

/// regions whose boxes overlap become one: a frame and what it frames,
/// or the parts of a signature, are one figure.
fn merged(mut boxes: Vec<(usize, usize, usize, usize)>) -> Vec<(usize, usize, usize, usize)> {
    let overlaps = |a: &(usize, usize, usize, usize), b: &(usize, usize, usize, usize)| {
        a.0 <= b.2 + 1 && b.0 <= a.2 + 1 && a.1 <= b.3 + 1 && b.1 <= a.3 + 1
    };
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if overlaps(&boxes[i], &boxes[j]) {
                    let other = boxes.remove(j);
                    let first = &mut boxes[i];
                    *first = (
                        first.0.min(other.0),
                        first.1.min(other.1),
                        first.2.max(other.2),
                        first.3.max(other.3),
                    );
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
    boxes
}

fn flood(inked: &mut [Vec<bool>], column: usize, row: usize) -> (usize, usize, usize, usize) {
    let mut bounds = (column, row, column, row);
    let mut waiting = vec![(column, row)];
    inked[row][column] = false;
    while let Some((c, r)) = waiting.pop() {
        bounds = (
            bounds.0.min(c),
            bounds.1.min(r),
            bounds.2.max(c),
            bounds.3.max(r),
        );
        let neighbours = [
            (c.wrapping_sub(1), r),
            (c + 1, r),
            (c, r.wrapping_sub(1)),
            (c, r + 1),
        ];
        for (nc, nr) in neighbours {
            if nr < inked.len() && nc < inked[nr].len() && inked[nr][nc] {
                inked[nr][nc] = false;
                waiting.push((nc, nr));
            }
        }
    }
    bounds
}
