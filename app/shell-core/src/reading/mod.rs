//! reading and writing long documents.
//!
//! - `store`: what Letters remembers per document on this computer: where
//!   the reader stopped, their bookmarks, their reading speed, and the
//!   writer's word goals and daily writing log. none of it changes the file
//! - `labels`: a pdf's printed page numbers ("iv", "A-3"), for Go To
//! - `outline`: writing bookmarks into a pdf as real outline entries,
//!   when the reader chooses "Save Bookmarks into the PDF"
//! - `versions`: named copies of a document the writer can go back to

pub mod labels;
pub mod outline;
pub mod store;
pub mod versions;

pub use store::{Bookmark, DocumentReading, ReadingStore, WritingLog};
pub use versions::{Version, VersionStore};
