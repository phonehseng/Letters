//! the values behind checkboxes and radio buttons (export values), the
//! values a reset goes back to (/DV), and resetting the form.

use lopdf::{Dictionary, Document, Object};

use crate::appearance::refresh_field;
use crate::error::{FormError, FormResult};
use crate::fill::FieldValue;
use crate::objects::{check_document, name, text_string};
use crate::read::{kind_of, FieldKind};
use crate::tree::{collect_fields, find_field, on_states};

/// changes the value a checkbox or radio button stands for when chosen.
/// `button` counts a radio group's buttons from 0.
pub fn set_export_value(
    doc: &mut Document,
    field_name: &str,
    button: usize,
    value: &str,
) -> FormResult<()> {
    check_document(doc)?;
    let field = find_field(doc, field_name)?;
    if !matches!(
        kind_of(doc, &field),
        Some(FieldKind::Checkbox | FieldKind::RadioGroup)
    ) {
        return Err(FormError::WrongValueKind {
            field: field_name.to_owned(),
            hint: "Only checkboxes and radio buttons have export values.".into(),
        });
    }
    if value.trim().is_empty() || value == "Off" {
        return Err(FormError::BadName);
    }
    let widget = *field
        .widgets
        .get(button)
        .ok_or_else(|| FormError::UnknownField(format!("{field_name} #{}", button + 1)))?;
    // with /Opt the export values live there, by widget
    if let Some(Object::Array(options)) = field.inherited(doc, b"Opt").cloned() {
        let mut options = options;
        if button < options.len() {
            options[button] = text_string(value);
            doc.get_dictionary_mut(field.id)?
                .set("Opt", Object::Array(options));
            return Ok(());
        }
    }
    let old = on_states(doc, widget)
        .into_iter()
        .next()
        .unwrap_or_else(|| b"Yes".to_vec());
    let new = value.as_bytes().to_vec();
    let mut widget_dict = doc.get_dictionary(widget)?.clone();
    for key in [b"N".as_slice(), b"D"] {
        let appearances = widget_dict
            .get(b"AP")
            .ok()
            .and_then(|ap| ap.as_dict().ok())
            .cloned();
        if let Some(mut appearances) = appearances {
            if let Ok(Object::Dictionary(states)) = appearances.get(key).cloned() {
                let mut renamed = Dictionary::new();
                for (state, drawing) in states.iter() {
                    let state = if *state == old {
                        new.clone()
                    } else {
                        state.clone()
                    };
                    renamed.set(state, drawing.clone());
                }
                appearances.set(key.to_vec(), Object::Dictionary(renamed));
                widget_dict.set("AP", Object::Dictionary(appearances));
            }
        }
    }
    if widget_dict
        .get(b"AS")
        .ok()
        .and_then(|state| state.as_name().ok())
        == Some(old.as_slice())
    {
        widget_dict.set("AS", Object::Name(new.clone()));
    }
    *doc.get_dictionary_mut(widget)? = widget_dict;
    let chosen = field
        .inherited(doc, b"V")
        .and_then(|v| v.as_name().ok())
        .map(<[u8]>::to_vec);
    if chosen.as_deref() == Some(old.as_slice()) {
        doc.get_dictionary_mut(field.id)?
            .set("V", Object::Name(new));
    }
    Ok(())
}

/// sets (or with none, removes) the value a field goes back to when the
/// form is reset (/DV).
pub fn set_default_value(
    doc: &mut Document,
    field_name: &str,
    value: Option<&FieldValue>,
) -> FormResult<()> {
    check_document(doc)?;
    let field = find_field(doc, field_name)?;
    let kind =
        kind_of(doc, &field).ok_or_else(|| FormError::UnknownField(field_name.to_owned()))?;
    let object = match (value, kind) {
        (None, _) => None,
        (Some(FieldValue::Checked(false)), FieldKind::Checkbox | FieldKind::RadioGroup) => {
            Some(name("Off"))
        }
        (Some(FieldValue::Checked(true)), FieldKind::Checkbox) => Some(Object::Name(
            on_states(doc, field.widgets[0])
                .into_iter()
                .next()
                .unwrap_or_else(|| b"Yes".to_vec()),
        )),
        (Some(FieldValue::Text(text)), FieldKind::Checkbox | FieldKind::RadioGroup) => {
            let (_, state) = crate::read::state_for_export(doc, &field, text).ok_or_else(|| {
                FormError::NotAnOption {
                    field: field_name.to_owned(),
                    value: text.clone(),
                }
            })?;
            Some(Object::Name(state))
        }
        (Some(FieldValue::Text(text)), _) => Some(text_string(text)),
        (Some(FieldValue::Selected(items)), FieldKind::List | FieldKind::Dropdown)
            if items.len() > 1 =>
        {
            Some(Object::Array(
                items.iter().map(|item| text_string(item)).collect(),
            ))
        }
        (Some(FieldValue::Selected(items)), _) => {
            Some(text_string(items.first().map(String::as_str).unwrap_or("")))
        }
        (Some(_), _) => {
            return Err(FormError::WrongValueKind {
                field: field_name.to_owned(),
                hint: "That kind of default doesn’t fit this field.".into(),
            })
        }
    };
    let dict = doc.get_dictionary_mut(field.id)?;
    match object {
        Some(object) => dict.set("DV", object),
        None => {
            dict.remove(b"DV");
        }
    }
    Ok(())
}

/// puts fields back to their default values (or empty), the way a reset
/// button does. none resets every field. returns the names reset.
/// signature fields are never reset.
pub fn reset_form(doc: &mut Document, only: Option<&[String]>) -> FormResult<Vec<String>> {
    check_document(doc)?;
    let mut reset = Vec::new();
    for field in collect_fields(doc) {
        if let Some(only) = only {
            if !only.iter().any(|wanted| {
                field.name == *wanted || field.name.starts_with(&format!("{wanted}."))
            }) {
                continue;
            }
        }
        let Some(kind) = kind_of(doc, &field) else {
            continue;
        };
        if matches!(kind, FieldKind::Signature | FieldKind::Button) {
            continue;
        }
        let default = field.inherited(doc, b"DV").cloned();
        {
            let dict = doc.get_dictionary_mut(field.id)?;
            match (&default, kind) {
                (Some(value), _) => dict.set("V", value.clone()),
                (None, FieldKind::Checkbox | FieldKind::RadioGroup) => dict.set("V", name("Off")),
                (None, _) => {
                    dict.remove(b"V");
                }
            }
            dict.remove(b"I");
            dict.remove(b"RV");
        }
        refresh_field(doc, &field, false)?;
        reset.push(field.name.clone());
    }
    Ok(reset)
}
