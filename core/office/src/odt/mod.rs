//! opendocument text files (odf 1.3), read into and written from the
//! shared model.

mod props;
mod read;
mod write;

pub use read::read_odt;
pub use write::write_odt;
