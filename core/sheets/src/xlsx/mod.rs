//! excel workbooks (.xlsx, and .xlsm with its macros carried but never
//! run): spreadsheetml read into the shared model and written back.
//!
//! reading streams each worksheet; writing keeps every part and element
//! the model does not rebuild exactly as it was (see `model::carry`).

mod read_cells;
mod read_extras;
mod read_sections;
mod read_sheet;
mod read_sheet_parts;
mod read_workbook;
pub mod relationships;
// the conditional formats drawn on screen read colours the same way
pub(crate) mod style_parts;
mod styles;
mod write_cells;
mod write_extras;
mod write_sections;
mod write_sheet;
mod write_workbook;
mod write_workbook_part;

pub use read_workbook::read_xlsx;
pub use write_cells::number_text;
pub use write_workbook::write_xlsx;
