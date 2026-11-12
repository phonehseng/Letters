//! filling in a field: checking the answer, storing it and drawing it.

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

use crate::appearance::refresh_field;
use crate::date::{format_date, parse_date, DEFAULT_PATTERN};
use crate::error::{FormError, FormResult};
use crate::objects::{check_document, text_string};
use crate::read::{date_pattern, export_value, kind_of, state_for_export, FieldKind};
use crate::text::{
    encode_win_ansi, encode_win_ansi_lines, line_breaks_to_spaces, normalize_line_breaks,
};
use crate::tree::{find_field, flags, options_of, FieldNode};

/// an answer for a field.
///
/// in json: {"kind": "text", "value": "Ada"}, {"kind": "checked", "value": true}
/// or {"kind": "selected", "value": ["Maths", "Art"]}.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum FieldValue {
    /// for text and date fields; also a radio group's or dropdown's choice
    Text(String),
    /// for checkboxes. `false` also clears a radio group
    Checked(bool),
    /// for lists (several when multi-select), dropdowns and radio groups
    Selected(Vec<String>),
}

/// fills one field and redraws it.
///
/// the value is checked first: a read-only field, a required field left
/// empty, text longer than the field allows, or a choice that isn't
/// offered is refused with a message, and nothing is changed.
///
/// line breaks (\n, \r\n or \r) are kept in multiline fields, stored as \n
/// in /V and drawn as new lines. one line fields can't show them, so there
/// each line break becomes a space.
pub fn set_value(doc: &mut Document, name: &str, value: &FieldValue) -> FormResult<()> {
    check_document(doc)?;
    let field = find_field(doc, name)?;
    let Some(kind) = kind_of(doc, &field) else {
        return Err(FormError::UnknownField(name.to_owned()));
    };
    if field.has_flag(doc, flags::READ_ONLY) {
        return Err(FormError::ReadOnly(name.to_owned()));
    }
    match kind {
        FieldKind::Text | FieldKind::MultilineText => fill_text(doc, &field, kind, value),
        FieldKind::Date => fill_date(doc, &field, value),
        FieldKind::Checkbox => fill_checkbox(doc, &field, value),
        FieldKind::RadioGroup => fill_radio(doc, &field, value),
        FieldKind::Dropdown | FieldKind::List => fill_choice(doc, &field, kind, value),
        FieldKind::Signature => Err(wrong_kind(&field, "Use Sign to put a signature here.")),
        FieldKind::Button => Err(wrong_kind(&field, "It is a button and holds no answer.")),
    }
}

fn wrong_kind(field: &FieldNode, hint: &str) -> FormError {
    FormError::WrongValueKind {
        field: field.name.clone(),
        hint: hint.to_owned(),
    }
}

/// the text inside an answer, or an error naming what the field wants.
fn text_of(field: &FieldNode, value: &FieldValue) -> FormResult<String> {
    match value {
        FieldValue::Text(text) => Ok(text.clone()),
        FieldValue::Selected(items) if items.len() <= 1 => {
            Ok(items.first().cloned().unwrap_or_default())
        }
        _ => Err(wrong_kind(field, "It takes text.")),
    }
}

fn check_required(doc: &Document, field: &FieldNode, empty: bool) -> FormResult<()> {
    if empty && field.has_flag(doc, flags::REQUIRED) {
        return Err(FormError::Required {
            field: field.name.clone(),
        });
    }
    Ok(())
}

fn fill_text(
    doc: &mut Document,
    field: &FieldNode,
    kind: FieldKind,
    value: &FieldValue,
) -> FormResult<()> {
    let text = text_of(field, value)?;
    let text = if kind == FieldKind::Text {
        // a one line field can't show line breaks, so they become spaces
        line_breaks_to_spaces(&text)
    } else {
        // multiline fields keep them, all written the same way (\n)
        normalize_line_breaks(&text)
    };
    check_required(doc, field, text.trim().is_empty())?;
    if let Some(max) = field.max_length(doc) {
        if text.chars().count() > max as usize {
            return Err(FormError::TooLong {
                field: field.name.clone(),
                max,
            });
        }
    }
    store_text(doc, field, &text)
}

fn store_text(doc: &mut Document, field: &FieldNode, text: &str) -> FormResult<()> {
    // line breaks are fine here: only multiline fields still have them
    encode_win_ansi_lines(text).map_err(FormError::UnsupportedCharacters)?;
    let dict = doc.get_dictionary_mut(field.id)?;
    dict.set("V", text_string(text));
    // a rich text copy of the old value would contradict the new one
    dict.remove(b"RV");
    refresh_field(doc, field, false)
}

