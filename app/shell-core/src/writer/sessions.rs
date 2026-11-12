//! every word processor document open in any window.
//!
//! the ui owns the editing (typing, undo, the dirty flag): it holds the
//! model and sends it back whole to save, autosave or export. the shell
//! owns the file: where it is, its format, what the ui never sees (carry
//! and picture bytes), the recovery copy and the safe save.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use letters_office::FileKind;
use serde::Serialize;
use serde_json::Value;

use crate::disk_stamp::{changed_on_disk_message, DiskStamp};
use crate::session::file_name;
use crate::writer::clipboard::{Copied, PastedHtml, PastedPicture};
use crate::writer::document::{ui_model, WriterDocument};
use crate::writer::export::{export_pdf, ExportError, ExportMethod};
use crate::writer::objects::{Compatibility, ObjectAnswer, ObjectOp};
use crate::writer::recovery::{RecoveryInfo, RecoveryStore};
use crate::writer::routing::save_kind_for_path;
use crate::writer::save::{odt_losses, save_document};
use letters_office::formats::{PrintPageSetup, SaveFormat};

/// what the ui gets when a document opens.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedWriter {
    pub id: u64,
    pub name: String,
    pub path: String,
    /// "docx" or "odt": the format save writes
    pub kind: &'static str,
    pub model: Value,
    /// pictures by media id, without their bytes (fetch them with `media`)
    pub media_ids: Vec<String>,
    /// a recovery copy left by a run that ended without saving
    pub recovery: Option<RecoveryInfo>,
    /// the name of the rich text or old word file this was converted from;
    /// saving writes `path` (a .docx beside it) and leaves that file alone
    pub converted_from: Option<String>,
}

/// the file a document belongs to after a save.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedWriter {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedPdf {
    pub path: String,
    /// "word" or "libreOffice"
    pub method: ExportMethod,
    /// the converter's name, for "Exported with Microsoft Word."
    pub method_label: &'static str,
    /// made by word, so it matches word page for page
    pub exact: bool,
    /// a plain sentence when it is not exact
    pub note: Option<String>,
    /// each section's paper and margins, for the print dialog
    pub pages: Vec<PrintPageSetup>,
}

pub fn kind_name(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Docx => "docx",
        FileKind::Odt => "odt",
    }
}

struct OpenWriter {
    document: WriterDocument,
    window: String,
    /// the document's file as this window last read or wrote it, so a
    /// save can tell when another window or program changed it since
    disk_stamp: Option<DiskStamp>,
}

pub struct WriterSessions {
    open: Mutex<HashMap<u64, Arc<Mutex<OpenWriter>>>>,
    next_id: AtomicU64,
    recovery: RecoveryStore,
    /// where word's layouts are kept, by document fingerprint
    layout_cache: PathBuf,
}

impl WriterSessions {
    pub fn new(cache_dir: &Path) -> WriterSessions {
        WriterSessions {
            open: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            recovery: RecoveryStore::new(cache_dir.join("writer recovery")),
            // the bridge hands word its copies in the system temp folder
            // (word can't open files under the app's own cache folder);
            // only the layout answers are kept here
            layout_cache: cache_dir.join("word layouts"),
        }
    }

