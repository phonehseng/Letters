//! "Save to Google Drive folder": Google Drive for desktop shows the
//! person's drive as a folder (G:\My Drive on Windows, a CloudStorage
//! folder on a mac). saving a file there uploads it with no sign-in in
//! Letters. it can't make a native google doc: that needs the drive api.

use std::path::{Path, PathBuf};

/// the "My Drive" folders of Google Drive for desktop on this computer.
pub fn drive_folders() -> Vec<PathBuf> {
    let mut found = Vec::new();
    if cfg!(windows) {
        for letter in b'D'..=b'Z' {
            found.push(PathBuf::from(format!("{}:\\My Drive", letter as char)));
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);
        found.push(home.join("Google Drive").join("My Drive"));
        found.push(home.join("My Drive"));
        if let Ok(entries) = std::fs::read_dir(home.join("Library").join("CloudStorage")) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with("GoogleDrive-") {
                    found.push(entry.path().join("My Drive"));
                }
            }
        }
    }
    existing(found)
}

/// keeps the candidates that are folders, once each.
pub fn existing(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if candidate.is_dir() && !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out
}

/// true when `path` is inside one of the drive folders.
pub fn is_in_drive(path: &Path, folders: &[PathBuf]) -> bool {
    folders.iter().any(|folder| path.starts_with(folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_folders_are_offered() {
        let dir = std::env::temp_dir().join(format!("letters-drive-{}", std::process::id()));
        let my_drive = dir.join("My Drive");
        std::fs::create_dir_all(&my_drive).unwrap();
        let found = existing(vec![my_drive.clone(), dir.join("missing"), my_drive.clone()]);
        assert_eq!(found, vec![my_drive.clone()]);
        assert!(is_in_drive(&my_drive.join("essay.docx"), &found));
        assert!(!is_in_drive(&dir.join("essay.docx"), &found));
    }
}
