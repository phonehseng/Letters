//! reads, renders and writes presentations (pptx and odp) through one
//! shared model.
//!
//! - `model` is the deck every format is read into
//! - `pptx` and `odp` read and write the two file formats, keeping what
//!   the model does not understand (`preserve`)
//! - `inherit` and `color` work out effective values (placeholder
//!   positions, text styles, theme colours) for the interface and renderer
//! - `geometry`, `layout` and `render` draw slides to pdf without any
//!   office application

pub mod base64;
pub mod blank;
pub mod color;
pub mod edit;
mod error;
pub mod export;
pub mod fonts;
pub mod geometry;
pub mod inherit;
pub mod layout;
pub mod model;
pub mod odp;
#[cfg(feature = "office")]
pub mod office_bridge;
pub mod outline;
pub mod package;
pub mod pptx;
pub mod preserve;
pub mod render;
pub mod shadow;
pub mod xml;

pub use error::SlidesError;
pub use model::Presentation;
pub use odp::{read_odp, write_odp};
pub use pptx::{read_pptx, write_pptx};
