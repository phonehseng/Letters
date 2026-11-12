//! what recognition found on a page, in page coordinates.
//!
//! every position here is in points (1/72 inch) on the page as the person
//! sees it, measured from its top left corner with y going down: the same
//! space the viewer and the other editing tools use.

use serde::{Deserialize, Serialize};

use crate::geometry::{Affine, Point, Rect};
use crate::hocr::{HocrLine, HocrPage, HocrWord};
use crate::preprocess::AppliedStep;

/// one recognised page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPage {
    /// size of the page the positions are measured on, in points
    pub width: f32,
    pub height: f32,
    /// every word in reading order
    pub words: Vec<OcrWord>,
    /// the languages it was read in, as tesseract codes joined by "+"
    pub language: String,
    /// how far the page leaned before it was straightened, in degrees,
    /// anticlockwise positive
    pub skew_angle: f32,
    /// how far the page was turned clockwise to stand upright: 0, 90, 180, 270
    pub orientation: u32,
    /// the average confidence over all words, 0 to 100
    pub mean_confidence: f32,
    /// the cleanup that was done before reading, in order
    pub steps: Vec<AppliedStep>,
}

/// one recognised word.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrWord {
    pub text: String,
    /// the upright box around the word on the page
    pub rect: Rect,
    /// where the bottoms of the letters sit, from the word's start to its
    /// end. on a crooked page it slopes with the text.
    pub baseline: Baseline,
    /// the size of the type, in points
    pub font_size: f32,
    /// 0 to 100: how sure tesseract is of this word
    pub confidence: f32,
    /// which line of the page the word is on, counted from 0
    pub line: usize,
    /// which paragraph, counted from 0
    pub paragraph: usize,
    /// which block (a column, a caption, a table cell...), counted from 0
    pub block: usize,
    /// the language code tesseract read this word's paragraph in
    pub language: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub start: Point,
    pub end: Point,
}

impl Baseline {
    pub fn length(&self) -> f32 {
        self.start.distance_to(self.end)
    }
}

impl OcrPage {
    /// the page's words as plain text: one line per line, a blank line
    /// between paragraphs.
    pub fn plain_text(&self) -> String {
        let mut text = String::new();
        let mut previous: Option<&OcrWord> = None;
        for word in &self.words {
            if let Some(before) = previous {
                if before.paragraph != word.paragraph {
                    text.push_str("\n\n");
                } else if before.line != word.line {
                    text.push('\n');
                } else {
                    text.push(' ');
                }
            }
            text.push_str(&word.text);
            previous = Some(word);
        }
        text
    }
}

/// turns tesseract's words, measured in the pixels of the cleaned picture,
/// into words on the page.
///
/// `to_original` maps cleaned pixels back to the original picture, and
/// `points_per_pixel` turns original pixels into points.
pub(crate) fn words_on_page(
    hocr: &HocrPage,
    to_original: &Affine,
    points_per_pixel: f32,
) -> Vec<OcrWord> {
    let to_page = to_original.then(&Affine::scale(points_per_pixel, points_per_pixel));
    let mut paragraphs = Numbering::default();
    let mut blocks = Numbering::default();
    let mut lines = Numbering::default();
    hocr.words
        .iter()
        .map(|word| {
            let line = &hocr.lines[word.line];
            OcrWord {
                text: word.text.clone(),
                rect: mapped_box(word, &to_page),
                baseline: mapped_baseline(word, line, &to_page),
                font_size: font_size(word, line) * points_per_pixel,
                confidence: word.confidence.clamp(0.0, 100.0),
                line: lines.number(word.line),
                paragraph: paragraphs.number(line.paragraph),
                block: blocks.number(line.block),
                language: word.language.clone(),
            }
        })
        .collect()
}

/// gives consecutive numbers from 0 to the ids hocr uses, which start at 1
/// and may skip numbers when a block held no words.
#[derive(Default)]
struct Numbering {
    last_id: Option<usize>,
    next: usize,
}

impl Numbering {
    fn number(&mut self, id: usize) -> usize {
        if self.last_id != Some(id) {
            self.last_id = Some(id);
            self.next += 1;
        }
        self.next - 1
    }
}

