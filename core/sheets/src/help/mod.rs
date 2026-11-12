//! what makes a spreadsheet easy for someone new to it, as data and
//! pure functions for the grid ui: the function catalogue and
//! autocomplete, argument hints and formula entry helpers, plain-word
//! explanations of formulas and of errors (with fixes), tips offered at
//! the right moment, keyboard shortcut sets, friendly formats and starter
//! workbooks, and compatibility notes between the spreadsheet families.

pub mod catalogue;
pub mod compat;
pub mod entry;
pub mod errors;
pub mod explain;
pub mod flash_fill;
pub mod presets;
pub mod search;
pub mod shortcuts;
pub mod suggest;
