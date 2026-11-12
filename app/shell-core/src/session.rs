//! the edit session of one open document: working copy, undo and save.
//!
//! the rule everything here keeps: an edit never touches the person's file
//! until they choose save.
//!
//! how it works:
//! - opening a document copies it into the session's own folder in the
//!   app's cache. that copy is what the pdf engine shows.
//! - every version of the document is a separate file in that folder,
//!   and a file is never changed once it has been written. an edit loads
//!   the current version, changes it in memory and writes the result to a
//!   new file. the version before becomes the undo snapshot as it is, so
//!   nothing has to be copied to make undo possible.
//! - because no file is ever rewritten, the engine (which reads its file
//!   lazily, a piece at a time) can never see half an edit, and a failed
//!   edit leaves the current version exactly as it was.
//! - saving copies the current version to a temporary file beside the
//!   original and then renames it over the original in one step, so a
//!   crash halfway through leaves either the old file or the new one,
//!   never half of each.
//!
//! nothing in here knows about windows or the engine: showing a new
//! version is done by a `reload` function the caller passes in, so all of
//! it can be tested with plain files.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lopdf::Document;

/// how many undo steps are kept. older ones are dropped first.
pub const MAX_HISTORY_STEPS: usize = 30;

/// how much disk the undo steps (and, separately, the redo steps) may use.
/// a long scanned document can be hundreds of megabytes per version, so
/// counting steps alone could fill a small disk.
pub const MAX_HISTORY_BYTES: u64 = 512 * 1024 * 1024;

/// the limits on the undo and redo history. each stack has its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryLimits {
    pub max_steps: usize,
    pub max_bytes: u64,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_steps: MAX_HISTORY_STEPS,
            max_bytes: MAX_HISTORY_BYTES,
        }
    }
}

/// one version of the document: a file that is never changed once written.
#[derive(Debug)]
struct Version {
    /// counts up within a session, so two versions are never confused
    number: u64,
    path: PathBuf,
    bytes: u64,
}

/// everything that can stop a session from starting, editing or saving.
///
/// the messages are shown to the person as they are, so each one says
/// what happened in plain words and, where there is one, what to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// the file could not be copied into the cache when it was opened
    CannotStart {
        name: String,
        detail: String,
    },
    /// the file to save over is no longer there
    OriginalMissing {
        name: String,
    },
    /// the file to save over is read-only
    OriginalLocked {
        name: String,
    },
    /// the new file could not be written beside the original
    CannotWrite {
        name: String,
        detail: String,
    },
    /// the pdf could not be read for editing (the viewer may still show it)
    CannotEdit {
        detail: String,
    },
    /// the working copy is still locked: it is view only until it is
    /// unlocked for editing with its password (see protect/unlock.rs)
    PasswordProtected,
    /// the changed document could not be stored in the cache
    CannotStore {
        detail: String,
    },
    /// the engine could not show the new version, so the change was not made
    CannotShow {
        detail: String,
    },
    NothingToUndo,
    NothingToRedo,
    /// forms and signatures: saving would rewrite a digitally signed file
    /// and break its signature (see forms/signed.rs)
    SignedOriginal {
        name: String,
    },
    /// protect: the clean save writer refused or failed (see protect/clean_save.rs)
    SaveRefused {
        name: String,
        reason: String,
    },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CannotStart { name, detail } => write!(
                f,
                "“{name}” can’t be opened for editing because Letters couldn’t make a working copy. Check that this computer has free space, then try again. ({detail})"
            ),
            Self::OriginalMissing { name } => write!(
                f,
                "“{name}” can’t be found. It may have been moved, renamed or deleted. Choose Save As… to save the changes as a new file."
            ),
            Self::OriginalLocked { name } => write!(
                f,
                "“{name}” is locked, so it can’t be changed. Choose Save As… to save the changes as a new file."
            ),
            Self::CannotWrite { name, detail } => write!(
                f,
                "The changes couldn’t be saved to “{name}”. Choose Save As… to save them somewhere else. ({detail})"
            ),
            Self::CannotEdit { detail } => write!(
                f,
                "This PDF can’t be changed because part of it can’t be read. ({detail})"
            ),
            Self::PasswordProtected => write!(
                f,
                "This PDF is locked with a password. To change it, unlock it under Protect in Tools, then try again."
            ),
            Self::CannotStore { detail } => write!(
                f,
                "The change couldn’t be made because Letters couldn’t store it. Check that this computer has free space, then try again. ({detail})"
            ),
            Self::CannotShow { detail } => write!(
                f,
                "The change couldn’t be shown, so it wasn’t made. ({detail})"
            ),
            Self::NothingToUndo => write!(f, "There is nothing to undo."),
            Self::NothingToRedo => write!(f, "There is nothing to redo."),
            Self::SignedOriginal { name } => write!(
                f,
                "“{name}” is digitally signed, and saving these changes over it would break its signature. Choose Save As… to save the changed document as a copy."
            ),
            Self::SaveRefused { name, reason } => write!(f, "“{name}” wasn’t saved. {reason}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// why `EditSession::apply` did not make a change.
///
/// either the edit itself refused (its own error, passed through as it
/// is) or the session could not load, store or show the result. in both
/// cases the document is exactly as it was before.
#[derive(Debug, PartialEq, Eq)]
pub enum ApplyError<E> {
    Edit(E),
    Session(SessionError),
}

impl<E: fmt::Display> fmt::Display for ApplyError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Edit(error) => error.fmt(f),
            Self::Session(error) => error.fmt(f),
        }
    }
}

