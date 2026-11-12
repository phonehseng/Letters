//! opendocument presentations (.odp) read into the model and written
//! back, keeping what the model does not read.

mod images;
pub mod read;
mod read_geometry;
mod read_paint;
mod read_shapes;
mod read_tables;
mod read_text;
mod styles;
pub mod units;
pub mod write;
mod write_shapes;
mod write_styles;
mod write_text;

pub use read::read_odp;
pub use write::write_odp;
