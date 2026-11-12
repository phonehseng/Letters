//! the whole reading of a page: clean it up, let tesseract read it, and
//! put the words back where they are on the page.

use image::GrayImage;
use serde::{Deserialize, Serialize};

use crate::cancel::{CancelFlag, Progress};
use crate::engine::Engine;
use crate::hocr;
use crate::page_image::PageImage;
use crate::preprocess::{normalize_contrast, prepare, shrink, PreprocessOptions};
use crate::result::{mean_confidence, words_on_page, OcrPage};
use crate::OcrError;

/// what to read and how.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OcrOptions {
    /// tesseract language codes, such as ["eng", "spa"]. several can be
    /// given for a page that mixes languages; each one makes reading slower.
    pub languages: Vec<String>,
    pub preprocess: PreprocessOptions,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            languages: vec!["eng".into()],
            preprocess: PreprocessOptions::default(),
        }
    }
}

/// orientation is only trusted when tesseract is at least this sure.
/// turning a page the wrong way ruins it, while leaving a sideways page
/// alone only costs a second try with the page turned by hand.
const ORIENTATION_CONFIDENCE: f32 = 2.0;

/// orientation is detected on a copy reduced to about this resolution:
/// it is just as reliable there and several times faster.
const ORIENTATION_DPI: f32 = 150.0;

/// reads the text on one page picture.
pub fn recognize(
    engine: &Engine,
    page: &PageImage,
    options: &OcrOptions,
    cancel: &CancelFlag,
) -> Result<OcrPage, OcrError> {
    engine.check_languages(&options.languages)?;
    let quarter_turns = if options.preprocess.orientation {
        upright_turns(engine, &page.pixels, page.dpi, cancel)?
    } else {
        0
    };
    let prepared = prepare(&page.pixels, page.dpi, &options.preprocess, quarter_turns);
    let hocr = engine.read_hocr(&prepared.image, page.dpi, &options.languages, cancel)?;
    let words = words_on_page(&hocr::parse(&hocr), &prepared.to_original, 72.0 / page.dpi);
    Ok(OcrPage {
        width: page.width_points(),
        height: page.height_points(),
        mean_confidence: mean_confidence(&words),
        words,
        language: options.languages.join("+"),
        skew_angle: prepared.skew_degrees,
        orientation: quarter_turns * 90,
        steps: prepared.steps,
    })
}

/// how many clockwise quarter turns stand the page upright, or 0 when
/// tesseract can't tell or isn't sure.
fn upright_turns(
    engine: &Engine,
    pixels: &GrayImage,
    dpi: f32,
    cancel: &CancelFlag,
) -> Result<u32, OcrError> {
    let factor = (dpi / ORIENTATION_DPI).floor().max(1.0) as u32;
    let mut small = shrink(pixels, factor);
    normalize_contrast(&mut small);
    let sample = orientation_sample(&small);
    let guess = engine.detect_orientation(&sample, dpi / factor as f32, cancel)?;
    Ok(guess
        .filter(|guess| guess.confidence >= ORIENTATION_CONFIDENCE)
        .map_or(0, |guess| guess.quarter_turns))
}

/// the share of the inked area, across and down, that orientation is
/// judged from. tesseract only needs a few dozen letters to tell which way
/// up a page is, but it lays out the whole picture it is given first, so a
/// smaller piece from the middle of the text makes the check much faster.
const SAMPLE_SHARE: (f32, f32) = (0.6, 0.4);

/// the middle of the part of the page that has ink on it.
fn orientation_sample(page: &GrayImage) -> GrayImage {
    let (width, height) = page.dimensions();
    // ink near the edges is more likely a scanner shadow than text
    let (margin_x, margin_y) = (width / 50, height / 50);
    let mut ink = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, pixel) in page.enumerate_pixels() {
        let inside =
            x >= margin_x && x < width - margin_x && y >= margin_y && y < height - margin_y;
        if inside && pixel.0[0] < 128 {
            ink = (ink.0.min(x), ink.1.min(y), ink.2.max(x), ink.3.max(y));
        }
    }
    if ink.0 > ink.2 || ink.1 > ink.3 {
        return page.clone();
    }
    let (ink_width, ink_height) = (ink.2 - ink.0 + 1, ink.3 - ink.1 + 1);
    let sample_width = (ink_width as f32 * SAMPLE_SHARE.0) as u32;
    let sample_height = (ink_height as f32 * SAMPLE_SHARE.1) as u32;
    let left = ink.0 + (ink_width - sample_width) / 2;
    let top = ink.1 + (ink_height - sample_height) / 2;
    image::imageops::crop_imm(page, left, top, sample_width.max(1), sample_height.max(1)).to_image()
}

/// reads many pages one after another.
///
/// `load_page(index)` is asked for each page's picture only when its turn
/// comes, and the picture is dropped before the next is loaded, so memory
/// holds one page however long the document. `progress` hears after every
/// page; `cancel` is checked between pages and while tesseract runs.
pub fn recognize_pages(
    engine: &Engine,
    page_count: usize,
    mut load_page: impl FnMut(usize) -> Result<PageImage, OcrError>,
    options: &OcrOptions,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<Vec<OcrPage>, OcrError> {
    // a missing language is reported before the first page is drawn
    engine.check_languages(&options.languages)?;
    let mut pages = Vec::with_capacity(page_count);
    for index in 0..page_count {
        if cancel.is_cancelled() {
            return Err(OcrError::Cancelled);
        }
        let picture = load_page(index)?;
        pages.push(recognize(engine, &picture, options, cancel)?);
        drop(picture);
        progress(Progress {
            page: index,
            done: index + 1,
            total: page_count,
        });
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn the_orientation_sample_is_the_middle_of_the_ink() {
        let mut page = GrayImage::from_pixel(1000, 1000, Luma([255]));
        // "text" from 100 to 900 across and 200 to 800 down
        for y in (200..800).step_by(20) {
            for x in 100..900 {
                page.put_pixel(x, y, Luma([0]));
            }
        }
        // a scanner shadow along the very edge is ignored
        for y in 0..1000 {
            page.put_pixel(2, y, Luma([0]));
        }
        let sample = orientation_sample(&page);
        assert_eq!(sample.dimensions(), (480, 232));
    }

    #[test]
    fn a_blank_page_is_sampled_whole() {
        let page = GrayImage::from_pixel(50, 40, Luma([255]));
        assert_eq!(orientation_sample(&page).dimensions(), (50, 40));
    }
}