/// numbers the session folders made by this run of the app.
static NEXT_SESSION_NUMBER: AtomicU64 = AtomicU64::new(1);

/// the open document's working copy and its history.
pub struct EditSession {
    /// the person's file: where save writes to
    original: PathBuf,
    /// this session's own folder in the cache, removed when the session ends
    folder: PathBuf,
    /// the version the engine shows
    current: Version,
    /// earlier versions, oldest first
    undo: Vec<Version>,
    /// versions undone, the next one to redo last
    redo: Vec<Version>,
    /// the version that matches the file on disk, if it is still around
    saved_version: Option<u64>,
    /// counts every change to what is shown (edit, undo, redo)
    revision: u64,
    next_version: u64,
    limits: HistoryLimits,
    /// forms and signatures: the person's file is digitally signed, so
    /// save must not write a rewritten version over it
    protected_original: bool,
    /// protect: when set, save writes what this makes instead of copying
    /// the working copy (a clean rewrite, maybe locked again)
    save_writer: Option<SaveWriter>,
    /// protect: earlier versions may hold removed content, so they are
    /// deleted once the document is saved
    forget_history_on_save: bool,
}

/// protect: makes the bytes save writes, from the current version's file.
/// an error is a sentence for the person, such as why the file can't be
/// locked again.
pub type SaveWriter = Box<dyn Fn(&Path) -> Result<Vec<u8>, String> + Send>;

impl EditSession {
    /// starts a session for `original`, with its working copy in a new
    /// folder under `sessions_root`.
    pub fn start(original: &Path, sessions_root: &Path) -> Result<Self, SessionError> {
        Self::start_with_limits(original, sessions_root, HistoryLimits::default())
    }

    pub fn start_with_limits(
        original: &Path,
        sessions_root: &Path,
        limits: HistoryLimits,
    ) -> Result<Self, SessionError> {
        let cannot_start = |error: io::Error| SessionError::CannotStart {
            name: file_name(original),
            detail: error.to_string(),
        };
        let folder = sessions_root.join(new_folder_name());
        fs::create_dir_all(&folder).map_err(cannot_start)?;
        let first = folder.join(version_file_name(0));
        if let Err(error) = fs::copy(original, &first) {
            let _ = fs::remove_dir_all(&folder);
            return Err(cannot_start(error));
        }
        let bytes = file_size(&first);
        Ok(Self {
            original: original.to_path_buf(),
            folder,
            current: Version {
                number: 0,
                path: first,
                bytes,
            },
            undo: Vec::new(),
            redo: Vec::new(),
            // the first version is a copy of the file on disk
            saved_version: Some(0),
            revision: 0,
            next_version: 1,
            limits,
            protected_original: false,
            save_writer: None,
            forget_history_on_save: false,
        })
    }

    /// the person's file, where save writes to.
    pub fn original_path(&self) -> &Path {
        &self.original
    }

    /// organise: the file was moved or renamed in Letters while open, so
    /// Save writes to its new place. the working copy is not touched.
    pub fn follow_move(&mut self, new_path: &Path) {
        self.original = new_path.to_path_buf();
    }

    /// the file the engine should show.
    pub fn working_copy(&self) -> &Path {
        &self.current.path
    }

