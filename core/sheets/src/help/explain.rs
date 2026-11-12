//! "what does this formula do?": the formula's tree read aloud in plain
//! sentences, outermost first, and the same tree with the value each part
//! gives (evaluate step by step), for learning and for finding mistakes.

use std::collections::HashMap;

use serde::Serialize;

use crate::address::CellRef;
use crate::calc::context::Context;
use crate::calc::eval::Evaluator;
use crate::calc::value::{to_text, CalcValue};
use crate::formula::{self, Area, BinaryOp, Expr, Reference, SheetRef, UnaryOp};
use crate::model::{Value, Workbook};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// how deep in the formula this part is (0 is the whole formula)
    pub depth: usize,
    /// the part as formula text
    pub part: String,
    pub sentence: String,
    /// the part's value, when evaluating step by step
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

fn sheet_words(sheet: &Option<SheetRef>) -> String {
    match sheet {
        Some(SheetRef::One { name }) => format!(" on sheet {name}"),
        Some(SheetRef::Span { first, last }) => format!(" on every sheet from {first} to {last}"),
        Some(SheetRef::External { name, .. }) => format!(" in another workbook (sheet {name})"),
        None => String::new(),
    }
}

/// "cell B2", "the cells A1 to A10", "all of column C"
pub fn describe_reference(reference: &Reference) -> String {
    let text = formula::print::area_text(&reference.area).replace('$', "");
    let place = match reference.area {
        Area::Cell { .. } => format!("cell {text}"),
        Area::Range { .. } => {
            let (a, b) = text.split_once(':').unwrap_or((&text, &text));
            format!("the cells {a} to {b}")
        }
        Area::Columns { first, last, .. } if first == last => {
            format!("all of column {}", crate::address::column_name(first))
        }
        Area::Columns { .. } => format!("all of columns {text}"),
        Area::Rows { first, last, .. } if first == last => format!("all of row {}", first + 1),
        Area::Rows { .. } => format!("all of rows {text}"),
    };
    format!("{place}{}", sheet_words(&reference.sheet))
}

/// a short phrase for an argument in a sentence
fn phrase(expr: &Expr) -> String {
    match expr {
        Expr::Ref { reference } => describe_reference(reference),
        Expr::Text { value } => format!("the text \"{value}\""),
        Expr::Number { value } => crate::calc::value::general_text(*value),
        Expr::Bool { value } => if *value { "TRUE" } else { "FALSE" }.to_string(),
        Expr::Missing => "nothing".to_string(),
        Expr::Paren { expr } => phrase(expr),
        other => formula::to_string(other),
    }
}

fn arg(args: &[Expr], index: usize) -> String {
    args.get(index)
        .map(phrase)
        .unwrap_or_else(|| "nothing".into())
}

fn is_false(args: &[Expr], index: usize) -> bool {
    matches!(
        args.get(index),
        Some(Expr::Bool { value: false }) | Some(Expr::Number { value: 0.0 })
    )
}

