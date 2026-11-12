//! evaluating the model: checking answers, working out calculated fields
//! in the form's calculation order, and drawing formatted values.

use std::collections::{HashMap, HashSet};

use lopdf::Document;
use serde::{Deserialize, Serialize};

use super::eval::{evaluate, run, FieldReader, RunError, Value};
use super::format::{display, read_input, FieldFormat};
use super::recognize::{calculation_order_ids, read_field_logic};
use super::script::{as_numbers, fields_in, fields_in_statements, parse_notation, parse_script};
use super::{
    Aggregate, Calculation, CalculationIssue, FieldLogic, FormLogic, Validation, CUSTOM_SCRIPT_NOTE,
};
use crate::appearance::{draw_shown_text, stored_text};
use crate::error::{FormError, FormResult};
use crate::fill::{set_value, FieldValue};
use crate::objects::{check_document, text_string};
use crate::read::{fields, kind_of, FieldKind};
use crate::tree::{collect_fields, find_field, FieldNode};

/// a field whose value changed, with what it now shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedField {
    pub name: String,
    /// the raw value stored in the file
    pub value: String,
    /// the value as the field shows it, formatted
    pub shown: String,
}

/// what filling a field (or recalculating) did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillReport {
    /// the filled field first, then calculated fields that changed, in order
    pub changed: Vec<ChangedField>,
    pub issues: Vec<CalculationIssue>,
    pub custom_script_note: Option<String>,
}

/// reads the form's calculations, formats and checks. changes nothing.
pub fn form_logic(doc: &Document) -> FormResult<FormLogic> {
    check_document(doc)?;
    let nodes = collect_fields(doc);
    let names: Vec<String> = nodes.iter().map(|node| node.name.clone()).collect();
    let logic: Vec<FieldLogic> = nodes
        .iter()
        .map(|node| read_field_logic(doc, node, &names))
        .collect();
    let order = calculation_order(doc, &nodes, &logic);
    let mut issues = Vec::new();
    let dependencies: HashMap<String, Vec<String>> = order
        .iter()
        .filter_map(|name| {
            let field = logic.iter().find(|field| &field.field == name)?;
            Some((name.clone(), dependencies_of(field.calculation.as_ref()?)))
        })
        .collect();
    for name in &order {
        for read in dependencies.get(name).into_iter().flatten() {
            if !names.contains(read) {
                issues.push(CalculationIssue::UnknownField {
                    field: name.clone(),
                    missing: read.clone(),
                    message: format!(
                        "The calculation of “{name}” uses a field called “{read}” that this form doesn’t have."
                    ),
                });
            }
        }
    }
    let cycles = find_cycles(&order, &dependencies);
    for cycle in &cycles {
        issues.push(CalculationIssue::Cycle {
            fields: cycle.clone(),
            message: format!(
                "These fields are calculated from each other in a circle, so Letters doesn’t calculate them: {}.",
                cycle.join(", ")
            ),
        });
    }
    let in_cycle: HashSet<&String> = cycles.iter().flatten().collect();
    for (position, name) in order.iter().enumerate() {
        if in_cycle.contains(name) {
            continue;
        }
        for read in dependencies.get(name).into_iter().flatten() {
            let later = order[position + 1..].contains(read);
            if later && !in_cycle.contains(read) {
                issues.push(CalculationIssue::OutOfOrder {
                    field: name.clone(),
                    reads: read.clone(),
                    message: format!(
                        "“{name}” is calculated before “{read}”, which it uses, so it may show an old value. Change the calculation order to fix this."
                    ),
                });
            }
        }
    }
    let custom: Vec<String> = logic
        .iter()
        .filter(|field| !field.unsupported.is_empty())
        .map(|field| field.field.clone())
        .collect();
    Ok(FormLogic {
        custom_script_note: custom_note(&custom),
        fields_with_custom_scripts: custom,
        fields: logic,
        calculation_order: order,
        issues,
    })
}

fn custom_note(fields: &[String]) -> Option<String> {
    (!fields.is_empty()).then(|| format!("{CUSTOM_SCRIPT_NOTE}: {}.", fields.join(", ")))
}

/// calculated fields in /CO order, then any calculated field /CO forgot.
fn calculation_order(doc: &Document, nodes: &[FieldNode], logic: &[FieldLogic]) -> Vec<String> {
    let calculated = |name: &str| {
        logic
            .iter()
            .any(|field| field.field == name && field.calculation.is_some())
    };
    let mut order: Vec<String> = Vec::new();
    for id in calculation_order_ids(doc) {
        let found = nodes
            .iter()
            .find(|node| node.id == id || node.lineage.contains(&id) && node.widgets.contains(&id));
        if let Some(node) = found {
            if calculated(&node.name) && !order.contains(&node.name) {
                order.push(node.name.clone());
            }
        }
    }
    for field in logic {
        if field.calculation.is_some() && !order.contains(&field.field) {
            order.push(field.field.clone());
        }
    }
    order
}

