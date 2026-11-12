//! recognising the standard form actions in a file and turning them into
//! the declarative model (`FieldLogic`).
//!
//! each field may carry up to four scripts in its additional actions
//! (/AA): /C calculates its value from other fields, /F formats what it
//! shows, /K checks each keystroke (and the final answer), /V validates
//! the answer. acrobat writes the standard ones as calls to its built-in
//! functions (AFSimple_Calculate, AFNumber_Format, AFRange_Validate...),
//! which this module reads by their arguments. other scripts are read by
//! the safe subset in `script.rs`; whatever is left is reported, never run.

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::format::{FieldFormat, NegativeStyle, SeparatorStyle, SpecialFormat, TIME_PATTERNS};
use super::script::{fits_notation, parse_script, simplify, to_notation, Expr, Stmt, UnaryOp};
use super::{Aggregate, Calculation, FieldLogic, ScriptTrigger, UnsupportedScript, Validation};
use crate::date::pattern_from_script;
use crate::objects::{acroform, decode_text_string, get, get_dict};
use crate::tree::{flags, FieldNode};

/// the text of one script action, if it is javascript.
pub fn action_script(doc: &Document, action: &Dictionary) -> Option<String> {
    let kind = get(doc, action, b"S")?.as_name().ok()?;
    if kind != b"JavaScript" {
        return None;
    }
    match get(doc, action, b"JS")? {
        Object::String(bytes, _) => Some(decode_text_string(bytes)),
        Object::Stream(stream) => stream
            .decompressed_content()
            .ok()
            .or_else(|| Some(stream.content.clone()))
            .map(|bytes| decode_text_string(&bytes)),
        _ => None,
    }
}

/// the additional actions of a field: on the field itself, or on its
/// widget when the widget is a separate dictionary.
pub fn field_actions<'a>(doc: &'a Document, field: &FieldNode) -> Option<&'a Dictionary> {
    let own = doc
        .get_dictionary(field.id)
        .ok()
        .and_then(|dict| get_dict(doc, dict, b"AA"))
        .filter(|actions| [b"C", b"F", b"K", b"V"].iter().any(|key| actions.has(*key)));
    own.or_else(|| {
        field.widgets.iter().find_map(|widget| {
            let dict = doc.get_dictionary(*widget).ok()?;
            get_dict(doc, dict, b"AA")
                .filter(|actions| [b"F", b"K", b"V", b"C"].iter().any(|key| actions.has(*key)))
        })
    })
}

/// one argument of a standard function call.
#[derive(Clone, Debug, PartialEq)]
pub enum Argument {
    Number(f64),
    Text(String),
    Bool(bool),
    List(Vec<String>),
}

impl Argument {
    fn number(&self) -> Option<f64> {
        match self {
            Argument::Number(value) => Some(*value),
            Argument::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Argument::Text(text) => text.trim().parse().ok(),
            Argument::List(_) => None,
        }
    }

    fn text(&self) -> Option<String> {
        match self {
            Argument::Text(text) => Some(text.clone()),
            _ => None,
        }
    }

    fn flag(&self) -> bool {
        match self {
            Argument::Bool(value) => *value,
            Argument::Number(value) => *value != 0.0,
            Argument::Text(text) => text == "true",
            Argument::List(_) => false,
        }
    }
}

/// reads a script that is one call to a named function, like
/// `AFNumber_Format(2, 0, 0, 0, "$", true);`, into its name and arguments.
pub fn single_call(script: &str) -> Option<(String, Vec<Argument>)> {
    let cleaned = strip_comments(script);
    let text = cleaned.trim().trim_end_matches(';').trim();
    let open = text.find('(')?;
    let name = text[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let body = text[open + 1..].strip_suffix(')')?;
    let arguments = split_arguments(body)?;
    Some((name.to_owned(), arguments))
}

fn strip_comments(script: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = script.chars().collect();
    let mut index = 0;
    let mut quote: Option<char> = None;
    while index < chars.len() {
        let c = chars[index];
        if let Some(open) = quote {
            out.push(c);
            if c == '\\' && index + 1 < chars.len() {
                out.push(chars[index + 1]);
                index += 2;
                continue;
            }
            if c == open {
                quote = None;
            }
            index += 1;
        } else if c == '"' || c == '\'' {
            quote = Some(c);
            out.push(c);
            index += 1;
        } else if c == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if c == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index += 2;
        } else {
            out.push(c);
            index += 1;
        }
    }
    out
}

