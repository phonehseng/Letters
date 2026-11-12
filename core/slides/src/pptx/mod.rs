//! pptx (presentationml with drawingml) read into the model and written
//! back, keeping everything the model does not read.

pub mod dml;
pub mod objects;
pub mod read;
pub mod rels;
pub mod text;
pub mod theme;
pub mod write;

pub use read::read_pptx;
pub use write::write_pptx;
