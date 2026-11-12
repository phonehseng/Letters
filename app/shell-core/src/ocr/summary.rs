//! what a finished run found, for the line the tab shows afterwards:
//! "Text found on 12 of 14 pages; 2 pages look blank", the average
//! confidence, and the pages worth checking by eye.

use letters_ocr::OcrPage;
use serde::Serialize;

/// a page whose words tesseract was on average less sure of than this
/// (out of 100) is listed for checking. fast language data rarely drops
/// below 80 on a clean scan, so 70 points at real trouble.
pub const LOW_CONFIDENCE: f32 = 70.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrSummary {
    /// pages read, counted from 0, in order
    pub pages: Vec<u32>,
    pub pages_with_text: usize,
    /// pages where nothing was found
    pub blank_pages: Vec<u32>,
    /// the average confidence over every word found, 0 to 100
    pub mean_confidence: f32,
    /// pages with text whose average confidence is under LOW_CONFIDENCE,
    /// with that average
    pub low_confidence_pages: Vec<LowPage>,
    pub word_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LowPage {
    pub page: u32,
    pub confidence: f32,
}

/// a word only counts as text if it holds a letter or a digit: tesseract
/// reads specks on a blank page as "." or "|".
fn is_real_word(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

pub fn summarize(results: &[(u32, OcrPage)]) -> OcrSummary {
    let mut summary = OcrSummary::default();
    let mut confidence_total = 0.0;
    for (page, words) in results {
        summary.pages.push(*page);
        let real: Vec<_> = words
            .words
            .iter()
            .filter(|word| is_real_word(&word.text))
            .collect();
        if real.is_empty() {
            summary.blank_pages.push(*page);
            continue;
        }
        summary.pages_with_text += 1;
        summary.word_count += real.len();
        let page_total: f32 = real.iter().map(|word| word.confidence).sum();
        confidence_total += page_total;
        let page_mean = page_total / real.len() as f32;
        if page_mean < LOW_CONFIDENCE {
            summary.low_confidence_pages.push(LowPage {
                page: *page,
                confidence: page_mean,
            });
        }
    }
    if summary.word_count > 0 {
        summary.mean_confidence = confidence_total / summary.word_count as f32;
    }
    summary
}
