//! Search All Documents: the commands of the search screen and its
//! settings, and the bridge that lets the indexing job read pdfs through
//! the app's one pdf engine.
//!
//! the index and its job live in shell-core/src/library_search; this file
//! only connects them to the running app.

use std::path::Path;

use letters_pdf::{DocumentId, PageText, PdfError};
use letters_search::{ExtractError, OutlineEntry, PdfText, SearchRequest, SearchResults};
use letters_shell_core::library_search::job::JobOptions;
use letters_shell_core::library_search::settings::{add_folder, remove_folder};
use letters_shell_core::library_search::{JobStatus, LibrarySearch, LibrarySettings, SourceLists};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::errors::CommandError;
use crate::state::AppState;

pub struct SearchState {
    /// the search, or why it could not start (the screen explains it)
    library: Result<LibrarySearch, String>,
    index_folder: String,
}

impl SearchState {
    pub fn start(app: &AppHandle) -> SearchState {
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        let index_folder = data_dir.join("search index");
        let pdf = EnginePdf {
            app: app.clone(),
            open: None,
        };
        let sources_app = app.clone();
        let library = LibrarySearch::start(
            &index_folder,
            data_dir.join("library-search.json"),
            Box::new(pdf),
            Box::new(move || known_files(&sources_app)),
            JobOptions::default(),
        );
        SearchState {
            library,
            index_folder: index_folder.to_string_lossy().into_owned(),
        }
    }

    fn library(&self) -> Result<&LibrarySearch, CommandError> {
        self.library
            .as_ref()
            .map_err(|e| CommandError::failed(e.clone()))
    }

    /// organise: a moved or renamed file keeps its index entry.
    pub fn document_moved(&self, from: &str, to: &str) {
        if let Ok(library) = &self.library {
            library.document_moved(from, to);
        }
    }

    /// writes a last checkpoint; called as the app quits.
    pub fn stop(&self) {
        if let Ok(library) = &self.library {
            library.stop();
        }
    }
}

/// the recent files and pinned folders the app already keeps.
fn known_files(app: &AppHandle) -> SourceLists {
    let state = app.state::<AppState>();
    let recent_files = state
        .recents
        .lock()
        .map(|r| r.list().iter().map(|f| f.path.clone()).collect())
        .unwrap_or_default();
    let pinned_folders = state
        .pinned_folders
        .lock()
        .map(|p| p.paths().to_vec())
        .unwrap_or_default();
    SourceLists {
        recent_files,
        pinned_folders,
    }
}

/// the job's way into the pdf engine. it opens files on its own, apart
/// from any window, and closes each before the next.
struct EnginePdf {
    app: AppHandle,
    open: Option<DocumentId>,
}

impl EnginePdf {
    fn worker(&self) -> Result<letters_pdf::RenderWorker, ExtractError> {
        self.app
            .state::<AppState>()
            .worker()
            .cloned()
            .map_err(ExtractError::Unreadable)
    }
}

fn extract_error(error: PdfError) -> ExtractError {
    match error {
        PdfError::PasswordRequired => ExtractError::Locked,
        other => ExtractError::Unreadable(other.to_string()),
    }
}

/// a page's characters as text, with a line break where a line ends.
fn page_string(text: &PageText) -> String {
    let mut out = String::with_capacity(text.chars.len() + text.lines.len());
    let mut line = None;
    for ch in &text.chars {
        if line.is_some_and(|l| l != ch.line) {
            out.push('\n');
        }
        line = Some(ch.line);
        out.push(ch.ch);
    }
    out
}

impl PdfText for EnginePdf {
    fn open(&mut self, path: &Path) -> Result<u32, ExtractError> {
        self.close();
        // no password is ever passed: locked files are indexed by name only
        let info = self.worker()?.open(path, None).map_err(extract_error)?;
        self.open = Some(info.id);
        Ok(info.page_sizes.len() as u32)
    }

