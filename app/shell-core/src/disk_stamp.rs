//! what a file looked like on disk when this window last read or wrote it,
//! so saving can tell when another window or program changed it since.
//!
//! saving over a file that changed would silently throw the other change
//! away (the same pdf open in two windows: the second save won). size and
//! modification time are enough to notice: every save changes one of them.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

/// a file's size and last change, as this window last saw them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskStamp {
    pub bytes: u64,
    /// milliseconds since 1970; 0 when the system can't say
    pub modified_ms: u64,
}

impl DiskStamp {
    /// the file as it is now, or none when it can't be read.
    pub fn of(path: &Path) -> Option<DiskStamp> {
        let meta = fs::metadata(path).ok()?;
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|since| since.as_millis() as u64)
            .unwrap_or(0);
        Some(DiskStamp {
            bytes: meta.len(),
            modified_ms,
        })
    }

    /// true when the file has changed since this stamp was taken. a file
    /// that is gone counts as unchanged here: other checks say it's missing.
    pub fn changed_since(&self, path: &Path) -> bool {
        DiskStamp::of(path).is_some_and(|now| now != *self)
    }
}

/// the sentence shown when saving would throw someone else's change away.
pub fn changed_on_disk_message(name: &str) -> String {
    format!(
        "“{name}” was changed by another window or program since you opened it. \
         Saving now would throw that change away. Use Save As to keep both."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_left_alone_is_unchanged() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("notes.pdf");
        fs::write(&path, b"first").unwrap();
        let stamp = DiskStamp::of(&path).unwrap();
        assert!(!stamp.changed_since(&path));
    }

    #[test]
    fn a_file_written_again_has_changed() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("notes.pdf");
        fs::write(&path, b"first").unwrap();
        let stamp = DiskStamp::of(&path).unwrap();
        fs::write(&path, b"second version").unwrap();
        assert!(stamp.changed_since(&path));
    }

    #[test]
    fn a_missing_file_is_left_to_the_missing_file_check() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("notes.pdf");
        fs::write(&path, b"first").unwrap();
        let stamp = DiskStamp::of(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(!stamp.changed_since(&path));
    }
}
