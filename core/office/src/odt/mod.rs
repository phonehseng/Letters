//! opendocument text files (odf 1.3), read into and written from the
//! shared model.

mod graphics;
mod paths;
mod props;
mod read;
mod table_props;
mod write;

pub use read::read_odt;
pub use write::write_odt;
