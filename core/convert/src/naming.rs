//! names for the files a conversion makes, chosen so nothing that is
//! already there is ever replaced.

use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::error::write_failed;
use crate::ConvertError;

/// "<base> - page 07.png": page numbers count from one and are padded so
/// the files sort in page order (page 10 after page 09, not after page 1).
pub(crate) fn page_file_name(base: &str, page: usize, last_page: usize, extension: &str) -> String {
    let width = (last_page + 1).to_string().len();
    format!("{base} - page {:0width$}.{extension}", page + 1)
}

/// creates `name` in `folder`, or "name (2)", "name (3)" and so on when it
/// is taken. the file is created only if it is new (checked by the file
/// system in the same step), so even a file appearing at the same moment
/// is never replaced.
pub(crate) fn create_unique_file(
    folder: &Path,
    name: &str,
) -> Result<(File, PathBuf), ConvertError> {
    let (stem, extension) = split_name(name);
    for attempt in 1..10_000 {
        let candidate = if attempt == 1 {
            name.to_string()
        } else {
            format!("{stem} ({attempt}){extension}")
        };
        let path = folder.join(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(write_failed(error)),
        }
    }
    Err(ConvertError::WriteFailed(
        "That folder already has too many files with this name. Choose another folder.".into(),
    ))
}

/// "notes.final.png" becomes ("notes.final", ".png").
fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    }
}

/// a file name made safe for every system: characters windows refuses
/// are replaced, and trailing dots and spaces dropped.
pub(crate) fn safe_base_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "Converted".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_numbers_are_padded_to_sort() {
        assert_eq!(page_file_name("Essay", 6, 11, "png"), "Essay - page 07.png");
        assert_eq!(page_file_name("Essay", 0, 0, "png"), "Essay - page 1.png");
    }

    #[test]
    fn taken_names_get_a_number() {
        let folder = std::env::temp_dir().join(format!("letters-naming-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let (_, first) = create_unique_file(&folder, "a.png").unwrap();
        let (_, second) = create_unique_file(&folder, "a.png").unwrap();
        assert_eq!(first.file_name().unwrap(), "a.png");
        assert_eq!(second.file_name().unwrap(), "a (2).png");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn unsafe_characters_are_replaced() {
        assert_eq!(safe_base_name("a/b: c?. "), "a-b- c-");
        assert_eq!(safe_base_name("..."), "Converted");
    }
}
