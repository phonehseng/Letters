//! recovery copies of workbooks, as the writer keeps them for documents
//! (see writer/recovery.rs): while a workbook has unsaved changes the ui
//! asks for a copy every few seconds; if the app ends without saving, the
//! next time that file is opened the copy is offered back.
//!
//! the copy is the workbook written as the engine saves it (xlsx, or ods
//! for an ods file), so everything the save keeps, the copy keeps; a
//! small json file beside it says which file it belongs to and when.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use letters_sheets::Workbook;
use serde::{Deserialize, Serialize};

use super::files::{read_workbook_bytes, workbook_bytes, SheetFormat};
use crate::recents::thumbnail_name;
use crate::writer::save::write_atomically;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetRecoveryInfo {
    /// seconds since 1970, so the ui can say "from 10:42"
    pub saved_at: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryNote {
    original_path: String,
    saved_at: u64,
    /// "xlsx" or "ods": how the copy beside the note is written
    copy_kind: String,
}

pub struct SheetRecoveryStore {
    folder: PathBuf,
}

impl SheetRecoveryStore {
    pub fn new(folder: PathBuf) -> SheetRecoveryStore {
        SheetRecoveryStore { folder }
    }

    fn note_for(&self, original: &Path) -> PathBuf {
        self.folder
            .join(format!("{}.json", thumbnail_name(original)))
    }

    fn copy_for(&self, original: &Path, kind: &str) -> PathBuf {
        self.folder
            .join(format!("{}.{kind}", thumbnail_name(original)))
    }

    /// writes the copy, then the note: a crash between the two leaves the
    /// old note pointing at a complete copy.
    pub fn write(
        &self,
        original: &Path,
        book: &Workbook,
        format: SheetFormat,
    ) -> Result<(), String> {
        fs::create_dir_all(&self.folder).map_err(|error| error.to_string())?;
        let (kind, copy_format) = match format {
            SheetFormat::Ods => ("ods", SheetFormat::Ods),
            _ => ("xlsx", SheetFormat::Xlsx),
        };
        let bytes = workbook_bytes(book, copy_format, 0, original)?;
        write_atomically(&bytes, &self.copy_for(original, kind))?;
        let note = RecoveryNote {
            original_path: original.to_string_lossy().into_owned(),
            saved_at: seconds_since_1970(),
            copy_kind: kind.into(),
        };
        let text = serde_json::to_vec(&note).map_err(|error| error.to_string())?;
        write_atomically(&text, &self.note_for(original))
    }

    pub fn find(&self, original: &Path) -> Option<SheetRecoveryInfo> {
        self.note(original).map(|note| SheetRecoveryInfo {
            saved_at: note.saved_at,
        })
    }

    /// the recovered workbook, if there is one that still reads.
    pub fn workbook(&self, original: &Path) -> Option<Workbook> {
        let note = self.note(original)?;
        let copy = self.copy_for(original, &note.copy_kind);
        let bytes = fs::read(&copy).ok()?;
        read_workbook_bytes(&bytes, &copy)
            .ok()
            .map(|(book, _)| book)
    }

    pub fn discard(&self, original: &Path) {
        let _ = fs::remove_file(self.note_for(original));
        for kind in ["xlsx", "ods"] {
            let _ = fs::remove_file(self.copy_for(original, kind));
        }
    }

    /// the note, unless it belongs to another file with the same name hash
    fn note(&self, original: &Path) -> Option<RecoveryNote> {
        let text = fs::read(self.note_for(original)).ok()?;
        let note: RecoveryNote = serde_json::from_slice(&text).ok()?;
        (Path::new(&note.original_path) == original).then_some(note)
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
    use letters_sheets::Value;

    fn store(name: &str) -> SheetRecoveryStore {
        let dir = std::env::temp_dir()
            .join("letters-sheets-tests")
            .join(format!("recovery-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        SheetRecoveryStore::new(dir)
    }

    #[test]
    fn a_copy_written_while_editing_is_found_after_a_restart() {
        let first = store("restart");
        let original = Path::new("C:/marks/grades.xlsx");
        let mut book = Workbook::new();
        book.set_value(0, 0, 0, Value::Number(42.0));
        first.write(original, &book, SheetFormat::Xlsx).unwrap();
        let next = SheetRecoveryStore::new(first.folder.clone());
        assert!(next.find(original).unwrap().saved_at > 0);
        let recovered = next.workbook(original).unwrap();
        assert_eq!(recovered.value(0, 0, 0), Value::Number(42.0));
    }

    #[test]
    fn other_files_have_no_copy_and_discarding_removes_it() {
        let store = store("discard");
        let original = Path::new("C:/marks/a.ods");
        store
            .write(original, &Workbook::new(), SheetFormat::Ods)
            .unwrap();
        assert!(store.find(Path::new("C:/marks/b.ods")).is_none());
        store.discard(original);
        assert!(store.find(original).is_none());
        assert_eq!(fs::read_dir(&store.folder).unwrap().count(), 0);
    }
}