fn mapped_box(word: &HocrWord, to_page: &Affine) -> Rect {
    let [left, top, right, bottom] = word.bbox;
    let corners = Rect::new(left, top, right - left, bottom - top)
        .corners()
        .map(|corner| to_page.apply(corner));
    Rect::around(&corners)
}

fn mapped_baseline(word: &HocrWord, line: &HocrLine, to_page: &Affine) -> Baseline {
    let [left, _, right, bottom] = word.bbox;
    // a baseline below the word's own box means hocr's line estimate is
    // off for this word (it happens on short lines); the box bottom is
    // then the better guess
    let at = |x: f32| line.baseline_at(x).min(bottom);
    Baseline {
        start: to_page.apply(Point::new(left, at(left))),
        end: to_page.apply(Point::new(right, at(right))),
    }
}

/// the line's font size when hocr gives one, else the word box's height.
fn font_size(word: &HocrWord, line: &HocrLine) -> f32 {
    let box_height = word.bbox[3] - word.bbox[1];
    line.font_size
        .filter(|size| *size > 0.0)
        .unwrap_or(box_height)
        .max(1.0)
}

/// the average word confidence, or 0 for a page without words.
pub(crate) fn mean_confidence(words: &[OcrWord]) -> f32 {
    if words.is_empty() {
        return 0.0;
    }
    words.iter().map(|word| word.confidence).sum::<f32>() / words.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hocr::parse;

    const HOCR: &str = "<div class='ocr_carea'><p class='ocr_par' lang='eng'>\
        <span class='ocr_line' title='bbox 100 100 400 140; baseline 0 -10; x_size 40'>\
        <span class='ocrx_word' title='bbox 100 100 200 130; x_wconf 90'>Hello</span>\
        <span class='ocrx_word' title='bbox 250 100 400 130; x_wconf 80'>world</span>\
        </span></p><p class='ocr_par'>\
        <span class='ocr_line' title='bbox 100 300 200 340; baseline 0 -10; x_size 40'>\
        <span class='ocrx_word' title='bbox 100 300 200 330; x_wconf 70'>Bye</span>\
        </span></p></div>";

    #[test]
    fn pixels_become_points() {
        // 300 dpi: 300 pixels are 72 points
        let words = words_on_page(&parse(HOCR), &Affine::IDENTITY, 72.0 / 300.0);
        let hello = &words[0];
        assert!((hello.rect.x - 24.0).abs() < 1e-3);
        assert!((hello.rect.width - 24.0).abs() < 1e-3);
        assert!((hello.font_size - 9.6).abs() < 1e-3);
        // the baseline is 10 pixels above the line bottom, at y = 130 pixels
        assert!((hello.baseline.start.y - 130.0 * 0.24).abs() < 1e-3);
        assert_eq!((hello.line, hello.paragraph, hello.block), (0, 0, 0));
        assert_eq!((words[2].line, words[2].paragraph), (1, 1));
    }

    #[test]
    fn positions_go_through_the_recorded_map() {
        // pretend the cleaned picture was shifted 50 pixels right
        let back = Affine::new(1.0, 0.0, 0.0, 1.0, -50.0, 0.0);
        let words = words_on_page(&parse(HOCR), &back, 1.0);
        assert_eq!(words[0].rect.x, 50.0);
        assert_eq!(words[0].baseline.start.x, 50.0);
    }

    #[test]
    fn plain_text_keeps_lines_and_paragraphs() {
        let words = words_on_page(&parse(HOCR), &Affine::IDENTITY, 1.0);
        let page = OcrPage {
            width: 600.0,
            height: 800.0,
            mean_confidence: mean_confidence(&words),
            words,
            language: "eng".into(),
            skew_angle: 0.0,
            orientation: 0,
            steps: Vec::new(),
        };
        assert_eq!(page.plain_text(), "Hello world\n\nBye");
        assert!((page.mean_confidence - 80.0).abs() < 1e-3);
    }

    #[test]
    fn serialises_with_camel_case_names() {
        let words = words_on_page(&parse(HOCR), &Affine::IDENTITY, 1.0);
        let json = serde_json::to_string(&words[0]).unwrap();
        assert!(json.contains("\"fontSize\""), "{json}");
        assert!(json.contains("\"baseline\":{\"start\""), "{json}");
    }
}
