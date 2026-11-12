//! the file operations themselves, each refusing to lose anything.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// what to do when a file of the same name is already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Conflict {
    /// stop and ask (the default: nothing is replaced silently)
    Ask,
    /// keep both: the new one is named "Essay (2).pdf"
    KeepBoth,
    /// replace the one there, only after the person chose this
    Replace,
    /// leave this file where it is
    Skip,
}

/// one finished operation, enough to undo it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Done {
    Moved { from: String, to: String },
    Copied { from: String, to: String },
    Renamed { from: String, to: String },
}

impl Done {
    /// the old and new path when the file itself now lives somewhere
    /// else (a copy leaves the original where it was).
    pub fn change(&self) -> Option<(&str, &str)> {
        match self {
            Done::Moved { from, to } | Done::Renamed { from, to } => Some((from, to)),
            Done::Copied { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Outcome {
    Done { done: Done },
    Skipped { path: String },
}

#[derive(Debug)]
pub enum OrganiseError {
    /// a file of that name is there; the caller asks what to do
    Exists {
        existing: String,
    },
    Message(String),
}

impl std::fmt::Display for OrganiseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrganiseError::Exists { existing } => {
                let name = Path::new(existing)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                write!(
                    f,
                    "An item named \u{201C}{name}\u{201D} already exists in this location."
                )
            }
            OrganiseError::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for OrganiseError {}

/// a plain sentence for an io error, naming the file.
fn explain(error: &io::Error, path: &Path) -> OrganiseError {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    // windows: 32 is "used by another process", 33 "locked"
    let message = match error.raw_os_error() {
        Some(32) | Some(33) => format!("\u{201C}{name}\u{201D} is open in another program. Close it there and try again."),
        Some(206) => format!("The path for \u{201C}{name}\u{201D} is too long. Choose a folder nearer the top of the drive."),
        _ => match error.kind() {
            io::ErrorKind::NotFound => format!("\u{201C}{name}\u{201D} is no longer there. It may have been moved or deleted."),
            io::ErrorKind::PermissionDenied => format!("Letters isn\u{2019}t allowed to change \u{201C}{name}\u{201D}. It may be open in another program or read-only."),
            _ => format!("\u{201C}{name}\u{201D} couldn\u{2019}t be changed ({error})."),
        },
    };
    OrganiseError::Message(message)
}

/// "Essay.pdf" -> "Essay (2).pdf", the first name not taken in `folder`.
pub fn unique_name(folder: &Path, name: &str) -> PathBuf {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut candidate = folder.join(name);
    let mut number = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{stem} ({number}){extension}"));
        number += 1;
    }
    candidate
}

/// where a file would go in `folder`, after the conflict rule; none to skip.
fn destination(
    source: &Path,
    folder: &Path,
    conflict: Conflict,
) -> Result<Option<PathBuf>, OrganiseError> {
    let name = source
        .file_name()
        .ok_or_else(|| OrganiseError::Message("That isn\u{2019}t a file.".into()))?;
    let target = folder.join(name);
    if same_file(source, &target) {
        return Ok(None);
    }
    if !target.exists() {
        return Ok(Some(target));
    }
    match conflict {
        Conflict::Ask => Err(OrganiseError::Exists {
            existing: target.to_string_lossy().into_owned(),
        }),
        Conflict::KeepBoth => Ok(Some(unique_name(folder, &name.to_string_lossy()))),
        Conflict::Replace => Ok(Some(target)),
        Conflict::Skip => Ok(None),
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// moves a file into `folder`.
pub fn move_into(
    source: &Path,
    folder: &Path,
    conflict: Conflict,
) -> Result<Outcome, OrganiseError> {
    if !source.is_file() {
        return Err(explain(&io::Error::from(io::ErrorKind::NotFound), source));
    }
    let Some(target) = destination(source, folder, conflict)? else {
        return Ok(skipped(source));
    };
    move_file(source, &target)?;
    Ok(Outcome::Done {
        done: Done::Moved {
            from: source.to_string_lossy().into_owned(),
            to: target.to_string_lossy().into_owned(),
        },
    })
}

fn skipped(source: &Path) -> Outcome {
    Outcome::Skipped {
        path: source.to_string_lossy().into_owned(),
    }
}

/// renames in place when it can; across drives it copies, checks the
/// copy, and only then removes the original.
fn move_file(source: &Path, target: &Path) -> Result<(), OrganiseError> {
    if target.exists() {
        // only reached for Replace: the old one goes to the recycle bin,
        // so even a replaced file can be got back
        super::trash::to_recycle_bin(target)?;
    }
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(error) if is_other_drive(&error) => copy_then_remove(source, target),
        Err(error) => Err(explain(&error, source)),
    }
}

/// "not the same device": 17 on windows, 18 (EXDEV) elsewhere.
fn is_other_drive(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(17) | Some(18))
        || error.kind() == io::ErrorKind::CrossesDevices
}

/// the slow, careful path for another drive. public so tests can run it
/// on one drive.
pub fn copy_then_remove(source: &Path, target: &Path) -> Result<(), OrganiseError> {
    fs::copy(source, target).map_err(|e| explain(&e, source))?;
    if !same_bytes(source, target).map_err(|e| explain(&e, target))? {
        let _ = fs::remove_file(target);
        return Err(OrganiseError::Message(
            "The copy didn\u{2019}t match the original, so the original was kept where it was."
                .into(),
        ));
    }
    fs::remove_file(source).map_err(|e| {
        // the copy is good; leave both rather than lose either
        explain(&e, source)
    })
}

fn same_bytes(a: &Path, b: &Path) -> io::Result<bool> {
    if fs::metadata(a)?.len() != fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (fs::File::open(a)?, fs::File::open(b)?);
    let mut left = vec![0u8; 1 << 16];
    let mut right = vec![0u8; 1 << 16];
    loop {
        let read = a.read(&mut left)?;
        if read == 0 {
            return Ok(true);
        }
        b.read_exact(&mut right[..read])?;
        if left[..read] != right[..read] {
            return Ok(false);
        }
    }
}

/// copies a file into `folder`.
pub fn copy_into(
    source: &Path,
    folder: &Path,
    conflict: Conflict,
) -> Result<Outcome, OrganiseError> {
    if !source.is_file() {
        return Err(explain(&io::Error::from(io::ErrorKind::NotFound), source));
    }
    let target = match destination(source, folder, conflict)? {
        Some(target) => target,
        // a copy onto itself makes "Essay (2).pdf", as the finder does
        None if conflict != Conflict::Skip && source.parent() == Some(folder) => unique_name(
            folder,
            &source.file_name().unwrap_or_default().to_string_lossy(),
        ),
        None => return Ok(skipped(source)),
    };
    if target.exists() {
        super::trash::to_recycle_bin(&target)?;
    }
    fs::copy(source, &target).map_err(|e| explain(&e, source))?;
    Ok(Outcome::Done {
        done: Done::Copied {
            from: source.to_string_lossy().into_owned(),
            to: target.to_string_lossy().into_owned(),
        },
    })
}

/// characters windows never allows in a name (macos only refuses "/" and ":").
const NOT_IN_NAMES: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// the typed name with the document's extension kept: "Lab report 3.5"
/// becomes "Lab report 3.5.pdf". anything after a dot in what was typed is
/// part of the name ("3.5", "12.11.2026"), and a different extension
/// ("final.docx") would only make a pdf look like something it isn't, so
/// the original extension is added unless the name already ends in it.
fn keep_extension(typed: &str, source: &Path) -> String {
    let Some(extension) = source.extension() else {
        return typed.to_string();
    };
    let ending = format!(".{}", extension.to_string_lossy()).to_lowercase();
    if typed.to_lowercase().ends_with(&ending) {
        typed.to_string()
    } else {
        format!("{typed}.{}", extension.to_string_lossy())
    }
}

/// renames a file in its folder, keeping its extension (see keep_extension),
/// so "Essay" and "Lab report 3.5" stay pdfs.
pub fn rename(source: &Path, new_name: &str) -> Result<Outcome, OrganiseError> {
    let typed = new_name.trim();
    if typed.is_empty() || typed == "." || typed == ".." {
        return Err(OrganiseError::Message(
            "Type a name for the document.".into(),
        ));
    }
    if let Some(bad) = typed
        .chars()
        .find(|c| NOT_IN_NAMES.contains(c) || c.is_control())
    {
        return Err(OrganiseError::Message(format!(
            "A name can\u{2019}t contain \u{201C}{bad}\u{201D}."
        )));
    }
    // windows drops a trailing dot or space without a word, so the file
    // would end up under a different name than the one recorded
    if typed.ends_with('.') || typed.ends_with(' ') {
        return Err(OrganiseError::Message(
            "A name can\u{2019}t end in a dot or a space.".into(),
        ));
    }
    let name = keep_extension(typed, source);
    let folder = source.parent().unwrap_or(Path::new(""));
    let target = folder.join(&name);
    // changing only the case of a name is a rename too
    let only_case =
        target.to_string_lossy().to_lowercase() == source.to_string_lossy().to_lowercase();
    if target.exists() && !only_case {
        return Err(OrganiseError::Exists {
            existing: target.to_string_lossy().into_owned(),
        });
    }
    fs::rename(source, &target).map_err(|e| explain(&e, source))?;
    Ok(Outcome::Done {
        done: Done::Renamed {
            from: source.to_string_lossy().into_owned(),
            to: target.to_string_lossy().into_owned(),
        },
    })
}

/// takes one operation back: a move or rename goes back where it came
/// from (never over a file that has since appeared there), a copy goes
/// to the recycle bin. returns the operation that undid it.
pub fn undo(done: &Done) -> Result<Done, OrganiseError> {
    match done {
        Done::Moved { from, to } | Done::Renamed { from, to } => {
            let (from, to) = (Path::new(from), Path::new(to));
            if from.exists() {
                return Err(OrganiseError::Exists {
                    existing: from.to_string_lossy().into_owned(),
                });
            }
            move_file(to, from)?;
            Ok(Done::Moved {
                from: to.to_string_lossy().into_owned(),
                to: from.to_string_lossy().into_owned(),
            })
        }
        Done::Copied { to, .. } => {
            super::trash::to_recycle_bin(Path::new(to))?;
            Ok(done.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        (root, a, b)
    }

    fn done(outcome: Outcome) -> Done {
        match outcome {
            Outcome::Done { done } => done,
            other => panic!("expected done, got {other:?}"),
        }
    }

    #[test]
    fn move_and_undo() {
        let (_root, a, b) = setup();
        let file = a.join("Essay.pdf");
        fs::write(&file, "words").unwrap();
        let moved = done(move_into(&file, &b, Conflict::Ask).unwrap());
        assert!(!file.exists() && b.join("Essay.pdf").exists());
        assert_eq!(
            moved.change().unwrap().1,
            b.join("Essay.pdf").to_string_lossy()
        );
        undo(&moved).unwrap();
        assert!(file.exists() && !b.join("Essay.pdf").exists());
    }

    #[test]
    fn conflicts_are_never_silent() {
        let (_root, a, b) = setup();
        fs::write(a.join("Essay.pdf"), "new").unwrap();
        fs::write(b.join("Essay.pdf"), "old").unwrap();
        let asked = move_into(&a.join("Essay.pdf"), &b, Conflict::Ask);
        assert!(matches!(asked, Err(OrganiseError::Exists { .. })));
        assert_eq!(fs::read_to_string(b.join("Essay.pdf")).unwrap(), "old");
        let skipped = move_into(&a.join("Essay.pdf"), &b, Conflict::Skip).unwrap();
        assert!(matches!(skipped, Outcome::Skipped { .. }));
        let both = done(move_into(&a.join("Essay.pdf"), &b, Conflict::KeepBoth).unwrap());
        assert_eq!(Path::new(both.change().unwrap().1), b.join("Essay (2).pdf"));
        assert_eq!(fs::read_to_string(b.join("Essay.pdf")).unwrap(), "old");
        assert_eq!(fs::read_to_string(b.join("Essay (2).pdf")).unwrap(), "new");
    }

    #[test]
    fn unique_names_count_up() {
        let (_root, a, _b) = setup();
        fs::write(a.join("Notes.md"), "").unwrap();
        fs::write(a.join("Notes (2).md"), "").unwrap();
        assert_eq!(unique_name(&a, "Notes.md"), a.join("Notes (3).md"));
        assert_eq!(unique_name(&a, "Other.md"), a.join("Other.md"));
    }

    #[test]
    fn copies_and_their_undo() {
        let (_root, a, b) = setup();
        let file = a.join("Lab.docx");
        fs::write(&file, "data").unwrap();
        let copied = done(copy_into(&file, &b, Conflict::Ask).unwrap());
        assert!(file.exists() && b.join("Lab.docx").exists());
        assert!(copied.change().is_none());
        // a copy into its own folder keeps both
        let again = done(copy_into(&file, &a, Conflict::Ask).unwrap());
        assert!(matches!(again, Done::Copied { ref to, .. } if to.ends_with("Lab (2).docx")));
    }

    #[test]
    fn renames_keep_the_extension_and_refuse_bad_names() {
        let (_root, a, _b) = setup();
        let file = a.join("draft.pdf");
        fs::write(&file, "x").unwrap();
        let renamed = done(rename(&file, "Final Essay").unwrap());
        assert!(a.join("Final Essay.pdf").exists());
        assert!(rename(&a.join("Final Essay.pdf"), "a/b").is_err());
        assert!(rename(&a.join("Final Essay.pdf"), "  ").is_err());
        fs::write(a.join("Taken.pdf"), "y").unwrap();
        assert!(matches!(
            rename(&a.join("Final Essay.pdf"), "Taken"),
            Err(OrganiseError::Exists { .. })
        ));
        // changing only capitals works
        done(rename(&a.join("Final Essay.pdf"), "final essay").unwrap());
        undo(&Done::Renamed {
            from: file.to_string_lossy().into_owned(),
            to: a.join("final essay.pdf").to_string_lossy().into_owned(),
        })
        .unwrap();
        assert!(file.exists());
        let _ = renamed;
    }

    #[test]
    fn names_with_dots_keep_the_documents_extension() {
        let (_root, a, _b) = setup();
        let mut current = a.join("essay.pdf");
        fs::write(&current, "x").unwrap();
        for (typed, expected) in [
            ("Lab report 3.5", "Lab report 3.5.pdf"),
            ("Notes 12.11.2026", "Notes 12.11.2026.pdf"),
            ("Chapter 1. Intro", "Chapter 1. Intro.pdf"),
            ("final.docx", "final.docx.pdf"),
            ("Already.PDF", "Already.PDF"),
        ] {
            done(rename(&current, typed).unwrap());
            current = a.join(expected);
            assert!(current.exists(), "{typed} should become {expected}");
        }
    }

    #[test]
    fn a_name_ending_in_a_dot_or_space_is_refused() {
        let (_root, a, _b) = setup();
        let file = a.join("essay.pdf");
        fs::write(&file, "x").unwrap();
        for typed in ["essay.", "essay. ", "notes ."] {
            let refused = rename(&file, typed);
            assert!(refused.is_err(), "{typed:?} was accepted");
        }
        assert!(file.exists());
    }

    #[test]
    fn the_other_drive_path_checks_the_copy_before_removing() {
        let (_root, a, b) = setup();
        let file = a.join("Big.pdf");
        fs::write(&file, vec![7u8; 200_000]).unwrap();
        copy_then_remove(&file, &b.join("Big.pdf")).unwrap();
        assert!(!file.exists());
        assert_eq!(fs::read(b.join("Big.pdf")).unwrap().len(), 200_000);
    }

    #[test]
    fn undo_never_lands_on_a_new_file() {
        let (_root, a, b) = setup();
        let file = a.join("x.pdf");
        fs::write(&file, "1").unwrap();
        let moved = done(move_into(&file, &b, Conflict::Ask).unwrap());
        fs::write(&file, "someone else's").unwrap();
        assert!(matches!(undo(&moved), Err(OrganiseError::Exists { .. })));
        assert_eq!(fs::read_to_string(&file).unwrap(), "someone else's");
    }

    #[test]
    fn missing_files_get_a_plain_message() {
        let (_root, a, b) = setup();
        let error = move_into(&a.join("gone.pdf"), &b, Conflict::Ask).unwrap_err();
        assert!(error.to_string().contains("no longer there"), "{error}");
    }
}
