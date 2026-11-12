//! recovery copies for code files, the same promise the writer makes
//! (see writer/recovery.rs): unsaved text is copied to the app's cache
//! every few seconds, and offered back if the app ended without saving.
//! a copy is deleted on save and on "Don't Save".

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::recents::thumbnail_name;
use crate::writer::save::write_atomically;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeRecoveryFile {
    original_path: String,
    saved_at: u64,
    text: String,
}

/// what the editor needs to offer a copy back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeRecovery {
    /// seconds since 1970
    pub saved_at: u64,
    pub text: String,
}

pub struct CodeRecoveryStore {
    folder: PathBuf,
}

impl CodeRecoveryStore {
    pub fn new(folder: PathBuf) -> CodeRecoveryStore {
        CodeRecoveryStore { folder }
    }

    // the "code-" prefix keeps these apart from the writer's copies, which
    // are named by the same hash of the path
    fn file_for(&self, original: &Path) -> PathBuf {
        self.folder
            .join(format!("code-{}.json", thumbnail_name(original)))
    }

    pub fn write(&self, original: &Path, text: &str) -> Result<(), String> {
        fs::create_dir_all(&self.folder).map_err(|error| error.to_string())?;
        let copy = CodeRecoveryFile {
            original_path: original.to_string_lossy().into_owned(),
            saved_at: seconds_since_1970(),
            text: text.to_owned(),
        };
        let json = serde_json::to_vec(&copy).map_err(|error| error.to_string())?;
        write_atomically(&json, &self.file_for(original))
    }

    /// the copy for this file, if there is one. a copy for another file
    /// whose name happens to share the hash is ignored.
    pub fn find(&self, original: &Path) -> Option<CodeRecovery> {
        let json = fs::read(self.file_for(original)).ok()?;
        let copy: CodeRecoveryFile = serde_json::from_slice(&json).ok()?;
        if Path::new(&copy.original_path) != original {
            return None;
        }
        Some(CodeRecovery {
            saved_at: copy.saved_at,
            text: copy.text,
        })
    }

    pub fn discard(&self, original: &Path) {
        let _ = fs::remove_file(self.file_for(original));
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

    #[test]
    fn a_copy_is_found_for_its_own_file_and_gone_after_discard() {
        let folder = tempfile::tempdir().unwrap();
        let store = CodeRecoveryStore::new(folder.path().join("recovery"));
        let original = folder.path().join("hello.py");
        store.write(&original, "print('hi')\n").unwrap();
        let found = store.find(&original).unwrap();
        assert_eq!(found.text, "print('hi')\n");
        assert!(store.find(&folder.path().join("other.py")).is_none());
        store.discard(&original);
        assert!(store.find(&original).is_none());
    }
}
