//! where the text recognition engine is: the installed app's resources,
//! next to the program, or vendor/tesseract in the source tree, where
//! tools/fetch-tesseract puts it. the same order engine_location.rs uses
//! for pdfium.

use std::path::{Path, PathBuf};

#[cfg(windows)]
const PROGRAM: &str = "tesseract.exe";
#[cfg(not(windows))]
const PROGRAM: &str = "tesseract";

/// the folders to look in, most specific first: LETTERS_TESSERACT_DIR,
/// then `resources/tesseract`, then `tesseract` beside the program, then
/// the source tree's vendor folder (given by the caller).
pub fn candidates(
    from_environment: Option<PathBuf>,
    resources: Option<PathBuf>,
    program_dir: Option<PathBuf>,
    vendor: PathBuf,
) -> Vec<PathBuf> {
    let mut list = Vec::new();
    list.extend(from_environment);
    list.extend(resources.map(|dir| dir.join("tesseract")));
    list.extend(program_dir.map(|dir| dir.join("tesseract")));
    list.push(vendor);
    list.into_iter().map(|dir| plain_path(&dir)).collect()
}

/// the path without windows' `\\?\` prefix. tauri hands out resource
/// folders in that form, and tesseract can't open its language data
/// through one ("Could not initialize tesseract").
pub fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\UNC\") {
        Some(rest) => PathBuf::from(format!(r"\\{rest}")),
        None => PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text)),
    }
}

/// the first folder laid out like vendor/tesseract (the program in `bin`,
/// the languages in `tessdata`), or none.
pub fn first_installed(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|dir| is_installed(dir)).cloned()
}

pub fn is_installed(dir: &Path) -> bool {
    dir.join("bin").join(PROGRAM).is_file() && dir.join("tessdata").is_dir()
}

/// what the tab says when the engine is missing. a developer's build is
/// told the command that fills vendor/tesseract; an installed app is told
/// to reinstall, which is the only fix a student has.
pub fn missing_message(developer_build: bool) -> String {
    if developer_build {
        "Text recognition isn’t set up in this copy of Letters. In the Letters folder, run tools\\fetch-tesseract.ps1 (tools/fetch-tesseract.sh on a Mac), then start Letters again.".to_owned()
    } else {
        "Text recognition is missing from this copy of Letters. Install Letters again to fix this."
            .to_owned()
    }
}
