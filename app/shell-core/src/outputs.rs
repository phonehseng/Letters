//! files the page tools make beside the open document: extracted pages,
//! the parts of a split and merged documents, and reading the other pdfs
//! those tools take in.
//!
//! two rules keep the person's files safe:
//! - a file chosen in the save panel may be replaced, because the panel
//!   already asked "replace it?". it is written to a temporary file first
//!   and renamed into place, so a crash never leaves half a file.
//! - the parts of a split go into a folder the person picked, under names
//!   made here. those must never replace anything, so a name that is
//!   taken gets a number, and each file is created only if it is new.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use letters_pages::PagesError;
use lopdf::Document;

use crate::session::file_name;

/// what can go wrong reading or writing these files. the messages are
/// shown to the person as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputError {
    /// another pdf the tool needs can't be read
    CannotRead {
        name: String,
        detail: String,
    },
    /// the other pdf needs a password. the words are the page tools' own
    Locked,
    CannotWrite {
        name: String,
        detail: String,
    },
    FolderMissing {
        name: String,
    },
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CannotRead { name, detail } => write!(
                f,
                "“{name}” can’t be used because it can’t be read as a PDF. ({detail})"
            ),
            Self::Locked => write!(f, "{}", PagesError::Locked),
            Self::CannotWrite { name, detail } => write!(
                f,
                "“{name}” couldn’t be saved. Check that the folder still exists and that you can save there, then try again. ({detail})"
            ),
            Self::FolderMissing { name } => write!(
                f,
                "The folder “{name}” can’t be found. Choose another folder, then try again."
            ),
        }
    }
}

impl std::error::Error for OutputError {}

/// opens another pdf (to insert, merge or use as a picture source) for reading.
pub fn load_other_pdf(path: &Path) -> Result<Document, OutputError> {
    let document = Document::load(path).map_err(|error| match error {
        lopdf::Error::InvalidPassword
        | lopdf::Error::Decryption(_)
        | lopdf::Error::UnsupportedSecurityHandler(_) => OutputError::Locked,
        other => OutputError::CannotRead {
            name: file_name(path),
            detail: other.to_string(),
        },
    })?;
    // lopdf opens files that only need an empty password by itself and
    // drops the /Encrypt entry; one still there means it is really locked
    if document.trailer.has(b"Encrypt") {
        return Err(OutputError::Locked);
    }
    Ok(document)
}

/// writes a new document to a place the person chose in the save panel,
/// replacing what is there (the panel already asked).
pub fn write_chosen_file(document: &mut Document, path: &Path) -> Result<(), OutputError> {
    let cannot_write = |error: io::Error| OutputError::CannotWrite {
        name: file_name(path),
        detail: error.to_string(),
    };
    let bytes = saved_bytes(document).map_err(cannot_write)?;
    let temporary = path.with_file_name(format!(".{}.letters-saving", file_name(path)));
    let written =
        write_and_flush(&temporary, &bytes, false).and_then(|_| fs::rename(&temporary, path));
    written.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        cannot_write(error)
    })
}

/// one part of a split, ready to be written.
pub struct NewPart {
    pub document: Document,
    /// a short text such as "pages 1-4" or a chapter title
    pub name_suffix: String,
}

/// writes the parts of a split into `folder` and returns the files made,
/// in order.
///
/// each is named "<base> - <suffix>.pdf". a name already taken in the
/// folder, or by an earlier part, gets " (2)", " (3)" and so on, so no
/// existing file is ever replaced. if a part fails, the parts already
/// written stay (they are complete files) and the error says which failed.
pub fn write_parts(
    folder: &Path,
    base_name: &str,
    parts: Vec<NewPart>,
) -> Result<Vec<PathBuf>, OutputError> {
    if !folder.is_dir() {
        return Err(OutputError::FolderMissing {
            name: file_name(folder),
        });
    }
    let base = safe_file_name(base_name).unwrap_or_else(|| "Document".to_owned());
    let mut written = Vec::new();
    for mut part in parts {
        let wanted = format!(
            "{base} - {}",
            safe_file_name(&part.name_suffix).unwrap_or_default()
        );
        let wanted = wanted.trim_end_matches([' ', '-']).to_owned();
        written.push(write_new_file(folder, &wanted, &mut part.document)?);
    }
    Ok(written)
}

/// writes a document under `stem` (plus a number if needed) in `folder`,
/// never replacing a file. returns the path it got.
fn write_new_file(
    folder: &Path,
    stem: &str,
    document: &mut Document,
) -> Result<PathBuf, OutputError> {
    let bytes = saved_bytes(document).map_err(|error| OutputError::CannotWrite {
        name: format!("{stem}.pdf"),
        detail: error.to_string(),
    })?;
    // a file may appear between looking and writing, so creating "only if
    // new" is what really protects it; looking first just saves attempts
    for attempt in 1.. {
        let path = folder.join(numbered_name(stem, attempt));
        if path.exists() {
            continue;
        }
        match write_and_flush(&path, &bytes, true) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(OutputError::CannotWrite {
                    name: file_name(&path),
                    detail: error.to_string(),
                })
            }
        }
    }
    unreachable!("the loop above only ends by returning")
}

