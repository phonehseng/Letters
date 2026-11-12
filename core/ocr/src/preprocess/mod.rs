//! cleaning up a page picture before tesseract reads it.
//!
//! scans and photos are rarely perfect: the page is a little crooked, the
//! paper is gray or unevenly lit, there are specks of dust and a dark band
//! where the scanner lid did not close. each step here fixes one of those,
//! can be switched off on its own, and is tested on its own.
//!
//! steps that move pixels (turning and straightening) record how to get
//! from the cleaned picture back to the original, so the words tesseract
//! finds can be placed on the page the person actually sees.

mod binarize;
mod components;
mod contrast;
mod deskew;
mod median;
mod turn;

use image::GrayImage;
use serde::{Deserialize, Serialize};

use crate::geometry::Affine;

pub use binarize::sauvola;
pub use components::{clear_border, remove_specks};
pub use contrast::normalize_contrast;
pub use deskew::{estimate_skew, rotate, shrink};
pub use median::{is_noisy, median3};
pub use turn::turn_clockwise;

/// which cleanup steps to run. all are on by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PreprocessOptions {
    /// find pages that are sideways or upside down and turn them upright
    pub orientation: bool,
    /// straighten a slightly crooked page
    pub deskew: bool,
    /// smooth away specks of dust and scanner noise
    pub denoise: bool,
    /// stretch faint or gray scans to full black and white
    pub contrast: bool,
    /// decide for every pixel whether it is ink or paper, allowing for
    /// uneven light across the page
    pub binarize: bool,
    /// remove dark bands and shadows touching the edges of the picture
    pub clean_borders: bool,
}

impl Default for PreprocessOptions {
    fn default() -> Self {
        Self {
            orientation: true,
            deskew: true,
            denoise: true,
            contrast: true,
            binarize: true,
            clean_borders: true,
        }
    }
}

impl PreprocessOptions {
    /// every step off: tesseract sees the picture exactly as it came.
    pub fn none() -> Self {
        Self {
            orientation: false,
            deskew: false,
            denoise: false,
            contrast: false,
            binarize: false,
            clean_borders: false,
        }
    }
}

/// one thing that was done to the picture, kept with the result so it is
/// clear later what the text was read from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum AppliedStep {
    /// turned clockwise by this many degrees (90, 180 or 270)
    Turn {
        degrees: u32,
    },
    Contrast,
    Median,
    /// straightened: the text leaned by this many degrees (anticlockwise
    /// positive) and was turned back
    Deskew {
        degrees: f32,
    },
    Binarize,
    /// this many specks were removed
    RemoveSpecks {
        count: usize,
    },
    /// this many dark shapes touching the edge were removed
    ClearBorder {
        count: usize,
    },
}

/// the cleaned picture and how to get back to the original.
pub struct Prepared {
    pub image: GrayImage,
    /// maps a pixel position in `image` to the same spot in the original
    pub to_original: Affine,
    pub steps: Vec<AppliedStep>,
    /// how far the text leaned, in degrees, anticlockwise positive
    pub skew_degrees: f32,
}

/// skews smaller than this are left alone: straightening blurs letters a
/// little, which costs more than a tilt this small.
const SMALLEST_SKEW_WORTH_FIXING: f32 = 0.1;

/// runs the chosen steps in the order that suits each best.
///
/// `quarter_turns` is how many times the page must be turned clockwise to
/// stand upright, found beforehand by tesseract's orientation detection
/// (pass 0 when it was not asked). `dpi` scales the sizes the steps use, so
/// a speck is a speck at any resolution.
pub fn prepare(
    original: &GrayImage,
    dpi: f32,
    options: &PreprocessOptions,
    quarter_turns: u32,
) -> Prepared {
    let mut steps = Vec::new();
    let mut to_original = Affine::IDENTITY;
    let mut image = original.clone();

    if !quarter_turns.is_multiple_of(4) {
        let (turned, back) = turn_clockwise(&image, quarter_turns);
        image = turned;
        to_original = back.then(&to_original);
        steps.push(AppliedStep::Turn {
            degrees: (quarter_turns % 4) * 90,
        });
    }
    if options.contrast && normalize_contrast(&mut image) {
        steps.push(AppliedStep::Contrast);
    }
    if options.denoise && is_noisy(&image) {
        image = median3(&image);
        steps.push(AppliedStep::Median);
    }
    let mut skew_degrees = 0.0;
    if options.deskew {
        skew_degrees = estimate_skew(&image, dpi);
        if skew_degrees.abs() >= SMALLEST_SKEW_WORTH_FIXING {
            let (straight, back) = rotate(&image, -skew_degrees);
            image = straight;
            to_original = back.then(&to_original);
            steps.push(AppliedStep::Deskew {
                degrees: skew_degrees,
            });
        }
    }
    if options.binarize {
        image = sauvola(&image, dpi);
        steps.push(AppliedStep::Binarize);
        // specks and borders are shapes of ink, which only exist once every
        // pixel has been called ink or paper
        if options.denoise {
            let count = remove_specks(&mut image, dpi);
            steps.push(AppliedStep::RemoveSpecks { count });
        }
        if options.clean_borders {
            let count = clear_border(&mut image);
            steps.push(AppliedStep::ClearBorder { count });
        }
    }
    Prepared {
        image,
        to_original,
        steps,
        skew_degrees,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use image::Luma;

    #[test]
    fn with_every_step_off_nothing_changes() {
        let mut page = GrayImage::from_pixel(40, 30, Luma([200]));
        page.put_pixel(3, 4, Luma([10]));
        let prepared = prepare(&page, 300.0, &PreprocessOptions::none(), 0);
        assert_eq!(prepared.image, page);
        assert!(prepared.steps.is_empty());
        assert_eq!(prepared.to_original, Affine::IDENTITY);
    }

    #[test]
    fn a_turn_is_recorded_and_can_be_undone() {
        let page = GrayImage::from_pixel(40, 30, Luma([255]));
        let prepared = prepare(&page, 300.0, &PreprocessOptions::none(), 1);
        assert_eq!(prepared.image.dimensions(), (30, 40));
        assert_eq!(prepared.steps, [AppliedStep::Turn { degrees: 90 }]);
        // the turned picture's top right corner was the original's top left
        let back = prepared.to_original.apply(Point::new(30.0, 0.0));
        assert!(back.distance_to(Point::new(0.0, 0.0)) < 1e-3);
    }
}
