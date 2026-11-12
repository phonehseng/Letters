//! the decks the slide tests open (core/slides' own small decks made by
//! powerpoint and libreoffice) and throwaway folders for what they write.

use std::path::{Path, PathBuf};

pub fn deck_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../core/slides/tests/decks")
        .join(name)
}

/// an empty folder of the system's temp folder, new for each run.
pub fn scratch_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir()
        .join("letters-slides-shell-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}
