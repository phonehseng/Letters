//! word documents (wordprocessingml, ecma-376), read into and written
//! from the shared model.

pub(crate) mod drawingml;
pub(crate) mod names;
pub(crate) mod order;
mod read;
pub(crate) mod rels;
mod write;

pub use read::read_docx;
pub use write::write_docx;

// small helpers the odt writer shares
pub(crate) use order::first_tag_name;
pub(crate) use write::{for_each_inline, foreign_text};
