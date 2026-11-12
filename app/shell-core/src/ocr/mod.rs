//! text recognition (the Scan tab), the parts that need no window.
//!
//! the reading itself is core/ocr (letters-ocr), which runs tesseract as a
//! separate program. around it the app needs: finding where the engine is
//! installed (`engine_folder`), running a job over many pages in the
//! background with progress and a cancel that really stops (`job`, `jobs`),
//! drawing a page at 300 dpi with our own renderer (`render`), what the
//! banner and the tab show about a document (`overview`), the summary
//! after a run (`summary`), and the words for the "Show Recognized Text"
//! overlay and correcting one of them (`words`).
//!
//! results are written into the document only at the end, in one edit
//! (`apply_layers` inside `AppState::edit`), so a run is one undo step and
//! a run that is cancelled or fails changes nothing.

pub mod engine_folder;
pub mod job;
pub mod jobs;
pub mod overview;
pub mod render;
pub mod summary;
pub mod words;

#[cfg(test)]
mod tests;

use letters_ocr::{add_text_layer, OcrError, OcrPage};

/// adds the text layer of every recognised page. used inside one edit, so
/// if any page fails the whole edit is refused and nothing changes.
pub fn apply_layers(doc: &mut lopdf::Document, pages: &[(u32, OcrPage)]) -> Result<(), OcrError> {
    for (page, words) in pages {
        add_text_layer(doc, *page, words)?;
    }
    Ok(())
}
