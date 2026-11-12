//! tools for building bigger forms: tab order, groups of fields with a
//! shared name, fields shown on several pages, export values, default
//! values, reset and print buttons, and how a field looks.
//!
//! a submit button is deliberately not offered: sending answers needs a
//! server or a mail program, and Letters keeps everything on this computer.
//! people send the saved file instead.

mod buttons;
mod groups;
mod look;
mod tab_order;
mod values;

pub use buttons::{add_button, ButtonAction, NewButton};
pub use groups::{duplicate_across_pages, group_fields};
pub use look::{set_field_appearance, BorderStyle, Color, FieldAppearance, FieldFont};
pub use tab_order::{set_tab_order, tab_order, TabOrder};
pub use values::{reset_form, set_default_value, set_export_value};

/// the sentence the ui shows where a submit button would be.
pub const NO_SUBMIT_NOTE: &str = "Letters doesn’t add submit buttons: sending answers needs a server or a mail program, and Letters keeps everything on this computer. Save the filled form and send the file instead.";
