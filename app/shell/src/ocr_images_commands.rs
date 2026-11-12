//! New from Images: making a pdf from pictures (scans, phone photos) with
//! core/ocr's `images_to_pdf`, optionally reading their text as it goes.
//!
//! the pdf is made on its own thread and written in the app's cache; when
//! the ui opens it, it becomes an ordinary document whose working copy
//! holds everything, and the cache file is removed, so it opens unsaved and
//! nothing lands in the person's folders until they choose where to save.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use letters_ocr::{
    images_to_pdf, CancelFlag, ImageSource, ImagesToPdfOptions, OcrOptions, PaperSize,
};
use letters_pdf::DocumentId;
use letters_shell_core::ocr::job::JobProgress;
use serde::Serialize;
use tauri::{AppHandle, Manager, State, Window};

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// half an inch of paper around a photo on a4 or letter pages, so the
/// picture doesn't run into a printer's margin; a scan's own size has none.
const PHOTO_MARGIN_POINTS: f32 = 36.0;

/// the pdfs being made, by run number. each run is behind its own lock so
/// its thread can report progress while the ui asks about another.
#[derive(Default)]
pub struct ImageJobs {
    next_id: AtomicU64,
    jobs: Mutex<HashMap<u64, Arc<Mutex<ImageJob>>>>,
}

struct ImageJob {
    cancel: CancelFlag,
    progress: JobProgress,
    /// the pdf written, or why it wasn't
    result: Option<Result<PathBuf, String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagesStatus {
    progress: JobProgress,
    finished: bool,
    error: Option<String>,
}

fn failed(error: impl ToString) -> CommandError {
    CommandError::failed(error.to_string())
}

/// makes a pdf from pictures in the background, one page each, optionally
/// reading their text as it goes. answers the run's number.
#[tauri::command(async)]
pub fn ocr_images_to_pdf(
    app: AppHandle,
    state: State<AppState>,
    paths: Vec<String>,
    paper: PaperSize,
    read_text: Option<OcrOptions>,
) -> Result<u64, CommandError> {
    if paths.is_empty() {
        return Err(CommandError::failed("Choose at least one picture."));
    }
    // a missing language is said now, before anything runs
    if let Some(options) = &read_text {
        let engine = state.ocr.engine()?;
        engine.check_languages(&options.languages).map_err(failed)?;
    }
    let (job, entry) = register(&state.ocr.images, paths.len());
    let cancel = entry.lock().unwrap().cancel.clone();
    let folder = state.ocr.new_documents.join(job.to_string());
    thread::Builder::new()
        .name("images-to-pdf".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            let request = PdfRequest {
                paths: &paths,
                paper,
                read_text: read_text.as_ref(),
                folder: &folder,
            };
            let result = write_images_pdf(state.inner(), &request, &entry, &cancel);
            entry.lock().unwrap().result = Some(result);
        })
        .map_err(failed)?;
    Ok(job)
}

/// a new run of `total` pictures, with its number.
fn register(images: &ImageJobs, total: usize) -> (u64, Arc<Mutex<ImageJob>>) {
    let job = images.next_id.fetch_add(1, Ordering::SeqCst) + 1;
    let entry = Arc::new(Mutex::new(ImageJob {
        cancel: CancelFlag::new(),
        progress: JobProgress {
            done: 0,
            total,
            current: None,
        },
        result: None,
    }));
    images.jobs.lock().unwrap().insert(job, entry.clone());
    (job, entry)
}

/// what one run makes: which pictures, on what paper, read or not, and
/// the cache folder the pdf is written to.
struct PdfRequest<'a> {
    paths: &'a [String],
    paper: PaperSize,
    read_text: Option<&'a OcrOptions>,
    folder: &'a Path,
}

