//! files opened from the system: a double click, "Open With", several
//! files at once on the command line, or a mac's open-document event.
//! each file gets a window of its own, through the existing window path
//! (and the single-instance path when Letters is already running).
//!
//! which screen a file opens in is decided by shell-core/src/associations.

use std::path::PathBuf;

use letters_shell_core::associations::{file_arguments, route_for, OpenRoute};
use tauri::AppHandle;

use crate::windows;

/// a window for each file on the command line, or one home window.
pub fn open_files_from_arguments(app: &AppHandle, arguments: &[String]) {
    open_files(app, file_arguments(arguments));
}

pub fn open_files(app: &AppHandle, files: Vec<PathBuf>) {
    if files.is_empty() {
        windows::open_window(app, None);
    }
    for file in files {
        windows::open_window(app, Some(file));
    }
}

/// where a file opens, for the ui (pdfs to the viewer, word documents to
/// the writer, pictures to New PDF from Picture, other office files as pdfs).
#[tauri::command(async)]
pub fn open_route(path: String) -> OpenRoute {
    route_for(std::path::Path::new(&path))
}

/// a mac hands files over as file:// addresses.
#[cfg(target_os = "macos")]
pub fn open_urls(app: &AppHandle, urls: &[tauri::Url]) {
    let files = urls
        .iter()
        .filter_map(|url| url.to_file_path().ok())
        .collect();
    open_files(app, files);
}
