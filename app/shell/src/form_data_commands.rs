//! Fill & Sign's Form Data: a form's answers out to a file (fdf, xfdf, csv
//! or json, by the name the save panel was given), answers in from such a
//! file, and many filled copies of a form merged into one csv table.
//!
//! importing is one call through `state.edit`, so one undo step; each
//! answer goes through its field's format and checks in the engine.

use std::path::Path;

use letters_forms::{self as forms, FormError, ImportReport, MergedData, SkippedField};
use letters_pdf::DocumentId;
use letters_shell_core::forms::unchanged_drawings::keep_unchanged_drawings;
use letters_shell_core::forms::{
    data_format_for, merge_filled_files, read_data_file, unknown_kind, write_chosen_bytes,
};
use serde::Serialize;
use tauri::State;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    document: DocumentState,
    report: ImportReport,
}

/// what merging wrote, without the whole table.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Merged {
    path: String,
    rows: usize,
    columns: usize,
    skipped: Vec<SkippedField>,
}

/// writes the form's answers to `path`, in the format its name says.
#[tauri::command(async)]
pub fn forms_export_data(
    state: State<AppState>,
    id: u64,
    path: String,
) -> Result<(), CommandError> {
    let path = Path::new(&path);
    let format = data_format_for(path).ok_or_else(|| CommandError::failed(unknown_kind(path)))?;
    let bytes = state.read_document(DocumentId(id), |document| {
        Ok(forms::export_data(document, format)?)
    })?;
    write_chosen_bytes(path, &bytes).map_err(CommandError::failed)
}

/// fills the form from a data file. answers that don't fit are listed in
/// the report with a sentence; the rest are filled and totals worked out.
#[tauri::command(async)]
pub fn forms_import_data(
    state: State<AppState>,
    id: u64,
    path: String,
) -> Result<Imported, CommandError> {
    let (format, bytes) = read_data_file(Path::new(&path)).map_err(CommandError::failed)?;
    let mut report = None;
    let document = state.edit(DocumentId(id), |document| {
        // as for filling: fields drawn again exactly as they were are left alone
        let before = document.clone();
        report = Some(forms::import_data(document, &bytes, format)?);
        keep_unchanged_drawings(&before, document);
        Ok::<(), FormError>(())
    })?;
    let report = report.ok_or_else(|| CommandError::failed("The answers couldn’t be read."))?;
    Ok(Imported { document, report })
}

/// reads many filled copies and writes their answers as one csv table.
#[tauri::command(async)]
pub fn forms_merge_data(paths: Vec<String>, destination: String) -> Result<Merged, CommandError> {
    if paths.is_empty() {
        return Err(CommandError::failed("Choose the filled PDFs to merge."));
    }
    let chosen: Vec<&Path> = paths.iter().map(Path::new).collect();
    let MergedData {
        columns,
        rows,
        csv,
        skipped,
    } = merge_filled_files(&chosen);
    if rows.is_empty() {
        return Err(CommandError::failed(
            "None of the chosen PDFs could be read as a filled form.",
        ));
    }
    write_chosen_bytes(Path::new(&destination), csv.as_bytes()).map_err(CommandError::failed)?;
    Ok(Merged {
        path: destination,
        rows: rows.len(),
        columns: columns.len(),
        skipped,
    })
}