fn write_images_pdf(
    state: &AppState,
    request: &PdfRequest,
    entry: &Arc<Mutex<ImageJob>>,
    cancel: &CancelFlag,
) -> Result<PathBuf, String> {
    let sources: Vec<ImageSource> = request
        .paths
        .iter()
        .map(|path| ImageSource::File(path.into()))
        .collect();
    let margin = if request.paper == PaperSize::FromImage {
        0.0
    } else {
        PHOTO_MARGIN_POINTS
    };
    let options = ImagesToPdfOptions {
        paper: request.paper,
        margin,
        ..ImagesToPdfOptions::default()
    };
    let engine = match request.read_text {
        Some(_) => Some(state.ocr.engine_or_message()?),
        None => None,
    };
    let reading = engine.zip(request.read_text);
    let mut report_progress = |progress: letters_ocr::Progress| {
        entry.lock().unwrap().progress = JobProgress {
            done: progress.done,
            total: progress.total,
            current: None,
        };
    };
    let mut document = images_to_pdf(&sources, &options, reading, &mut report_progress, cancel)
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(request.folder).map_err(|error| error.to_string())?;
    let path = request
        .folder
        .join(format!("{}.pdf", new_document_stem(request.paths)));
    document.save(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

/// the new pdf is named after the first picture: "IMG_0012.pdf".
fn new_document_stem(paths: &[String]) -> String {
    let first_stem = paths
        .first()
        .and_then(|path| Path::new(path).file_stem())
        .and_then(|stem| stem.to_str());
    first_stem
        .and_then(letters_shell_core::outputs::safe_file_name)
        .unwrap_or_else(|| "Pictures".to_owned())
}

fn image_job(state: &AppState, job: u64) -> Result<Arc<Mutex<ImageJob>>, CommandError> {
    let jobs = state.ocr.images.jobs.lock().unwrap();
    jobs.get(&job)
        .cloned()
        .ok_or_else(|| CommandError::failed("Making that PDF is no longer running."))
}

#[tauri::command(async)]
pub fn ocr_images_status(state: State<AppState>, job: u64) -> Result<ImagesStatus, CommandError> {
    let entry = image_job(&state, job)?;
    let entry = entry.lock().unwrap();
    let error = match &entry.result {
        Some(Err(message)) => Some(message.clone()),
        _ => None,
    };
    Ok(ImagesStatus {
        progress: entry.progress.clone(),
        finished: entry.result.is_some(),
        error,
    })
}

/// stops a run; images_to_pdf checks the flag between pictures.
#[tauri::command(async)]
pub fn ocr_images_cancel(state: State<AppState>, job: u64) {
    if let Ok(entry) = image_job(&state, job) {
        entry.lock().unwrap().cancel.cancel();
    }
}

/// opens the pdf a finished run made, in this window, as an unsaved
/// document: the file in the cache is removed once the working copy is made.
#[tauri::command(async)]
pub fn ocr_open_new_document(
    window: Window,
    state: State<AppState>,
    job: u64,
) -> Result<DocumentState, CommandError> {
    let path = take_finished_pdf(&state, job)?;
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let opened = state.documents.open(worker, &path, None)?;
    let id = DocumentId(opened.id);
    state.remember_document(window.label(), id);
    // nothing on disk holds it yet, so it counts as changed and Save is offered
    let (_, shown) = state.documents.with_session(worker, id, |session, _| {
        session.mark_unsaved();
        Ok(((), None))
    })?;
    // the working copy has it all now; nothing stays in the cache folder
    let _ = fs::remove_file(&path);
    if let Some(folder) = path.parent() {
        let _ = fs::remove_dir(folder);
    }
    Ok(shown)
}

/// the pdf a run wrote, once; the run is forgotten afterwards.
fn take_finished_pdf(state: &AppState, job: u64) -> Result<PathBuf, CommandError> {
    let entry = image_job(state, job)?;
    let result = entry.lock().unwrap().result.take();
    let path = match result {
        Some(Ok(path)) => path,
        Some(Err(message)) => return Err(CommandError::failed(message)),
        None => return Err(CommandError::failed("The PDF isn’t ready yet.")),
    };
    state.ocr.images.jobs.lock().unwrap().remove(&job);
    Ok(path)
}
