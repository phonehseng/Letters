//! the function catalogue the ui shows while someone types a formula:
//! for every function letters calculates, a plain description, its
//! arguments, two worked examples, common mistakes and related functions.
//! the records are one line each in `math_and_stats.rs` and
//! `lookup_text_dates.rs`; this file turns them into structured records.

mod lookup_text_dates;
mod math_and_stats;

use serde::Serialize;

/// what an argument expects, for the hint and for colouring
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArgumentKind {
    Range,
    Number,
    Text,
    Condition,
    Logical,
    Any,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Argument {
    pub name: String,
    pub description: String,
    pub optional: bool,
    /// the argument can be given again (number1, number2, ...)
    pub repeating: bool,
    pub kind: ArgumentKind,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub formula: String,
    pub result: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: String,
    pub category: String,
    pub description: String,
    pub arguments: Vec<Argument>,
    pub examples: Vec<Example>,
    pub mistakes: String,
    pub related: Vec<String>,
    /// where google sheets or libreoffice differ, in plain words
    #[serde(skip_serializing_if = "String::is_empty")]
    pub other_family: String,
}

/// one record as written in the tables: name, category, description,
/// arguments, examples, mistakes, related, other family.
///
/// arguments are "name|kind|description" separated by "; "; a name in
/// brackets is optional and one ending in "..." repeats. examples are
/// "formula => result" separated by " ;; ".
pub type Record = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

fn kind(text: &str) -> ArgumentKind {
    match text {
        "range" => ArgumentKind::Range,
        "number" => ArgumentKind::Number,
        "text" => ArgumentKind::Text,
        "condition" => ArgumentKind::Condition,
        "logical" => ArgumentKind::Logical,
        _ => ArgumentKind::Any,
    }
}

fn argument(spec: &str) -> Option<Argument> {
    let mut parts = spec.splitn(3, '|');
    let raw_name = parts.next()?.trim();
    let kind = kind(parts.next()?.trim());
    let description = parts.next().unwrap_or("").trim().to_string();
    let repeating = raw_name.ends_with("...");
    let name = raw_name.trim_end_matches("...");
    let optional = name.starts_with('[');
    let name = name.trim_matches(['[', ']']).to_string();
    Some(Argument {
        name,
        description,
        optional,
        repeating,
        kind,
    })
}

fn build(record: &Record) -> FunctionInfo {
    let (name, category, description, arguments, examples, mistakes, related, other) = *record;
    FunctionInfo {
        name: name.to_string(),
        category: category.to_string(),
        description: description.to_string(),
        arguments: arguments
            .split("; ")
            .filter(|s| !s.is_empty())
            .filter_map(argument)
            .collect(),
        examples: examples
            .split(" ;; ")
            .filter_map(|e| e.split_once(" => "))
            .map(|(formula, result)| Example {
                formula: formula.to_string(),
                result: result.to_string(),
            })
            .collect(),
        mistakes: mistakes.to_string(),
        related: related
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string())
            .collect(),
        other_family: other.to_string(),
    }
}

/// every function's record, sorted by name
pub fn catalogue() -> Vec<FunctionInfo> {
    let mut all: Vec<FunctionInfo> = math_and_stats::RECORDS
        .iter()
        .chain(lookup_text_dates::RECORDS)
        .map(build)
        .collect();
    all.sort_by(|a, b| a.name.cmp(&b.name));
    all
}

/// one function's record
pub fn function_info(name: &str) -> Option<FunctionInfo> {
    let upper = name.to_ascii_uppercase();
    math_and_stats::RECORDS
        .iter()
        .chain(lookup_text_dates::RECORDS)
        .find(|r| r.0 == upper)
        .map(build)
}

/// how many arguments a function needs at least: those before its first
/// optional one. the calculator checks this before calling a function, so
/// SUBSTITUTE("a","b") gives #VALUE! instead of reading a third argument
/// that isn't there. read from the records once, then kept.
pub fn required_arguments(name: &str) -> usize {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static REQUIRED: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    let required = REQUIRED.get_or_init(|| {
        math_and_stats::RECORDS
            .iter()
            .chain(lookup_text_dates::RECORDS)
            .map(|record| {
                let count = record
                    .3
                    .split("; ")
                    .filter(|s| !s.is_empty())
                    .take_while(|spec| !spec.trim_start().starts_with('['))
                    .count();
                (record.0, count)
            })
            .collect()
    });
    required.get(name).copied().unwrap_or(0)
}