fn fill_date(doc: &mut Document, field: &FieldNode, value: &FieldValue) -> FormResult<()> {
    let input = text_of(field, value)?;
    check_required(doc, field, input.trim().is_empty())?;
    if input.trim().is_empty() {
        return store_text(doc, field, "");
    }
    let pattern = date_pattern(doc, field).unwrap_or_else(|| DEFAULT_PATTERN.to_owned());
    let date = parse_date(&input, &pattern)?;
    store_text(doc, field, &format_date(date, &pattern))
}

fn fill_checkbox(doc: &mut Document, field: &FieldNode, value: &FieldValue) -> FormResult<()> {
    let chosen_export = match value {
        FieldValue::Checked(true) => Some(export_value(doc, field, field.widgets[0], 0)),
        FieldValue::Checked(false) => None,
        // a checkbox can also be set by naming its on value
        other => Some(text_or_first(other)).filter(|text| !text.is_empty() && text != "Off"),
    };
    set_button_state(doc, field, chosen_export)
}

fn text_or_first(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(text) => text.clone(),
        FieldValue::Selected(items) => items.first().cloned().unwrap_or_default(),
        FieldValue::Checked(_) => String::new(),
    }
}

fn fill_radio(doc: &mut Document, field: &FieldNode, value: &FieldValue) -> FormResult<()> {
    let chosen = match value {
        FieldValue::Checked(false) => None,
        FieldValue::Checked(true) => return Err(wrong_kind(field, "Choose one of its options.")),
        FieldValue::Selected(items) if items.len() > 1 => {
            return Err(wrong_kind(field, "Only one option can be chosen."))
        }
        _ => Some(text_or_first(value)).filter(|text| !text.is_empty()),
    };
    set_button_state(doc, field, chosen)
}

/// sets /V of a checkbox or radio group to the state that stands for
/// `export` (or Off) and redraws the widgets so /AS follows.
fn set_button_state(
    doc: &mut Document,
    field: &FieldNode,
    export: Option<String>,
) -> FormResult<()> {
    check_required(doc, field, export.is_none())?;
    let state = match &export {
        None => b"Off".to_vec(),
        Some(export) => match state_for_export(doc, field, export) {
            Some((_, state)) => state,
            None => {
                return Err(FormError::NotAnOption {
                    field: field.name.clone(),
                    value: export.clone(),
                })
            }
        },
    };
    doc.get_dictionary_mut(field.id)?
        .set("V", Object::Name(state));
    refresh_field(doc, field, false)
}

fn fill_choice(
    doc: &mut Document,
    field: &FieldNode,
    kind: FieldKind,
    value: &FieldValue,
) -> FormResult<()> {
    let chosen: Vec<String> = match value {
        FieldValue::Text(text) if text.is_empty() => Vec::new(),
        FieldValue::Text(text) => vec![text.clone()],
        FieldValue::Selected(items) => items.clone(),
        FieldValue::Checked(_) => return Err(wrong_kind(field, "Choose one of its options.")),
    };
    let multi = kind == FieldKind::List && field.has_flag(doc, flags::MULTI_SELECT);
    if chosen.len() > 1 && !multi {
        return Err(wrong_kind(field, "Only one option can be chosen."));
    }
    check_required(doc, field, chosen.is_empty())?;

    let options = options_of(doc, field);
    let editable = kind == FieldKind::Dropdown && field.has_flag(doc, flags::EDIT);
    let mut indices = Vec::new();
    for item in &chosen {
        // people may pick by the label they see as well as by the stored value
        let found = options
            .iter()
            .position(|(export, label)| export == item || label == item);
        match found {
            Some(index) => indices.push(index),
            None if editable => {}
            None => {
                return Err(FormError::NotAnOption {
                    field: field.name.clone(),
                    value: item.clone(),
                })
            }
        }
    }
    let stored: Vec<String> = chosen
        .iter()
        .map(|item| {
            options
                .iter()
                .find(|(export, label)| export == item || label == item)
                .map(|(export, _)| export.clone())
                .unwrap_or_else(|| item.clone())
        })
        .collect();
    for item in &stored {
        encode_win_ansi(item).map_err(FormError::UnsupportedCharacters)?;
    }
    indices.sort_unstable();

    let dict = doc.get_dictionary_mut(field.id)?;
    match stored.len() {
        0 => {
            dict.remove(b"V");
        }
        1 => dict.set("V", text_string(&stored[0])),
        _ => dict.set(
            "V",
            Object::Array(stored.iter().map(|item| text_string(item)).collect()),
        ),
    }
    if indices.is_empty() {
        dict.remove(b"I");
    } else {
        dict.set(
            "I",
            Object::Array(
                indices
                    .iter()
                    .map(|index| Object::Integer(*index as i64))
                    .collect(),
            ),
        );
    }
    refresh_field(doc, field, false)
}
