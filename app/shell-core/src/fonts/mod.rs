//! the font library's place in the app: where its folders are and where
//! the fonts shipped with Letters can be found. the library itself is
//! core/fonts (crate letters-fonts); the commands are
//! app/shell/src/fonts_commands.rs.

use std::path::{Path, PathBuf};

use letters_fonts::scan::system_font_folders;
use letters_fonts::LibraryPaths;

/// the library's folders inside the app's data and cache folders.
/// `bundled_candidates` are the places the shipped fonts may be, best
/// first; the first that holds fonts is used.
pub fn library_paths(data_dir: &Path, cache_dir: &Path, bundled_candidates: &[PathBuf]) -> LibraryPaths {
    LibraryPaths {
        data: data_dir.join("fonts"),
        cache: cache_dir.join("fonts"),
        bundled: bundled_fonts_folder(bundled_candidates),
        system: system_font_folders(),
    }
}

/// the first folder that holds at least one family folder of fonts, as
/// tools/fetch-fonts lays them out (vendor/fonts/<family>/...).
pub fn bundled_fonts_folder(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|folder| {
            std::fs::read_dir(folder).is_ok_and(|entries| {
                entries.flatten().any(|entry| {
                    entry.path().is_dir()
                        && std::fs::read_dir(entry.path()).is_ok_and(|files| {
                            files
                                .flatten()
                                .any(|file| letters_fonts::container::has_font_extension(&file.path()))
                        })
                })
            })
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_folder_with_fonts_is_used() {
        let root = std::env::temp_dir().join(format!("letters-bundled-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let empty = root.join("empty");
        let full = root.join("full");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(full.join("carlito")).unwrap();
        std::fs::write(full.join("carlito").join("Carlito-Regular.ttf"), b"x").unwrap();
        let candidates = vec![root.join("missing"), empty.clone(), full.clone()];
        assert_eq!(bundled_fonts_folder(&candidates), Some(full));
        assert_eq!(bundled_fonts_folder(&[empty]), None);
        let paths = library_paths(&root.join("data"), &root.join("cache"), &candidates);
        assert_eq!(paths.data, root.join("data").join("fonts"));
        assert_eq!(paths.custom(), root.join("data").join("fonts").join("added"));
        std::fs::remove_dir_all(&root).ok();
    }
}
