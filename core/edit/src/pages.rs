//! finding a page, and refusing locked documents.

use lopdf::{Document, ObjectId};

use crate::error::EditError;

/// refuses to work on an encrypted file: changes written into it could
/// not be read back, so it has to be unlocked first.
pub(crate) fn ensure_unlocked(doc: &Document) -> Result<(), EditError> {
    if doc.is_encrypted() {
        Err(EditError::Locked)
    } else {
        Ok(())
    }
}

/// the page object for a 0-based page number.
pub(crate) fn page_id(doc: &Document, page: usize) -> Result<ObjectId, EditError> {
    ensure_unlocked(doc)?;
    let number = u32::try_from(page + 1).map_err(|_| EditError::UnknownPage(page))?;
    doc.get_pages()
        .get(&number)
        .copied()
        .ok_or(EditError::UnknownPage(page))
}