/// the fields a calculation reads.
pub(crate) fn dependencies_of(calculation: &Calculation) -> Vec<String> {
    let mut out = Vec::new();
    match calculation {
        Calculation::Aggregate { fields, .. } => out.extend(fields.iter().cloned()),
        Calculation::Formula { formula } => {
            if let Ok(expr) = parse_notation(formula) {
                fields_in(&expr, &mut out);
            }
        }
        Calculation::Script { source } => {
            if let Ok(statements) = parse_script(source) {
                fields_in_statements(&statements, &mut out);
            }
        }
    }
    out
}

/// groups of calculated fields that depend on each other in a circle
/// (tarjan's strongly connected components), each in calculation order.
fn find_cycles(order: &[String], dependencies: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    struct Search<'a> {
        dependencies: &'a HashMap<String, Vec<String>>,
        index: HashMap<&'a str, usize>,
        low: HashMap<&'a str, usize>,
        stack: Vec<&'a str>,
        on_stack: HashSet<&'a str>,
        counter: usize,
        found: Vec<Vec<String>>,
    }
    impl<'a> Search<'a> {
        fn visit(&mut self, name: &'a str) {
            self.index.insert(name, self.counter);
            self.low.insert(name, self.counter);
            self.counter += 1;
            self.stack.push(name);
            self.on_stack.insert(name);
            let next: Vec<&'a str> = self
                .dependencies
                .get(name)
                .map(|reads| reads.iter().map(String::as_str).collect())
                .unwrap_or_default();
            for read in next {
                if !self.dependencies.contains_key(read) {
                    continue;
                }
                if !self.index.contains_key(read) {
                    self.visit(read);
                    let low = self.low[name].min(self.low[read]);
                    self.low.insert(name, low);
                } else if self.on_stack.contains(read) {
                    let low = self.low[name].min(self.index[read]);
                    self.low.insert(name, low);
                }
            }
            if self.low[name] == self.index[name] {
                let mut group = Vec::new();
                while let Some(member) = self.stack.pop() {
                    self.on_stack.remove(member);
                    group.push(member.to_owned());
                    if member == name {
                        break;
                    }
                }
                let reads_itself = self
                    .dependencies
                    .get(name)
                    .is_some_and(|reads| reads.iter().any(|read| read == name));
                if group.len() > 1 || reads_itself {
                    self.found.push(group);
                }
            }
        }
    }
    let mut search = Search {
        dependencies,
        index: HashMap::new(),
        low: HashMap::new(),
        stack: Vec::new(),
        on_stack: HashSet::new(),
        counter: 0,
        found: Vec::new(),
    };
    for name in order {
        if !search.index.contains_key(name.as_str()) {
            search.visit(name);
        }
    }
    let mut cycles = search.found;
    for cycle in &mut cycles {
        cycle.sort_by_key(|member| order.iter().position(|name| name == member));
    }
    cycles.sort_by_key(|cycle| order.iter().position(|name| *name == cycle[0]));
    cycles
}

/// field values as scripts read them: text, a checkbox's on value or Off,
/// a list's first choice.
struct Values(HashMap<String, String>);

impl Values {
    fn read(doc: &Document) -> FormResult<Self> {
        let mut values = HashMap::new();
        for info in fields(doc)? {
            let value = match info.kind {
                FieldKind::Checkbox | FieldKind::RadioGroup => {
                    info.value.clone().unwrap_or_else(|| "Off".to_owned())
                }
                _ => info.value.clone().unwrap_or_default(),
            };
            values.insert(info.name, value);
        }
        Ok(Self(values))
    }
}

impl FieldReader for Values {
    fn value_of(&self, field: &str) -> Option<String> {
        self.0.get(field).cloned()
    }
}