    /// goes up by one with every edit, undo and redo, so the ui can tell
    /// that what it shows is out of date.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// true when what is shown differs from the file on disk. undoing
    /// back to the saved version makes it false again.
    pub fn is_dirty(&self) -> bool {
        self.saved_version != Some(self.current.number)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// makes one edit.
    ///
    /// the current version is loaded, `edit` changes it in memory, the
    /// result is written as a new version, and `reload` is asked to show
    /// that file. only when all of that worked does the new version become
    /// current; otherwise the new file is deleted and nothing has changed.
    /// returns whatever `reload` returned (for the shell, the new page sizes).
    pub fn apply<T, E>(
        &mut self,
        edit: impl FnOnce(&mut Document) -> Result<(), E>,
        reload: impl FnOnce(&Path) -> Result<T, String>,
    ) -> Result<T, ApplyError<E>> {
        let mut document = self.load_current().map_err(ApplyError::Session)?;
        edit(&mut document).map_err(ApplyError::Edit)?;
        let next = self
            .store_version(&mut document)
            .map_err(ApplyError::Session)?;
        let shown = match reload(&next.path) {
            Ok(shown) => shown,
            Err(detail) => {
                let _ = fs::remove_file(&next.path);
                return Err(ApplyError::Session(SessionError::CannotShow { detail }));
            }
        };
        let previous = std::mem::replace(&mut self.current, next);
        self.undo.push(previous);
        // a new edit starts a new future: what was undone can't come back
        for undone in self.redo.drain(..) {
            let _ = fs::remove_file(&undone.path);
        }
        self.trim_history();
        self.revision += 1;
        Ok(shown)
    }

    /// goes back one version. `reload` is asked to show it first, so if
    /// that fails nothing changes.
    pub fn undo<T>(
        &mut self,
        reload: impl FnOnce(&Path) -> Result<T, String>,
    ) -> Result<T, SessionError> {
        let target = self.undo.last().ok_or(SessionError::NothingToUndo)?;
        let shown = reload(&target.path).map_err(|detail| SessionError::CannotShow { detail })?;
        let target = self.undo.pop().expect("checked above");
        let previous = std::mem::replace(&mut self.current, target);
        self.redo.push(previous);
        self.trim_history();
        self.revision += 1;
        Ok(shown)
    }

    /// goes forward again to the version undo went back from.
    pub fn redo<T>(
        &mut self,
        reload: impl FnOnce(&Path) -> Result<T, String>,
    ) -> Result<T, SessionError> {
        let target = self.redo.last().ok_or(SessionError::NothingToRedo)?;
        let shown = reload(&target.path).map_err(|detail| SessionError::CannotShow { detail })?;
        let target = self.redo.pop().expect("checked above");
        let previous = std::mem::replace(&mut self.current, target);
        self.undo.push(previous);
        self.trim_history();
        self.revision += 1;
        Ok(shown)
    }

    /// writes the current version over the person's file.
    ///
    /// refuses, without touching anything, when the file is gone or
    /// read-only; the message suggests Save As instead.
    pub fn save(&mut self) -> Result<(), SessionError> {
        // a signed file is only ever written exactly (adopt_exact_bytes)
        if self.protected_original && self.is_dirty() {
            return Err(SessionError::SignedOriginal {
                name: file_name(&self.original),
            });
        }
        check_can_replace(&self.original)?;
        self.write_current(&self.original)?;
        self.saved_version = Some(self.current.number);
        self.after_save();
        Ok(())
    }

    /// writes the current version to a new place. from then on the
    /// session belongs to that file: save writes there, and the file it
    /// came from is left as it was.
    pub fn save_as(&mut self, destination: &Path) -> Result<(), SessionError> {
        if destination.exists() {
            check_can_replace(destination)?;
        }
        // an unchanged signed document is copied exactly, even when saves
        // are otherwise rewritten (Protect): a rewrite would break its
        // signature, and the signed bytes were themselves made from a
        // clean save, so they hold nothing a clean save would remove
        if self.protected_original && !self.is_dirty() {
            replace_file(&self.current.path, destination)?;
        } else {
            self.write_current(destination)?;
        }
        // an unchanged signed document makes a copy that is still signed
        // and stays protected; a changed one has nothing left to protect
        if self.is_dirty() {
            self.protected_original = false;
        }
        self.original = destination.to_path_buf();
        self.saved_version = Some(self.current.number);
        self.after_save();
        Ok(())
    }

    // ---- clean saves and unlocking (Protect, see protect/) ----

    /// from now on save and save as write what `writer` makes from the
    /// current version instead of copying it. there is no way to turn it
    /// off again: once a redaction was made, every save of this session
    /// must be a clean one. a later call replaces the writer (new
    /// password settings); `forget_history` can only be switched on.
    pub fn write_saves_with(&mut self, writer: SaveWriter, forget_history: bool) {
        self.save_writer = Some(writer);
        self.forget_history_on_save |= forget_history;
    }

    /// true once saves are written by a writer (see `write_saves_with`).
    pub fn saves_are_rewritten(&self) -> bool {
        self.save_writer.is_some()
    }

    /// makes the document count as changed, so it can be saved, for a
    /// change that lives outside the pdf until save (a new password).
    pub fn mark_unsaved(&mut self) {
        self.saved_version = None;
    }

    /// deletes every undo and redo version from the disk. after a
    /// redaction is saved, the versions before it still hold what was
    /// removed, so they must not stay in the cache.
    pub fn forget_history(&mut self) {
        for version in self.undo.drain(..).chain(self.redo.drain(..)) {
            let _ = fs::remove_file(&version.path);
        }
    }

    /// every file this session keeps in the cache: the current version
    /// first, then the history. for checks and tests.
    pub fn version_files(&self) -> Vec<PathBuf> {
        let history = self.undo.iter().chain(self.redo.iter());
        std::iter::once(&self.current)
            .chain(history)
            .map(|version| version.path.clone())
            .collect()
    }

    /// swaps a locked working copy for the unlocked `bytes` of the same
    /// document. the old versions are locked copies nothing can edit, so
    /// they are deleted. the document still matches the file on disk, so
    /// it is not marked as changed.
    pub fn start_over_unlocked<T>(
        &mut self,
        bytes: &[u8],
        reload: impl FnOnce(&Path) -> Result<T, String>,
    ) -> Result<T, SessionError> {
        let next = self.store_exact_version(bytes)?;
        let shown = match reload(&next.path) {
            Ok(shown) => shown,
            Err(detail) => {
                let _ = fs::remove_file(&next.path);
                return Err(SessionError::CannotShow { detail });
            }
        };
        let locked = std::mem::replace(&mut self.current, next);
        let _ = fs::remove_file(&locked.path);
        self.forget_history();
        let was_saved = self.saved_version == Some(locked.number);
        self.saved_version = was_saved.then_some(self.current.number);
        self.revision += 1;
        Ok(shown)
    }

    /// writes the current version to `destination`: through the writer
    /// when there is one, otherwise as an exact copy.
    fn write_current(&self, destination: &Path) -> Result<(), SessionError> {
        let Some(writer) = &self.save_writer else {
            return replace_file(&self.current.path, destination);
        };
        let bytes = writer(&self.current.path).map_err(|reason| SessionError::SaveRefused {
            name: file_name(destination),
            reason,
        })?;
        replace_file_with_bytes(&bytes, destination)
    }

    fn after_save(&mut self) {
        if self.forget_history_on_save {
            self.forget_history();
        }
    }

    // ---- signed documents (forms and signatures, see forms/signed.rs) ----

    /// turns the guard for a digitally signed file on or off. while it is
    /// on, save refuses to write a changed version over the file.
    pub fn protect_original(&mut self, protect: bool) {
        self.protected_original = protect;
    }

    pub fn original_is_protected(&self) -> bool {
        self.protected_original
    }

    /// makes `bytes` the new version and the person's file, exactly as
    /// given, without passing through lopdf: a signature covers exact
    /// bytes. it is one undo step like an edit, and the file is then
    /// protected. `reload` shows a version; if writing the file fails the
    /// version before is shown again and nothing has changed.
    pub fn adopt_exact_bytes<T>(
        &mut self,
        bytes: &[u8],
        reload: impl Fn(&Path) -> Result<T, String>,
    ) -> Result<T, SessionError> {
        check_can_replace(&self.original)?;
        let next = self.store_exact_version(bytes)?;
        let shown = match reload(&next.path) {
            Ok(shown) => shown,
            Err(detail) => {
                let _ = fs::remove_file(&next.path);
                return Err(SessionError::CannotShow { detail });
            }
        };
        if let Err(error) = replace_file(&next.path, &self.original) {
            let _ = reload(&self.current.path);
            let _ = fs::remove_file(&next.path);
            return Err(error);
        }
        let previous = std::mem::replace(&mut self.current, next);
        self.undo.push(previous);
        for undone in self.redo.drain(..) {
            let _ = fs::remove_file(&undone.path);
        }
        self.trim_history();
        self.revision += 1;
        self.saved_version = Some(self.current.number);
        self.protected_original = true;
        // signing is a save: versions a clean save would delete go too
        self.after_save();
        Ok(shown)
    }

    fn store_exact_version(&mut self, bytes: &[u8]) -> Result<Version, SessionError> {
        let number = self.next_version;
        self.next_version += 1;
        let path = self.folder.join(version_file_name(number));
        if let Err(error) = fs::write(&path, bytes) {
            let _ = fs::remove_file(&path);
            return Err(SessionError::CannotStore {
                detail: error.to_string(),
            });
        }
        Ok(Version {
            number,
            path,
            bytes: bytes.len() as u64,
        })
    }

    /// ends the session and deletes its working copy and history.
    /// the person's file is not touched.
    pub fn discard(self) {
        // dropping does the work (see the drop impl below)
    }

    /// the document as it is shown now, loaded for reading. nothing is
    /// changed and no undo step is made, which is what tools that make
    /// other files (extract, split) need.
    pub fn read_document(&self) -> Result<Document, SessionError> {
        self.load_current()
    }

    fn load_current(&self) -> Result<Document, SessionError> {
        let document = Document::load(&self.current.path).map_err(describe_load_error)?;
        if document.was_encrypted() || document.is_encrypted() {
            return Err(SessionError::PasswordProtected);
        }
        Ok(document)
    }

    fn store_version(&mut self, document: &mut Document) -> Result<Version, SessionError> {
        let number = self.next_version;
        self.next_version += 1;
        let path = self.folder.join(version_file_name(number));
        // save returns the open file: it is dropped (closed) right away so
        // the engine can open it next
        if let Err(error) = document.save(&path).map(drop) {
            let _ = fs::remove_file(&path);
            return Err(SessionError::CannotStore {
                detail: error.to_string(),
            });
        }
        let bytes = file_size(&path);
        Ok(Version {
            number,
            path,
            bytes,
        })
    }

    fn trim_history(&mut self) {
        trim(&mut self.undo, self.limits);
        trim(&mut self.redo, self.limits);
    }
}

impl Drop for EditSession {
    /// a session that ends for any reason (closed, window gone, panic)
    /// takes its files with it. if that fails, for example because a file
    /// is still open somewhere, the next start of the app cleans up.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.folder);
    }
}