/// tailored sentences for the functions people use most
fn function_sentence(name: &str, args: &[Expr]) -> String {
    match name {
        "VLOOKUP" | "HLOOKUP" => {
            let side = if name == "VLOOKUP" { "first column" } else { "first row" };
            let which = if name == "VLOOKUP" { "column" } else { "row" };
            let matching = if is_false(args, 3) { "exact match only" } else { "approximate match: the first column must be sorted" };
            format!("Looks up {} in the {side} of {} and returns the matching value from {which} {}; {matching}.", arg(args, 0), arg(args, 1), arg(args, 2))
        }
        "XLOOKUP" => format!("Looks for {} in {} and returns the value in the same place of {}{}.", arg(args, 0), arg(args, 1), arg(args, 2), if args.len() > 3 { format!(", or {} when there is no match", arg(args, 3)) } else { String::new() }),
        "IF" => format!("Checks whether {} is true: if so it gives {}, otherwise {}.", arg(args, 0), arg(args, 1), if args.len() > 2 { arg(args, 2) } else { "FALSE".into() }),
        "SUM" => format!("Adds up {}.", args.iter().map(phrase).collect::<Vec<_>>().join(" and ")),
        "AVERAGE" => format!("Works out the average of {}.", args.iter().map(phrase).collect::<Vec<_>>().join(" and ")),
        "COUNTIF" => format!("Counts the cells in {} that match {}.", arg(args, 0), arg(args, 1)),
        "SUMIF" => format!("Adds up {} where {} matches {}.", if args.len() > 2 { arg(args, 2) } else { arg(args, 0) }, arg(args, 0), arg(args, 1)),
        "INDEX" => format!("Takes the value at row {}{} of {}.", arg(args, 1), if args.len() > 2 { format!(", column {}", arg(args, 2)) } else { String::new() }, arg(args, 0)),
        "MATCH" => format!("Finds the position of {} in {}{}.", arg(args, 0), arg(args, 1), if is_false(args, 2) { " (exact match)" } else { "" }),
        "IFERROR" => format!("Works out {}, and if that is an error shows {} instead.", arg(args, 0), arg(args, 1)),
        "ROUND" => format!("Rounds {} to {} decimal places.", arg(args, 0), arg(args, 1)),
        _ => match super::catalogue::function_info(name) {
            Some(info) => format!("{} ({name})", info.description.trim_end_matches('.')).to_string() + &with_arguments(&info, args),
            None => format!("Uses {name}, a function letters does not know; the value stored in the file is shown."),
        },
    }
}

fn with_arguments(info: &super::catalogue::FunctionInfo, args: &[Expr]) -> String {
    if args.is_empty() {
        return ".".into();
    }
    let parts: Vec<String> = args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let name = info
                .arguments
                .get(i)
                .or(info.arguments.last())
                .map(|a| a.name.clone())
                .unwrap_or_default();
            format!("{name} = {}", phrase(a))
        })
        .collect();
    format!(", with {}.", parts.join(", "))
}

fn operator_sentence(op: BinaryOp, left: &Expr, right: &Expr) -> String {
    let (l, r) = (phrase(left), phrase(right));
    match op {
        BinaryOp::Add => format!("Adds {r} to {l}."),
        BinaryOp::Subtract => format!("Takes {r} away from {l}."),
        BinaryOp::Multiply => format!("Multiplies {l} by {r}."),
        BinaryOp::Divide => format!("Divides {l} by {r}."),
        BinaryOp::Power => format!("Raises {l} to the power {r}."),
        BinaryOp::Concat => format!("Joins {l} and {r} into one text."),
        BinaryOp::Equal => format!("Checks whether {l} equals {r}."),
        BinaryOp::NotEqual => format!("Checks whether {l} differs from {r}."),
        BinaryOp::Less => format!("Checks whether {l} is less than {r}."),
        BinaryOp::LessEqual => format!("Checks whether {l} is at most {r}."),
        BinaryOp::Greater => format!("Checks whether {l} is more than {r}."),
        BinaryOp::GreaterEqual => format!("Checks whether {l} is at least {r}."),
        BinaryOp::Range => format!("Takes every cell from {l} to {r}."),
        BinaryOp::Intersect => format!("Takes the cells {l} and {r} have in common."),
        BinaryOp::Union => format!("Takes {l} together with {r}."),
    }
}

fn sentence(expr: &Expr) -> Option<String> {
    Some(match expr {
        Expr::Function { name, args } => function_sentence(name, args),
        Expr::Binary { op, left, right } => operator_sentence(*op, left, right),
        Expr::Unary {
            op: UnaryOp::Minus,
            expr,
        } => format!("Makes {} negative.", phrase(expr)),
        Expr::Unary {
            op: UnaryOp::Percent,
            expr,
        } => format!("Takes {} as a percentage (divides by 100).", phrase(expr)),
        Expr::Ref { reference } => format!("Uses the value of {}.", describe_reference(reference)),
        Expr::Name { name } => format!("Uses the defined name {name}."),
        Expr::Structured { reference } => format!(
            "Uses the table reference {}.",
            formula::print::structured_text(reference)
        ),
        _ => return None,
    })
}

fn children(expr: &Expr) -> Vec<&Expr> {
    match expr {
        Expr::Function { args, .. } => args.iter().collect(),
        Expr::Binary { left, right, .. } => vec![left, right],
        Expr::Unary { expr, .. } | Expr::Paren { expr } => vec![expr],
        _ => Vec::new(),
    }
}