/// how many arguments a function takes at most, or `None` when its last
/// one repeats (SUM's number1, number2, ...). more than that gives #VALUE!,
/// as too few does: LEN("a","b") used to give 1, PI(1) gave pi.
pub fn allowed_arguments(name: &str) -> Option<usize> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static ALLOWED: OnceLock<HashMap<&'static str, Option<usize>>> = OnceLock::new();
    let allowed = ALLOWED.get_or_init(|| {
        math_and_stats::RECORDS
            .iter()
            .chain(lookup_text_dates::RECORDS)
            .map(|record| {
                let specs: Vec<&str> = record.3.split("; ").filter(|s| !s.is_empty()).collect();
                // a name ending in "..." (before its kind) repeats
                let repeats = specs
                    .iter()
                    .any(|spec| spec.split('|').next().unwrap_or("").trim().ends_with("..."));
                (record.0, (!repeats).then_some(specs.len()))
            })
            .collect()
    });
    allowed.get(name).copied().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Value, Workbook};

    #[test]
    fn every_calculated_function_is_described() {
        let described: Vec<String> = catalogue().into_iter().map(|f| f.name).collect();
        for (_, names) in crate::calc::functions::FAMILIES {
            for name in *names {
                assert!(
                    described.iter().any(|d| d == name),
                    "{name} has no catalogue record"
                );
            }
        }
    }

    /// the worked examples are checked by calculating them
    #[test]
    fn every_example_gives_the_result_it_shows() {
        let mut wrong = Vec::new();
        for info in catalogue() {
            assert_eq!(info.examples.len(), 2, "{} needs two examples", info.name);
            assert!(
                !info.arguments.is_empty()
                    || ["PI", "RAND", "TODAY", "NOW", "NA", "TRUE", "FALSE", "ROW", "COLUMN"]
                        .contains(&info.name.as_str()),
                "{}",
                info.name
            );
            for example in &info.examples {
                if example.result == "varies" {
                    continue;
                }
                let mut book = Workbook::new();
                book.set_formula(0, 0, 0, &example.formula);
                crate::calc::recalculate(&mut book);
                let shown = match book.value(0, 0, 0) {
                    Value::Number(n) => {
                        crate::calc::value::general_text(crate::calc::value::round_significant(n))
                    }
                    Value::Text(t) => format!("\"{t}\""),
                    Value::Bool(b) => if b { "TRUE" } else { "FALSE" }.to_string(),
                    Value::Error(e) => e.as_str().to_string(),
                    Value::Blank => String::new(),
                };
                if shown != example.result {
                    wrong.push(format!(
                        "{} gives {shown}, not {}",
                        example.formula, example.result
                    ));
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// every function given one argument more than it takes shows #VALUE!
    #[test]
    fn too_many_arguments_give_value_errors() {
        for info in catalogue() {
            let Some(most) = allowed_arguments(&info.name) else {
                continue;
            };
            let arguments = vec!["1"; most + 1].join(",");
            let formula = format!("{}({arguments})", info.name);
            let mut book = Workbook::new();
            book.set_formula(0, 0, 0, &formula);
            crate::calc::recalculate(&mut book);
            assert_eq!(
                book.value(0, 0, 0),
                Value::Error(crate::model::ErrorValue::Value),
                "{formula}"
            );
        }
    }

    /// every function given fewer arguments than it needs shows #VALUE!
    /// (SUBSTITUTE with two used to stop the calculation)
    #[test]
    fn too_few_arguments_give_value_errors() {
        for info in catalogue() {
            let needed = required_arguments(&info.name);
            for given in 0..needed {
                let arguments = vec!["1"; given].join(",");
                let formula = format!("{}({arguments})", info.name);
                let mut book = Workbook::new();
                book.set_formula(0, 0, 0, &formula);
                crate::calc::recalculate(&mut book);
                assert_eq!(
                    book.value(0, 0, 0),
                    Value::Error(crate::model::ErrorValue::Value),
                    "{formula}"
                );
            }
        }
    }
}
