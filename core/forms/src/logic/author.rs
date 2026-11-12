//! writing logic set up in Letters back into the file, as the standard
//! actions other pdf apps run: AFSimple_Calculate, AFNumber_Format and
//! friends, AFRange_Validate, and plain javascript for formulas (with the
//! formula itself kept in acrobat's "simplified field notation" comment).

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use super::engine::{dependencies_of, form_logic, recalculate};
use super::format::{FieldFormat, TIME_PATTERNS};
use super::recognize::field_actions;
use super::script::{parse_notation, parse_script, to_javascript, to_notation, Expr};
use super::{Calculation, FieldLogic, Validation};
use crate::add::ensure_form;
use crate::error::{FormError, FormResult};
use crate::objects::{
    check_document, name, push_to_array, references_in, remove_from_array, text_string,
};
use crate::tree::{collect_fields, find_field, flags, FieldNode};

/// the logic a field should have. it replaces what the field had: leave
/// something out and it is removed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldLogicChanges {
    #[serde(default)]
    pub calculation: Option<Calculation>,
    #[serde(default)]
    pub format: Option<FieldFormat>,
    /// at most one of range, condition or script; required and max length
    /// set the field's flags
    #[serde(default)]
    pub validations: Vec<Validation>,
}

/// sets a field's calculation, format and checks, writes them as standard
/// actions, keeps the calculation order (/CO) in step, works the form out
/// again and returns the logic as read back from the file.
pub fn set_field_logic(
    doc: &mut Document,
    field_name: &str,
    changes: &FieldLogicChanges,
) -> FormResult<FieldLogic> {
    check_document(doc)?;
    let field = find_field(doc, field_name)?;
    let names: Vec<String> = collect_fields(doc)
        .into_iter()
        .map(|node| node.name)
        .collect();
    let calculate = changes
        .calculation
        .as_ref()
        .map(|calculation| calculation_script(calculation, &names))
        .transpose()?;
    let (format_script, keystroke_script) = match &changes.format {
        Some(format) => format_scripts(format),
        None => (None, None),
    };
    let own_checks: Vec<&Validation> = changes
        .validations
        .iter()
        .filter(|validation| {
            !matches!(
                validation,
                Validation::Required | Validation::MaxLength { .. }
            )
        })
        .collect();
    if own_checks.len() > 1 {
        return Err(FormError::TooManyChecks);
    }
    let validate = own_checks
        .first()
        .map(|validation| validation_script(validation, &names))
        .transpose()?;

    // everything is checked; now the file changes
    clear_widget_actions(doc, &field)?;
    let mut actions = existing_actions(doc, field.id);
    for (key, script) in [
        ("C", calculate),
        ("F", format_script),
        ("K", keystroke_script),
        ("V", validate),
    ] {
        match script {
            Some(script) => actions.set(key, javascript_action(&script)),
            None => {
                actions.remove(key.as_bytes());
            }
        }
    }
    {
        let dict = doc.get_dictionary_mut(field.id)?;
        if actions.is_empty() {
            dict.remove(b"AA");
        } else {
            dict.set("AA", Object::Dictionary(actions));
        }
    }
    set_flags_and_length(doc, &field, &changes.validations)?;
    update_calculation_order(doc, field.id, changes.calculation.is_some())?;
    recalculate(doc)?;
    let logic = form_logic(doc)?;
    logic
        .fields
        .into_iter()
        .find(|logic| logic.field == field_name)
        .ok_or_else(|| FormError::UnknownField(field_name.to_owned()))
}

