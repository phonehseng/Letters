//! the look of a scanned line, measured from its pixels: ink and paper
//! colour, stroke weight, stroke contrast (serif faces have thin
//! horizontals and thick verticals; sans and typewriter faces are even),
//! slant, and whether the letters are all one width. the closest standard
//! face is chosen from these.
//!
//! the measures are deliberately simple: stems are the median width of
//! runs of ink along rows, horizontal strokes the median height of runs
//! along columns, and slant the shear that makes the ink's columns most
//! sharply separated.

use image::RgbImage;
use serde::{Deserialize, Serialize};

use crate::fonts::standard::StandardFont;

use super::pixels::{luminance, median_color};

/// stems wider than this share of the type size read as bold (regular
/// Helvetica and Times stems are about 0.08 em, their bolds about 0.14).
const BOLD_STEM: f32 = 0.115;
/// horizontal strokes thinner than this share of the stems read as a
/// face with contrast, so a serif face. measured at 216 dpi, 14 pt:
/// Times 0.50, Times Italic 0.67, Georgia 0.75; Helvetica, Arial,
/// Verdana and Courier 1.00. the line sits between the two groups.
const SERIF_CONTRAST: f32 = 0.85;
/// a shear (x per y) past this reads as italic; romans measure under
/// 0.05 and italics 0.2 or more.
const ITALIC_SHEAR: f32 = 0.12;
/// words whose widths per letter vary less than this are monospaced.
const MONO_SPREAD: f32 = 0.07;
/// the shears tried when looking for slant.
const SHEARS: [f32; 9] = [-0.1, -0.05, 0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3];

/// what a line looks like.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStyle {
    pub serif: bool,
    pub monospace: bool,
    pub bold: bool,
    pub italic: bool,
    /// stem width in ems, horizontal-to-vertical stroke ratio and shear,
    /// as measured
    pub stem_em: f32,
    pub contrast: f32,
    pub slant: f32,
}

/// a line's ink mask and colours, measured once.
pub(crate) struct LinePixels {
    pub ink: Vec<Vec<bool>>,
    pub ink_color: [u8; 3],
    pub paper_color: [u8; 3],
}

/// the ink of a box of pixels: darker than halfway between paper and ink.
pub(crate) fn line_pixels(pixels: &RgbImage, area: (u32, u32, u32, u32)) -> LinePixels {
    let (left, top, right, bottom) = area;
    let mut levels: Vec<f32> = Vec::new();
    for y in top..bottom {
        for x in left..right {
            levels.push(luminance(pixels.get_pixel(x, y)));
        }
    }
    levels.sort_by(f32::total_cmp);
    let dark = levels.get(levels.len() / 50).copied().unwrap_or(0.0);
    let light = levels.get(levels.len() * 9 / 10).copied().unwrap_or(255.0);
    let threshold = (dark + light) / 2.0;
    let mut ink_colors = Vec::new();
    let mut paper_colors = Vec::new();
    let ink: Vec<Vec<bool>> = (top..bottom)
        .map(|y| {
            (left..right)
                .map(|x| {
                    let pixel = pixels.get_pixel(x, y);
                    let is_ink = luminance(pixel) < threshold;
                    // the colour of ink is read from its darker core, away
                    // from edges blended with the paper
                    if luminance(pixel) < (dark + threshold) / 2.0 {
                        ink_colors.push(pixel.0);
                    } else if !is_ink {
                        paper_colors.push(pixel.0);
                    }
                    is_ink
                })
                .collect()
        })
        .collect();
    LinePixels {
        ink,
        ink_color: median_color(ink_colors).unwrap_or([0, 0, 0]),
        paper_color: median_color(paper_colors).unwrap_or([255, 255, 255]),
    }
}

/// the lengths of the runs of `true` in a sequence.
fn runs(values: impl Iterator<Item = bool>) -> Vec<usize> {
    let mut found = Vec::new();
    let mut length = 0;
    for value in values {
        if value {
            length += 1;
        } else if length > 0 {
            found.push(length);
            length = 0;
        }
    }
    if length > 0 {
        found.push(length);
    }
    found
}

fn median(mut values: Vec<usize>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_unstable();
    values[values.len() / 2] as f32
}