/// splits "2, 0, \"$\", new Array(\"a\", \"b\")" into its arguments.
fn split_arguments(body: &str) -> Option<Vec<Argument>> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut current = String::new();
    let mut escaped = false;
    for c in body.chars() {
        if let Some(open) = quote {
            current.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                current.push(c);
            }
            '(' | '[' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    if !current.trim().is_empty() || !parts.is_empty() {
        parts.push(current);
    }
    parts.iter().map(|part| argument(part.trim())).collect()
}

fn argument(text: &str) -> Option<Argument> {
    if let Some(inner) = text
        .strip_prefix("new Array")
        .map(str::trim)
        .and_then(|rest| rest.strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))
        .or_else(|| {
            text.strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
        })
    {
        let items = split_arguments(inner)?;
        return items
            .into_iter()
            .map(|item| item.text())
            .collect::<Option<Vec<_>>>()
            .map(Argument::List);
    }
    match text {
        "true" => return Some(Argument::Bool(true)),
        "false" => return Some(Argument::Bool(false)),
        _ => {}
    }
    let first = text.chars().next()?;
    if (first == '"' || first == '\'') && text.len() >= 2 && text.ends_with(first) {
        let inner = &text[1..text.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some(other) => out.push(other),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        return Some(Argument::Text(out));
    }
    text.parse().ok().map(Argument::Number)
}

/// everything Letters understands about one field's scripts.
pub fn read_field_logic(doc: &Document, field: &FieldNode, all_names: &[String]) -> FieldLogic {
    let mut logic = FieldLogic {
        field: field.name.clone(),
        calculation: None,
        format: None,
        validations: Vec::new(),
        unsupported: Vec::new(),
    };
    let field_flags = field.flags(doc);
    if field_flags & flags::REQUIRED != 0 {
        logic.validations.push(Validation::Required);
    }
    if let Some(max) = field.max_length(doc) {
        logic.validations.push(Validation::MaxLength { max });
    }
    let Some(actions) = field_actions(doc, field) else {
        return logic;
    };
    let script_of =
        |key: &[u8]| get_dict(doc, actions, key).and_then(|action| action_script(doc, action));
    let unsupported = logic_unsupported;

    if let Some(script) = script_of(b"C") {
        match read_calculation(&script, all_names) {
            Ok(calculation) => logic.calculation = Some(calculation),
            Err(reason) => {
                logic
                    .unsupported
                    .push(unsupported(ScriptTrigger::Calculate, &script, reason))
            }
        }
    }
    let format_script = script_of(b"F");
    let keystroke_script = script_of(b"K");
    let from_format = format_script.as_deref().map(read_format);
    let from_keystroke = keystroke_script.as_deref().map(read_format);
    match (from_format, from_keystroke) {
        (Some(Some(format)), _) => logic.format = Some(format),
        (_, Some(Some(format))) => logic.format = Some(format),
        _ => {}
    }
    // a format or keystroke script letters didn't recognise is reported
    if let (Some(script), Some(None)) = (&format_script, read_format_opt(&format_script)) {
        logic.unsupported.push(unsupported(
            ScriptTrigger::Format,
            script,
            "it isn’t one of the standard formats".into(),
        ));
    }
    if let (Some(script), Some(None)) = (&keystroke_script, read_format_opt(&keystroke_script)) {
        logic.unsupported.push(unsupported(
            ScriptTrigger::Keystroke,
            script,
            "it isn’t one of the standard keystroke checks".into(),
        ));
    }
    if let Some(script) = script_of(b"V") {
        match read_validation(&script) {
            Ok(validation) => logic.validations.push(validation),
            Err(reason) => {
                logic
                    .unsupported
                    .push(unsupported(ScriptTrigger::Validate, &script, reason))
            }
        }
    }
    logic
}

fn read_format_opt(script: &Option<String>) -> Option<Option<FieldFormat>> {
    script.as_deref().map(read_format)
}

fn logic_unsupported(trigger: ScriptTrigger, script: &str, reason: String) -> UnsupportedScript {
    let excerpt: String = script.trim().chars().take(120).collect();
    UnsupportedScript {
        trigger,
        excerpt,
        reason,
    }
}