/// works out one calculation. none means "leave the field alone".
fn calculate(calculation: &Calculation, current: &str, values: &Values) -> Result<Value, RunError> {
    match calculation {
        Calculation::Aggregate { function, fields } => {
            let numbers: Vec<f64> = fields
                .iter()
                .map(|name| {
                    let text = values
                        .value_of(name)
                        .ok_or_else(|| RunError::UnknownField(name.clone()))?;
                    // AFSimple_Calculate counts empty or non-numeric fields as 0
                    let number = Value::from_field_text(&text.replace(',', "."));
                    Ok(match number {
                        Value::Number(number) => number,
                        _ => 0.0,
                    })
                })
                .collect::<Result<_, RunError>>()?;
            let result = match function {
                Aggregate::Sum => numbers.iter().sum(),
                Aggregate::Product => numbers.iter().product(),
                Aggregate::Average if numbers.is_empty() => 0.0,
                Aggregate::Average => numbers.iter().sum::<f64>() / numbers.len() as f64,
                Aggregate::Minimum => numbers.iter().copied().fold(f64::INFINITY, f64::min),
                Aggregate::Maximum => numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            };
            Ok(Value::Number(
                if result.is_infinite() && numbers.is_empty() {
                    0.0
                } else {
                    result
                },
            ))
        }
        Calculation::Formula { formula } => {
            let expr = parse_notation(formula).map_err(|_| RunError::TooManySteps)?;
            evaluate(&as_numbers(&expr), Value::from_field_text(current), values)
        }
        Calculation::Script { source } => {
            let statements = parse_script(source).map_err(|_| RunError::TooManySteps)?;
            run(&statements, Value::from_field_text(current), values).map(|outcome| outcome.value)
        }
    }
}

