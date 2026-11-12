//! everything the running app remembers, shared by all windows.

use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Mutex;

use letters_pdf::{DocumentId, RenderWorker};
use tauri::{AppHandle, Manager};

use crate::documents::{DocumentState, OpenDocuments};
use crate::engine_location::find_pdfium_dir;
use crate::errors::CommandError;
use letters_shell_core::pinned::PinnedFolders;
use letters_shell_core::recents::RecentFiles;
use letters_shell_core::settings::{system_user_name, Settings};

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
    /// every open document with its working copy and undo history
    pub documents: OpenDocuments,
    /// which documents each window has open, so they can be closed with it
    documents_by_window: Mutex<HashMap<String, Vec<DocumentId>>>,
    /// a file a new window should open as soon as its ui is ready
    startup_files: Mutex<HashMap<String, PathBuf>>,
    /// which file each window shows, so a file opens in one window only
    pub window_files: Mutex<letters_shell_core::window_files::WindowFiles>,
    pub recents: Mutex<RecentFiles>,
    pub pinned_folders: Mutex<PinnedFolders>,
    /// the author name on comments, and any later settings
    settings: Mutex<Settings>,
    pub thumbnail_dir: PathBuf,
    /// protect: each open document's password settings and review copies
    pub protection: letters_shell_core::protect::protection::ProtectionTable,
    /// scan: text recognition runs, the words they found, new from images
    pub ocr: crate::ocr_commands::OcrState,
}

impl AppState {
    pub fn start(app: &AppHandle) -> Self {
        let worker = RenderWorker::start(&find_pdfium_dir(app), TILE_CACHE_BUDGET_BYTES)
            .map_err(|error| error.to_string());
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        let cache_dir = app.path().app_cache_dir().unwrap_or_default();
        Self {
            worker,
            documents: OpenDocuments::new(cache_dir.join("working copies")),
            documents_by_window: Mutex::new(HashMap::new()),
            startup_files: Mutex::new(HashMap::new()),
            window_files: Mutex::default(),
            recents: Mutex::new(RecentFiles::load(data_dir.join("recent-files.json"))),
            pinned_folders: Mutex::new(PinnedFolders::load(data_dir.join("pinned-folders.json"))),
            settings: Mutex::new(Settings::load(data_dir.join("settings.json"))),
            thumbnail_dir: cache_dir.join("thumbnails"),
            protection: Default::default(),
            ocr: crate::ocr_commands::OcrState::start(app),
        }
    }

    pub fn worker(&self) -> Result<&RenderWorker, String> {
        self.worker.as_ref().map_err(Clone::clone)
    }

    /// changes a document. this is the one door every editing tool uses.
    ///
    /// `edit` gets the document's working copy loaded with lopdf and changes
    /// it in memory. if it returns an error (its message is shown to the
    /// person as it is) or anything else fails, the document stays exactly
    /// as it was. otherwise the change becomes an undo step, the windows'
    /// old tiles are dropped, and the new state is returned for the ui.
    /// the person's file is not touched until they save.
    ///
    /// see `rotate_pages` in page_commands.rs for the example to copy.
    pub fn edit<E: Display>(
        &self,
        id: DocumentId,
        edit: impl FnOnce(&mut lopdf::Document) -> Result<(), E>,
    ) -> Result<DocumentState, CommandError> {
        let worker = self.worker().map_err(CommandError::engine_unavailable)?;
        self.documents.edit(worker, id, edit)
    }

    /// reads a document as it is shown now, for tools that make other
    /// files from it (extract, split). the document itself is not changed
    /// and no undo step is made. `read` reports its own errors.
    pub fn read_document<T>(
        &self,
        id: DocumentId,
        read: impl FnOnce(&lopdf::Document) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        self.documents.read(id, read)
    }

    /// the name written on new comments and marks: the one the person
    /// chose, or the name they sign in to the computer with.
    pub fn author_name(&self) -> String {
        let settings = self.settings.lock().unwrap();
        settings.author_name(system_user_name().as_deref())
    }

    /// returns the name now in use (a blank one means the computer's).
    pub fn set_author_name(&self, name: &str) -> String {
        let mut settings = self.settings.lock().unwrap();
        settings.set_author_name(name, system_user_name().as_deref())
    }

    pub fn remember_document(&self, window: &str, id: DocumentId) {
        let mut documents = self.documents_by_window.lock().unwrap();
        documents.entry(window.to_owned()).or_default().push(id);
    }

    /// closes one document of a window and throws away its session.
    pub fn close_document(&self, window: &str, id: DocumentId) {
        let mut by_window = self.documents_by_window.lock().unwrap();
        if let Some(ids) = by_window.get_mut(window) {
            ids.retain(|other| *other != id);
        }
        drop(by_window);
        // protect: review copies live in the session's folder, so they close first
        crate::protect_commands::forget_document(self, id);
        // scan: a recognition run of the document stops with it
        crate::ocr_commands::forget_document(self, id);
        self.documents.close(self.worker.as_ref().ok(), id);
    }

    pub fn close_documents_of_window(&self, window: &str) {
        let ids = self.documents_by_window.lock().unwrap().remove(window);
        for id in ids.unwrap_or_default() {
            crate::protect_commands::forget_document(self, id);
            crate::ocr_commands::forget_document(self, id);
            self.documents.close(self.worker.as_ref().ok(), id);
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
