//! recovery copies: while a document has unsaved changes, the ui sends
//! the document every few seconds and it is written to the app's cache.
//! if the app ends without saving (a crash, the power going off), the
//! next time that file is opened the copy is offered back.
//!
//! a copy is deleted when the document is saved, or closed with "Don't
//! Save", so only changes that were really lost are ever offered.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use letters_office::model::Document;
use serde::{Deserialize, Serialize};

use crate::recents::thumbnail_name;
use crate::writer::save::write_atomically;

/// what is known about a recovery copy without reading the whole document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    /// seconds since 1970, so the ui can say "from 10:42"
    pub saved_at: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryFile {
    original_path: String,
    saved_at: u64,
    document: Document,
}

pub struct RecoveryStore {
    folder: PathBuf,
}

impl RecoveryStore {
    pub fn new(folder: PathBuf) -> RecoveryStore {
        RecoveryStore { folder }
    }

    /// one file per document, named from its path so the next open finds it.
    fn file_for(&self, original: &Path) -> PathBuf {
        self.folder.join(format!("{}.json", thumbnail_name(original)))
    }

    /// writes the copy. it is written in one step, so a crash while writing
    /// never replaces a good copy with half of one.
    pub fn write(&self, original: &Path, document: &Document) -> Result<(), String> {
        fs::create_dir_all(&self.folder).map_err(|error| error.to_string())?;
        let file = RecoveryFile {
            original_path: original.to_string_lossy().into_owned(),
            saved_at: seconds_since_1970(),
            document: document.clone(),
        };
        let text = serde_json::to_vec(&file).map_err(|error| error.to_string())?;
        write_atomically(&text, &self.file_for(original))
    }

    /// whether there is a copy for this file. a copy for another file that
    /// happens to share the name hash is ignored.
    pub fn find(&self, original: &Path) -> Option<RecoveryInfo> {
        let file = self.read(original)?;
        Some(RecoveryInfo {
            saved_at: file.saved_at,
        })
    }

    /// the recovered document, if there is one.
    pub fn document(&self, original: &Path) -> Option<Document> {
        self.read(original).map(|file| file.document)
    }

    pub fn discard(&self, original: &Path) {
        let _ = fs::remove_file(self.file_for(original));
    }

    fn read(&self, original: &Path) -> Option<RecoveryFile> {
        let text = fs::read(self.file_for(original)).ok()?;
        let file: RecoveryFile = serde_json::from_slice(&text).ok()?;
        (Path::new(&file.original_path) == original).then_some(file)
    }
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
    use letters_office::model::Block;

    fn store(name: &str) -> RecoveryStore {
        let dir = std::env::temp_dir()
            .join("letters-writer-tests")
            .join(format!("recovery-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        RecoveryStore::new(dir)
    }

    fn edited() -> Document {
        let mut document = Document::new();
        document.blocks = vec![Block::text("Unsaved words")];
        document
    }

    #[test]
    fn a_copy_written_while_editing_is_found_after_a_restart() {
        let first = store("restart");
        let original = Path::new("C:/essays/essay.docx");
        first.write(original, &edited()).unwrap();
        // a new store on the same folder stands for the next run of the app
        let next = RecoveryStore::new(first.folder.clone());
        assert!(next.find(original).unwrap().saved_at > 0);
        let recovered = next.document(original).unwrap();
        assert_eq!(recovered.blocks[0].paragraph().unwrap().plain_text(), "Unsaved words");
    }

    #[test]
    fn other_files_have_no_copy_and_discarding_removes_it() {
        let store = store("discard");
        let original = Path::new("C:/essays/a.odt");
        assert!(store.find(original).is_none());
        store.write(original, &edited()).unwrap();
        assert!(store.find(Path::new("C:/essays/b.odt")).is_none());
        store.discard(original);
        assert!(store.find(original).is_none());
    }

    #[test]
    fn a_damaged_copy_is_ignored() {
        let store = store("damaged");
        let original = Path::new("C:/essays/a.docx");
        fs::create_dir_all(&store.folder).unwrap();
        fs::write(store.file_for(original), b"{ not json").unwrap();
        assert!(store.find(original).is_none());
    }

    #[test]
    fn a_newer_copy_replaces_the_older_one() {
        let store = store("replace");
        let original = Path::new("C:/essays/a.docx");
        store.write(original, &Document::new()).unwrap();
        store.write(original, &edited()).unwrap();
        let recovered = store.document(original).unwrap();
        assert_eq!(recovered.blocks.len(), 1);
        assert_eq!(fs::read_dir(&store.folder).unwrap().count(), 1);
    }
}
