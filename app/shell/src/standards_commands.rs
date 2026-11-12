//! Prepare & Check: PDF/A checks and conversion, Check This PDF
//! (preflight), making a smaller copy, and PDF/X print checks, one command
//! each over core/standards.
//!
//! checks read the document as it is shown and change nothing. converting
//! and optimising make a new file where the save panel said; the open
//! document is never changed. every pdf/a result carries the engine's
//! CHECKER_NOTE: these are Letters' own checks, not a validator's.

use std::path::{Path, PathBuf};

use letters_pdf::DocumentId;
use letters_shell_core::forms::write_chosen_bytes;
use letters_shell_core::standards::{convert_copy, optimized_copy, FileRenderer, LibraryFonts};
use letters_standards::{
    check_pdfa_file, check_pdfx, preflight, ConversionReport, OptimizeOptions, OptimizeReport,
    PdfaLevel, PdfaReport, PdfxLevel, PdfxOptions, PdfxReport, PreflightReport, CHECKER_NOTE,
};
use serde::Serialize;
use tauri::State;

use crate::errors::CommandError;
use crate::fonts_commands::FontsState;
use crate::state::AppState;

/// a copy written, and what making it did.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WrittenCopy<R> {
    path: String,
    report: R,
    checker_note: &'static str,
}

/// the working copy's path, for tools that read the file itself.
fn shown_file(state: &AppState, id: u64) -> Result<PathBuf, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let (path, _) = state
        .documents
        .with_session(worker, DocumentId(id), |session, _| {
            Ok((session.working_copy().to_path_buf(), None))
        })?;
    Ok(path)
}

fn shown_bytes(state: &AppState, id: u64) -> Result<Vec<u8>, CommandError> {
    let path = shown_file(state, id)?;
    std::fs::read(path).map_err(|error| CommandError::failed(error.to_string()))
}

fn sentence(error: impl ToString) -> CommandError {
    CommandError::failed(error.to_string())
}

/// checks what is shown against a pdf/a level.
#[tauri::command(async)]
pub fn standards_check_pdfa(
    state: State<AppState>,
    id: u64,
    level: PdfaLevel,
) -> Result<PdfaReport, CommandError> {
    check_pdfa_file(&shown_bytes(&state, id)?, level).map_err(sentence)
}

/// converts what is shown to a pdf/a level and saves it as a new file.
#[tauri::command(async)]
pub fn standards_convert_pdfa(
    state: State<AppState>,
    fonts: State<FontsState>,
    id: u64,
    level: PdfaLevel,
    destination: String,
) -> Result<WrittenCopy<ConversionReport>, CommandError> {
    let source = shown_file(&state, id)?;
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let font_source = LibraryFonts {
        library: fonts.library(),
    };
    let renderer = FileRenderer {
        worker,
        pdf: source.clone(),
    };
    let converted =
        convert_copy(&source, level, Some(&font_source), Some(&renderer)).map_err(sentence)?;
    write_chosen_bytes(Path::new(&destination), &converted.bytes).map_err(CommandError::failed)?;
    Ok(WrittenCopy {
        path: destination,
        report: converted.report,
        checker_note: CHECKER_NOTE,
    })
}

/// Check This PDF: structure, fonts, pictures, boxes, size and portability.
#[tauri::command(async)]
pub fn standards_preflight(
    state: State<AppState>,
    id: u64,
) -> Result<PreflightReport, CommandError> {
    preflight(&shown_bytes(&state, id)?).map_err(sentence)
}

/// a smaller copy of what is shown, saved as a new file.
#[tauri::command(async)]
pub fn standards_optimize(
    state: State<AppState>,
    id: u64,
    options: OptimizeOptions,
    destination: String,
) -> Result<WrittenCopy<OptimizeReport>, CommandError> {
    let source = shown_file(&state, id)?;
    let report = optimized_copy(&source, &options).map_err(sentence)?;
    write_chosen_bytes(Path::new(&destination), &report.bytes).map_err(CommandError::failed)?;
    Ok(WrittenCopy {
        path: destination,
        report,
        checker_note: CHECKER_NOTE,
    })
}

/// pdf/x print checks. a report only: Letters doesn't convert to pdf/x.
#[tauri::command(async)]
pub fn standards_check_pdfx(
    state: State<AppState>,
    id: u64,
    level: PdfxLevel,
) -> Result<PdfxReport, CommandError> {
    state.read_document(DocumentId(id), |document| {
        Ok(check_pdfx(document, level, &PdfxOptions::default()))
    })
}