/// sets the order calculated fields are worked out in. fields not named
/// keep their place after the named ones.
pub fn set_calculation_order(doc: &mut Document, order: &[String]) -> FormResult<()> {
    check_document(doc)?;
    let form = ensure_form(doc)?;
    let mut ids: Vec<ObjectId> = Vec::new();
    for field_name in order {
        ids.push(find_field(doc, field_name)?.id);
    }
    let existing = references_in(doc, doc.get_dictionary(form)?, b"CO");
    for id in existing {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    doc.get_dictionary_mut(form)?.set(
        "CO",
        Object::Array(ids.into_iter().map(Object::Reference).collect()),
    );
    Ok(())
}

fn javascript_action(script: &str) -> Object {
    let mut action = Dictionary::new();
    action.set("S", name("JavaScript"));
    action.set("JS", text_string(script));
    Object::Dictionary(action)
}

fn quoted(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn check_names(expr_fields: &[String], names: &[String]) -> FormResult<()> {
    for field in expr_fields {
        if !names.contains(field) {
            return Err(FormError::UnknownField(field.clone()));
        }
    }
    Ok(())
}

fn calculation_script(calculation: &Calculation, names: &[String]) -> FormResult<String> {
    check_names(&dependencies_of_checked(calculation)?, names)?;
    Ok(match calculation {
        Calculation::Aggregate { function, fields } => {
            if fields.is_empty() {
                return Err(FormError::BadFormula("it adds up no fields".into()));
            }
            let list: Vec<String> = fields.iter().map(|field| quoted(field)).collect();
            format!(
                "AFSimple_Calculate({}, new Array({}));",
                quoted(function.code()),
                list.join(", ")
            )
        }
        Calculation::Formula { formula } => {
            let expr = parse_notation(formula).map_err(|error| FormError::BadFormula(error.0))?;
            // the comment is how acrobat keeps a simplified formula
            // editable; the line after it is what actually runs
            format!(
                "/** BVCALC {} EVCALC **/\nevent.value = {};",
                to_notation(&expr).replace("*/", "* /"),
                to_javascript(&expr)
            )
        }
        Calculation::Script { source } => {
            parse_script(source).map_err(|error| FormError::BadFormula(error.0))?;
            source.clone()
        }
    })
}

fn dependencies_of_checked(calculation: &Calculation) -> FormResult<Vec<String>> {
    match calculation {
        Calculation::Formula { formula } => {
            parse_notation(formula).map_err(|error| FormError::BadFormula(error.0))?;
        }
        Calculation::Script { source } => {
            parse_script(source).map_err(|error| FormError::BadFormula(error.0))?;
        }
        Calculation::Aggregate { .. } => {}
    }
    Ok(dependencies_of(calculation))
}

/// the format (/F) and keystroke (/K) scripts for a format.
fn format_scripts(format: &FieldFormat) -> (Option<String>, Option<String>) {
    let both = |function: &str, arguments: String| {
        (
            Some(format!("{function}_Format({arguments});")),
            Some(format!("{function}_Keystroke({arguments});")),
        )
    };
    match format {
        FieldFormat::Number {
            decimals,
            separator,
            negative,
            currency,
            currency_before,
        } => both(
            "AFNumber",
            format!(
                "{decimals}, {}, {}, 0, {}, {currency_before}",
                separator.code(),
                negative.code(),
                quoted(currency)
            ),
        ),
        FieldFormat::Percent {
            decimals,
            separator,
        } => both("AFPercent", format!("{decimals}, {}", separator.code())),
        FieldFormat::Date { pattern } => (
            Some(crate::date::format_script(pattern)),
            Some(crate::date::keystroke_script(pattern)),
        ),
        FieldFormat::Time { pattern } => {
            match TIME_PATTERNS.iter().position(|known| known == pattern) {
                Some(index) => both("AFTime", index.to_string()),
                None => (
                    Some(format!("AFTime_FormatEx({});", quoted(pattern))),
                    Some(format!("AFTime_KeystrokeEx({});", quoted(pattern))),
                ),
            }
        }
        FieldFormat::Special { kind } => both("AFSpecial", kind.code().to_string()),
        FieldFormat::Mask { mask } => (
            None,
            Some(format!("AFSpecial_KeystrokeEx({});", quoted(mask))),
        ),
    }
}

fn validation_script(validation: &Validation, names: &[String]) -> FormResult<String> {
    let number = super::eval::number_to_text;
    Ok(match validation {
        Validation::Range { min, max } => format!(
            "AFRange_Validate({}, {}, {}, {});",
            min.is_some(),
            number(min.unwrap_or(0.0)),
            max.is_some(),
            number(max.unwrap_or(0.0))
        ),
        Validation::Condition { condition, message } => {
            let expr = parse_notation(condition).map_err(|error| FormError::BadFormula(error.0))?;
            let mut read = Vec::new();
            super::script::fields_in(&expr, &mut read);
            check_names(&read, names)?;
            let alert = message
                .as_deref()
                .filter(|message| !message.trim().is_empty())
                .map(|message| {
                    format!(
                        "app.alert({}); ",
                        to_javascript(&Expr::Text(message.to_owned()))
                    )
                })
                .unwrap_or_default();
            format!(
                "if (!({})) {{ {alert}event.rc = false; }}",
                to_javascript(&expr)
            )
        }
        Validation::Script { source } => {
            parse_script(source).map_err(|error| FormError::BadFormula(error.0))?;
            source.clone()
        }
        Validation::Required | Validation::MaxLength { .. } => unreachable!("filtered out"),
    })
}

/// a copy of the field's own /AA, so actions for other events are kept.
fn existing_actions(doc: &Document, field: ObjectId) -> Dictionary {
    doc.get_dictionary(field)
        .ok()
        .and_then(|dict| crate::objects::get_dict(doc, dict, b"AA"))
        .cloned()
        .unwrap_or_default()
}

/// some files keep these actions on a separate widget. they move to the
/// field, so there is only ever one copy.
fn clear_widget_actions(doc: &mut Document, field: &FieldNode) -> FormResult<()> {
    let Some(_) = field_actions(doc, field) else {
        return Ok(());
    };
    for widget in field.widgets.clone() {
        if widget == field.id {
            continue;
        }
        let actions = doc
            .get_dictionary(widget)
            .ok()
            .and_then(|dict| crate::objects::get_dict(doc, dict, b"AA"))
            .cloned();
        if let Some(mut actions) = actions {
            for key in [b"C".as_slice(), b"F", b"K", b"V"] {
                actions.remove(key);
            }
            let dict = doc.get_dictionary_mut(widget)?;
            if actions.is_empty() {
                dict.remove(b"AA");
            } else {
                dict.set("AA", Object::Dictionary(actions));
            }
        }
    }
    Ok(())
}

fn set_flags_and_length(
    doc: &mut Document,
    field: &FieldNode,
    validations: &[Validation],
) -> FormResult<()> {
    let required = validations.contains(&Validation::Required);
    let max = validations.iter().find_map(|validation| match validation {
        Validation::MaxLength { max } => Some(*max),
        _ => None,
    });
    let current = field.flags(doc);
    let wanted = if required {
        current | flags::REQUIRED
    } else {
        current & !flags::REQUIRED
    };
    let dict = doc.get_dictionary_mut(field.id)?;
    if wanted != current || dict.has(b"Ff") {
        dict.set("Ff", Object::Integer(wanted as i64));
    }
    match max {
        Some(max) if max > 0 => dict.set("MaxLen", Object::Integer(max as i64)),
        _ => {
            dict.remove(b"MaxLen");
        }
    }
    Ok(())
}

fn update_calculation_order(
    doc: &mut Document,
    field: ObjectId,
    calculated: bool,
) -> FormResult<()> {
    let form = ensure_form(doc)?;
    let listed = references_in(doc, doc.get_dictionary(form)?, b"CO").contains(&field);
    if calculated && !listed {
        push_to_array(doc, form, b"CO", Object::Reference(field))?;
    } else if !calculated && listed {
        remove_from_array(doc, form, b"CO", field)?;
    }
    Ok(())
}