fn walk(expr: &Expr, depth: usize, steps: &mut Vec<Step>, value: &dyn Fn(&Expr) -> Option<String>) {
    let inner = if let Expr::Paren { expr } = expr {
        expr
    } else {
        expr
    };
    if let Some(sentence) = sentence(inner) {
        steps.push(Step {
            depth,
            part: formula::to_string(inner),
            sentence,
            value: value(inner),
        });
    }
    for child in children(inner) {
        walk(child, depth + 1, steps, value);
    }
}

/// plain sentences for a formula ("=" optional), outermost part first
pub fn explain_formula(text: &str) -> Vec<Step> {
    match formula::parse(text) {
        Ok(expr) => {
            let mut steps = Vec::new();
            walk(&expr, 0, &mut steps, &|_| None);
            steps
        }
        Err(error) => vec![Step {
            depth: 0,
            part: text.to_string(),
            sentence: format!("This formula can't be read: {error}."),
            value: None,
        }],
    }
}

fn value_text(value: &CalcValue, ev: &Evaluator) -> String {
    match value {
        CalcValue::Scalar(Value::Text(t)) => format!("\"{t}\""),
        CalcValue::Scalar(v) => to_text(v).unwrap_or_else(|e| e.as_str().to_string()),
        CalcValue::Array(a) => format!("a list of {} values", a.items.len()),
        CalcValue::Ref(areas) if areas.len() == 1 && areas[0].range.cell_count() == 1 => {
            let at = areas[0].range.start;
            match ev.cell_value(areas[0].sheet, at.row, at.col) {
                Value::Text(t) => format!("\"{t}\""),
                v => to_text(&v).unwrap_or_else(|e| e.as_str().to_string()),
            }
        }
        CalcValue::Ref(areas) => format!(
            "{} cells",
            areas.iter().map(|a| a.range.cell_count()).sum::<u64>()
        ),
    }
}

/// the same steps with the value of each part, as the formula sits in
/// `cell` of `sheet` (relative parts and table rows depend on it)
pub fn evaluate_steps(book: &Workbook, sheet: usize, cell: CellRef, text: &str) -> Vec<Step> {
    let Ok(expr) = formula::parse(text) else {
        return explain_formula(text);
    };
    let formulas = HashMap::new();
    let spills = HashMap::new();
    let ctx = Context::new(
        book,
        &formulas,
        &spills,
        crate::calc::context::local_now_serial(book.date_system),
        1,
    );
    let ev = Evaluator {
        ctx: &ctx,
        sheet,
        row: cell.row,
        col: cell.col,
        legacy: false,
        depth: 0,
    };
    let mut steps = Vec::new();
    walk(&expr, 0, &mut steps, &|part| {
        Some(value_text(&ev.eval(part), &ev))
    });
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_are_read_aloud() {
        let steps = explain_formula("=VLOOKUP(A2,Prices!A:C,3,FALSE)");
        assert_eq!(steps[0].sentence, "Looks up cell A2 in the first column of all of columns A:C on sheet Prices and returns the matching value from column 3; exact match only.");
        let steps = explain_formula("=IF(B2>=50,\"pass\",\"fail\")");
        assert_eq!(steps[0].sentence, "Checks whether B2>=50 is true: if so it gives the text \"pass\", otherwise the text \"fail\".");
        assert_eq!(steps[1].sentence, "Checks whether cell B2 is at least 50.");
        assert!(explain_formula("=SUM(")
            .first()
            .unwrap()
            .sentence
            .starts_with("This formula can't be read"));
    }

    #[test]
    fn steps_show_values() {
        let mut book = Workbook::new();
        book.set_value(0, 0, 0, Value::Number(4.0));
        book.set_value(0, 1, 0, Value::Number(6.0));
        let steps = evaluate_steps(&book, 0, CellRef::new(5, 5), "=ROUND(AVERAGE(A1:A2)/3,1)");
        assert_eq!(steps[0].value.as_deref(), Some("1.7"));
        assert_eq!(steps[1].value.as_deref(), Some("1.66666666666667"));
        assert_eq!(steps[2].value.as_deref(), Some("5"));
    }
}
