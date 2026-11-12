//! moving, copying, renaming and recycling documents without leaving
//! Letters, so that nothing is ever lost:
//!
//! - an existing file is never replaced unless the person chose Replace;
//!   Keep Both adds " (2)" to the new one's name
//! - a move to another drive copies first, checks the copy byte for byte,
//!   and only then deletes the original
//! - a file another program holds gets a plain message, and nothing moves
//! - "Move to Recycle Bin" uses the system's recycle bin (the Trash on a
//!   mac); nothing here ever deletes a person's file for good
//! - every move, copy and rename can be undone (see `undo`)
//!
//! the callers keep the rest of the app in step with each change (Recents,
//! the search index, bookmarks, versions); `changes` lists what moved where.

pub mod files;
pub mod trash;

#[cfg(test)]
mod follow_tests;

pub use files::{
    copy_into, move_into, rename, undo, unique_name, Conflict, Done, OrganiseError, Outcome,
};
