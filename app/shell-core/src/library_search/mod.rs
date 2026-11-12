//! Search All Documents: which files are searched, and the background
//! job that keeps their index up to date.
//!
//! - `settings`: the library folders, which pinned folders are searched,
//!   folders never to index, and whether indexing is paused
//! - `sources`: turning those choices into a list of files
//! - `job`: the indexing thread (incremental, throttled, pausable, safe
//!   to stop at any moment) and the handle the app searches through
//!
//! the index itself is letters-search; it lives in the app's data folder
//! and never leaves the computer.

pub mod job;
pub mod settings;
pub mod sources;

pub use job::{JobPhase, JobStatus, LibrarySearch, SourceLists};
pub use settings::{LibrarySettings, LibrarySettingsStore};
