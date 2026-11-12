//! number formats: excel format codes parsed and applied to values, the
//! built-in codes xlsx files refer to by number, excel's "General", and
//! opendocument data styles converted to and from excel codes.

pub mod builtin;
mod dates;
mod fraction;
pub mod general;
pub mod odf;
pub mod parse;
pub mod render;
mod scientific;

pub use render::{format_number, format_value, Formatted, Locale};