/// "Essay - pages 1-4.pdf", then "Essay - pages 1-4 (2).pdf" and so on.
pub fn numbered_name(stem: &str, attempt: u32) -> String {
    if attempt <= 1 {
        format!("{stem}.pdf")
    } else {
        format!("{stem} ({attempt}).pdf")
    }
}

/// a name with the characters no file name may hold replaced by spaces,
/// or none when nothing usable is left.
pub fn safe_file_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"\/:*?"<>|"#.contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_end_matches(['.', ' ']).to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn saved_bytes(document: &mut Document) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    document.save_to(&mut bytes)?;
    Ok(bytes)
}

/// writes bytes and waits until they are on the disk. with `only_if_new`
/// an existing file is an error instead of being replaced.
fn write_and_flush(path: &Path, bytes: &[u8], only_if_new: bool) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true);
    if only_if_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    let mut file = options.open(path)?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    if result.is_err() && only_if_new {
        drop(file);
        let _ = fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pdf::{temp_dir, write_pdf};

    fn part(suffix: &str) -> NewPart {
        // tests run at the same time, so every source gets its own folder
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = temp_dir(&format!("part-source-{number}"));
        let path = dir.join("one.pdf");
        write_pdf(&path, 1);
        NewPart {
            document: Document::load(&path).unwrap(),
            name_suffix: suffix.to_owned(),
        }
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths.iter().map(|path| file_name(path)).collect()
    }

    #[test]
    fn parts_are_named_after_the_document() {
        let folder = temp_dir("split-names");
        let written =
            write_parts(&folder, "Essay", vec![part("pages 1-4"), part("pages 5-9")]).unwrap();
        assert_eq!(
            names(&written),
            ["Essay - pages 1-4.pdf", "Essay - pages 5-9.pdf"]
        );
        assert!(written.iter().all(|path| path.is_file()));
    }

    #[test]
    fn existing_files_are_never_replaced() {
        let folder = temp_dir("split-no-clobber");
        fs::write(folder.join("Essay - page 1.pdf"), b"keep me").unwrap();
        fs::write(folder.join("Essay - page 1 (2).pdf"), b"keep me too").unwrap();

        let written = write_parts(&folder, "Essay", vec![part("page 1")]).unwrap();

        assert_eq!(names(&written), ["Essay - page 1 (3).pdf"]);
        assert_eq!(
            fs::read(folder.join("Essay - page 1.pdf")).unwrap(),
            b"keep me"
        );
        assert_eq!(
            fs::read(folder.join("Essay - page 1 (2).pdf")).unwrap(),
            b"keep me too"
        );
    }

    #[test]
    fn two_parts_with_the_same_suffix_both_survive() {
        // overlapping ranges can give the same name twice
        let folder = temp_dir("split-same-suffix");
        let written = write_parts(&folder, "Essay", vec![part("page 1"), part("page 1")]).unwrap();
        assert_eq!(
            names(&written),
            ["Essay - page 1.pdf", "Essay - page 1 (2).pdf"]
        );
    }

    #[test]
    fn names_that_files_cannot_hold_are_cleaned() {
        let folder = temp_dir("split-unsafe");
        let written = write_parts(&folder, "Week 1: Notes?", vec![part("Part 2 / Intro")]).unwrap();
        assert_eq!(names(&written), ["Week 1 Notes - Part 2 Intro.pdf"]);
        assert_eq!(safe_file_name("  ..  "), None);
    }

    #[test]
    fn a_missing_folder_is_reported_plainly() {
        let folder = temp_dir("split-missing").join("gone");
        let result = write_parts(&folder, "Essay", vec![part("page 1")]);
        assert_eq!(
            result.unwrap_err(),
            OutputError::FolderMissing {
                name: "gone".into()
            }
        );
    }

    #[test]
    fn a_chosen_file_is_replaced_in_one_step() {
        let folder = temp_dir("write-chosen");
        let target = folder.join("extract.pdf");
        fs::write(&target, b"old").unwrap();
        let mut document = part("x").document;

        write_chosen_file(&mut document, &target).unwrap();

        assert!(Document::load(&target).is_ok());
        assert!(!folder.join(".extract.pdf.letters-saving").exists());
    }

    #[test]
    fn numbered_names_count_from_two() {
        assert_eq!(numbered_name("Essay", 1), "Essay.pdf");
        assert_eq!(numbered_name("Essay", 2), "Essay (2).pdf");
    }

    #[test]
    fn other_pdfs_that_are_not_pdfs_say_so() {
        let folder = temp_dir("load-other");
        let path = folder.join("notes.pdf");
        fs::write(&path, b"just text").unwrap();
        assert!(matches!(
            load_other_pdf(&path),
            Err(OutputError::CannotRead { .. })
        ));
    }

    #[test]
    fn a_locked_pdf_gives_the_page_tools_message() {
        assert_eq!(
            OutputError::Locked.to_string(),
            "This PDF is locked with a password. Unlock it first, then try again."
        );
    }
}
