//! untitled code windows: "Open Block in Code Editor" shows code taken
//! from a document in a window of its own, before it has a file.
//!
//! windows are opened with a path (see app/shell/src/windows.rs), so the
//! text is handed over as a file in a folder of the temp directory that
//! only Letters writes to. the code screen sees a path inside that folder
//! and treats the window as untitled: Save asks where, and Recents never
//! lists it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::writer::save::write_atomically;

static NEXT_UNTITLED: AtomicU64 = AtomicU64::new(1);

/// the folder untitled texts are handed over in, under `temp`.
pub fn untitled_folder(temp: &Path) -> PathBuf {
    temp.join("letters-untitled")
}

pub fn is_untitled(temp: &Path, path: &Path) -> bool {
    path.starts_with(untitled_folder(temp))
}

/// writes `text` as "Untitled.<extension>" in a folder of its own and
/// returns its path. the extension decides the language the window starts in.
pub fn write_untitled(temp: &Path, text: &str, extension: &str) -> Result<PathBuf, String> {
    let number = NEXT_UNTITLED.fetch_add(1, Ordering::SeqCst);
    let folder = untitled_folder(temp).join(format!("{}-{number}", std::process::id()));
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let path = folder.join(format!("Untitled.{}", safe_extension(extension)));
    write_atomically(text.as_bytes(), &path)?;
    Ok(path)
}

// only letters and digits, so an extension can never reach outside the folder
fn safe_extension(extension: &str) -> String {
    let cleaned: String = extension
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(10)
        .collect();
    if cleaned.is_empty() {
        "txt".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untitled_text_is_written_where_it_is_recognised_as_untitled() {
        let temp = tempfile::tempdir().unwrap();
        let path = write_untitled(temp.path(), "int x;\n", "c").unwrap();
        assert_eq!(path.file_name().unwrap(), "Untitled.c");
        assert_eq!(fs::read_to_string(&path).unwrap(), "int x;\n");
        assert!(is_untitled(temp.path(), &path));
        assert!(!is_untitled(temp.path(), Path::new("/home/sam/Untitled.c")));
    }

    #[test]
    fn odd_extensions_are_cleaned() {
        assert_eq!(safe_extension("../../evil"), "evil");
        assert_eq!(safe_extension(""), "txt");
    }
}
