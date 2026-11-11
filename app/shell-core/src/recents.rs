//! the list of recently opened files shown on the home screen.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// enough to fill the home screen without it turning into a long list.
const MAX_RECENT_FILES: usize = 24;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    pub name: String,
    pub page_count: usize,
    /// seconds since 1970, so the ui can show "2 days ago"
    pub opened_at: u64,
    /// names the thumbnail image for this file
    pub thumbnail: String,
}

pub struct RecentFiles {
    store: PathBuf,
    files: Vec<RecentFile>,
}

impl RecentFiles {
    /// reads the saved list. a missing or unreadable file just means
    /// an empty list: losing the recents is never worth an error.
    pub fn load(store: PathBuf) -> Self {
        let files = fs::read_to_string(&store)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { store, files }
    }

    pub fn list(&self) -> &[RecentFile] {
        &self.files
    }

    /// puts a file at the top of the list and saves the list.
    pub fn add(&mut self, path: &Path, page_count: usize) -> RecentFile {
        let entry = RecentFile {
            path: path.to_string_lossy().into_owned(),
            name: file_name(path),
            page_count,
            opened_at: seconds_since_1970(),
            thumbnail: thumbnail_name(path),
        };
        self.files.retain(|file| file.path != entry.path);
        self.files.insert(0, entry.clone());
        self.files.truncate(MAX_RECENT_FILES);
        self.save();
        entry
    }

    pub fn remove(&mut self, path: &str) {
        self.files.retain(|file| file.path != path);
        self.save();
    }

    fn save(&self) {
        if let Some(folder) = self.store.parent() {
            let _ = fs::create_dir_all(folder);
        }
        if let Ok(text) = serde_json::to_string_pretty(&self.files) {
            let _ = fs::write(&self.store, text);
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// a short name made only of hex digits, derived from the file's path.
///
/// it is used as a file name and inside a url, so it must not contain
/// anything from the real path (slashes, dots, spaces).
pub fn thumbnail_name(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// true only for names made by `thumbnail_name`, so a url can never be
/// used to read some other file from disk.
pub fn is_thumbnail_name(name: &str) -> bool {
    name.len() == 16 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn seconds_since_1970() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("letters-shell-tests")
            .join(format!("{test_name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("recent-files.json")
    }

    #[test]
    fn newest_file_comes_first() {
        let mut recents = RecentFiles::load(temp_store("order"));
        recents.add(Path::new("/docs/a.pdf"), 1);
        recents.add(Path::new("/docs/b.pdf"), 2);

        let names: Vec<_> = recents
            .list()
            .iter()
            .map(|file| file.name.as_str())
            .collect();
        assert_eq!(names, ["b.pdf", "a.pdf"]);
    }

    #[test]
    fn reopening_a_file_moves_it_to_the_top_without_a_duplicate() {
        let mut recents = RecentFiles::load(temp_store("reopen"));
        recents.add(Path::new("/docs/a.pdf"), 1);
        recents.add(Path::new("/docs/b.pdf"), 2);
        recents.add(Path::new("/docs/a.pdf"), 1);

        let names: Vec<_> = recents
            .list()
            .iter()
            .map(|file| file.name.as_str())
            .collect();
        assert_eq!(names, ["a.pdf", "b.pdf"]);
    }

    #[test]
    fn the_list_is_capped() {
        let mut recents = RecentFiles::load(temp_store("cap"));
        for number in 0..40 {
            recents.add(Path::new(&format!("/docs/{number}.pdf")), 1);
        }
        assert_eq!(recents.list().len(), MAX_RECENT_FILES);
        assert_eq!(recents.list()[0].name, "39.pdf");
    }

    #[test]
    fn the_list_survives_a_restart() {
        let store = temp_store("restart");
        RecentFiles::load(store.clone()).add(Path::new("/docs/a.pdf"), 7);

        let reloaded = RecentFiles::load(store);
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.list()[0].page_count, 7);
    }

    #[test]
    fn a_damaged_list_is_treated_as_empty() {
        let store = temp_store("damaged");
        fs::create_dir_all(store.parent().unwrap()).unwrap();
        fs::write(&store, "not json").unwrap();
        assert!(RecentFiles::load(store).list().is_empty());
    }

    #[test]
    fn removing_a_file_forgets_it() {
        let mut recents = RecentFiles::load(temp_store("remove"));
        let added = recents.add(Path::new("/docs/a.pdf"), 1);
        recents.remove(&added.path);
        assert!(recents.list().is_empty());
    }

    #[test]
    fn thumbnail_names_are_safe_to_use_in_urls_and_paths() {
        let name = thumbnail_name(Path::new("C:\\my files\\..\\essay (final).pdf"));
        assert!(is_thumbnail_name(&name));
        assert!(!is_thumbnail_name("../../secret"));
        assert!(!is_thumbnail_name("0123456789abcdeg"));
        assert!(!is_thumbnail_name(""));
    }
}
