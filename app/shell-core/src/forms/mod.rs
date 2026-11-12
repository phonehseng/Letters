//! the parts of Fill & Sign that need no window.
//!
//! - `signed`: signing the saved file's exact bytes through the edit
//!   session, and saving every later change as an incremental update, so
//!   a signature is never rewritten by a later save.
//! - `data_files`: form data files (fdf, xfdf, csv, json): which kind a
//!   file is, reading one, and merging many filled copies into one table.
//! - `signature_pictures`: the pictures of a person's signature, kept in
//!   the app's data folder (never in a document) so they can be reused.
//! - `files`: reading a picture or certificate the person chose, and
//!   writing a new certificate file in one step.
//!
//! the form engine itself is core/forms; everything here is glue that
//! can be tested with plain files.

pub mod data_files;
pub mod files;
pub mod signature_pictures;
pub mod signed;

pub use data_files::{data_format_for, merge_filled_files, read_data_file, unknown_kind};
pub use files::{data_url, picture_mime, read_chosen_file, write_chosen_bytes};
pub use signature_pictures::{PictureKind, SavedSignature, SignaturePictures};
pub use signed::{password_blocks_signing, sign_saved_document, SignError, PASSWORD_MESSAGE};

#[cfg(test)]
mod audit_tests;
#[cfg(test)]
mod incremental_tests;
#[cfg(test)]
mod tests;
