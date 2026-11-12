//! the parts of the desktop shell that need no window.
//!
//! they live in their own crate so they can be unit tested without
//! starting the app: the recent files list, pinned folders, the image
//! addresses the ui uses, reading the command line, the edit session
//! (working copy, undo, save), the files page tools make beside the
//! document (extract, split, merge), the person's settings (the author
//! name on comments) and reading a picture's size.

pub mod address;
pub mod outputs;
pub mod picture;
pub mod pinned;
pub mod recents;
pub mod session;
pub mod settings;
pub mod startup;
// word-processor mode (.docx and .odt), see writer/mod.rs
pub mod writer;
// save to google drive (desktop folder or direct upload), see drive/mod.rs
pub mod drive;
// forms and signatures (Fill & Sign), see forms/mod.rs
pub mod forms;
// redaction, hidden information and passwords (Protect), see protect/mod.rs
pub mod protect;
// the font library's folders, see fonts/mod.rs
pub mod fonts;

#[cfg(test)]
mod annotation_tools_tests;
#[cfg(test)]
mod page_tools_tests;
#[cfg(test)]
mod test_pdf;
