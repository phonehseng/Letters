//! the recognised words of a page, for "Show Recognized Text", and
//! correcting one of them.
//!
//! right after a run the app still has tesseract's own words, with how
//! sure it was of each (`RecognizedPages`). a file opened later, or a page
//! read by another program, only has the invisible text in the pdf; its
//! words are then read back from the page with letters-edit, without
//! confidence. the overlay uses tesseract's words only while they still
//! match what is on the page, so after an undo it never shows words that
//! are no longer there.
//!
//! letters-ocr has no "change one word" call, so a corrected word means
//! the page's layer is written again from a corrected `OcrPage`.

use std::collections::HashMap;
use std::sync::Mutex;

use letters_edit::PageObjects;
use letters_ocr::{Baseline, OcrPage, OcrWord, Point, Rect};
use serde::Serialize;

/// a word a person corrected counts as certain.
const CORRECTED_CONFIDENCE: f32 = 100.0;

/// tesseract's words for each page read in this run of the app, by
/// document and page.
#[derive(Default)]
pub struct RecognizedPages {
    pages: Mutex<HashMap<(u64, u32), OcrPage>>,
}

impl RecognizedPages {
    pub fn remember(&self, document: u64, page: u32, words: OcrPage) {
        self.lock().insert((document, page), words);
    }

    pub fn get(&self, document: u64, page: u32) -> Option<OcrPage> {
        self.lock().get(&(document, page)).cloned()
    }

    pub fn forget_document(&self, document: u64) {
        self.lock().retain(|(owner, _), _| *owner != document);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(u64, u32), OcrPage>> {
        self.pages
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// one word as the overlay draws it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayWord {
    /// its place in the page's word list, for correcting it
    pub index: usize,
    pub text: String,
    pub rect: Rect,
    /// 0 to 100, when tesseract's own words are known
    pub confidence: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayPage {
    pub page: u32,
    pub words: Vec<OverlayWord>,
    /// true when the words carry tesseract's confidence
    pub with_confidence: bool,
}

/// the words to draw over a page: tesseract's while they match the
/// invisible text on the page, else the invisible text itself.
pub fn overlay_words(page: u32, stored: Option<&OcrPage>, objects: &PageObjects) -> OverlayPage {
    let on_page = ocr_page_from_objects(objects);
    let source = match stored {
        Some(stored) if same_words(stored, &on_page) => stored,
        _ => &on_page,
    };
    let with_confidence = stored.is_some_and(|stored| std::ptr::eq(source, stored));
    OverlayPage {
        page,
        with_confidence,
        words: source
            .words
            .iter()
            .enumerate()
            .map(|(index, word)| OverlayWord {
                index,
                text: word.text.clone(),
                rect: word.rect,
                confidence: with_confidence.then_some(word.confidence),
            })
            .collect(),
    }
}

/// the words a page should get when one of them is corrected: tesseract's
/// own when they still match the page, else those read back from it.
pub fn words_to_correct(stored: Option<&OcrPage>, objects: &PageObjects) -> OcrPage {
    let on_page = ocr_page_from_objects(objects);
    match stored {
        Some(stored) if same_words(stored, &on_page) => stored.clone(),
        _ => on_page,
    }
}

/// the page with word `index` spelled as the person typed it.
pub fn correct_word(page: &OcrPage, index: usize, text: &str) -> Result<OcrPage, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type the word as it is on the page.".to_owned());
    }
    if text.chars().any(char::is_control) {
        return Err("A word can’t hold line breaks or tabs.".to_owned());
    }
    let mut corrected = page.clone();
    let word = corrected.words.get_mut(index).ok_or_else(|| {
        "That word has changed since it was shown. Show the recognized text again.".to_owned()
    })?;
    word.text = text.to_owned();
    word.confidence = CORRECTED_CONFIDENCE;
    corrected.mean_confidence = corrected
        .words
        .iter()
        .map(|word| word.confidence)
        .sum::<f32>()
        / corrected.words.len() as f32;
    Ok(corrected)
}

/// the same letters in the same order. spaces are left out: reading the
/// layer back from the page can join or split words where the gaps
/// between them are small, though not one letter differs.
fn same_words(a: &OcrPage, b: &OcrPage) -> bool {
    let letters = |page: &OcrPage| -> String {
        page.words
            .iter()
            .flat_map(|word| word.text.chars())
            .filter(|character| !character.is_whitespace())
            .collect()
    };
    letters(a) == letters(b)
}

/// the invisible words on a page as recognition results: one word per
/// stretch of letters without a space, its box around their boxes.
pub fn ocr_page_from_objects(objects: &PageObjects) -> OcrPage {
    let mut words = Vec::new();
    for (block_number, block) in objects.text_blocks.iter().enumerate() {
        for line in &block.lines {
            let line_number = words.last().map_or(0, |word: &OcrWord| word.line + 1);
            for run in line.runs.iter().filter(|run| run.invisible) {
                words.extend(run_words(run, line_number, block_number));
            }
        }
    }
    OcrPage {
        width: objects.width,
        height: objects.height,
        mean_confidence: 0.0,
        words,
        language: String::new(),
        skew_angle: 0.0,
        orientation: 0,
        steps: Vec::new(),
    }
}

fn run_words(run: &letters_edit::TextRun, line: usize, block: usize) -> Vec<OcrWord> {
    let mut words = Vec::new();
    let mut current: Vec<(char, letters_edit::Rect)> = Vec::new();
    let chars: Vec<char> = run.text.chars().collect();
    let mut finish = |current: &mut Vec<(char, letters_edit::Rect)>| {
        if let Some(word) = word_from_chars(current, run.size, line, block) {
            words.push(word);
        }
        current.clear();
    };
    for (index, character) in chars.iter().enumerate() {
        match run.char_boxes.get(index) {
            Some(rect) if !character.is_whitespace() => current.push((*character, *rect)),
            _ => finish(&mut current),
        }
    }
    finish(&mut current);
    words
}

fn word_from_chars(
    chars: &[(char, letters_edit::Rect)],
    size: f32,
    line: usize,
    block: usize,
) -> Option<OcrWord> {
    if chars.is_empty() {
        return None;
    }
    let left = chars
        .iter()
        .map(|(_, rect)| rect.x)
        .fold(f32::MAX, f32::min);
    let top = chars
        .iter()
        .map(|(_, rect)| rect.y)
        .fold(f32::MAX, f32::min);
    let right = chars
        .iter()
        .map(|(_, rect)| rect.right())
        .fold(f32::MIN, f32::max);
    let bottom = chars
        .iter()
        .map(|(_, rect)| rect.bottom())
        .fold(f32::MIN, f32::max);
    let rect = Rect::new(left, top, right - left, bottom - top);
    // the letters' boxes run from the top of the type to its descent; the
    // baseline sits about a fifth of the size above their bottom
    let baseline_y = bottom - size * 0.2;
    Some(OcrWord {
        text: chars.iter().map(|(character, _)| *character).collect(),
        rect,
        baseline: Baseline {
            start: Point::new(left, baseline_y),
            end: Point::new(right, baseline_y),
        },
        font_size: size.max(1.0),
        confidence: 0.0,
        line,
        paragraph: block,
        block,
        language: None,
    })
}
