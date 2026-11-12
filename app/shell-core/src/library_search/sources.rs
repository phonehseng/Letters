//! which files Search All Documents reads: recent files, the folders the
//! person chose (with every folder inside them), less the folders they
//! said never to index.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use letters_search::index::path_key;
use letters_search::DocKind;

use super::settings::LibrarySettings;

/// one walk stops after this many files, so pointing the library at a
/// whole drive cannot make the job run for hours or fill memory.
pub const MAX_FILES: usize = 50_000;
/// folders deeper than this are not entered (and loops through links end)
const MAX_DEPTH: usize = 24;

/// every file the index should hold, each once, in a stable order.
pub fn files_to_index(
    settings: &LibrarySettings,
    recent_files: &[String],
    pinned_folders: &[String],
) -> Vec<PathBuf> {
    let excluded: Vec<String> = settings
        .excluded_folders
        .iter()
        .map(|f| folder_key(f))
        .collect();
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut take = |path: PathBuf, files: &mut Vec<PathBuf>| {
        if files.len() < MAX_FILES
            && DocKind::of_path(&path).is_some()
            && !is_excluded(&path, &excluded)
            && seen.insert(path_key(&path.to_string_lossy()))
        {
            files.push(path);
        }
    };
    if settings.include_recents {
        for file in recent_files {
            let path = PathBuf::from(file);
            if path.is_file() {
                take(path, &mut files);
            }
        }
    }
    let searched_pinned = pinned_folders.iter().filter(|folder| {
        settings
            .searched_pinned_folders
            .iter()
            .any(|chosen| path_key(chosen) == path_key(folder))
    });
    for folder in settings.library_folders.iter().chain(searched_pinned) {
        walk(Path::new(folder), 0, &excluded, &mut |path| {
            take(path, &mut files)
        });
    }
    files
}

fn walk(folder: &Path, depth: usize, excluded: &[String], found: &mut dyn FnMut(PathBuf)) {
    if depth > MAX_DEPTH || is_excluded(folder, excluded) {
        return;
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if is_hidden(&path) {
            continue;
        }
        // file_type does not follow links, so a link to a folder is not
        // entered: that is what keeps a loop of links from going on forever
        let Ok(kind) = fs::symlink_metadata(&path).map(|m| m.file_type()) else {
            continue;
        };
        if kind.is_dir() {
            walk(&path, depth + 1, excluded, found);
        } else if kind.is_file() || kind.is_symlink() {
            found(path);
        }
    }
}

/// hidden folders (".git", "~$" lock files of word) are never indexed.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') || name.starts_with("~$"))
}

fn folder_key(folder: &str) -> String {
    let key = path_key(folder);
    key.trim_end_matches(['\\', '/']).to_string()
}

/// true when `path` is an excluded folder or inside one.
pub fn is_excluded(path: &Path, excluded: &[String]) -> bool {
    let key = path_key(&path.to_string_lossy());
    excluded.iter().any(|folder| {
        key == *folder
            || key
                .strip_prefix(folder.as_str())
                .is_some_and(|rest| rest.starts_with(['\\', '/']))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn folders_are_walked_and_exclusions_respected() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path();
        touch(&base.join("library/a.pdf"));
        touch(&base.join("library/deep/er/b.docx"));
        touch(&base.join("library/private/secret.pdf"));
        touch(&base.join("library/.git/c.md"));
        touch(&base.join("library/photo.jpg"));
        touch(&base.join("library/~$lock.docx"));
        touch(&base.join("pinned/p.txt"));
        touch(&base.join("unchosen/u.pdf"));
        let settings = LibrarySettings {
            library_folders: vec![base.join("library").to_string_lossy().into()],
            searched_pinned_folders: vec![base.join("pinned").to_string_lossy().into()],
            excluded_folders: vec![base.join("library/private").to_string_lossy().into()],
            ..LibrarySettings::default()
        };
        let pinned = vec![
            base.join("pinned").to_string_lossy().into_owned(),
            base.join("unchosen").to_string_lossy().into_owned(),
        ];
        let recent = vec![
            base.join("library/a.pdf").to_string_lossy().into_owned(),
            base.join("gone.pdf").to_string_lossy().into_owned(),
        ];
        let files = files_to_index(&settings, &recent, &pinned);
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        // the recent file first and only once; nothing private, hidden or unchosen
        assert_eq!(names, vec!["a.pdf", "b.docx", "p.txt"]);
    }

    #[test]
    fn recents_can_be_left_out() {
        let root = tempfile::tempdir().unwrap();
        touch(&root.path().join("r.pdf"));
        let settings = LibrarySettings {
            include_recents: false,
            ..LibrarySettings::default()
        };
        let recent = vec![root.path().join("r.pdf").to_string_lossy().into_owned()];
        assert!(files_to_index(&settings, &recent, &[]).is_empty());
    }

    #[test]
    fn exclusion_needs_a_whole_folder_name() {
        let excluded = vec![folder_key("C:/School/Private")];
        assert!(is_excluded(Path::new("C:/School/Private/a.pdf"), &excluded));
        assert!(!is_excluded(
            Path::new("C:/School/Private Notes/a.pdf"),
            &excluded
        ));
    }
}
