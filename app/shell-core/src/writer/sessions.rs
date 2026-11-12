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

use crate::session::file_name;
use crate::writer::document::{ui_model, WriterDocument};
use crate::writer::export::{export_pdf, ExportError, ExportMethod};
use crate::writer::recovery::{RecoveryInfo, RecoveryStore};
use crate::writer::routing::save_kind_for_path;
use crate::writer::save::{odt_losses, save_document};

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
}

pub struct WriterSessions {
    open: Mutex<HashMap<u64, Arc<Mutex<OpenWriter>>>>,
    next_id: AtomicU64,
    recovery: RecoveryStore,
    /// where exports write their temporary copies
    work_dir: PathBuf,
}

impl WriterSessions {
    pub fn new(cache_dir: &Path) -> WriterSessions {
        WriterSessions {
            open: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            recovery: RecoveryStore::new(cache_dir.join("writer recovery")),
            // word can't open files under the app's own cache folder
            // (measured: "we couldn't find your file"), but it can in the
            // system temp folder, so exports and layouts work there
            work_dir: std::env::temp_dir().join("letters-writer-export"),
        }
    }

    /// opens a .docx or .odt. `recovered` opens the recovery copy instead
    /// of the file (it then starts with unsaved changes in the ui).
    pub fn open(&self, window: &str, path: &Path, recovered: bool) -> Result<OpenedWriter, String> {
        let (mut writer, mut document) = WriterDocument::open(path).map_err(|error| error.to_string())?;
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
            kind: kind_name(writer.kind),
            model: ui_model(&document),
            media_ids: document.media.iter().map(|item| item.id.clone()).collect(),
            recovery: None,
            converted_from: Some(converted_from.to_string()),
        };
        let entry = OpenWriter {
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
            let document = open.document.merge(model)?;
            save_document(&document, &target, kind)?;
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
    pub fn add_picture_bytes(&self, id: u64, name: &str, bytes: Vec<u8>) -> Result<(String, String), String> {
        self.with(id, |open| {
            let media_id = open.document.add_picture(name, bytes);
            let url = open.document.media_data_url(&media_id).unwrap_or_default();
            Ok((media_id, url))
        })
    }

    /// makes a pdf of the document as it is in the editor now.
    pub fn export_pdf(&self, id: u64, model: Value, pdf: &Path) -> Result<ExportedPdf, ExportError> {
        let docx = self.work_dir.join(format!("export-{id}.docx"));
        self.with(id, |open| {
            let document = open.document.merge(model)?;
            std::fs::create_dir_all(&self.work_dir).map_err(|error| error.to_string())?;
            save_document(&document, &docx, FileKind::Docx)
        })
        .map_err(ExportError::Failed)?;
        let result = export_pdf(&docx, pdf, &self.work_dir);
        let _ = std::fs::remove_file(&docx);
        let method = result?;
        Ok(ExportedPdf {
            path: pdf.to_string_lossy().into_owned(),
            method,
            method_label: method.label(),
        })
    }

    /// Word's page breaks for the document as it is in the editor.
    pub fn word_layout(&self, id: u64, model: Value) -> Result<crate::writer::layout::WordLayout, String> {
        let docx = self.work_dir.join(format!("layout-{id}.docx"));
        let document = self.with(id, |open| open.document.merge(model))?;
        std::fs::create_dir_all(&self.work_dir).map_err(|error| error.to_string())?;
        save_document(&document, &docx, FileKind::Docx)?;
        let result = crate::writer::layout::word_layout(&docx, &document.blocks, &self.work_dir.join(format!("layout-{id}")));
        let _ = std::fs::remove_file(&docx);
        result
    }

    /// writes a copy in another format (Export a Copy, the Drive folder):
    /// .docx, .odt, .html or .txt by the name; the document stays its own.
    pub fn export_copy(&self, id: u64, model: Value, path: &Path) -> Result<(), String> {
        let document = self.with(id, |open| open.document.merge(model))?;
        let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        match extension.as_str() {
            "docx" => save_document(&document, path, FileKind::Docx),
            "odt" => save_document(&document, path, FileKind::Odt),
            "html" | "htm" => crate::writer::save::write_atomically(letters_office::export::to_html(&document).as_bytes(), path),
            "txt" => crate::writer::save::write_atomically(letters_office::export::to_plain_text(&document).as_bytes(), path),
            other => Err(format!("Letters can’t save as .{other} yet.")),
        }
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
        self.open.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn with<T>(&self, id: u64, work: impl FnOnce(&mut OpenWriter) -> Result<T, String>) -> Result<T, String> {
        let entry = self
            .map()
            .get(&id)
            .cloned()
            .ok_or("This document is no longer open.")?;
        let mut open = entry.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&mut open)
    }
}

/// "Essay" saved from a docx becomes "Essay.docx"; "Essay.odt" stays.
pub fn with_writer_extension(path: &Path, current: FileKind) -> PathBuf {
    if save_kind_for_path(path).is_some() {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(if current == FileKind::Odt { ".odt" } else { ".docx" });
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Block, Document};
    use std::fs;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("letters-writer-tests")
            .join(format!("sessions-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn docx_with(text: &str, path: &Path) {
        let mut document = Document::new();
        document.blocks = vec![Block::text(text)];
        fs::write(path, letters_office::write_docx(&document).unwrap()).unwrap();
    }

    fn edited(model: &Value, text: &str) -> Value {
        let mut model = model.clone();
        model["blocks"] = serde_json::json!([{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }]);
        model
    }

    fn read_text(path: &Path) -> String {
        let document = letters_office::open(&fs::read(path).unwrap()).unwrap();
        document.blocks[0].paragraph().unwrap().plain_text()
    }

    #[test]
    fn open_edit_save_and_save_as_odt() {
        let dir = folder("save");
        let path = dir.join("essay.docx");
        docx_with("First draft", &path);
        let sessions = WriterSessions::new(&dir.join("cache"));
        let opened = sessions.open("window-1", &path, false).unwrap();
        assert_eq!(opened.kind, "docx");
        assert!(opened.recovery.is_none());
        sessions.save(opened.id, edited(&opened.model, "Second draft"), None).unwrap();
        assert_eq!(read_text(&path), "Second draft");

        let saved = sessions
            .save(opened.id, edited(&opened.model, "As odt"), Some(&dir.join("essay copy")))
            .unwrap();
        assert_eq!(saved.kind, "docx", "a name without an extension keeps the format");
        let odt = sessions
            .save(opened.id, edited(&opened.model, "As odt"), Some(&dir.join("essay.odt")))
            .unwrap();
        assert_eq!(odt.kind, "odt");
        assert_eq!(letters_office::detect(&fs::read(dir.join("essay.odt")).unwrap()).unwrap(), FileKind::Odt);
        assert_eq!(sessions.path_of(opened.id).unwrap(), dir.join("essay.odt"));
    }

    #[test]
    fn a_crash_leaves_a_recovery_copy_that_the_next_open_offers() {
        let dir = folder("recover");
        let path = dir.join("essay.docx");
        docx_with("Saved words", &path);
        let cache = dir.join("cache");
        let sessions = WriterSessions::new(&cache);
        let opened = sessions.open("window-1", &path, false).unwrap();
        sessions.autosave(opened.id, edited(&opened.model, "Unsaved words")).unwrap();
        drop(sessions); // the app ends without saving

        let next_run = WriterSessions::new(&cache);
        let again = next_run.open("window-1", &path, false).unwrap();
        assert!(again.recovery.is_some());
        let recovered = next_run.open("window-1", &path, true).unwrap();
        assert_eq!(recovered.model["blocks"][0]["content"][0]["text"], "Unsaved words");
        // saving the recovered version makes the copy go away
        next_run.save(recovered.id, recovered.model.clone(), None).unwrap();
        assert!(next_run.open("window-1", &path, false).unwrap().recovery.is_none());
        assert_eq!(read_text(&path), "Unsaved words");
    }

    #[test]
    fn dont_save_discards_the_recovery_copy() {
        let dir = folder("discard");
        let path = dir.join("essay.docx");
        docx_with("Saved", &path);
        let sessions = WriterSessions::new(&dir.join("cache"));
        let opened = sessions.open("w", &path, false).unwrap();
        sessions.autosave(opened.id, edited(&opened.model, "Lost")).unwrap();
        sessions.discard_recovery(opened.id).unwrap();
        sessions.close(opened.id);
        assert!(sessions.open("w", &path, false).unwrap().recovery.is_none());
    }

    #[test]
    fn closing_a_window_closes_its_documents_only() {
        let dir = folder("windows");
        let path = dir.join("a.docx");
        docx_with("A", &path);
        let sessions = WriterSessions::new(&dir.join("cache"));
        sessions.open("window-1", &path, false).unwrap();
        let other = sessions.open("window-2", &path, false).unwrap();
        sessions.close_window("window-1");
        assert_eq!(sessions.open_count(), 1);
        assert!(sessions.path_of(other.id).is_some());
    }

    #[test]
    fn a_file_that_is_not_a_document_gives_a_sentence() {
        let dir = folder("bad");
        let path = dir.join("fake.docx");
        fs::write(&path, b"not a zip").unwrap();
        let error = WriterSessions::new(&dir.join("cache")).open("w", &path, false).unwrap_err();
        assert!(error.contains("damaged") || error.contains("isn’t"), "{error}");
    }

    /// writes the ui models the editor's round-trip tests use
    /// (app/ui/src/lib/writer/fixtures). run with LETTERS_FIXTURE_DOCX set
    /// to a docx made by word and LETTERS_FIXTURE_DIR to the folder.
    #[test]
    #[ignore]
    fn write_ui_fixtures() {
        let docx = PathBuf::from(std::env::var("LETTERS_FIXTURE_DOCX").unwrap());
        let out = PathBuf::from(std::env::var("LETTERS_FIXTURE_DIR").unwrap());
        let document = letters_office::open(&fs::read(&docx).unwrap()).unwrap();
        let pretty = |value: &Value| serde_json::to_string_pretty(value).unwrap();
        fs::write(out.join("word-made.json"), pretty(&ui_model(&document))).unwrap();
        let odt = letters_office::write_odt(&document).unwrap();
        let from_odt = letters_office::open(&odt).unwrap();
        fs::write(out.join("word-made-as-odt.json"), pretty(&ui_model(&from_odt))).unwrap();
    }

    #[test]
    fn names_get_the_right_extension() {
        assert_eq!(with_writer_extension(Path::new("a"), FileKind::Odt), PathBuf::from("a.odt"));
        assert_eq!(with_writer_extension(Path::new("a.DOCX"), FileKind::Odt), PathBuf::from("a.DOCX"));
    }
}
