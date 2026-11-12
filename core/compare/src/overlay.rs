//! pictures for the overlay view: both versions drawn on top of each other.
//!
//! `overlay_image` blends the two pages into one picture: what both have is
//! grey, what only the old page has is red, what only the new page has is
//! green. `aligned_images` gives the two pages as separate pictures of the
//! same size, lined up, for a blink view or an opacity slider.

use crate::result::PagePair;
use crate::source::{PageSource, RgbaImage};
use crate::CompareError;

/// below this much ink (0 to 255) a pixel is paper.
const PAPER_INK: u8 = 24;

/// ink amounts closer than this are the same mark on both pages.
const SAME_INK: i32 = 48;

/// the two pages of a pair as pictures of the same size, the new one moved
/// so its content lines up with the old one. a page the pair doesn't have
/// is blank.
#[derive(Clone, Debug, PartialEq)]
pub struct AlignedImages {
    pub left: RgbaImage,
    pub right: RgbaImage,
}

/// draws both pages of a pair at `scale` pixels per point and lines them up.
pub fn aligned_images(
    left: &dyn PageSource,
    right: &dyn PageSource,
    pair: &PagePair,
    scale: f32,
) -> Result<AlignedImages, CompareError> {
    let old = pair
        .left_page
        .map(|page| left.render(page, scale))
        .transpose()?;
    let new = pair
        .right_page
        .map(|page| right.render(page, scale))
        .transpose()?;
    let shift = (
        (pair.offset.dx * scale).round() as i64,
        (pair.offset.dy * scale).round() as i64,
    );
    let size_of = |image: &Option<RgbaImage>| {
        image
            .as_ref()
            .map(|image| (image.width, image.height))
            .unwrap_or((0, 0))
    };
    let (old_size, new_size) = (size_of(&old), size_of(&new));
    let width = old_size.0.max(new_size.0).max(1);
    let height = old_size.1.max(new_size.1).max(1);
    Ok(AlignedImages {
        left: placed(old.as_ref(), width, height, (0, 0)),
        right: placed(new.as_ref(), width, height, shift),
    })
}

/// the overlay picture of a pair: shared content grey, removed red, added green.
pub fn overlay_image(
    left: &dyn PageSource,
    right: &dyn PageSource,
    pair: &PagePair,
    scale: f32,
) -> Result<RgbaImage, CompareError> {
    let aligned = aligned_images(left, right, pair, scale)?;
    let mut overlay = RgbaImage::white(aligned.left.width, aligned.left.height);
    for y in 0..overlay.height {
        for x in 0..overlay.width {
            let old = aligned.left.rgb_on_white(x, y);
            let new = aligned.right.rgb_on_white(x, y);
            overlay.set_pixel(x, y, overlay_pixel(old, new));
        }
    }
    Ok(overlay)
}

/// a picture copied onto a white canvas, with its content moved back by `shift`.
fn placed(image: Option<&RgbaImage>, width: u32, height: u32, shift: (i64, i64)) -> RgbaImage {
    let mut canvas = RgbaImage::white(width, height);
    let Some(image) = image else {
        return canvas;
    };
    for y in 0..height {
        for x in 0..width {
            let (source_x, source_y) = (x as i64 + shift.0, y as i64 + shift.1);
            if source_x >= 0
                && source_y >= 0
                && source_x < image.width as i64
                && source_y < image.height as i64
            {
                canvas.set_pixel(x, y, image.pixel(source_x as u32, source_y as u32));
            }
        }
    }
    canvas
}

/// how dark a pixel is, from 0 (white) to 255 (black).
fn ink(colour: [u8; 3]) -> u8 {
    // the usual weights for how bright each channel looks
    let luminance = 0.299 * colour[0] as f32 + 0.587 * colour[1] as f32 + 0.114 * colour[2] as f32;
    255 - luminance.round().clamp(0.0, 255.0) as u8
}

/// one overlay pixel from the old and new page's pixels.
pub(crate) fn overlay_pixel(old: [u8; 3], new: [u8; 3]) -> [u8; 4] {
    let (old_ink, new_ink) = (ink(old), ink(new));
    if old_ink < PAPER_INK && new_ink < PAPER_INK {
        return [255, 255, 255, 255];
    }
    let difference = old_ink as i32 - new_ink as i32;
    let fade = |amount: u8, strength: f32| (255.0 - amount as f32 * strength).round() as u8;
    if difference.abs() <= SAME_INK {
        // on both pages: grey, as dark as the darker of the two
        let level = fade(old_ink.max(new_ink), 0.55);
        [level, level, level, 255]
    } else if difference > 0 {
        // only on the old page: red
        [255, fade(old_ink, 0.75), fade(old_ink, 0.75), 255]
    } else {
        // only on the new page: green
        [
            fade(new_ink, 0.8),
            fade(new_ink, 0.35),
            fade(new_ink, 0.75),
            255,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 3] = [255, 255, 255];
    const BLACK: [u8; 3] = [0, 0, 0];

    #[test]
    fn paper_stays_white_and_shared_ink_is_grey() {
        assert_eq!(overlay_pixel(WHITE, WHITE), [255, 255, 255, 255]);
        let shared = overlay_pixel(BLACK, BLACK);
        assert_eq!(shared[0], shared[1]);
        assert_eq!(shared[1], shared[2]);
        assert!(shared[0] > 90 && shared[0] < 140, "{shared:?}");
    }

    #[test]
    fn removed_ink_is_red_and_added_ink_is_green() {
        let removed = overlay_pixel(BLACK, WHITE);
        assert_eq!(removed[0], 255);
        assert!(removed[1] < 100 && removed[2] < 100, "{removed:?}");
        let added = overlay_pixel(WHITE, BLACK);
        assert!(
            added[1] > added[0] + 80 && added[1] > added[2] + 80,
            "{added:?}"
        );
    }
}
