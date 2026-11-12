//! the Export dialog's commands, office files to pdf, and "Open PDF as
//! Document…".
//!
//! the work is in letters-shell-core (src/export), where it is tested.
//! an export runs on its own thread inside an async command, sends
//! "export-progress" events to its window, and stops when Cancel is
//! pressed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use letters_convert::{find_office_converters, CancelFlag, PageSelection, PdfSource};
use letters_pdf::DocumentId;
use letters_shell_core::export::choices::{export_capabilities, ExportCapabilities};
use letters_shell_core::export::exact::ExactPlaces;
use letters_shell_core::export::office::{convert_to_pdf, shown_as_pdf, ConvertedToPdf};
use letters_shell_core::export::request::{ExportJob, ExportOutcome, ExportProgress};
use letters_shell_core::export::run::{easy_to_edit_document, run_export, ExportTools};
use letters_shell_core::writer::sessions::OpenedWriter;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Window};

use crate::errors::CommandError;
use crate::state::AppState;
use crate::writer_commands::WriterState;

/// the exports running now, by the id the ui gave them, so Cancel can
/// reach them.
#[derive(Default)]
pub struct ExportState {
    running: Mutex<HashMap<u64, (CancelFlag, letters_exact::CancelFlag)>>,
    /// pdfs a new window should open as editable documents
    as_document: Mutex<Vec<PathBuf>>,
}

fn failed(message: impl Into<String>) -> CommandError {
    CommandError::failed(message)
}

/// a copy of the document as it is shown (edits included) in the temp
/// folder. versions in the session are never rewritten, but the copy
/// keeps an export safe from the document closing while it runs.
fn snapshot(state: &AppState, id: u64) -> Result<(PathBuf, PathBuf), CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let ((working, original), _) =
        state
            .documents
            .with_session(worker, DocumentId(id), |session, _| {
                Ok((
                    (
                        session.working_copy().to_path_buf(),
                        session.original_path().to_path_buf(),
                    ),
                    None,
                ))
            })?;
    let folder = std::env::temp_dir().join("letters-export");
    std::fs::create_dir_all(&folder).map_err(|error| failed(error.to_string()))?;
    let copy = folder.join(format!("document-{id}-{}.pdf", std::process::id()));
    std::fs::copy(&working, &copy).map_err(|error| failed(error.to_string()))?;
    Ok((copy, original))
}

fn exact_places(app: &AppHandle) -> ExactPlaces {
    let cache = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    ExactPlaces {
        cache_dir: cache.join("exact-match"),
        // word can't open files under the app's own cache folder, but it can in the temp folder
        work_dir: std::env::temp_dir().join("letters-exact-match"),
    }
}

/// what this computer can export to.
#[tauri::command(async)]
pub fn export_capabilities_list() -> ExportCapabilities {
    export_capabilities(&find_office_converters())
}

/// progress for the ui, with the export it belongs to.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    export_id: u64,
    #[serde(flatten)]
    progress: ExportProgress,
}

/// runs one export of an open document.
#[tauri::command(async)]
pub fn export_document(
    app: AppHandle,
    window: Window,
    state: State<AppState>,
    exports: State<ExportState>,
    id: u64,
    export_id: u64,
    job: ExportJob,
) -> Result<ExportOutcome, CommandError> {
    let (pdf, _) = snapshot(&state, id)?;
    let worker = state
        .worker()
        .map_err(CommandError::engine_unavailable)?
        .clone();
    let cancel = CancelFlag::new();
    let exact_cancel = letters_exact::CancelFlag::new();
    exports
        .running
        .lock()
        .unwrap()
        .insert(export_id, (cancel.clone(), exact_cancel.clone()));
    let progress_window = window.clone();
    let tools = ExportTools {
        worker,
        exact: exact_places(&app),
        progress: Arc::new(move |progress| {
            let _ = progress_window.emit_to(
                progress_window.label(),
                "export-progress",
                ProgressEvent {
                    export_id,
                    progress,
                },
            );
        }),
        cancel,
        exact_cancel,
    };
    let result = run_export(&pdf, &job, &tools);
    exports.running.lock().unwrap().remove(&export_id);
    let _ = std::fs::remove_file(&pdf);
    result.map_err(failed)
}

#[tauri::command(async)]
pub fn export_cancel(exports: State<ExportState>, export_id: u64) {
    if let Some((cancel, exact)) = exports.running.lock().unwrap().get(&export_id) {
        cancel.cancel();
        exact.cancel();
    }
}

/// "Convert to PDF": an office file into a pdf the person chose a place for.
#[tauri::command(async)]
pub fn convert_office_to_pdf(
    path: String,
    destination: String,
) -> Result<ConvertedToPdf, CommandError> {
    convert_to_pdf(
        Path::new(&path),
        Path::new(&destination),
        &CancelFlag::new(),
    )
    .map_err(failed)
}

/// a spreadsheet, a presentation or an rtf shown in the viewer as a pdf
/// made in the temp folder; nothing is written beside the person's file.
#[tauri::command(async)]
pub fn show_office_as_pdf(path: String) -> Result<ConvertedToPdf, CommandError> {
    shown_as_pdf(Path::new(&path), &CancelFlag::new()).map_err(failed)
}

/// "Open PDF as Document…" opens a new window for the result: the pdf is
/// remembered here, and that window asks for it once its ui is ready.
#[tauri::command(async)]
pub fn open_pdf_as_document_window(app: AppHandle, exports: State<ExportState>, path: String) {
    exports
        .as_document
        .lock()
        .unwrap()
        .push(PathBuf::from(&path));
    crate::windows::open_window(&app, Some(PathBuf::from(path)));
}

/// true once for a file a window was opened to show as a document.
#[tauri::command(async)]
pub fn take_open_as_document(exports: State<ExportState>, path: String) -> bool {
    let mut waiting = exports.as_document.lock().unwrap();
    match waiting.iter().position(|pdf| pdf == Path::new(&path)) {
        Some(index) => {
            waiting.remove(index);
            true
        }
        None => false,
    }
}

/// the pdf's easy to edit structure opened in the writer, in memory. it
/// starts unsaved, and Save writes it beside the pdf under a name that
/// isn't taken (or Save As chooses another). the pdf is read as saved.
#[tauri::command(async)]
pub fn open_pdf_file_as_document(
    window: Window,
    writer: State<WriterState>,
    path: String,
) -> Result<OpenedWriter, CommandError> {
    let original = PathBuf::from(&path);
    let source = PdfSource::open(&original).map_err(|error| failed(error.to_string()))?;
    let (document, _) = easy_to_edit_document(
        &source,
        &PageSelection::All,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .map_err(failed)?;
    let suggested = free_name_beside(&original, "docx");
    let from = letters_shell_core::session::file_name(&original);
    writer
        .sessions()
        .open_converted(window.label(), &suggested, document, &from)
        .map_err(failed)
}

/// "Essay.docx" beside "Essay.pdf", or "Essay (2).docx" when that is taken.
fn free_name_beside(pdf: &Path, extension: &str) -> PathBuf {
    let folder = pdf.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = pdf
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into());
    (1..)
        .map(|attempt| {
            let name = if attempt == 1 {
                format!("{stem}.{extension}")
            } else {
                format!("{stem} ({attempt}).{extension}")
            };
            folder.join(name)
        })
        .find(|candidate| !candidate.exists())
        .expect("some name is always free")
}