    /// opens a .docx or .odt. `recovered` opens the recovery copy instead
    /// of the file (it then starts with unsaved changes in the ui).
    pub fn open(&self, window: &str, path: &Path, recovered: bool) -> Result<OpenedWriter, String> {
        let (mut writer, mut document) =
            WriterDocument::open(path).map_err(|error| error.to_string())?;
        if recovered {
            let copy = self
                .recovery
                .document(path)
                .ok_or("The recovered version is no longer there.")?;
            writer = WriterDocument::from_document(path, writer.source_kind, &copy);
            document = copy;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let converted_from = (writer.path != path).then(|| file_name(path));
        let opened = OpenedWriter {
            id,
            name: file_name(&writer.path),
            path: writer.path.to_string_lossy().into_owned(),
            kind: kind_name(writer.kind),
            model: ui_model(&document),
            media_ids: document.media.iter().map(|item| item.id.clone()).collect(),
            recovery: if recovered {
                None
            } else {
                self.recovery.find(&writer.path)
            },
            converted_from,
        };
        let entry = OpenWriter {
            disk_stamp: DiskStamp::of(&writer.path),
            document: writer,
            window: window.to_string(),
        };
        self.map().insert(id, Arc::new(Mutex::new(entry)));
        Ok(opened)
    }

    // --- conversion and export: "Open PDF as Document…" (see src/export)

    /// opens a document made in memory, such as a pdf rebuilt as an easy
    /// to edit document. `path` is where Save will write it (a .docx name
    /// that isn't taken); `converted_from` names the file it came from.
    pub fn open_converted(
        &self,
        window: &str,
        path: &Path,
        document: letters_office::model::Document,
        converted_from: &str,
    ) -> Result<OpenedWriter, String> {
        let writer = WriterDocument::from_document(path, FileKind::Docx, &document);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let opened = OpenedWriter {
            id,
            name: file_name(path),
            path: path.to_string_lossy().into_owned(),
            kind: kind_name(FileKind::Docx),
            model: ui_model(&document),
            media_ids: document.media.iter().map(|item| item.id.clone()).collect(),
            recovery: None,
            converted_from: Some(converted_from.to_string()),
        };
        let entry = OpenWriter {
            disk_stamp: DiskStamp::of(&writer.path),
            document: writer,
            window: window.to_string(),
        };
        self.map().insert(id, Arc::new(Mutex::new(entry)));
        Ok(opened)
    }

    /// saves to the document's file, or to `to` (save as). the format comes
    /// from the name; a name without .docx or .odt gets the current one.
    pub fn save(&self, id: u64, model: Value, to: Option<&Path>) -> Result<SavedWriter, String> {
        self.with(id, |open| {
            let target = match to {
                Some(path) => with_writer_extension(path, open.document.kind),
                None => open.document.path.clone(),
            };
            let kind = save_kind_for_path(&target).unwrap_or(open.document.kind);
            // saving over its own file, which another window or program
            // changed since: refuse, so that change isn't thrown away
            let over_own_file = target == open.document.path;
            if over_own_file
                && open
                    .disk_stamp
                    .is_some_and(|stamp| stamp.changed_since(&target))
            {
                return Err(changed_on_disk_message(&file_name(&target)));
            }
            let document = open.document.merge(model)?;
            save_document(&document, &target, kind)?;
            open.disk_stamp = DiskStamp::of(&target);
            // the old file's recovery copy is no longer needed either way
            self.recovery.discard(&open.document.path);
            open.document.follow_new_path(&target, kind);
            Ok(SavedWriter {
                name: file_name(&target),
                path: target.to_string_lossy().into_owned(),
                kind: kind_name(kind),
            })
        })
    }

    /// what saving this model as odt would lose, or `None`.
    pub fn odt_losses(&self, id: u64, model: Value) -> Result<Option<String>, String> {
        self.with(id, |open| {
            let document = open.document.merge(model)?;
            Ok(odt_losses(&document, open.document.source_kind))
        })
    }

    /// writes the recovery copy of a document with unsaved changes.
    pub fn autosave(&self, id: u64, model: Value) -> Result<(), String> {
        self.with(id, |open| {
            let document = open.document.merge(model)?;
            self.recovery.write(&open.document.path, &document)
        })
    }

    /// forgets the recovery copy: after "Don't Save", or when a recovered
    /// version was declined.
    pub fn discard_recovery(&self, id: u64) -> Result<(), String> {
        self.with(id, |open| {
            self.recovery.discard(&open.document.path);
            Ok(())
        })
    }

    pub fn media(&self, id: u64, media_id: &str) -> Result<Option<String>, String> {
        self.with(id, |open| Ok(open.document.media_data_url(media_id)))
    }

    /// reads a picture file into the document; returns its media id and
    /// its data address for showing it at once.
    pub fn add_picture(&self, id: u64, picture: &Path) -> Result<(String, String), String> {
        let bytes = std::fs::read(picture)
            .map_err(|error| format!("“{}” can’t be read. ({error})", file_name(picture)))?;
        self.with(id, |open| {
            let media_id = open.document.add_picture(&file_name(picture), bytes);
            let url = open.document.media_data_url(&media_id).unwrap_or_default();
            Ok((media_id, url))
        })
    }

    /// a picture given as bytes (pasted or dropped); returns its media id and data address.
    pub fn add_picture_bytes(
        &self,
        id: u64,
        name: &str,
        bytes: Vec<u8>,
    ) -> Result<(String, String), String> {
        self.with(id, |open| {
            let media_id = open.document.add_picture(name, bytes);
            let url = open.document.media_data_url(&media_id).unwrap_or_default();
            Ok((media_id, url))
        })
    }

    /// makes a pdf of the document as it is in the editor now, through
    /// word (exact) or libreoffice (close) by way of the engine's bridge.
    pub fn export_pdf(
        &self,
        id: u64,
        model: Value,
        pdf: &Path,
    ) -> Result<ExportedPdf, ExportError> {
        let document = self
            .with(id, |open| open.document.merge(model))
            .map_err(ExportError::Failed)?;
        let docx = letters_office::write_docx(&document)
            .map_err(|error| ExportError::Failed(error.to_string()))?;
        let made = export_pdf(&docx, pdf)?;
        Ok(ExportedPdf {
            path: pdf.to_string_lossy().into_owned(),
            method: made.method,
            method_label: made.method.label(),
            exact: made.exact,
            note: made.note,
            pages: letters_office::formats::page_setups(&document),
        })
    }

    /// Word's lines and page breaks for the document as it is in the
    /// editor, cached by the document's bytes.
    pub fn word_layout(
        &self,
        id: u64,
        model: Value,
    ) -> Result<crate::writer::layout::WordLayout, String> {
        let document = self.with(id, |open| open.document.merge(model))?;
        crate::writer::layout::word_layout(&document, &self.layout_cache)
    }

    /// Letters' own lines and pages (core/layout), the same answer as word's.
    pub fn engine_layout(
        &self,
        id: u64,
        model: Value,
    ) -> Result<crate::writer::layout::WordLayout, String> {
        let document = self.with(id, |open| open.document.merge(model))?;
        Ok(crate::writer::engine_layout::engine_layout(&document))
    }

    /// writes a copy in another format (Save As for formats that don't
    /// become the document's own file, Export a Copy, the Drive folder):
    /// any of the engine's `save_as` formats, chosen by the name. markdown
    /// puts its pictures in a folder beside the file.
    pub fn export_copy(&self, id: u64, model: Value, path: &Path) -> Result<(), String> {
        let document = self.with(id, |open| open.document.merge(model))?;
        let format = save_format_for_path(path).ok_or_else(|| {
            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("Letters can’t save as .{extension}.")
        })?;
        let saved = letters_office::formats::save_as(&document, format)
            .map_err(|error| error.to_string())?;
        crate::writer::save::write_atomically(&saved.bytes, path)?;
        let folder = path.parent().unwrap_or(Path::new("."));
        for (relative, bytes) in &saved.extra_files {
            let target = folder.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            crate::writer::save::write_atomically(bytes, &target)?;
        }
        Ok(())
    }

    /// runs one command on the page's objects (see objects.rs).
    pub fn objects(&self, id: u64, model: Value, op: &ObjectOp) -> Result<ObjectAnswer, String> {
        let mut document = self.with(id, |open| open.document.merge(model))?;
        Ok(crate::writer::objects::apply(&mut document, op))
    }

    /// the engine's feature table and what word would show differently.
    pub fn compatibility(&self, id: u64, model: Value) -> Result<Compatibility, String> {
        let document = self.with(id, |open| open.document.merge(model))?;
        Ok(crate::writer::objects::compatibility(&document))
    }

    /// the clipboard forms of some blocks of the document (`blocks` is the
    /// copied part, as the editor has it).
    pub fn copy(&self, id: u64, model: Value, blocks: Value) -> Result<Copied, String> {
        let document = self.with(id, |open| open.document.merge(model))?;
        let blocks = serde_json::from_value(blocks)
            .map_err(|error| format!("The selection couldn’t be copied. ({error})"))?;
        Ok(crate::writer::clipboard::copy(&document, blocks))
    }

    /// picture bytes from the clipboard or a dropped file, through the
    /// engine (bitmaps become png, sizes fit the text width).
    pub fn paste_picture(
        &self,
        id: u64,
        model: Value,
        bytes: &[u8],
        description: Option<String>,
    ) -> Result<PastedPicture, String> {
        self.with(id, |open| {
            let mut document = open.document.merge(model)?;
            let (pasted, item) =
                crate::writer::clipboard::paste_picture(&mut document, bytes, description)?;
            open.document.adopt(item);
            Ok(pasted)
        })
    }

    /// html from the clipboard as blocks, its pictures added to the document.
    pub fn paste_html(&self, id: u64, html: &str) -> Result<PastedHtml, String> {
        self.with(id, |open| {
            let taken: Vec<String> = open
                .document
                .media()
                .iter()
                .map(|item| item.id.clone())
                .collect();
            let (pasted, items) = crate::writer::clipboard::paste_html(html, &taken);
            items.into_iter().for_each(|item| open.document.adopt(item));
            Ok(pasted)
        })
    }
    /// the document's file was moved or renamed (organising): saves and
    /// recovery copies follow it to `to`.
    pub fn follow_move(&self, id: u64, to: &Path) -> Result<SavedWriter, String> {
        self.with(id, |open| {
            let kind = open.document.kind;
            open.document.follow_new_path(to, kind);
            Ok(SavedWriter {
                name: file_name(to),
                path: to.to_string_lossy().into_owned(),
                kind: kind_name(kind),
            })
        })
    }

    /// the bytes of the document in a format, for an upload.
    pub fn bytes_as(&self, id: u64, model: Value, kind: FileKind) -> Result<Vec<u8>, String> {
        let document = self.with(id, |open| open.document.merge(model))?;
        letters_office::save(&document, kind).map_err(|error| error.to_string())
    }

    pub fn path_of(&self, id: u64) -> Option<PathBuf> {
        self.with(id, |open| Ok(open.document.path.clone())).ok()
    }

    pub fn close(&self, id: u64) {
        self.map().remove(&id);
    }

    /// a closed window's documents go too. unsaved changes were offered
    /// for saving by the window's close sheet; a recovery copy stays only
    /// if the window ended without that question (a crash).
    pub fn close_window(&self, window: &str) {
        self.map().retain(|_, entry| {
            entry
                .lock()
                .map(|open| open.window != window)
                .unwrap_or(false)
        });
    }

    pub fn open_count(&self) -> usize {
        self.map().len()
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Arc<Mutex<OpenWriter>>>> {
        self.open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn with<T>(
        &self,
        id: u64,
        work: impl FnOnce(&mut OpenWriter) -> Result<T, String>,
    ) -> Result<T, String> {
        let entry = self
            .map()
            .get(&id)
            .cloned()
            .ok_or("This document is no longer open.")?;
        let mut open = entry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&mut open)
    }
}

/// the engine's format for a file name, by its extension.
pub fn save_format_for_path(path: &Path) -> Option<SaveFormat> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    Some(match extension.as_str() {
        "docx" => SaveFormat::Docx,
        "odt" => SaveFormat::Odt,
        "rtf" => SaveFormat::Rtf,
        "html" | "htm" => SaveFormat::Html,
        "txt" => SaveFormat::PlainText,
        "md" | "markdown" => SaveFormat::Markdown,
        "doc" => SaveFormat::Doc,
        _ => return None,
    })
}

/// "Essay" saved from a docx becomes "Essay.docx"; "Essay.odt" stays.
pub fn with_writer_extension(path: &Path, current: FileKind) -> PathBuf {
    if save_kind_for_path(path).is_some() {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(if current == FileKind::Odt {
        ".odt"
    } else {
        ".docx"
    });
    PathBuf::from(name)
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
