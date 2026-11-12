//! the Scan tab: text recognition (OCR) with core/ocr (letters-ocr). New
//! from Images is in ocr_images_commands.rs.
//!
//! recognising pages takes seconds each, so it runs on its own thread
//! while the document stays usable. the ui starts a run (`ocr_start`),
//! asks how it is doing (`ocr_job_status`), can cancel it (`ocr_cancel`,
//! which stops tesseract at once), and when it has finished adds every
//! page's text in one edit (`ocr_apply`, through `state.edit`), so the run
//! is one undo step and a cancelled or failed run changes nothing.

use std::path::{Path, PathBuf};
use std::thread;

use letters_edit::page_objects;
use letters_ocr::{
    add_text_layer, language_name, remove_text_layer, CancelFlag, Engine, OcrError, OcrOptions,
    OcrPage, PageImage,
};
use letters_pdf::{DocumentId, PageSize};
use letters_shell_core::ocr::apply_layers;
use letters_shell_core::ocr::engine_folder::{candidates, first_installed, missing_message};
use letters_shell_core::ocr::job::PageReader;
use letters_shell_core::ocr::jobs::{JobStatus, OcrJobs};
use letters_shell_core::ocr::overview::{scan_overview, ScanOverview};
use letters_shell_core::ocr::render::render_page;
use letters_shell_core::ocr::summary::OcrSummary;
use letters_shell_core::ocr::words::{
    correct_word, overlay_words, words_to_correct, OverlayPage, RecognizedPages,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::ocr_images_commands::ImageJobs;
use crate::state::AppState;

/// what the Scan tab keeps while the app runs. it lives inside AppState
/// (`state.ocr`), so a closed document's run can be stopped there.
pub struct OcrState {
    engine: Result<Engine, String>,
    pub jobs: OcrJobs,
    pub recognized: RecognizedPages,
    /// New from Images runs (ocr_images_commands.rs)
    pub images: ImageJobs,
    /// where new pdfs are written before they are opened
    pub new_documents: PathBuf,
}

impl OcrState {
    pub fn start(app: &AppHandle) -> Self {
        let resources = app.path().resource_dir().ok();
        let program_dir = std::env::current_exe()
            .ok()
            .and_then(|program| program.parent().map(Path::to_path_buf));
        let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/tesseract");
        let folders = candidates(
            std::env::var_os("LETTERS_TESSERACT_DIR").map(PathBuf::from),
            resources,
            program_dir,
            vendor,
        );
        let engine = match first_installed(&folders) {
            Some(folder) => Engine::locate(&folder).map_err(|error| error.to_string()),
            None => Err(missing_message(cfg!(debug_assertions))),
        };
        let cache = app.path().app_cache_dir().unwrap_or_default();
        Self {
            engine,
            jobs: OcrJobs::new(),
            recognized: RecognizedPages::default(),
            images: ImageJobs::default(),
            new_documents: cache.join("new documents"),
        }
    }

    /// the engine, or the sentence saying why it is missing, as a command error.
    pub fn engine(&self) -> Result<&Engine, CommandError> {
        self.engine_or_message().map_err(CommandError::failed)
    }

    /// the engine, or the sentence saying why it is missing.
    pub fn engine_or_message(&self) -> Result<&Engine, String> {
        self.engine.as_ref().map_err(Clone::clone)
    }
}

/// a document closed: its run stops and its words are forgotten.
pub fn forget_document(state: &AppState, id: DocumentId) {
    state.ocr.jobs.forget_document(id.0);
    state.ocr.recognized.forget_document(id.0);
}

fn failed(error: impl ToString) -> CommandError {
    CommandError::failed(error.to_string())
}

// ---- the engine ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    code: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrStatus {
    available: bool,
    /// why it isn't, and what to do
    message: Option<String>,
    languages: Vec<Language>,
    can_detect_orientation: bool,
}

/// whether text recognition can run here, and in which languages.
#[tauri::command(async)]
pub fn ocr_status(state: State<AppState>) -> OcrStatus {
    match &state.ocr.engine {
        Ok(engine) => OcrStatus {
            available: true,
            message: None,
            languages: engine
                .installed_languages()
                .into_iter()
                .map(|code| Language {
                    name: language_name(&code),
                    code,
                })
                .collect(),
            can_detect_orientation: engine.can_detect_orientation(),
        },
        Err(message) => OcrStatus {
            available: false,
            message: Some(message.clone()),
            languages: Vec::new(),
            can_detect_orientation: false,
        },
    }
}

/// which pages are scans and which already have recognised text.
#[tauri::command(async)]
pub fn ocr_scan_overview(state: State<AppState>, id: u64) -> Result<ScanOverview, CommandError> {
    state.read_document(DocumentId(id), |document| Ok(scan_overview(document)))
}

// ---- a recognition run ----

/// reads pages of an open document: each is drawn at 300 dpi by our own
/// renderer and read by tesseract.
struct RenderedReader<'a> {
    state: &'a AppState,
    document: DocumentId,
    sizes: Vec<PageSize>,
    options: OcrOptions,
}

impl PageReader for RenderedReader<'_> {
    fn read(&mut self, page: u32, cancel: &CancelFlag) -> Result<OcrPage, OcrError> {
        let worker = self.state.worker().map_err(OcrError::EngineFailed)?;
        let engine = self
            .state
            .ocr
            .engine
            .as_ref()
            .map_err(|message| OcrError::EngineMissing(message.clone()))?;
        let size = *self
            .sizes
            .get(page as usize)
            .ok_or(OcrError::UnknownPage(page))?;
        let picture: PageImage = render_page(worker, self.document, page, size, cancel)?;
        letters_ocr::recognize(engine, &picture, &self.options, cancel)
    }
}

