//! the shell's side of the spreadsheet screen (.xlsx, .xlsm, .ods, .csv).
//!
//! the engine is core/sheets; this module keeps each open workbook with
//! its file, its calculation engine and its undo history, and answers the
//! grid's questions in small pieces.
//!
//! - `files`: reading and safely writing each format, what csv and ods leave out
//! - `document`: one workbook, its edits, undo and redo, recalculation
//! - `history`: edits (the engine's operations and size changes) and undo steps
//! - `layout`: column widths, row heights and hiding
//! - `entry`: the formula-typing helpers, with the cursor counted as javascript does
//! - `formats`: number formats, bold, colours and style presets on a selection
//! - `navigate`: where ctrl with the arrows lands, and ctrl a's block of data
//! - `paste`: text pasted from other apps, typed into cells
//! - `look`: the tabs, a sheet's sizes, styles and the cells on screen
//! - `sessions`: every open workbook, opening, new workbooks, saving, recovery
//! - `recovery`: recovery copies offered after a crash
//! - `tips`: tips a person dismissed, kept between runs

pub mod document;
pub mod entry;
pub mod files;
pub mod formats;
pub mod history;
pub mod layout;
pub mod look;
pub mod navigate;
pub mod paste;
pub mod recovery;
pub mod sessions;
pub mod tips;
