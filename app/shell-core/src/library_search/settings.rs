//! the person's choices about what Search All Documents covers.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LibrarySettings {
    /// folders added in Settings, searched with every folder inside them
    pub library_folders: Vec<String>,
    /// pinned folders the person chose to search (pinning alone does not)
    pub searched_pinned_folders: Vec<String>,
    /// "Don't Index This Folder": never read, even inside a library folder
    pub excluded_folders: Vec<String>,
    /// recently opened files are searched unless this is turned off
    pub include_recents: bool,
    /// indexing waits until it is resumed, across restarts too
    pub paused: bool,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        LibrarySettings {
            library_folders: Vec::new(),
            searched_pinned_folders: Vec::new(),
            excluded_folders: Vec::new(),
            include_recents: true,
            paused: false,
        }
    }
}

/// adds `path` to `list` once, comparing without regard to case on windows.
pub fn add_folder(list: &mut Vec<String>, path: &str) -> bool {
    let key = letters_search::index::path_key(path);
    if list
        .iter()
        .any(|p| letters_search::index::path_key(p) == key)
    {
        return false;
    }
    list.push(path.to_string());
    true
}

pub fn remove_folder(list: &mut Vec<String>, path: &str) -> bool {
    let key = letters_search::index::path_key(path);
    let before = list.len();
    list.retain(|p| letters_search::index::path_key(p) != key);
    list.len() != before
}

/// the settings, kept in a json file in the app's data folder.
pub struct LibrarySettingsStore {
    file: PathBuf,
    current: Mutex<LibrarySettings>,
}

impl LibrarySettingsStore {
    pub fn load(file: PathBuf) -> LibrarySettingsStore {
        let current = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        LibrarySettingsStore {
            file,
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> LibrarySettings {
        self.current.lock().unwrap().clone()
    }

    /// changes the settings and writes them. returns the new settings.
    pub fn update(&self, change: impl FnOnce(&mut LibrarySettings)) -> LibrarySettings {
        let mut current = self.current.lock().unwrap();
        change(&mut current);
        if let Some(folder) = self.file.parent() {
            let _ = fs::create_dir_all(folder);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&*current) {
            let _ = fs::write(&self.file, bytes);
        }
        current.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_kept_between_runs() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("library-search.json");
        let store = LibrarySettingsStore::load(file.clone());
        assert!(store.get().include_recents);
        store.update(|s| {
            add_folder(&mut s.library_folders, "C:/Notes");
            s.paused = true;
        });
        let again = LibrarySettingsStore::load(file);
        assert_eq!(again.get().library_folders, vec!["C:/Notes".to_string()]);
        assert!(again.get().paused);
    }

    #[test]
    fn folders_are_added_once() {
        let mut list = Vec::new();
        assert!(add_folder(&mut list, "C:/Notes"));
        assert!(!add_folder(&mut list, "C:/Notes"));
        if cfg!(windows) {
            assert!(!add_folder(&mut list, "c:\\notes"));
        }
        assert!(remove_folder(&mut list, "C:/Notes"));
        assert!(list.is_empty());
    }
}
