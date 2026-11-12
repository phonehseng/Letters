//! reads, calculates and writes spreadsheets (xlsx and ods) through one
//! shared model. see README.md for the support tables.

pub mod address;
pub mod analysis;
pub mod calc;
pub mod conditional;
pub mod csv;
pub mod dates;
pub mod edit;
pub mod error;
pub mod export;
pub mod formula;
pub mod help;
pub mod model;
pub mod numfmt;
pub mod ods;
pub mod package;
pub mod print;
pub mod view;
pub mod xlsx;
pub mod xml;

pub use address::{CellRef, Range};
pub use error::SheetError;
pub use model::{Cell, ErrorValue, Sheet, Value, Workbook};