    fn page_text(&mut self, page: u32) -> Result<String, ExtractError> {
        let id = self.open.ok_or(ExtractError::Stopped)?;
        let text = self.worker()?.page_text(id, page).map_err(extract_error)?;
        Ok(page_string(&text))
    }

    fn outline(&mut self) -> Vec<OutlineEntry> {
        let (Some(id), Ok(worker)) = (self.open, self.worker()) else {
            return Vec::new();
        };
        worker
            .outline(id)
            .unwrap_or_default()
            .into_iter()
            .map(|item| OutlineEntry {
                title: item.title,
                page: item.page,
                level: item.level,
            })
            .collect()
    }

    fn close(&mut self) {
        if let (Some(id), Ok(worker)) = (self.open.take(), self.worker()) {
            worker.close(id);
        }
    }
}

/// what the settings section and the search screen show about the index.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryOverview {
    settings: LibrarySettings,
    status: JobStatus,
    index_folder: String,
    /// every pinned folder, so each can be chosen for searching
    pinned_folders: Vec<String>,
}

fn overview(state: &SearchState, app: &AppState) -> Result<LibraryOverview, CommandError> {
    let library = state.library()?;
    Ok(LibraryOverview {
        settings: library.settings(),
        status: library.status(),
        index_folder: state.index_folder.clone(),
        pinned_folders: app.pinned_folders.lock().unwrap().paths().to_vec(),
    })
}

#[tauri::command(async)]
pub fn library_search(
    search: State<SearchState>,
    request: SearchRequest,
) -> Result<SearchResults, CommandError> {
    search
        .library()?
        .search(&request)
        .map_err(CommandError::failed)
}

#[tauri::command(async)]
pub fn library_overview(
    search: State<SearchState>,
    app: State<AppState>,
) -> Result<LibraryOverview, CommandError> {
    overview(&search, &app)
}

/// asks the job to look for new and changed files now.
#[tauri::command(async)]
pub fn library_refresh(search: State<SearchState>) -> Result<(), CommandError> {
    search.library()?.wake();
    Ok(())
}

/// one change to what is searched. `action` names it, so the settings
/// screen needs a single command: "addFolder", "removeFolder",
/// "excludeFolder", "includeFolder", "searchPinned", "skipPinned",
/// "includeRecents", "skipRecents".
#[tauri::command(async)]
pub fn library_change(
    search: State<SearchState>,
    app: State<AppState>,
    action: String,
    path: Option<String>,
) -> Result<LibraryOverview, CommandError> {
    let path = path.unwrap_or_default();
    let library = search.library()?;
    library.update_settings(|s| match action.as_str() {
        "addFolder" => {
            add_folder(&mut s.library_folders, &path);
            remove_folder(&mut s.excluded_folders, &path);
        }
        "removeFolder" => {
            remove_folder(&mut s.library_folders, &path);
        }
        "excludeFolder" => {
            add_folder(&mut s.excluded_folders, &path);
        }
        "includeFolder" => {
            remove_folder(&mut s.excluded_folders, &path);
        }
        "searchPinned" => {
            add_folder(&mut s.searched_pinned_folders, &path);
        }
        "skipPinned" => {
            remove_folder(&mut s.searched_pinned_folders, &path);
        }
        "includeRecents" => s.include_recents = true,
        "skipRecents" => s.include_recents = false,
        _ => {}
    });
    overview(&search, &app)
}

/// "pause", "resume", "rebuild" or "delete" the index.
#[tauri::command(async)]
pub fn library_control(
    search: State<SearchState>,
    app: State<AppState>,
    action: String,
) -> Result<LibraryOverview, CommandError> {
    let library = search.library()?;
    match action.as_str() {
        "pause" => library.pause(),
        "resume" => library.resume(),
        "rebuild" => library.rebuild(),
        "delete" => library.delete_index().map_err(CommandError::failed)?,
        other => return Err(CommandError::failed(format!("Unknown action {other}."))),
    }
    overview(&search, &app)
}