fn read_calculation(script: &str, all_names: &[String]) -> Result<Calculation, String> {
    if let Some((name, arguments)) = single_call(script) {
        if name == "AFSimple_Calculate" {
            return read_simple_calculate(&arguments, all_names)
                .ok_or_else(|| "its AFSimple_Calculate can’t be read".to_owned());
        }
    }
    let statements = parse_script(script).map_err(|error| error.0)?;
    Ok(match statements.as_slice() {
        [Stmt::SetValue(expr)] if fits_notation(expr) => Calculation::Formula {
            formula: to_notation(&simplify(expr)),
        },
        _ => Calculation::Script {
            source: script.trim().to_owned(),
        },
    })
}

fn read_simple_calculate(arguments: &[Argument], all_names: &[String]) -> Option<Calculation> {
    let function = match arguments.first()?.text()?.to_ascii_uppercase().as_str() {
        "SUM" => Aggregate::Sum,
        "PRD" => Aggregate::Product,
        "AVG" => Aggregate::Average,
        "MIN" => Aggregate::Minimum,
        "MAX" => Aggregate::Maximum,
        _ => return None,
    };
    let named: Vec<String> = match arguments.get(1)? {
        Argument::List(items) => items.clone(),
        // acrobat also accepts one string of names separated by commas
        Argument::Text(text) => text.split(',').map(|name| name.trim().to_owned()).collect(),
        _ => return None,
    };
    // a name may stand for a group: every field under it counts
    let mut fields = Vec::new();
    for name in named.into_iter().filter(|name| !name.is_empty()) {
        let prefix = format!("{name}.");
        let under: Vec<&String> = all_names
            .iter()
            .filter(|candidate| candidate.starts_with(&prefix))
            .collect();
        if all_names.contains(&name) || under.is_empty() {
            fields.push(name);
        } else {
            fields.extend(under.into_iter().cloned());
        }
    }
    Some(Calculation::Aggregate { function, fields })
}

/// reads a format or keystroke script into a format, if it is a standard one.
pub fn read_format(script: &str) -> Option<FieldFormat> {
    if let Some(pattern) = pattern_from_script(script) {
        return Some(FieldFormat::Date { pattern });
    }
    let (name, arguments) = single_call(script)?;
    let number = |index: usize| arguments.get(index).and_then(Argument::number);
    match name.as_str() {
        "AFNumber_Format" | "AFNumber_Keystroke" => Some(FieldFormat::Number {
            decimals: number(0)?.clamp(0.0, 10.0) as u32,
            separator: SeparatorStyle::from_code(number(1).unwrap_or(0.0) as i64),
            negative: NegativeStyle::from_code(number(2).unwrap_or(0.0) as i64),
            currency: arguments
                .get(4)
                .and_then(Argument::text)
                .unwrap_or_default(),
            currency_before: arguments.get(5).map(Argument::flag).unwrap_or(true),
        }),
        "AFPercent_Format" | "AFPercent_Keystroke" => Some(FieldFormat::Percent {
            decimals: number(0)?.clamp(0.0, 10.0) as u32,
            separator: SeparatorStyle::from_code(number(1).unwrap_or(0.0) as i64),
        }),
        "AFTime_Format" | "AFTime_Keystroke" => {
            let index = number(0).unwrap_or(0.0) as usize;
            TIME_PATTERNS.get(index).map(|pattern| FieldFormat::Time {
                pattern: (*pattern).to_owned(),
            })
        }
        "AFTime_FormatEx" | "AFTime_KeystrokeEx" => Some(FieldFormat::Time {
            pattern: arguments.first()?.text()?,
        }),
        "AFSpecial_Format" | "AFSpecial_Keystroke" => {
            SpecialFormat::from_code(number(0)? as i64).map(|kind| FieldFormat::Special { kind })
        }
        "AFSpecial_KeystrokeEx" => {
            let mask = arguments.first()?.text()?;
            // the four standard masks as acrobat spells them
            let standard = match mask.as_str() {
                "99999" => Some(SpecialFormat::ZipCode),
                "99999-9999" => Some(SpecialFormat::ZipPlus4),
                "999-99-9999" => Some(SpecialFormat::SocialSecurity),
                _ => None,
            };
            Some(match standard {
                Some(kind) => FieldFormat::Special { kind },
                None => FieldFormat::Mask { mask },
            })
        }
        _ => None,
    }
}