/// drops the oldest steps until a stack is within its limits.
///
/// the newest step is always kept, even when it alone is over the byte
/// limit: one undo for a huge file is better than none.
fn trim(stack: &mut Vec<Version>, limits: HistoryLimits) {
    let over = |stack: &Vec<Version>| {
        let bytes: u64 = stack.iter().map(|version| version.bytes).sum();
        stack.len() > limits.max_steps || (bytes > limits.max_bytes && stack.len() > 1)
    };
    while over(stack) {
        let oldest = stack.remove(0);
        let _ = fs::remove_file(&oldest.path);
    }
}

/// removes session folders left behind by an earlier run that did not
/// end cleanly (a crash, or the computer turning off).
///
/// only one copy of Letters runs at a time, so at startup every folder
/// not made by this run is a leftover. folders of this run start with
/// its process id and are kept, so calling this later is still safe.
pub fn clean_leftover_sessions(sessions_root: &Path) {
    let Ok(entries) = fs::read_dir(sessions_root) else {
        return;
    };
    let ours = format!("{}-", std::process::id());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&ours) {
            continue;
        }
        let path = entry.path();
        let _ = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
    }
}

/// a folder name no other session of this run, or of any other run, uses.
fn new_folder_name() -> String {
    let number = NEXT_SESSION_NUMBER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{number}", std::process::id())
}

