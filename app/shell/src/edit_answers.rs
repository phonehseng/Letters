//! running the Edit tab's newer changes (letters-edit's round two) through
//! `state.edit`, so each is one undo step, and what they answer.
//!
//! three answers are not plain errors: an id that is out of date (`stale`:
//! nothing changed and the page's fresh objects come back, as round one
//! does), a reflow the person must choose for (`applied: false`: nothing
//! changed and the choices come back), and a signed file (the error's kind
//! says so, so the ui can ask, naming the signer, and send the change again
//! with consent).

use letters_edit::{
    self as edit, ChangesSummary, ContentEditResult, EditError, FontNotice, PageObjects,
};
use letters_pdf::DocumentId;
use serde::Serialize;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// how one change ended.
pub enum Outcome<R> {
    /// it was made: one undo step
    Made(DocumentState, R),
    /// the engine answered but the change is not to be kept (a reflow
    /// waiting for an overflow choice); nothing changed
    Held(DocumentState, R),
    /// the id was out of date; nothing changed
    Stale(DocumentState),
}

/// a refusal for a signed file becomes its own kind of error; any other
/// becomes a message.
fn refusal(error: EditError) -> CommandError {
    match error {
        EditError::SignaturesWouldBreak(_) => CommandError::needs_consent(error.to_string()),
        other => CommandError::failed(other.to_string()),
    }
}

/// runs one engine call on the working copy. `keep` says whether its
/// answer is a change to keep.
pub fn run_change<R>(
    state: &AppState,
    id: u64,
    work: impl FnOnce(&mut lopdf::Document) -> Result<R, EditError>,
    keep: impl FnOnce(&R) -> bool,
) -> Result<Outcome<R>, CommandError> {
    let mut made: Option<R> = None;
    let mut held: Option<R> = None;
    let mut refused: Option<EditError> = None;
    let changed = state.edit(DocumentId(id), |document| {
        let answer = work(document).inspect_err(|error| refused = Some(error.clone()))?;
        if keep(&answer) {
            made = Some(answer);
            Ok(())
        } else {
            held = Some(answer);
            // an error leaves the working copy exactly as it was
            Err(EditError::InvalidValue(String::new()))
        }
    });
    let now = || state.documents.state(DocumentId(id));
    match (changed, made, held, refused) {
        (Ok(document), Some(answer), _, _) => Ok(Outcome::Made(document, answer)),
        (Err(_), _, Some(answer), _) => Ok(Outcome::Held(now()?, answer)),
        (Err(_), _, _, Some(EditError::UnknownObject)) => Ok(Outcome::Stale(now()?)),
        (Err(_), _, _, Some(error)) => Err(refusal(error)),
        (Err(error), _, _, None) => Err(error),
        (Ok(_), None, _, _) => Err(CommandError::failed(
            "The change couldn’t be made. Try again.",
        )),
    }
}

/// the page as it is now, for an answer to a stale id.
pub fn page_objects_now(
    state: &AppState,
    id: u64,
    page: usize,
) -> Result<PageObjects, CommandError> {
    state.read_document(DocumentId(id), |document| {
        edit::page_objects(document, page).map_err(|error| CommandError::failed(error.to_string()))
    })
}

/// what a change to shapes, pictures or text in the page's style answers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentAnswer {
    pub document: DocumentState,
    /// the page as it is now, with fresh ids
    pub objects: PageObjects,
    pub notices: Vec<FontNotice>,
    /// everything else that moved or changed, for the quiet line after it
    pub summary: ChangesSummary,
    /// the ids of what the change was about, as they are now
    pub object_ids: Vec<String>,
    pub stale: bool,
}

/// runs a change that answers letters-edit's `ContentEditResult`.
pub fn content_change(
    state: &AppState,
    id: u64,
    page: usize,
    work: impl FnOnce(&mut lopdf::Document) -> Result<ContentEditResult, EditError>,
) -> Result<ContentAnswer, CommandError> {
    match run_change(state, id, work, |_| true)? {
        Outcome::Made(document, result) | Outcome::Held(document, result) => Ok(ContentAnswer {
            document,
            objects: result.objects,
            notices: result.notices,
            summary: result.summary,
            object_ids: result.object_ids,
            stale: false,
        }),
        Outcome::Stale(document) => Ok(ContentAnswer {
            document,
            objects: page_objects_now(state, id, page)?,
            notices: Vec::new(),
            summary: ChangesSummary::default(),
            object_ids: Vec::new(),
            stale: true,
        }),
    }
}
