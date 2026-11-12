//! the parts of the desktop shell that need no window.
//!
//! they live in their own crate so they can be unit tested without
//! starting the app: the recent files list, pinned folders, the image
//! addresses the ui uses, reading the command line, the edit session
//! (working copy, undo, save) and the reference edit, turning pages.

pub mod address;
pub mod pinned;
pub mod recents;
pub mod rotate;
pub mod session;
pub mod startup;

#[cfg(test)]
mod test_pdf;
