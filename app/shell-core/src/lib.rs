//! the parts of the desktop shell that need no window.
//!
//! they live in their own crate so they can be unit tested without
//! starting the app: the recent files list, pinned folders, the image
//! addresses the ui uses, and reading the command line.

pub mod address;
pub mod pinned;
pub mod recents;
pub mod startup;
