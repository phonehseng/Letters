//! the parts of the desktop shell that need no window.
//!
//! they live in their own crate so they can be unit tested without
//! starting the app: the recent files list, pinned folders, the image
//! addresses the ui uses, reading the command line, the edit session
//! (working copy, undo, save), the files page tools make beside the
//! document (extract, split, merge), the person's settings (the author
//! name on comments) and reading a picture's size.

pub mod address;
// what a file looked like when a window last read or wrote it (see disk_stamp.rs)
pub mod disk_stamp;
pub mod outputs;
pub mod picture;
pub mod pinned;
pub mod recents;
pub mod session;
pub mod settings;
pub mod startup;
// which file each window shows, so a file opens once (see window_files.rs)
pub mod window_files;
// in-app updates: when a quiet check is due, and short release notes (see updates.rs)
pub mod updates;
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
// comparing two versions of a document, see compare/mod.rs
pub mod compare;
// the accessibility checker and fix wizard, see a11y/mod.rs
pub mod a11y;
// direct editing of page text and images (the Edit tab), see edit/mod.rs
pub mod edit;
// text recognition for scans and photos (the Scan tab), see ocr/mod.rs
pub mod ocr;
// conversion and export (the Export dialog), see export/mod.rs
pub mod export;
// the optional ai tools, off until turned on in Settings, see ai/mod.rs
pub mod ai;
// the Settings window's stored choices, see settings_store/mod.rs
pub mod settings_store;
// which screen each kind of file opens in, see associations/mod.rs
pub mod associations;
// search all documents (the index's job and settings), see library_search/mod.rs
pub mod library_search;
// reading and writing long documents: positions, bookmarks, labels, versions
pub mod reading;
// moving, copying, renaming and recycling documents, see organise/mod.rs
pub mod organise;
// code files: the code editor's files, checks and exports, see code/mod.rs
pub mod code;
// presentations: the slide editor's decks, pictures and saves, see slides/mod.rs
pub mod slides;
// spreadsheets: open workbooks, their edits, undo and saving, see sheets/mod.rs
pub mod sheets;
// forms round two: pdf/a, preflight and optimising copies, see standards/mod.rs
pub mod standards;

#[cfg(test)]
mod annotation_tools_tests;
#[cfg(test)]
mod page_tools_tests;
#[cfg(test)]
mod test_pdf;
