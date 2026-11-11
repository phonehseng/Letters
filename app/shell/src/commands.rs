//! the functions the ui can call.
//!
//! every command is marked `async` so it runs on a background thread:
//! opening a big file must never freeze a window.

use std::fs;
use std::path::{Path, PathBuf};

use letters_pdf::{DocumentId, DocumentInfo, PageSize, PdfError, RenderWorker, TileKey};
use serde::Serialize;
use tauri::{AppHandle, State, Window};

use crate::state::AppState;
use crate::windows;
use letters_shell_core::recents::{thumbnail_name, RecentFile};

/// thumbnails on the home screen are drawn about this wide.
const THUMBNAIL_WIDTH_PIXELS: f32 = 240.0;

/// an error the ui can both show and react to.
///
/// `kind` lets the ui tell "ask for a password" apart from "show a message".
#[derive(Debug, Serialize)]
pub struct CommandError {
    kind: &'static str,
    message: String,
}

impl From<PdfError> for CommandError {
    fn from(error: PdfError) -> Self {
        let kind = match error {
            PdfError::PasswordRequired => "password-required",
            PdfError::EngineUnavailable(_) => "engine-unavailable",
            _ => "failed",
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}

fn engine_unavailable(reason: String) -> CommandError {
    CommandError {
        kind: "engine-unavailable",
        message: reason,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedDocument {
    id: u64,
    name: String,
    path: String,
    /// page sizes in points (1/72 inch)
    pages: Vec<PageDto>,
}

#[derive(Serialize)]
pub struct PageDto {
    width: f32,
    height: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStats {
    open_documents: usize,
    cached_tiles: usize,
    cached_bytes: usize,
}

#[tauri::command(async)]
pub fn open_document(
    window: Window,
    state: State<AppState>,
    path: String,
    password: Option<String>,
) -> Result<OpenedDocument, CommandError> {
    let worker = state.worker().map_err(engine_unavailable)?;
    let path = PathBuf::from(path);
    let info = worker.open(&path, password.as_deref())?;
    state.remember_document(window.label(), info.id);

    let recent = state
        .recents
        .lock()
        .unwrap()
        .add(&path, info.page_sizes.len());
    save_thumbnail(worker, &info, &state.thumbnail_dir, &path);

    Ok(OpenedDocument {
        id: info.id.0,
        name: recent.name,
        path: recent.path,
        pages: info.page_sizes.iter().map(page_dto).collect(),
    })
}

#[tauri::command(async)]
pub fn close_document(window: Window, state: State<AppState>, id: u64) {
    let id = DocumentId(id);
    state.forget_document(window.label(), id);
    if let Ok(worker) = state.worker() {
        worker.close(id);
    }
}

/// the file this window was opened for, if any. answers only once, so
/// reloading the window does not reopen the file a second time.
#[tauri::command(async)]
pub fn take_startup_file(window: Window, state: State<AppState>) -> Option<String> {
    state
        .take_startup_file(window.label())
        .map(|file| file.to_string_lossy().into_owned())
}

#[tauri::command(async)]
pub fn new_window(app: AppHandle, path: Option<String>) {
    windows::open_window(&app, path.map(PathBuf::from));
}

/// recent files that still exist on disk, newest first.
#[tauri::command(async)]
pub fn recent_files(state: State<AppState>) -> Vec<RecentFile> {
    let recents = state.recents.lock().unwrap();
    recents
        .list()
        .iter()
        .filter(|file| Path::new(&file.path).is_file())
        .cloned()
        .collect()
}

#[tauri::command(async)]
pub fn forget_recent_file(state: State<AppState>, path: String) {
    state.recents.lock().unwrap().remove(&path);
}

#[tauri::command(async)]
pub fn engine_stats(state: State<AppState>) -> Result<EngineStats, CommandError> {
    let stats = state.worker().map_err(engine_unavailable)?.stats()?;
    Ok(EngineStats {
        open_documents: stats.open_documents,
        cached_tiles: stats.cached_tiles,
        cached_bytes: stats.cached_bytes,
    })
}

fn page_dto(size: &PageSize) -> PageDto {
    PageDto {
        width: size.width_pt,
        height: size.height_pt,
    }
}

/// draws a small picture of the first page for the home screen.
///
/// a failed thumbnail is not worth failing the open for, so errors are ignored.
fn save_thumbnail(worker: &RenderWorker, info: &DocumentInfo, thumbnail_dir: &Path, path: &Path) {
    let Some(first_page) = info.page_sizes.first() else {
        return;
    };
    let scale = THUMBNAIL_WIDTH_PIXELS / first_page.width_pt.max(1.0);
    let key = TileKey {
        document: info.id,
        page: 0,
        scale_milli: (scale * 1000.0) as u32,
        column: 0,
        row: 0,
    };
    if let Ok(tile) = worker.tile(key) {
        let _ = fs::create_dir_all(thumbnail_dir);
        let _ = fs::write(
            thumbnail_dir.join(format!("{}.png", thumbnail_name(path))),
            tile.as_slice(),
        );
    }
}