fn read_validation(script: &str) -> Result<Validation, String> {
    if let Some((name, arguments)) = single_call(script) {
        if name == "AFRange_Validate" {
            let get = |index: usize| arguments.get(index);
            let bound = |flag: usize, value: usize| -> Option<f64> {
                if get(flag)?.flag() {
                    get(value)?.number()
                } else {
                    None
                }
            };
            return Ok(Validation::Range {
                min: bound(0, 1),
                max: bound(2, 3),
            });
        }
    }
    let statements = parse_script(script).map_err(|error| error.0)?;
    if let Some(validation) = canonical_condition(&statements) {
        return Ok(validation);
    }
    Ok(Validation::Script {
        source: script.trim().to_owned(),
    })
}

/// the shape Letters writes for a condition:
/// `if (!(condition)) { app.alert("message"); event.rc = false; }`
fn canonical_condition(statements: &[Stmt]) -> Option<Validation> {
    let [Stmt::If(Expr::Unary(UnaryOp::Not, condition), then, otherwise)] = statements else {
        return None;
    };
    if !otherwise.is_empty() || !fits_notation(condition) {
        return None;
    }
    let (message, refuses) = match then.as_slice() {
        [Stmt::Alert(Expr::Text(message)), Stmt::SetAccepted(Expr::Bool(false))] => {
            (Some(message.clone()), true)
        }
        [Stmt::SetAccepted(Expr::Bool(false))] => (None, true),
        _ => (None, false),
    };
    refuses.then(|| Validation::Condition {
        condition: to_notation(&simplify(condition)),
        message,
    })
}

/// the calculation order (/CO) as field ids.
pub fn calculation_order_ids(doc: &Document) -> Vec<ObjectId> {
    acroform(doc)
        .map(|form| crate::objects::references_in(doc, form, b"CO"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_standard_calls() {
        let (name, arguments) = single_call(r#"AFNumber_Format(2, 0, 0, 0, "$", true);"#).unwrap();
        assert_eq!(name, "AFNumber_Format");
        assert_eq!(arguments[4], Argument::Text("$".into()));
        let (_, arguments) =
            single_call(r#"/* sum */ AFSimple_Calculate("SUM", new Array ("a", "b.c"));"#).unwrap();
        assert_eq!(arguments[1], Argument::List(vec!["a".into(), "b.c".into()]));
        assert!(single_call("AFNumber_Format(2); app.alert('x')").is_none());
    }

    #[test]
    fn recognises_every_standard_format() {
        let cases = [
            (r#"AFNumber_Format(2, 2, 3, 0, " €", false);"#, "Number"),
            ("AFPercent_Format(1, 0);", "Percent"),
            (r#"AFDate_FormatEx("dd/mm/yyyy");"#, "Date"),
            ("AFTime_Format(1);", "Time"),
            ("AFSpecial_Format(2);", "Special"),
            (r#"AFSpecial_KeystrokeEx("AA-9999");"#, "Mask"),
        ];
        for (script, kind) in cases {
            let format = read_format(script).unwrap_or_else(|| panic!("{script}"));
            assert!(format!("{format:?}").starts_with(kind), "{format:?}");
        }
        assert_eq!(
            read_format(r#"AFNumber_Format(2, 2, 3, 0, " €", false);"#),
            Some(FieldFormat::Number {
                decimals: 2,
                separator: SeparatorStyle::DotComma,
                negative: NegativeStyle::RedParentheses,
                currency: " €".into(),
                currency_before: false
            })
        );
        assert_eq!(read_format("event.value = 1;"), None);
    }

    #[test]
    fn reads_ranges_and_conditions() {
        assert_eq!(
            read_validation("AFRange_Validate(true, 0, true, 100);").unwrap(),
            Validation::Range {
                min: Some(0.0),
                max: Some(100.0)
            }
        );
        assert_eq!(
            read_validation("AFRange_Validate(false, 0, true, 10);").unwrap(),
            Validation::Range {
                min: None,
                max: Some(10.0)
            }
        );
        let written = r#"if (!(AFMakeNumber(event.value) <= AFMakeNumber(this.getField("budget").value))) { app.alert("Over budget."); event.rc = false; }"#;
        assert_eq!(
            read_validation(written).unwrap(),
            Validation::Condition {
                condition: "value <= budget".into(),
                message: Some("Over budget.".into())
            }
        );
    }
}