/// starts reading `pages` (counted from 0) in the background. answers the
/// run's number for `ocr_job_status`, `ocr_cancel` and `ocr_apply`.
#[tauri::command(async)]
pub fn ocr_start(
    app: AppHandle,
    state: State<AppState>,
    id: u64,
    pages: Vec<u32>,
    options: OcrOptions,
) -> Result<u64, CommandError> {
    // a missing language or engine is said now, before anything runs
    state
        .ocr
        .engine()?
        .check_languages(&options.languages)
        .map_err(failed)?;
    let shown = state.documents.state(DocumentId(id))?;
    if pages.is_empty() {
        return Err(CommandError::failed("Choose at least one page to read."));
    }
    if let Some(page) = pages
        .iter()
        .find(|page| **page as usize >= shown.pages.len())
    {
        return Err(CommandError::failed(format!(
            "Page {} doesn’t exist in this document.",
            page + 1
        )));
    }
    let sizes: Vec<PageSize> = shown
        .pages
        .iter()
        .map(|page| PageSize {
            width_pt: page.width,
            height_pt: page.height,
        })
        .collect();
    let (job, cancel) = state
        .ocr
        .jobs
        .start(id, pages.len())
        .map_err(CommandError::failed)?;
    thread::Builder::new()
        .name("text-recognition".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            let state = state.inner();
            let mut reader = RenderedReader {
                state,
                document: DocumentId(id),
                sizes,
                options,
            };
            state.ocr.jobs.run(job, &pages, &mut reader, &cancel);
        })
        .map_err(failed)?;
    Ok(job)
}

#[tauri::command(async)]
pub fn ocr_job_status(state: State<AppState>, job: u64) -> Result<JobStatus, CommandError> {
    state
        .ocr
        .jobs
        .status(job)
        .ok_or_else(|| CommandError::failed("That text recognition is no longer running."))
}

/// stops a run. tesseract is stopped straight away; nothing is added.
#[tauri::command(async)]
pub fn ocr_cancel(state: State<AppState>, job: u64) -> bool {
    state.ocr.jobs.cancel(job)
}

/// forgets a run that ended without being added (cancelled or failed).
#[tauri::command(async)]
pub fn ocr_discard(state: State<AppState>, job: u64) {
    state.ocr.jobs.forget(job);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedText {
    document: DocumentState,
    summary: OcrSummary,
}

/// adds every page a finished run read, as one edit and one undo step.
#[tauri::command(async)]
pub fn ocr_apply(state: State<AppState>, id: u64, job: u64) -> Result<AppliedText, CommandError> {
    let summary = state
        .ocr
        .jobs
        .status(job)
        .and_then(|status| status.summary)
        .unwrap_or_default();
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
    let page_count = state.documents.state(DocumentId(id))?.pages.len();
    if pages.iter().any(|(page, _)| *page as usize >= page_count) {
        return Err(CommandError::failed(
            "Pages were removed while the text was being read, so it wasn’t added. Choose Recognize Text again.",
        ));
    }
    let shown = state.edit(DocumentId(id), |doc| apply_layers(doc, &pages))?;
    for (page, words) in pages {
        state.ocr.recognized.remember(id, page, words);
    }
    Ok(AppliedText {
        document: shown,
        summary,
    })
}

/// takes Letters' recognised text off pages. refuses when there is none,
/// instead of making an undo step that changes nothing.
#[tauri::command(async)]
pub fn ocr_remove_text(
    state: State<AppState>,
    id: u64,
    pages: Vec<u32>,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |doc| {
        let mut removed = 0;
        for page in &pages {
            if remove_text_layer(doc, *page).map_err(|error| error.to_string())? {
                removed += 1;
            }
        }
        if removed == 0 {
            return Err("These pages have no text that Letters recognized.".to_owned());
        }
        Ok(())
    })
}

/// the recognised words of a page, for "Show Recognized Text".
#[tauri::command(async)]
pub fn ocr_overlay(
    state: State<AppState>,
    id: u64,
    page: u32,
) -> Result<OverlayPage, CommandError> {
    overlay_for(&state, id, page)
}

fn overlay_for(state: &AppState, id: u64, page: u32) -> Result<OverlayPage, CommandError> {
    let stored = state.ocr.recognized.get(id, page);
    state.read_document(DocumentId(id), |document| {
        let objects = page_objects(document, page as usize).map_err(failed)?;
        Ok(overlay_words(page, stored.as_ref(), &objects))
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectedWord {
    document: DocumentState,
    overlay: OverlayPage,
}

/// rewrites one recognised word: the page's text layer is written again
/// from the corrected words, as one undo step.
#[tauri::command(async)]
pub fn ocr_correct_word(
    state: State<AppState>,
    id: u64,
    page: u32,
    index: usize,
    text: String,
) -> Result<CorrectedWord, CommandError> {
    let stored = state.ocr.recognized.get(id, page);
    let mut corrected: Option<OcrPage> = None;
    let shown = state.edit(DocumentId(id), |doc| {
        let objects = page_objects(doc, page as usize).map_err(|error| error.to_string())?;
        let words = correct_word(&words_to_correct(stored.as_ref(), &objects), index, &text)?;
        add_text_layer(doc, page, &words).map_err(|error| error.to_string())?;
        corrected = Some(words);
        Ok::<(), String>(())
    })?;
    if let Some(words) = corrected {
        state.ocr.recognized.remember(id, page, words);
    }
    let overlay = overlay_for(&state, id, page)?;
    Ok(CorrectedWord {
        document: shown,
        overlay,
    })
}
