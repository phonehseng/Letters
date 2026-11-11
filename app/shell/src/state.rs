//! everything the running app remembers, shared by all windows.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use letters_pdf::{DocumentId, RenderWorker};
use tauri::{AppHandle, Manager};

use crate::engine_location::find_pdfium_dir;
use letters_shell_core::recents::RecentFiles;

/// the most memory finished tiles may use, across every window.
///
/// a tile is about 0.1 mb and a screen shows around a dozen, so 24 mb
/// keeps roughly twenty screens ready for scrolling back. measured:
/// with 96 mb the app held on to over a thousand tiles nobody would
/// look at again, for no visible gain.
const TILE_CACHE_BUDGET_BYTES: usize = 24 * 1024 * 1024;

pub struct AppState {
    /// the engine, or the reason it could not start. keeping the reason
    /// lets the ui explain the problem instead of the app just closing
    worker: Result<RenderWorker, String>,
    /// which documents each window has open, so they can be closed with it
    documents_by_window: Mutex<HashMap<String, Vec<DocumentId>>>,
    /// a file a new window should open as soon as its ui is ready
    startup_files: Mutex<HashMap<String, PathBuf>>,
    pub recents: Mutex<RecentFiles>,
    pub thumbnail_dir: PathBuf,
}

impl AppState {
    pub fn start(app: &AppHandle) -> Self {
        let worker = RenderWorker::start(&find_pdfium_dir(app), TILE_CACHE_BUDGET_BYTES)
            .map_err(|error| error.to_string());
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        let cache_dir = app.path().app_cache_dir().unwrap_or_default();
        Self {
            worker,
            documents_by_window: Mutex::new(HashMap::new()),
            startup_files: Mutex::new(HashMap::new()),
            recents: Mutex::new(RecentFiles::load(data_dir.join("recent-files.json"))),
            thumbnail_dir: cache_dir.join("thumbnails"),
        }
    }

    pub fn worker(&self) -> Result<&RenderWorker, String> {
        self.worker.as_ref().map_err(Clone::clone)
    }

    pub fn remember_document(&self, window: &str, id: DocumentId) {
        let mut documents = self.documents_by_window.lock().unwrap();
        documents.entry(window.to_owned()).or_default().push(id);
    }

    pub fn forget_document(&self, window: &str, id: DocumentId) {
        let mut documents = self.documents_by_window.lock().unwrap();
        if let Some(ids) = documents.get_mut(window) {
            ids.retain(|other| *other != id);
        }
    }

    pub fn close_documents_of_window(&self, window: &str) {
        let ids = self.documents_by_window.lock().unwrap().remove(window);
        if let (Some(ids), Ok(worker)) = (ids, self.worker()) {
            for id in ids {
                worker.close(id);
            }
        }
    }

    pub fn set_startup_file(&self, window: &str, file: PathBuf) {
        let mut files = self.startup_files.lock().unwrap();
        files.insert(window.to_owned(), file);
    }

    pub fn take_startup_file(&self, window: &str) -> Option<PathBuf> {
        self.startup_files.lock().unwrap().remove(window)
    }
}