fn version_file_name(number: u64) -> String {
    format!("version-{number}.pdf")
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// the name people know a file by, for messages.
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// adds ".pdf" to a name typed without it, so the saved file still opens
/// as a pdf. "Essay" becomes "Essay.pdf"; "Essay.PDF" stays as it is.
pub fn with_pdf_extension(path: &Path) -> PathBuf {
    let has_pdf_extension = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"));
    if has_pdf_extension {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".pdf");
    PathBuf::from(name)
}

/// says why `path` can't be written over, before anything is written.
fn check_can_replace(path: &Path) -> Result<(), SessionError> {
    let name = file_name(path);
    match fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(SessionError::OriginalMissing { name })
        }
        Err(error) => Err(SessionError::CannotWrite {
            name,
            detail: error.to_string(),
        }),
        Ok(meta) if meta.is_dir() => Err(SessionError::CannotWrite {
            name,
            detail: "it is a folder".into(),
        }),
        Ok(meta) if meta.permissions().readonly() => Err(SessionError::OriginalLocked { name }),
        Ok(_) => Ok(()),
    }
}

/// puts a copy of `source` at `destination` in one step.
///
/// the copy is written to a temporary file in the destination's folder
/// first (a rename is only instant within one drive), flushed to the
/// disk, and then renamed over the destination. if anything fails the
/// temporary file is removed and the destination is untouched.
fn replace_file(source: &Path, destination: &Path) -> Result<(), SessionError> {
    let temporary = temporary_path_beside(destination);
    let result =
        copy_and_flush(source, &temporary).and_then(|_| fs::rename(&temporary, destination));
    result.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        SessionError::CannotWrite {
            name: file_name(destination),
            detail: error.to_string(),
        }
    })
}

/// protect: like `replace_file`, for bytes made in memory (a clean save).
fn replace_file_with_bytes(bytes: &[u8], destination: &Path) -> Result<(), SessionError> {
    let temporary = temporary_path_beside(destination);
    let write = || -> io::Result<()> {
        let mut writer = fs::File::create(&temporary)?;
        io::Write::write_all(&mut writer, bytes)?;
        writer.sync_all()?;
        drop(writer);
        fs::rename(&temporary, destination)
    };
    write().map_err(|error| {
        let _ = fs::remove_file(&temporary);
        SessionError::CannotWrite {
            name: file_name(destination),
            detail: error.to_string(),
        }
    })
}

/// copies a file and waits until the copy is really on the disk, not
/// just in the system's memory, so the rename after it can't publish a
/// file that a power cut would leave empty.
fn copy_and_flush(source: &Path, destination: &Path) -> io::Result<()> {
    let mut reader = fs::File::open(source)?;
    let mut writer = fs::File::create(destination)?;
    io::copy(&mut reader, &mut writer)?;
    writer.sync_all()
}

/// "essay.pdf" becomes ".essay.pdf.letters-saving" in the same folder.
fn temporary_path_beside(destination: &Path) -> PathBuf {
    destination.with_file_name(format!(".{}.letters-saving", file_name(destination)))
}

