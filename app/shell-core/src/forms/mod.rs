//! the parts of Fill & Sign that need no window.
//!
//! - `signed`: signing the saved file's exact bytes through the edit
//!   session, so the signature is never rewritten by a later full save.
//! - `scripts`: whether a form carries automatic calculations, which
//!   Letters never runs.
//! - `signature_pictures`: the pictures of a person's signature, kept in
//!   the app's data folder (never in a document) so they can be reused.
//! - `files`: reading a picture or certificate the person chose, and
//!   writing a new certificate file in one step.
//!
//! the form engine itself is core/forms; everything here is glue that
//! can be tested with plain files.

pub mod files;
pub mod scripts;
pub mod signature_pictures;
pub mod signed;

pub use files::{data_url, picture_mime, read_chosen_file, write_chosen_bytes};
pub use scripts::has_calculations;
pub use signature_pictures::{PictureKind, SavedSignature, SignaturePictures};
pub use signed::{password_blocks_signing, sign_saved_document, SignError, PASSWORD_MESSAGE};

#[cfg(test)]
mod tests;
