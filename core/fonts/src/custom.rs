//! fonts the person adds: files or whole folders of them, checked and
//! copied into Letters' own folder. nothing is installed on the system
//! and no administrator rights are needed; removing one deletes only
//! Letters' copy.
//!
//! font files are untrusted input. each is read with a size limit, its
//! format is told from its first bytes (not its name), woff and woff2 are
//! decoded by our own bounds-checked code, and every face must parse
//! before anything is copied. nothing in a font is ever run.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sha1::{Digest, Sha1};

use crate::container::{
    display_name, has_font_extension, read_limited, to_plain_font, Format, MAX_CUSTOM_FONT_BYTES,
};
use crate::error::FontError;
use crate::facts::read_facts;
use crate::model::{FaceRecord, Source};
use crate::sfnt::face_count;

/// a folder may hold many fonts, but not without end.
const MAX_FILES_PER_ADD: usize = 2000;

/// one file that couldn't be added, and why, in a sentence.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rejected {
    pub file: String,
    pub reason: String,
}

/// what adding fonts did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddReport {
    pub added: Vec<FaceRecord>,
    pub rejected: Vec<Rejected>,
    /// files that were added before (the same font), so nothing was copied
    pub already_added: Vec<String>,
}

/// the files to add: each file as it is, each folder's font files.
fn expand(paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<Rejected>) {
    let mut files = Vec::new();
    let mut rejected = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found = crate::scan::find_font_files(std::slice::from_ref(path));
            found.truncate(MAX_FILES_PER_ADD);
            files.extend(found.into_iter().map(|file| file.path));
        } else if has_font_extension(path) {
            files.push(path.clone());
        } else {
            rejected.push(Rejected {
                file: display_name(path),
                reason: FontError::NotAFont(display_name(path)).to_string(),
            });
        }
    }
    files.truncate(MAX_FILES_PER_ADD);
    (files, rejected)
}

/// checks a file and returns its plain font bytes, or why it can't be used.
pub fn validate(path: &Path) -> Result<Vec<u8>, FontError> {
    let name = display_name(path);
    let plain = to_plain_font(read_limited(path, MAX_CUSTOM_FONT_BYTES)?, &name)?;
    let faces = face_count(&plain);
    if faces == 0 {
        return Err(FontError::Damaged(name));
    }
    for index in 0..faces {
        if read_facts(&plain, index).is_none() {
            return Err(FontError::Damaged(name));
        }
    }
    Ok(plain)
}

/// a file name that is safe on every system: letters, digits, dashes.
fn safe_stem(path: &Path) -> String {
    let stem: String = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(60)
        .collect();
    if stem.trim_matches('-').is_empty() {
        "font".to_string()
    } else {
        stem
    }
}

/// adds font files and folders: each valid font is decoded to a plain
/// font and copied into `folder` under a name made from its content, so
/// adding the same font twice copies it once.
pub fn add_fonts(paths: &[PathBuf], folder: &Path) -> AddReport {
    let (files, rejected) = expand(paths);
    let mut report = AddReport {
        rejected,
        ..AddReport::default()
    };
    for path in files {
        let name = display_name(&path);
        let plain = match validate(&path) {
            Ok(plain) => plain,
            Err(error) => {
                report.rejected.push(Rejected {
                    file: name,
                    reason: error.to_string(),
                });
                continue;
            }
        };
        let hash: String = Sha1::digest(&plain)
            .iter()
            .take(4)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let target = folder.join(format!(
            "{}-{hash}.{}",
            safe_stem(&path),
            Format::plain_extension(&plain)
        ));
        if target.exists() {
            report.already_added.push(name);
            continue;
        }
        if let Err(error) = write_new(&target, &plain) {
            report.rejected.push(Rejected {
                file: name,
                reason: error.to_string(),
            });
            continue;
        }
        report
            .added
            .extend(records_of(&target, &plain, Source::Custom));
    }
    report
}

/// writes a new file through a temporary name, so a half-written font is
/// never picked up by a scan.
fn write_new(target: &Path, bytes: &[u8]) -> Result<(), FontError> {
    let saving = |error: std::io::Error| FontError::CannotSave(error.to_string());
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(saving)?;
    }
    let temporary = target.with_extension("part");
    std::fs::write(&temporary, bytes).map_err(saving)?;
    std::fs::rename(&temporary, target).map_err(saving)
}

/// the face records of a plain font file.
pub fn records_of(path: &Path, plain: &[u8], source: Source) -> Vec<FaceRecord> {
    (0..face_count(plain))
        .filter_map(|index| {
            Some(FaceRecord {
                key: FaceRecord::make_key(source, path, index),
                source,
                path: path.to_path_buf(),
                index,
                facts: read_facts(plain, index)?,
                license: crate::scan::license_beside(path),
            })
        })
        .collect()
}

/// deletes Letters' copy of an added font. only files inside `folder`
/// can be removed, so a mistaken call can never delete a system font.
pub fn remove_font(path: &Path, folder: &Path) -> Result<(), FontError> {
    let inside = match (path.canonicalize(), folder.canonicalize()) {
        (Ok(path), Ok(folder)) => path.starts_with(&folder) && path != folder,
        _ => false,
    };
    if !inside {
        return Err(FontError::NotRemovable);
    }
    std::fs::remove_file(path).map_err(|error| FontError::CannotSave(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("letters-custom-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&folder).ok();
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn broken_files_are_rejected_with_a_reason() {
        let folder = temporary("broken");
        let fake = folder.join("fake.ttf");
        std::fs::write(&fake, b"\x00\x01\x00\x00garbage that is not a font").unwrap();
        let text = folder.join("notes.txt");
        std::fs::write(&text, b"hello").unwrap();
        let report = add_fonts(&[fake, text], &folder.join("custom"));
        assert!(report.added.is_empty());
        assert_eq!(report.rejected.len(), 2);
        assert!(report
            .rejected
            .iter()
            .any(|r| r.reason.contains("is damaged")));
        assert!(report
            .rejected
            .iter()
            .any(|r| r.reason.contains("isn’t a font file")));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn only_added_fonts_can_be_removed() {
        let folder = temporary("remove");
        let custom = folder.join("custom");
        std::fs::create_dir_all(&custom).unwrap();
        let outside = folder.join("outside.ttf");
        std::fs::write(&outside, b"x").unwrap();
        assert_eq!(remove_font(&outside, &custom), Err(FontError::NotRemovable));
        let inside = custom.join("mine.ttf");
        std::fs::write(&inside, b"x").unwrap();
        assert!(remove_font(&inside, &custom).is_ok());
        assert!(!inside.exists());
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_stem(Path::new("/a/My Font (v2).ttf")), "My-Font--v2-");
        assert_eq!(safe_stem(Path::new("/a/....ttf")), "font");
    }
}