/// lopdf decrypts what it can while loading; a file it can't decrypt is
/// reported as locked rather than as damaged.
fn describe_load_error(error: lopdf::Error) -> SessionError {
    match error {
        lopdf::Error::InvalidPassword
        | lopdf::Error::Decryption(_)
        | lopdf::Error::UnsupportedSecurityHandler(_)
        | lopdf::Error::AlreadyEncrypted => SessionError::PasswordProtected,
        other => SessionError::CannotEdit {
            detail: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pdf::{page_rotation, temp_dir, write_pdf};

    /// a reload that always works and hands back the file it was asked to show.
    fn show(path: &Path) -> Result<PathBuf, String> {
        Ok(path.to_path_buf())
    }

    /// an edit that turns the first page a quarter turn.
    fn turn_first_page(document: &mut Document) -> Result<(), String> {
        let first = *document.get_pages().get(&1).ok_or("no pages")?;
        let page = document
            .get_dictionary_mut(first)
            .map_err(|e| e.to_string())?;
        let turned = page.get(b"Rotate").and_then(|r| r.as_i64()).unwrap_or(0) + 90;
        page.set("Rotate", turned);
        Ok(())
    }

    fn files_in(folder: &Path) -> usize {
        fs::read_dir(folder)
            .map(|entries| entries.count())
            .unwrap_or(0)
    }

    /// a session on a fresh two page pdf, and the folders it uses.
    struct Fixture {
        original: PathBuf,
        root: PathBuf,
        session: EditSession,
    }

    fn fixture(test_name: &str, limits: HistoryLimits) -> Fixture {
        let dir = temp_dir(test_name);
        let original = dir.join("essay.pdf");
        write_pdf(&original, 2);
        let root = dir.join("sessions");
        let session = EditSession::start_with_limits(&original, &root, limits).unwrap();
        Fixture {
            original,
            root,
            session,
        }
    }

    #[test]
    fn starting_makes_a_working_copy_and_leaves_the_file_alone() {
        let Fixture {
            original, session, ..
        } = fixture("start", HistoryLimits::default());

        assert_ne!(session.working_copy(), original.as_path());
        assert_eq!(
            fs::read(session.working_copy()).unwrap(),
            fs::read(&original).unwrap()
        );
        assert!(!session.is_dirty());
        assert!(!session.can_undo());
        assert!(!session.can_redo());
        assert_eq!(session.revision(), 0);
    }

    #[test]
    fn an_edit_changes_the_working_copy_but_not_the_file() {
        let Fixture {
            original,
            mut session,
            ..
        } = fixture("edit", HistoryLimits::default());
        let before = fs::read(&original).unwrap();
        let first_copy = session.working_copy().to_path_buf();

        let shown = session.apply(turn_first_page, show).unwrap();

        assert_eq!(shown, session.working_copy());
        assert_ne!(session.working_copy(), first_copy.as_path());
        assert_eq!(page_rotation(session.working_copy(), 1), 90);
        assert_eq!(fs::read(&original).unwrap(), before);
        assert!(session.is_dirty());
        assert!(session.can_undo());
        assert_eq!(session.revision(), 1);
    }

    #[test]
    fn a_failed_edit_leaves_the_working_copy_exactly_as_it_was() {
        let Fixture {
            mut session, root, ..
        } = fixture("failed-edit", HistoryLimits::default());
        let copy = session.working_copy().to_path_buf();
        let bytes = fs::read(&copy).unwrap();

        let result = session.apply(
            |document: &mut Document| {
                // change something, then give up: the change must not stick
                turn_first_page(document)?;
                Err("the edit gave up".to_string())
            },
            show,
        );

        assert_eq!(
            result,
            Err(ApplyError::Edit("the edit gave up".to_string()))
        );
        assert_eq!(session.working_copy(), copy.as_path());
        assert_eq!(fs::read(&copy).unwrap(), bytes);
        assert!(!session.is_dirty());
        assert!(!session.can_undo());
        assert_eq!(session.revision(), 0);
        let folder = copy.parent().unwrap();
        assert_eq!(files_in(folder), 1, "no stray version is left behind");
        assert_eq!(files_in(&root), 1);
    }

    #[test]
    fn a_failed_reload_leaves_everything_as_it_was() {
        let Fixture { mut session, .. } = fixture("failed-reload", HistoryLimits::default());
        let copy = session.working_copy().to_path_buf();

        let result: Result<(), _> = session.apply(turn_first_page, |_| Err("engine busy".into()));

        assert_eq!(
            result,
            Err(ApplyError::Session(SessionError::CannotShow {
                detail: "engine busy".into()
            }))
        );
        assert_eq!(session.working_copy(), copy.as_path());
        assert_eq!(files_in(copy.parent().unwrap()), 1);
        assert!(!session.can_undo());
    }

    #[test]
    fn undo_and_redo_move_between_versions() {
        let Fixture { mut session, .. } = fixture("undo-redo", HistoryLimits::default());
        let first = fs::read(session.working_copy()).unwrap();
        session.apply(turn_first_page, show).unwrap();
        let edited = fs::read(session.working_copy()).unwrap();

        session.undo(show).unwrap();
        assert_eq!(fs::read(session.working_copy()).unwrap(), first);
        assert!(!session.is_dirty(), "back at the saved version");
        assert!(session.can_redo());
        assert!(!session.can_undo());

        session.redo(show).unwrap();
        assert_eq!(fs::read(session.working_copy()).unwrap(), edited);
        assert!(session.is_dirty());
        assert!(!session.can_redo());
        assert_eq!(session.revision(), 3, "edit, undo and redo each count");
    }

    #[test]
    fn undo_and_redo_say_when_there_is_nothing_to_do() {
        let Fixture { mut session, .. } = fixture("nothing", HistoryLimits::default());
        assert_eq!(session.undo(show), Err(SessionError::NothingToUndo));
        assert_eq!(session.redo(show), Err(SessionError::NothingToRedo));
    }

    #[test]
    fn a_failed_undo_changes_nothing() {
        let Fixture { mut session, .. } = fixture("failed-undo", HistoryLimits::default());
        session.apply(turn_first_page, show).unwrap();
        let copy = session.working_copy().to_path_buf();

        let result: Result<(), _> = session.undo(|_| Err("engine busy".into()));

        assert!(matches!(result, Err(SessionError::CannotShow { .. })));
        assert_eq!(session.working_copy(), copy.as_path());
        assert!(session.can_undo());
        assert!(!session.can_redo());
    }

    #[test]
    fn a_new_edit_clears_what_was_undone() {
        let Fixture { mut session, .. } = fixture("new-future", HistoryLimits::default());
        session.apply(turn_first_page, show).unwrap();
        session.undo(show).unwrap();
        let folder = session.working_copy().parent().unwrap().to_path_buf();

        session.apply(turn_first_page, show).unwrap();

        assert!(!session.can_redo());
        // the current version and the one before it, nothing else
        assert_eq!(files_in(&folder), 2);
    }

    #[test]
    fn history_is_capped_by_count_dropping_the_oldest() {
        let limits = HistoryLimits {
            max_steps: 3,
            max_bytes: u64::MAX,
        };
        let Fixture { mut session, .. } = fixture("cap-count", limits);
        for _ in 0..5 {
            session.apply(turn_first_page, show).unwrap();
        }
        let folder = session.working_copy().parent().unwrap().to_path_buf();
        assert_eq!(files_in(&folder), 4, "the current version and three steps");

        let mut undone = 0;
        while session.undo(show).is_ok() {
            undone += 1;
        }
        assert_eq!(undone, 3);
        // five quarter turns, three undone: two remain
        assert_eq!(page_rotation(session.working_copy(), 1), 180);
        assert!(session.is_dirty(), "the saved version was dropped long ago");
    }

    #[test]
    fn history_is_capped_by_size_but_keeps_one_step() {
        let limits = HistoryLimits {
            max_steps: 30,
            max_bytes: 10,
        };
        let Fixture { mut session, .. } = fixture("cap-bytes", limits);
        for _ in 0..4 {
            session.apply(turn_first_page, show).unwrap();
        }

        assert!(session.undo(show).is_ok());
        assert_eq!(session.undo(show), Err(SessionError::NothingToUndo));
    }

    #[test]
    fn save_replaces_the_file_and_leaves_no_temporary_file() {
        let Fixture {
            original,
            mut session,
            ..
        } = fixture("save", HistoryLimits::default());
        session.apply(turn_first_page, show).unwrap();

        session.save().unwrap();

        assert_eq!(page_rotation(&original, 1), 90);
        assert!(!session.is_dirty());
        assert!(session.can_undo(), "undo still works after saving");
        let folder = original.parent().unwrap();
        let leftovers: Vec<_> = fs::read_dir(folder)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".letters-saving")
            })
            .collect();
        assert!(leftovers.is_empty());

        // undoing past the save makes the document differ from the file again
        session.undo(show).unwrap();
        assert!(session.is_dirty());
    }

    #[test]
    fn save_refuses_a_read_only_file_and_suggests_save_as() {
        let Fixture {
            original,
            mut session,
            ..
        } = fixture("save-locked", HistoryLimits::default());
        session.apply(turn_first_page, show).unwrap();
        let before = fs::read(&original).unwrap();
        set_read_only(&original, true);

        let result = session.save();

        set_read_only(&original, false);
        assert_eq!(
            result,
            Err(SessionError::OriginalLocked {
                name: "essay.pdf".into()
            })
        );
        assert!(result.unwrap_err().to_string().contains("Save As…"));
        assert_eq!(fs::read(&original).unwrap(), before);
        assert!(session.is_dirty());
    }

    #[test]
    fn save_refuses_a_file_that_is_gone() {
        let Fixture {
            original,
            mut session,
            ..
        } = fixture("save-missing", HistoryLimits::default());
        session.apply(turn_first_page, show).unwrap();
        fs::remove_file(&original).unwrap();

        let result = session.save();

        assert_eq!(
            result,
            Err(SessionError::OriginalMissing {
                name: "essay.pdf".into()
            })
        );
        assert!(!original.exists(), "nothing was written in its place");
        assert!(session.is_dirty());
    }

    #[test]
    fn save_as_writes_a_new_file_and_the_session_follows_it() {
        let Fixture {
            original,
            mut session,
            ..
        } = fixture("save-as", HistoryLimits::default());
        let before = fs::read(&original).unwrap();
        session.apply(turn_first_page, show).unwrap();
        let copy = original.with_file_name("essay turned.pdf");

        session.save_as(&copy).unwrap();

        assert_eq!(page_rotation(&copy, 1), 90);
        assert_eq!(fs::read(&original).unwrap(), before);
        assert_eq!(session.original_path(), copy.as_path());
        assert!(!session.is_dirty());

        session.apply(turn_first_page, show).unwrap();
        session.save().unwrap();
        assert_eq!(page_rotation(&copy, 1), 180);
        assert_eq!(fs::read(&original).unwrap(), before);
    }

    #[test]
    fn save_as_refuses_to_write_over_a_read_only_file() {
        let Fixture {
            original,
            mut session,
            ..
        } = fixture("save-as-locked", HistoryLimits::default());
        let locked = original.with_file_name("locked.pdf");
        write_pdf(&locked, 1);
        set_read_only(&locked, true);

        let result = session.save_as(&locked);

        set_read_only(&locked, false);
        assert!(matches!(result, Err(SessionError::OriginalLocked { .. })));
        assert_eq!(session.original_path(), original.as_path());
    }

    #[test]
    fn save_as_into_a_missing_folder_says_so() {
        let Fixture {
            original,
            mut session,
            ..
        } = fixture("save-as-nowhere", HistoryLimits::default());
        let nowhere = original.with_file_name("no such folder").join("essay.pdf");

        let result = session.save_as(&nowhere);

        assert!(matches!(result, Err(SessionError::CannotWrite { .. })));
        assert_eq!(session.original_path(), original.as_path());
    }

    #[test]
    fn discarding_or_dropping_a_session_removes_its_files() {
        let Fixture {
            original,
            root,
            mut session,
        } = fixture("discard", HistoryLimits::default());
        session.apply(turn_first_page, show).unwrap();
        assert_eq!(files_in(&root), 1);
        session.discard();
        assert_eq!(files_in(&root), 0);

        {
            let _dropped = EditSession::start(&original, &root).unwrap();
            assert_eq!(files_in(&root), 1);
        }
        assert_eq!(files_in(&root), 0);
        assert!(original.exists(), "the person's file is never removed");
    }

    #[test]
    fn leftovers_from_an_earlier_run_are_cleaned_but_ours_are_kept() {
        let Fixture { root, session, .. } = fixture("leftovers", HistoryLimits::default());
        let leftover = root.join("12345678-9");
        fs::create_dir_all(&leftover).unwrap();
        fs::write(leftover.join("version-3.pdf"), b"old").unwrap();

        clean_leftover_sessions(&root);

        assert!(!leftover.exists());
        assert!(session.working_copy().exists());
    }

    #[test]
    fn a_file_that_is_not_a_pdf_can_be_viewed_but_not_edited() {
        let dir = temp_dir("not-a-pdf");
        let original = dir.join("notes.pdf");
        fs::write(&original, b"just some text").unwrap();
        let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();

        let result = session.apply(turn_first_page, show);

        assert!(matches!(
            result,
            Err(ApplyError::Session(SessionError::CannotEdit { .. }))
        ));
    }

    #[test]
    fn a_session_for_a_missing_file_does_not_start() {
        let dir = temp_dir("missing");
        let root = dir.join("sessions");
        let result = EditSession::start(&dir.join("gone.pdf"), &root);
        assert!(matches!(result, Err(SessionError::CannotStart { .. })));
        assert_eq!(files_in(&root), 0, "no empty folder is left behind");
    }

    #[test]
    fn a_name_typed_without_pdf_gets_it_added() {
        assert_eq!(
            with_pdf_extension(Path::new("notes/Essay")),
            PathBuf::from("notes/Essay.pdf")
        );
        assert_eq!(
            with_pdf_extension(Path::new("Essay.PDF")),
            PathBuf::from("Essay.PDF")
        );
        assert_eq!(
            with_pdf_extension(Path::new("week 1.final")),
            PathBuf::from("week 1.final.pdf")
        );
    }

    fn set_read_only(path: &Path, read_only: bool) {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(read_only);
        fs::set_permissions(path, permissions).unwrap();
    }
}
