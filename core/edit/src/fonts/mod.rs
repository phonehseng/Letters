//! everything about fonts: encodings, character maps, the standard
//! fonts, reading embedded programs, and adding new fonts to a file.

pub(crate) mod append;
pub(crate) mod cmap;
pub(crate) mod donor;
pub(crate) mod embed;
pub(crate) mod encodings;
pub(crate) mod extend;
pub(crate) mod metrics;
pub(crate) mod pdf_font;
pub(crate) mod standard;
pub(crate) mod system;
pub(crate) mod truetype;
pub(crate) mod type1;
