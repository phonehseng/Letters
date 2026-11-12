//! the documents open in any window, each with its edit session.
//!
//! every edit goes through `OpenDocuments::edit`: the session makes the
//! change on a new working copy file, the pdf worker is pointed at that
//! file under the same document id (which drops the old tiles), and the
//! ui gets back a `DocumentState` describing the document as it is now.
//! the person's file is only written by save and save as.

use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use letters_pdf::{DocumentId, PageSize, RenderWorker};
use letters_shell_core::session::{clean_leftover_sessions, file_name, EditSession};
use serde::Serialize;

use crate::errors::CommandError;

/// a page's size in points (1/72 inch), as the ui lays it out.
#[derive(Serialize)]
pub struct PageDto {
    pub width: f32,
    pub height: f32,
}

/// what the ui needs to show a document after it was opened or changed.
///
/// it is sent after every open, edit, undo, redo and save, so the ui
/// never has to guess: page sizes may have changed, and `revision` goes
/// up whenever what is shown changes, which the ui adds to tile
/// addresses so the web view fetches the new pictures.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentState {
    pub id: u64,
    /// the file's name, for the title
    pub name: String,
    /// where save writes to. changes after save as
    pub path: String,
    pub pages: Vec<PageDto>,
    pub revision: u64,
    /// true when there are changes the file on disk does not have yet
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

/// one open document: its session and the page sizes the engine reported.
struct OpenDocument {
    session: EditSession,
    page_sizes: Vec<PageSize>,
}

impl OpenDocument {
    fn state(&self, id: DocumentId) -> DocumentState {
        let path = self.session.original_path();
        DocumentState {
            id: id.0,
            name: file_name(path),
            path: path.to_string_lossy().into_owned(),
            pages: self.page_sizes.iter().map(page_dto).collect(),
            revision: self.session.revision(),
            dirty: self.session.is_dirty(),
            can_undo: self.session.can_undo(),
            can_redo: self.session.can_redo(),
        }
    }
}

pub struct OpenDocuments {
    /// where each session keeps its working copy and history
    sessions_root: PathBuf,
    /// each document has its own lock, so a slow edit of one document
    /// never holds up another
    open: Mutex<HashMap<DocumentId, Arc<Mutex<OpenDocument>>>>,
}

impl OpenDocuments {
    /// also removes working copies a crashed run left behind.
    pub fn new(sessions_root: PathBuf) -> Self {
        clean_leftover_sessions(&sessions_root);
        Self {
            sessions_root,
            open: Mutex::new(HashMap::new()),
        }
    }

    /// makes a working copy of `path` and opens that copy in the engine.
    pub fn open(
        &self,
        worker: &RenderWorker,
        path: &Path,
        password: Option<&str>,
    ) -> Result<DocumentState, CommandError> {
        let session = EditSession::start(path, &self.sessions_root)?;
        // if this fails (a wrong password, say) the session is dropped
        // here and its working copy deleted with it
        let info = worker.open(session.working_copy(), password)?;
        let document = OpenDocument {
            session,
            page_sizes: info.page_sizes,
        };
        let state = document.state(info.id);
        self.map().insert(info.id, Arc::new(Mutex::new(document)));
        Ok(state)
    }

    /// runs one edit on a document. see the module comment.
    pub fn edit<E: Display>(
        &self,
        worker: &RenderWorker,
        id: DocumentId,
        edit: impl FnOnce(&mut lopdf::Document) -> Result<(), E>,
    ) -> Result<DocumentState, CommandError> {
        self.with_document(id, |document| {
            let info = document
                .session
                .apply(edit, |path| reload(worker, id, path))?;
            document.page_sizes = info.page_sizes;
            Ok(document.state(id))
        })
    }

    pub fn undo(
        &self,
        worker: &RenderWorker,
        id: DocumentId,
    ) -> Result<DocumentState, CommandError> {
        self.with_document(id, |document| {
            let info = document.session.undo(|path| reload(worker, id, path))?;
            document.page_sizes = info.page_sizes;
            Ok(document.state(id))
        })
    }

    pub fn redo(
        &self,
        worker: &RenderWorker,
        id: DocumentId,
    ) -> Result<DocumentState, CommandError> {
        self.with_document(id, |document| {
            let info = document.session.redo(|path| reload(worker, id, path))?;
            document.page_sizes = info.page_sizes;
            Ok(document.state(id))
        })
    }

    pub fn save(&self, id: DocumentId) -> Result<DocumentState, CommandError> {
        self.with_document(id, |document| {
            document.session.save()?;
            Ok(document.state(id))
        })
    }

    pub fn save_as(&self, id: DocumentId, path: &Path) -> Result<DocumentState, CommandError> {
        self.with_document(id, |document| {
            document.session.save_as(path)?;
            Ok(document.state(id))
        })
    }

    pub fn state(&self, id: DocumentId) -> Result<DocumentState, CommandError> {
        self.with_document(id, |document| Ok(document.state(id)))
    }

    /// closes a document and throws away its unsaved changes and history.
    /// the ui asks the person first (see the unsaved changes sheet).
    pub fn close(&self, worker: Option<&RenderWorker>, id: DocumentId) {
        let removed = self.map().remove(&id);
        // the engine lets go of the working copy before it is deleted
        if let Some(worker) = worker {
            worker.close(id);
        }
        drop(removed);
    }

    fn map(&self) -> MutexGuard<'_, HashMap<DocumentId, Arc<Mutex<OpenDocument>>>> {
        self.open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// runs `work` with one document locked. the list of documents is
    /// only locked long enough to find it.
    fn with_document<T>(
        &self,
        id: DocumentId,
        work: impl FnOnce(&mut OpenDocument) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let entry = self
            .map()
            .get(&id)
            .cloned()
            .ok_or_else(CommandError::not_open)?;
        let mut document = entry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&mut document)
    }
}

/// points the engine at a new version of the document, under the same id.
fn reload(
    worker: &RenderWorker,
    id: DocumentId,
    path: &Path,
) -> Result<letters_pdf::DocumentInfo, String> {
    worker.reload(id, path).map_err(|error| error.to_string())
}

pub fn page_dto(size: &PageSize) -> PageDto {
    PageDto {
        width: size.width_pt,
        height: size.height_pt,
    }
}
