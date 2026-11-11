//! finds the folder that holds the pdfium library.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// looks in the places pdfium can be, in order:
/// 1. LETTERS_PDFIUM_DIR, for developers who keep it somewhere else
/// 2. the installed app's resources
/// 3. next to the program
/// 4. vendor/pdfium in the source tree, where tools/fetch-pdfium puts it
///
/// if none exists the last candidate is returned anyway, so the error
/// message names the folder a developer most likely needs to fill.
pub fn find_pdfium_dir(app: &AppHandle) -> PathBuf {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("LETTERS_PDFIUM_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    if let Ok(resources) = app.path().resource_dir() {
        candidates.push(resources.join("pdfium"));
    }
    if let Some(program_dir) = std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(Path::to_path_buf))
    {
        candidates.push(program_dir.join("pdfium"));
        candidates.push(program_dir);
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium"));

    first_with_library(&candidates).unwrap_or_else(|| candidates.pop().unwrap())
}

fn first_with_library(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|dir| LIBRARY_NAMES.iter().any(|name| dir.join(name).is_file()))
        .cloned()
}

const LIBRARY_NAMES: [&str; 3] = ["pdfium.dll", "libpdfium.dylib", "libpdfium.so"];
