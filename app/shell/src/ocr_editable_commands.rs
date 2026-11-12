//! the Scan tab's "Make Editable Text": scanned pages become real, visible
//! text the Edit tab can change, through letters-edit's
//! `convert_scan_to_text` (see letters_shell_core::ocr::editable).
//!
//! the words come from a recognition run that just finished (`job`, run by
//! ocr_start as for Recognize Text), or, for pages read before, from the
//! words kept since or the hidden text layer on the page. all pages change
//! in one edit, so one Undo brings every scan back.

use letters_edit::{page_objects, EditConsent, ScanMode, ScanOptions};
use letters_ocr::OcrPage;
use letters_pdf::DocumentId;
use letters_shell_core::ocr::editable::{make_pages_editable, EditableSummary};
use letters_shell_core::ocr::words::words_to_correct;
use serde::Serialize;
use tauri::State;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableAnswer {
    document: DocumentState,
    summary: EditableSummary,
}

/// turns `pages` (counted from 0) into editable text. `job` is a finished
/// recognition run of those pages; without one, the pages must have been
/// recognised before.
#[tauri::command(async)]
pub fn ocr_make_editable(
    state: State<AppState>,
    id: u64,
    pages: Vec<u32>,
    job: Option<u64>,
    mode: ScanMode,
    consent: Option<EditConsent>,
) -> Result<EditableAnswer, CommandError> {
    let words = match job {
        Some(job) => words_from_run(&state, id, job)?,
        None => words_on_pages(&state, id, &pages)?,
    };
    let options = ScanOptions {
        mode,
        consent: consent.unwrap_or_default(),
        ..ScanOptions::default()
    };
    let mut summary = EditableSummary::default();
    let document = state.edit(DocumentId(id), |doc| {
        summary = make_pages_editable(doc, &words, &options)?;
        Ok::<(), String>(())
    })?;
    Ok(EditableAnswer { document, summary })
}

/// the words a finished run read.
fn words_from_run(
    state: &AppState,
    id: u64,
    job: u64,
) -> Result<Vec<(u32, OcrPage)>, CommandError> {
    let (document, pages) = state
        .ocr
        .jobs
        .take_pages(job)
        .ok_or_else(|| CommandError::failed("That text recognition hasn’t finished."))?;
    if document != id {
        return Err(CommandError::failed(
            "That text was read from another document.",
        ));
    }
    Ok(pages)
}

/// the words of pages recognised before: tesseract's own while they still
/// match the page, else the hidden text layer read back.
fn words_on_pages(
    state: &AppState,
    id: u64,
    pages: &[u32],
) -> Result<Vec<(u32, OcrPage)>, CommandError> {
    let mut words = Vec::new();
    for page in pages {
        let stored = state.ocr.recognized.get(id, *page);
        let read = state.read_document(DocumentId(id), |document| {
            let objects = page_objects(document, *page as usize)
                .map_err(|error| CommandError::failed(error.to_string()))?;
            Ok(words_to_correct(stored.as_ref(), &objects))
        })?;
        if read.words.is_empty() {
            return Err(CommandError::failed(format!(
                "Page {} has no recognized text yet. Choose Make Editable Text to read it first.",
                page + 1
            )));
        }
        words.push((*page, read));
    }
    Ok(words)
}
