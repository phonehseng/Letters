//! the parts of the desktop shell that need no window.
//!
//! they live in their own crate so they can be unit tested without
//! starting the app: the recent files list, pinned folders, the image
//! addresses the ui uses, reading the command line, the edit session
//! (working copy, undo, save) and the files page tools make beside the
//! document (extract, split, merge).

pub mod address;
pub mod outputs;
pub mod pinned;
pub mod recents;
pub mod session;
pub mod startup;

#[cfg(test)]
mod page_tools_tests;
#[cfg(test)]
mod test_pdf;
