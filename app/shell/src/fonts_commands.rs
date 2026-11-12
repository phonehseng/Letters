//! the font library: one command per question the Font Library and the
//! font picker ask. everything is in core/fonts; these only pass the
//! calls through and turn errors into `CommandError`.
//!
//! the library scans system fonts on its own thread when the app starts;
//! the ui follows it with `fonts_status`. nothing here contacts the
//! internet except `fonts_download` and `fonts_fetch_preview`, which the
//! person starts, after the one-time notice.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use letters_fonts::coverage::WRITING;
use letters_fonts::{
    AddReport, Catalogue, Downloader, FamilyDetails, FamilySummary, FontError, FontLibrary,
    LibraryStatus, MissingFont, Personal, Preview, Resolution, SearchQuery, SearchResults, Similar,
};
use letters_shell_core::fonts::library_paths;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};

use crate::errors::CommandError;

/// the library, shared by every window.
pub struct FontsState {
    library: Arc<FontLibrary>,
}

impl FontsState {
    /// finds the folders and starts scanning system fonts in the background.
    pub fn start(app: &AppHandle) -> Self {
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        let cache_dir = app.path().app_cache_dir().unwrap_or_default();
        let paths = library_paths(&data_dir, &cache_dir, &bundled_candidates(app));
        let library = FontLibrary::new(paths, Catalogue::shipped(), Downloader::library());
        library.start_scan();
        // the scan thread needs no stopping: it only reads, and the cache
        // is written through a temporary file, so quitting mid-scan is safe
        Self { library }
    }

    /// edit tab: the library's fonts as they are now, lent to edits that
    /// add letters to a document's font (see edit_flow_commands.rs)
    pub fn index(&self) -> Arc<letters_fonts::FontIndex> {
        self.library.index()
    }

    /// forms round two: pdf/a conversion embeds fonts from the library
    /// (standards_commands.rs)
    pub fn library(&self) -> &FontLibrary {
        &self.library
    }
}

/// where the shipped fonts can be: the installed app's resources, beside
/// the program, or vendor/fonts in the source tree (tools/fetch-fonts).
fn bundled_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(resources) = app.path().resource_dir() {
        candidates.push(resources.join("fonts"));
    }
    if let Some(program_dir) = std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(Path::to_path_buf))
    {
        candidates.push(program_dir.join("fonts"));
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/fonts"));
    candidates
}

impl From<FontError> for CommandError {
    fn from(error: FontError) -> Self {
        CommandError::failed(error.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyList {
    revision: u64,
    families: Vec<FamilySummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAnswer {
    revision: u64,
    results: SearchResults,
}

/// one writing system the language filter offers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritingChoice {
    id: &'static str,
    name: &'static str,
}

#[tauri::command(async)]
pub fn fonts_status(fonts: State<FontsState>) -> LibraryStatus {
    fonts.library.status()
}

#[tauri::command(async)]
pub fn fonts_families(fonts: State<FontsState>) -> FamilyList {
    let (revision, families) = fonts.library.families();
    FamilyList { revision, families }
}

#[tauri::command(async)]
pub fn fonts_search(fonts: State<FontsState>, query: SearchQuery) -> SearchAnswer {
    let (revision, results) = fonts.library.search(&query);
    SearchAnswer { revision, results }
}

#[tauri::command(async)]
pub fn fonts_details(
    fonts: State<FontsState>,
    name: String,
) -> Result<FamilyDetails, CommandError> {
    Ok(fonts.library.details(&name)?)
}

#[tauri::command(async)]
pub fn fonts_writing_systems() -> Vec<WritingChoice> {
    WRITING
        .iter()
        .map(|writing| WritingChoice {
            id: writing.id,
            name: writing.name,
        })
        .collect()
}

/// a face's bytes as raw binary (not json), for the web view's FontFace.
#[tauri::command(async)]
pub fn fonts_face_bytes(fonts: State<FontsState>, key: String) -> Result<Response, CommandError> {
    Ok(Response::new(fonts.library.face_bytes(&key)?))
}

#[tauri::command(async)]
pub fn fonts_preview(
    fonts: State<FontsState>,
    name: String,
    text: String,
    size: f32,
    weight: Option<u16>,
    italic: bool,
) -> Result<Preview, CommandError> {
    Ok(fonts.library.preview(&name, &text, size, weight, italic)?)
}

#[tauri::command(async)]
pub fn fonts_add(fonts: State<FontsState>, paths: Vec<PathBuf>) -> AddReport {
    fonts.library.add_fonts(&paths)
}

#[tauri::command(async)]
pub fn fonts_remove(fonts: State<FontsState>, name: String) -> Result<(), CommandError> {
    Ok(fonts.library.remove_family(&name)?)
}

#[tauri::command(async)]
pub fn fonts_set_favourite(fonts: State<FontsState>, name: String, favourite: bool) -> Personal {
    fonts.library.set_favourite(&name, favourite)
}

#[tauri::command(async)]
pub fn fonts_note_used(fonts: State<FontsState>, name: String) -> Personal {
    fonts.library.note_used(&name)
}

#[tauri::command(async)]
pub fn fonts_personal(fonts: State<FontsState>) -> Personal {
    fonts.library.personal_lists()
}

#[tauri::command(async)]
pub fn fonts_accept_download_notice(fonts: State<FontsState>) {
    fonts.library.accept_download_notice();
}

/// downloads go only after the notice was seen; the ui shows it first.
fn require_notice(fonts: &FontsState) -> Result<(), CommandError> {
    if fonts.library.status().download_notice_accepted {
        Ok(())
    } else {
        Err(CommandError::failed(
            "Downloading fonts contacts raw.githubusercontent.com. Choose Download again to continue.",
        ))
    }
}

#[tauri::command(async)]
pub fn fonts_download(fonts: State<FontsState>, name: String) -> Result<(), CommandError> {
    require_notice(&fonts)?;
    Ok(fonts.library.start_download(&name)?)
}

#[tauri::command(async)]
pub fn fonts_cancel_download(fonts: State<FontsState>, name: String) {
    fonts.library.cancel_download(&name);
}

#[tauri::command(async)]
pub fn fonts_fetch_preview(fonts: State<FontsState>, name: String) -> Result<(), CommandError> {
    require_notice(&fonts)?;
    Ok(fonts.library.fetch_preview(&name)?)
}

#[tauri::command(async)]
pub fn fonts_missing(fonts: State<FontsState>, names: Vec<String>) -> Vec<MissingFont> {
    fonts.library.missing_fonts(&names)
}

#[tauri::command(async)]
pub fn fonts_resolve(
    fonts: State<FontsState>,
    name: String,
    weight: u16,
    italic: bool,
) -> Resolution {
    fonts.library.resolve(&name, weight, italic)
}

#[tauri::command(async)]
pub fn fonts_similar(fonts: State<FontsState>, name: String) -> Vec<Similar> {
    fonts.library.similar(&name, 12)
}
