//! "Save a Version": named copies of a document, kept in the app's data
//! folder, that the writer can look back at and restore.
//!
//! each document gets a folder named after a checksum of its path; it
//! holds the copies and a small list of them (name, date, word count).
//! restoring first keeps the current file as a version too, so nothing
//! is ever lost by restoring the wrong one.

use std::fs;
use std::path::{Path, PathBuf};

use letters_search::index::path_key;
use serde::{Deserialize, Serialize};

use super::store::now_millis;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    pub name: String,
    /// milliseconds since 1970
    pub saved: u64,
    pub words: u32,
    pub bytes: u64,
    /// where the copy is, so it can be opened or compared
    pub file: String,
}

pub struct VersionStore {
    folder: PathBuf,
}

impl VersionStore {
    pub fn new(folder: PathBuf) -> VersionStore {
        VersionStore { folder }
    }

    fn document_folder(&self, document: &str) -> PathBuf {
        // fnv-1a: a short, stable name for the path's folder
        let checksum = path_key(document)
            .bytes()
            .fold(0x811c_9dc5u32, |hash, byte| {
                (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
            });
        let stem = Path::new(document)
            .file_stem()
            .map(|s| s.to_string_lossy().chars().take(40).collect::<String>())
            .unwrap_or_default();
        let safe: String = stem
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.folder.join(format!("{safe} {checksum:08x}"))
    }

    fn list_file(&self, document: &str) -> PathBuf {
        self.document_folder(document).join("versions.json")
    }

    /// every version of a document, newest first.
    pub fn list(&self, document: &str) -> Vec<Version> {
        let mut versions: Vec<Version> = fs::read(self.list_file(document))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        versions.retain(|v| Path::new(&v.file).is_file());
        // newest first
        versions.sort_by_key(|v| std::cmp::Reverse(v.saved));
        versions
    }

    fn write_list(&self, document: &str, versions: &[Version]) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(versions).map_err(|e| e.to_string())?;
        fs::write(self.list_file(document), bytes).map_err(|e| e.to_string())
    }

    /// copies the document's file as it is on disk now.
    pub fn save(&self, document: &str, name: &str, words: u32) -> Result<Version, String> {
        let source = Path::new(document);
        let folder = self.document_folder(document);
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let saved = now_millis();
        let extension = source
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "bin".into());
        let mut versions = self.list(document);
        let id = format!("v{saved}-{}", versions.len());
        let copy = folder.join(format!("{id}.{extension}"));
        let bytes = fs::copy(source, &copy)
            .map_err(|e| format!("The version couldn\u{2019}t be saved ({e})."))?;
        let name = name.trim();
        let version = Version {
            id,
            name: if name.is_empty() {
                "Untitled Version".into()
            } else {
                name.chars().take(120).collect()
            },
            saved,
            words,
            bytes,
            file: copy.to_string_lossy().into_owned(),
        };
        versions.push(version.clone());
        self.write_list(document, &versions)?;
        Ok(version)
    }

    /// puts a version back in place of the document's file. the current
    /// file is saved as a version first.
    pub fn restore(&self, document: &str, id: &str, current_words: u32) -> Result<Version, String> {
        let version = self
            .list(document)
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| "That version is no longer there.".to_string())?;
        self.save(document, "Before Restoring", current_words)?;
        fs::copy(&version.file, document)
            .map_err(|e| format!("The version couldn\u{2019}t be restored ({e})."))?;
        Ok(version)
    }

    /// a document was moved or renamed: its versions follow it.
    pub fn move_document(&self, old: &str, new: &str) -> Result<(), String> {
        let from = self.document_folder(old);
        if !from.is_dir() {
            return Ok(());
        }
        let to = self.document_folder(new);
        if to.exists() {
            return Err(
                "The versions of the moved document couldn\u{2019}t be moved with it.".into(),
            );
        }
        fs::rename(&from, &to).map_err(|e| e.to_string())?;
        // the list names each copy's file, which now sits in the new folder
        let versions: Vec<Version> = fs::read(to.join("versions.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<Version>>(&bytes).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|mut v| {
                if let Some(name) = Path::new(&v.file).file_name() {
                    v.file = to.join(name).to_string_lossy().into_owned();
                }
                v
            })
            .collect();
        self.write_list(new, &versions)
    }

    pub fn delete(&self, document: &str, id: &str) -> Result<(), String> {
        let mut versions = self.list(document);
        if let Some(version) = versions.iter().find(|v| v.id == id) {
            let _ = fs::remove_file(&version.file);
        }
        versions.retain(|v| v.id != id);
        self.write_list(document, &versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_saved_listed_restored_and_deleted() {
        let root = tempfile::tempdir().unwrap();
        let document = root.path().join("Essay.docx");
        fs::write(&document, "first draft").unwrap();
        let store = VersionStore::new(root.path().join("versions"));
        let draft = store
            .save(&document.to_string_lossy(), "First Draft", 2)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(&document, "second draft, longer").unwrap();
        store.save(&document.to_string_lossy(), " ", 3).unwrap();
        let listed = store.list(&document.to_string_lossy());
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].name, "Untitled Version");
        assert_eq!(listed[1].name, "First Draft");

        fs::write(&document, "a mistake").unwrap();
        store
            .restore(&document.to_string_lossy(), &draft.id, 2)
            .unwrap();
        assert_eq!(fs::read_to_string(&document).unwrap(), "first draft");
        let listed = store.list(&document.to_string_lossy());
        assert_eq!(listed[0].name, "Before Restoring");
        assert_eq!(fs::read_to_string(&listed[0].file).unwrap(), "a mistake");

        store
            .delete(&document.to_string_lossy(), &draft.id)
            .unwrap();
        assert_eq!(store.list(&document.to_string_lossy()).len(), 2);

        // the versions follow the document when it moves
        let moved = root.path().join("Final Essay.docx");
        store
            .move_document(&document.to_string_lossy(), &moved.to_string_lossy())
            .unwrap();
        assert_eq!(store.list(&moved.to_string_lossy()).len(), 2);
        assert!(store.list(&document.to_string_lossy()).is_empty());
        assert!(store
            .restore(&document.to_string_lossy(), &draft.id, 0)
            .is_err());
    }
}