/// how sharply the ink's columns stand apart when sheared: the sum of
/// squared column counts, which peaks when slanted stems are upright.
fn column_sharpness(ink: &[Vec<bool>], shear: f32) -> f32 {
    let height = ink.len() as f32;
    let width = ink.first().map_or(0, Vec::len);
    let mut columns = vec![0f32; width + ink.len()];
    for (row, line) in ink.iter().enumerate() {
        // rows higher up move left, by `shear` pixels per row from the bottom
        let offset = (height - row as f32) * shear;
        for (column, is_ink) in line.iter().enumerate() {
            if *is_ink {
                let at = (column as f32 - offset + height * 0.3).round();
                if at >= 0.0 && (at as usize) < columns.len() {
                    columns[at as usize] += 1.0;
                }
            }
        }
    }
    columns.iter().map(|count| count * count).sum()
}

/// measures a line's style. `size_pixels` is the type size in pixels;
/// `letter_widths` are each word's width per letter, in points.
pub(crate) fn measure(line: &LinePixels, size_pixels: f32, letter_widths: &[f32]) -> ScanStyle {
    let slant = SHEARS
        .iter()
        .copied()
        .max_by(|a, b| column_sharpness(&line.ink, *a).total_cmp(&column_sharpness(&line.ink, *b)))
        .unwrap_or(0.0);
    // strokes are measured with the slant taken out, or slanted stems
    // would read as wide and thin all at once
    let upright = unsheared(&line.ink, slant);
    let stems = median(
        upright
            .iter()
            .flat_map(|row| runs(row.iter().copied()))
            .collect(),
    );
    let width = upright.first().map_or(0, Vec::len);
    let strokes = median(
        (0..width)
            .flat_map(|column| runs(upright.iter().map(|row| row[column])))
            .collect(),
    );
    let stem_em = stems / size_pixels.max(1.0);
    let contrast = strokes / stems.max(1.0);
    let monospace = spread(letter_widths) < MONO_SPREAD;
    ScanStyle {
        serif: contrast < SERIF_CONTRAST && !monospace,
        monospace,
        bold: stem_em > BOLD_STEM,
        italic: slant > ITALIC_SHEAR,
        stem_em,
        contrast,
        slant,
    }
}

/// the ink mask with each row shifted back by the slant, so slanted
/// stems stand upright.
fn unsheared(ink: &[Vec<bool>], shear: f32) -> Vec<Vec<bool>> {
    let height = ink.len() as f32;
    let width = ink.first().map_or(0, Vec::len);
    let room = (height * shear.abs()).ceil() as usize;
    ink.iter()
        .enumerate()
        .map(|(row, line)| {
            let offset = ((height - row as f32) * shear).round() as isize;
            let mut shifted = vec![false; width + room];
            for (column, is_ink) in line.iter().enumerate() {
                let at = column as isize - offset + room as isize / 2;
                if *is_ink && at >= 0 && (at as usize) < shifted.len() {
                    shifted[at as usize] = true;
                }
            }
            shifted
        })
        .collect()
}
/// how much a set of widths varies, as a share of their mean.
fn spread(widths: &[f32]) -> f32 {
    if widths.len() < 3 {
        // too few words to tell
        return 1.0;
    }
    let mean = widths.iter().sum::<f32>() / widths.len() as f32;
    let variance = widths.iter().map(|w| (w - mean).powi(2)).sum::<f32>() / widths.len() as f32;
    variance.sqrt() / mean.max(1e-6)
}

/// the standard face closest to a style.
pub(crate) fn face_for(style: &ScanStyle) -> StandardFont {
    let family = if style.monospace {
        "courier"
    } else if style.serif {
        "times"
    } else {
        "helvetica"
    };
    let name = format!(
        "{family}{}{}",
        if style.bold { " bold" } else { "" },
        if style.italic { " italic" } else { "" }
    );
    StandardFont::from_name(&name).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_are_counted() {
        assert_eq!(runs([true, true, false, true].into_iter()), [2, 1]);
        assert!(runs([false, false].into_iter()).is_empty());
    }

    #[test]
    fn even_widths_are_monospace() {
        assert!(spread(&[6.0, 6.1, 5.9, 6.0]) < MONO_SPREAD);
        assert!(spread(&[4.0, 6.5, 5.0, 7.5]) > MONO_SPREAD);
    }
}