/// works out every calculated field again, in the calculation order, and
/// draws formatted values. fields in a circle are left alone.
pub fn recalculate(doc: &mut Document) -> FormResult<FillReport> {
    let logic = form_logic(doc)?;
    let mut values = Values::read(doc)?;
    let mut report = FillReport {
        changed: Vec::new(),
        issues: logic.issues.clone(),
        custom_script_note: logic.custom_script_note.clone(),
    };
    let in_cycle: HashSet<String> = logic
        .issues
        .iter()
        .filter_map(|issue| match issue {
            CalculationIssue::Cycle { fields, .. } => Some(fields.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    for name in &logic.calculation_order {
        if in_cycle.contains(name) {
            continue;
        }
        let Some(field_logic) = logic.fields.iter().find(|field| &field.field == name) else {
            continue;
        };
        let Some(calculation) = &field_logic.calculation else {
            continue;
        };
        let current = values.value_of(name).unwrap_or_default();
        let result = match calculate(calculation, &current, &values) {
            Ok(value) => value,
            Err(RunError::UnknownField(_)) => continue, // already reported
            Err(RunError::TooManySteps) => {
                report.issues.push(CalculationIssue::TooLong {
                    field: name.clone(),
                    message: format!(
                        "The calculation of “{name}” took too long, so Letters stopped it."
                    ),
                });
                continue;
            }
        };
        let text = match result {
            Value::Number(number) if !number.is_finite() => {
                report.issues.push(CalculationIssue::NotANumber {
                    field: name.clone(),
                    message: format!(
                        "“{name}” can’t be worked out from these answers (it may divide by zero), so it is left empty."
                    ),
                });
                String::new()
            }
            other => other.to_field_text(),
        };
        if text != current {
            let node = find_field(doc, name)?;
            store_calculated(doc, &node, &text)?;
            values.0.insert(name.clone(), text.clone());
            report.changed.push(ChangedField {
                name: name.clone(),
                shown: shown_text(field_logic.format.as_ref(), &text),
                value: text,
            });
        }
    }
    redraw_formatted(doc, &logic.fields)?;
    Ok(report)
}

fn shown_text(format: Option<&FieldFormat>, raw: &str) -> String {
    format
        .map(|format| display(format, raw).text)
        .unwrap_or_else(|| raw.to_owned())
}

/// writes a calculated value. calculated fields are often read-only to the
/// person, so this skips the read-only check `set_value` makes.
fn store_calculated(doc: &mut Document, field: &FieldNode, text: &str) -> FormResult<()> {
    if !matches!(
        kind_of(doc, field),
        Some(FieldKind::Text | FieldKind::MultilineText | FieldKind::Date)
    ) {
        return Ok(());
    }
    crate::text::encode_win_ansi_lines(text).map_err(FormError::UnsupportedCharacters)?;
    let dict = doc.get_dictionary_mut(field.id)?;
    dict.set("V", text_string(text));
    dict.remove(b"RV");
    draw_shown_text(doc, field, text, false)
}

/// draws every formatted text field showing its formatted value.
fn redraw_formatted(doc: &mut Document, logic: &[FieldLogic]) -> FormResult<()> {
    for field_logic in logic {
        let Some(format) = &field_logic.format else {
            continue;
        };
        if matches!(
            format,
            FieldFormat::Date { .. } | FieldFormat::Time { .. } | FieldFormat::Mask { .. }
        ) {
            continue;
        }
        let Ok(node) = find_field(doc, &field_logic.field) else {
            continue;
        };
        if !matches!(
            kind_of(doc, &node),
            Some(FieldKind::Text | FieldKind::MultilineText)
        ) {
            continue;
        }
        let raw = stored_text(doc, &node);
        if raw.is_empty() {
            continue;
        }
        let shown = display(format, &raw);
        draw_shown_text(doc, &node, &shown.text, shown.red)?;
    }
    Ok(())
}

/// fills a field the way a form app does: the answer is read in the
/// field's format and checked by its validations (refused with the form's
/// own message, or a plain sentence), stored raw, shown formatted, and
/// then every calculated field is worked out again.
///
/// nothing changes when the answer is refused.
pub fn fill_field(doc: &mut Document, name: &str, value: &FieldValue) -> FormResult<FillReport> {
    let logic = form_logic(doc)?;
    let field_logic = fill_without_recalculating(doc, &logic, name, value)?;
    let mut report = recalculate(doc)?;
    let node = find_field(doc, name)?;
    let raw = stored_text(doc, &node);
    let shown = shown_text(field_logic.format.as_ref(), &raw);
    report.changed.retain(|changed| changed.name != name);
    report.changed.insert(
        0,
        ChangedField {
            name: name.to_owned(),
            value: raw,
            shown,
        },
    );
    Ok(report)
}

/// checks and stores one answer without working out the calculations,
/// for filling many fields at once (importing data). call `recalculate`
/// afterwards. returns the field's logic.
pub(crate) fn fill_without_recalculating(
    doc: &mut Document,
    logic: &FormLogic,
    name: &str,
    value: &FieldValue,
) -> FormResult<FieldLogic> {
    let node = find_field(doc, name)?;
    let field_logic = logic
        .fields
        .iter()
        .find(|field| field.field == name)
        .cloned()
        .ok_or_else(|| FormError::UnknownField(name.to_owned()))?;
    let text_kind = matches!(
        kind_of(doc, &node),
        Some(FieldKind::Text | FieldKind::MultilineText | FieldKind::Date)
    );
    let stored_value = match (value, text_kind) {
        (FieldValue::Text(typed), true) => {
            let raw = match &field_logic.format {
                Some(format) => {
                    read_input(format, name, typed).map_err(|message| FormError::AnswerRefused {
                        field: name.to_owned(),
                        message,
                    })?
                }
                None => typed.clone(),
            };
            check_validations(&field_logic, name, &raw, &Values::read(doc)?)?;
            FieldValue::Text(raw)
        }
        _ => value.clone(),
    };
    set_value(doc, name, &stored_value)?;
    Ok(field_logic)
}

/// runs a field's checks on a new raw value.
fn check_validations(logic: &FieldLogic, name: &str, raw: &str, values: &Values) -> FormResult<()> {
    let refuse = |message: String| FormError::AnswerRefused {
        field: name.to_owned(),
        message,
    };
    if raw.trim().is_empty() {
        // empty answers are the required check's business (in set_value)
        return Ok(());
    }
    for validation in &logic.validations {
        match validation {
            Validation::Range { min, max } => {
                let number = Value::from_field_text(raw);
                let Value::Number(number) = number else {
                    return Err(refuse(format!("“{name}” takes a number.")));
                };
                let low = min.is_some_and(|min| number < min);
                let high = max.is_some_and(|max| number > max);
                if low || high {
                    let text = super::eval::number_to_text;
                    let message = match (min, max) {
                        (Some(min), Some(max)) => format!(
                            "“{name}” must be between {} and {}.",
                            text(*min),
                            text(*max)
                        ),
                        (Some(min), None) => format!("“{name}” must be at least {}.", text(*min)),
                        (None, Some(max)) => format!("“{name}” must be at most {}.", text(*max)),
                        (None, None) => unreachable!("a refusal needs a bound"),
                    };
                    return Err(refuse(message));
                }
            }
            Validation::Condition { condition, message } => {
                let Ok(expr) = parse_notation(condition) else {
                    continue;
                };
                match evaluate(&as_numbers(&expr), Value::from_field_text(raw), values) {
                    Ok(result) if !result.truthy() => {
                        return Err(refuse(message.clone().unwrap_or_else(|| {
                            format!(
                                "This answer for “{name}” doesn’t pass its check ({condition})."
                            )
                        })))
                    }
                    _ => {}
                }
            }
            Validation::Script { source } => {
                let Ok(statements) = parse_script(source) else {
                    continue;
                };
                if let Ok(outcome) = run(&statements, Value::from_field_text(raw), values) {
                    if !outcome.accepted {
                        let message = outcome.alerts.first().cloned().unwrap_or_else(|| {
                            format!("This answer for “{name}” isn’t accepted by the form.")
                        });
                        return Err(refuse(message));
                    }
                }
            }
            // these two are checked by set_value, with its own messages
            Validation::Required | Validation::MaxLength { .. } => {}
        }
    }
    Ok(())
}
