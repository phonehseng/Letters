//! errors a person can act on: for an error in a cell, why it happened
//! in that cell (which reference is empty or text, which name is unknown
//! and what was probably meant, which value a lookup didn't find), and
//! fixes as edit operations the ui can apply in one click.

use std::collections::HashMap;

use serde::Serialize;

use crate::address::{CellRef, Range};
use crate::calc::context::{CellKey, Context};
use crate::calc::eval::Evaluator;
use crate::calc::value::{to_number, CalcValue};
use crate::edit::{ClearKind, Operation};
use crate::formula::{self, BinaryOp, Expr};
use crate::model::{ErrorValue, Value, Workbook};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    pub title: String,
    pub operation: Operation,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorHelp {
    /// "#DIV/0!", "#N/A"... or "circular reference"
    pub error: String,
    /// what the error means in general
    pub meaning: String,
    /// why it happened in this cell
    pub why: String,
    pub fixes: Vec<Fix>,
}

fn meaning(error: ErrorValue) -> &'static str {
    match error {
        ErrorValue::Div0 => "The formula divides by zero or by an empty cell.",
        ErrorValue::NA => "A value the formula looked for isn't there.",
        ErrorValue::Name => "The formula uses a name Excel doesn't know: a misspelt function, a missing defined name, or text without quotes.",
        ErrorValue::Ref => "The formula refers to cells that no longer exist.",
        ErrorValue::Value => "A value has the wrong kind for what the formula does, usually text where a number is needed.",
        ErrorValue::Num => "A number is out of range for the calculation (a square root of a negative number, a result too large).",
        ErrorValue::Spill => "The result has several values, but some of the cells it needs are not empty.",
        ErrorValue::Null => "Two ranges with a space between them have no cells in common.",
        ErrorValue::Calc => "The calculation gives no result, such as FILTER keeping no rows.",
        ErrorValue::GettingData => "The value is still being fetched.",
    }
}

fn set_formula(sheet: usize, at: CellRef, text: String) -> Operation {
    Operation::SetCell {
        sheet,
        row: at.row,
        col: at.col,
        input: format!("={text}"),
    }
}

/// why a cell shows an error, and what would fix it
pub fn diagnose(book: &Workbook, sheet: usize, at: CellRef) -> Option<ErrorHelp> {
    let cell = book.sheets.get(sheet)?.cells.get(at.row, at.col)?;
    let Value::Error(error) = cell.value else {
        return None;
    };
    let text = cell.formula.as_ref().map(|f| f.text.clone());
    let mut help = ErrorHelp {
        error: error.as_str().into(),
        meaning: meaning(error).into(),
        why: String::new(),
        fixes: Vec::new(),
    };
    let Some(text) = text else {
        help.why = "The error was typed into the cell or came from the file as a value.".into();
        return Some(help);
    };
    let expr = formula::parse(&text).ok()?;
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
        row: at.row,
        col: at.col,
        legacy: false,
        depth: 0,
    };
    match error {
        ErrorValue::Div0 => division(&ev, &expr, &mut help),
        ErrorValue::Name => unknown_name(book, sheet, at, &expr, &mut help),
        ErrorValue::NA => not_found(&ev, &expr, &mut help),
        ErrorValue::Value => wrong_kind(&ev, &expr, sheet, &mut help),
        ErrorValue::Spill => blocked(book, &ev, &expr, sheet, at, &mut help),
        ErrorValue::Ref => help.why = "A row, column or sheet this formula used was deleted; the reference now reads #REF!. Undo the deletion, or type the reference again.".into(),
        _ => {}
    }
    if !matches!(error, ErrorValue::Spill | ErrorValue::Ref) {
        let wrapped = format!("IFERROR({text},\"\")");
        help.fixes.push(Fix {
            title: "Show nothing instead of the error (wrap in IFERROR)".into(),
            operation: set_formula(sheet, at, wrapped),
        });
    }
    Some(help)
}

/// finds the first part of the tree that matches, depth first
fn find<'e>(expr: &'e Expr, test: &mut dyn FnMut(&'e Expr) -> bool) -> Option<&'e Expr> {
    if test(expr) {
        return Some(expr);
    }
    let children: Vec<&Expr> = match expr {
        Expr::Function { args, .. } => args.iter().collect(),
        Expr::Binary { left, right, .. } => vec![left, right],
        Expr::Unary { expr, .. } | Expr::Paren { expr } => vec![expr],
        _ => Vec::new(),
    };
    children.into_iter().find_map(|child| find(child, test))
}

fn scalar_of(ev: &Evaluator, expr: &Expr) -> Value {
    ev.intersect(ev.eval(expr))
}

fn describe(expr: &Expr) -> String {
    match expr {
        Expr::Ref { reference } => super::explain::describe_reference(reference),
        other => formula::to_string(other),
    }
}

fn division(ev: &Evaluator, expr: &Expr, help: &mut ErrorHelp) {
    let divisor = find(
        expr,
        &mut |e| matches!(e, Expr::Binary { op: BinaryOp::Divide, right, .. } if matches!(scalar_of(ev, right), Value::Blank | Value::Number(0.0))),
    );
    if let Some(Expr::Binary { right, .. }) = divisor {
        let state = if scalar_of(ev, right).is_blank() {
            "is empty"
        } else {
            "is 0"
        };
        help.why = format!("The formula divides by {}, which {state}.", describe(right));
        return;
    }
    if find(
        expr,
        &mut |e| matches!(e, Expr::Function { name, .. } if name.starts_with("AVERAGE")),
    )
    .is_some()
    {
        help.why = "AVERAGE found no numbers to average, so it divides by zero.".into();
    }
}

fn unknown_name(book: &Workbook, sheet: usize, at: CellRef, expr: &Expr, help: &mut ErrorHelp) {
    let original = formula::to_string(expr);
    if let Some(Expr::Function { name, .. }) = find(
        expr,
        &mut |e| matches!(e, Expr::Function { name, .. } if !crate::calc::functions::is_supported(name)),
    ) {
        match super::search::closest_function(name) {
            Some(closest) => {
                help.why = format!("There is no function called {name}. Did you mean {closest}?");
                let fixed = original.replacen(&format!("{name}("), &format!("{closest}("), 1);
                help.fixes.push(Fix {
                    title: format!("Change {name} to {closest}"),
                    operation: set_formula(sheet, at, fixed),
                });
            }
            None => help.why = format!("{name} isn't a function Letters or Excel knows."),
        }
        return;
    }
    if let Some(Expr::Name { name }) = find(
        expr,
        &mut |e| matches!(e, Expr::Name { name } if book.find_name(name, Some(sheet)).is_none() && book.find_table(name).is_none()),
    ) {
        help.why = format!("“{name}” isn't a defined name or a table. If it is meant as text, put it in quotes: \"{name}\".");
    }
}

fn not_found(ev: &Evaluator, expr: &Expr, help: &mut ErrorHelp) {
    let lookup = find(
        expr,
        &mut |e| matches!(e, Expr::Function { name, .. } if ["VLOOKUP", "HLOOKUP", "XLOOKUP", "MATCH", "XMATCH", "LOOKUP"].contains(&name.as_str())),
    );
    let Some(Expr::Function { name, args }) = lookup else {
        help.why =
            "Something the formula needs gave #N/A (perhaps NA() or a lookup in another cell)."
                .into();
        return;
    };
    let wanted = args.first().map(|a| scalar_of(ev, a)).unwrap_or_default();
    let shown = match &wanted {
        Value::Text(t) => format!("\"{t}\""),
        other => crate::calc::value::to_text(other).unwrap_or_default(),
    };
    let place = args.get(1).map(describe).unwrap_or_default();
    help.why = format!(
        "{name} looked for {shown} ({}) in {place} and didn't find it.",
        args.first().map(describe).unwrap_or_default()
    );
    if let (Value::Number(n), Some(CalcValue::Ref(areas))) =
        (&wanted, args.get(1).map(|a| ev.eval(a)))
    {
        let text_number = areas.first().map(|area| {
            let mut found = false;
            ev.for_each_value(*area, &mut |_, _, v| {
                found |= matches!(v, Value::Text(t) if t.trim().parse::<f64>() == Ok(*n))
            });
            found
        });
        if text_number == Some(true) {
            help.why
                .push_str(" The number is there, but stored as text, so it doesn't match.");
        }
    }
}

fn wrong_kind(ev: &Evaluator, expr: &Expr, sheet: usize, help: &mut ErrorHelp) {
    let arithmetic = [
        BinaryOp::Add,
        BinaryOp::Subtract,
        BinaryOp::Multiply,
        BinaryOp::Divide,
        BinaryOp::Power,
    ];
    let culprit = find(expr, &mut |e| match e {
        Expr::Ref { .. } => {
            matches!(scalar_of(ev, e), Value::Text(ref t) if to_number(&Value::Text(t.clone()), ev.system()).is_err())
        }
        _ => false,
    });
    let in_arithmetic = find(
        expr,
        &mut |e| matches!(e, Expr::Binary { op, .. } if arithmetic.contains(op)),
    )
    .is_some();
    match culprit {
        Some(reference) if in_arithmetic => {
            let shown = crate::calc::value::to_text(&scalar_of(ev, reference)).unwrap_or_default();
            help.why = format!("{} holds the text \"{shown}\", which can't be used in arithmetic. SUM and other functions skip text, but + - * / don't.", describe(reference));
            if let Expr::Ref { reference: r } = reference {
                let at = r.area.to_range().start;
                help.fixes.push(Fix {
                    title: format!("Clear {}", at.to_a1()),
                    operation: Operation::Clear {
                        sheet,
                        range: Range::cell(at),
                        what: ClearKind::Contents,
                    },
                });
            }
        }
        _ => help.why =
            "A value has the wrong kind for this formula, usually text where a number is needed."
                .into(),
    }
}

fn blocked(
    book: &Workbook,
    ev: &Evaluator,
    expr: &Expr,
    sheet: usize,
    at: CellRef,
    help: &mut ErrorHelp,
) {
    let size = match ev.operand(ev.eval(expr)) {
        CalcValue::Array(array) => (array.rows as u32, array.cols as u32),
        _ => (1, 1),
    };
    let area = Range::from_bounds(at.row, at.col, at.row + size.0 - 1, at.col + size.1 - 1);
    let blockers: Vec<CellRef> = area
        .iter()
        .filter(|c| *c != at)
        .filter(|c| {
            book.sheets[sheet]
                .cells
                .get(c.row, c.col)
                .is_some_and(|cell| !cell.is_empty())
        })
        .collect();
    let names: Vec<String> = blockers.iter().take(5).map(|c| c.to_a1()).collect();
    help.why = format!(
        "The result needs {} ({} rows by {} columns), but {} not empty.",
        area.to_a1(),
        size.0,
        size.1,
        if names.len() == 1 {
            format!("{} is", names[0])
        } else {
            format!("{} are", names.join(", "))
        }
    );
    if !blockers.is_empty() {
        let mut clear_area = Range::cell(blockers[0]);
        for b in &blockers {
            clear_area = Range::new(
                CellRef::new(
                    clear_area.start.row.min(b.row),
                    clear_area.start.col.min(b.col),
                ),
                CellRef::new(clear_area.end.row.max(b.row), clear_area.end.col.max(b.col)),
            );
        }
        help.fixes.push(Fix {
            title: format!("Clear {} so the result can spill", clear_area.to_a1()),
            operation: Operation::Clear {
                sheet,
                range: clear_area,
                what: ClearKind::Contents,
            },
        });
    }
}

/// the help for cells in a circular reference (from the calc report)
pub fn circular(cells: &[CellKey], book: &Workbook) -> ErrorHelp {
    let names: Vec<String> = cells
        .iter()
        .take(6)
        .map(|(s, r, c)| {
            format!(
                "{}!{}",
                book.sheets.get(*s).map(|s| s.name.as_str()).unwrap_or("?"),
                CellRef::new(*r, *c).to_a1()
            )
        })
        .collect();
    ErrorHelp {
        error: "circular reference".into(),
        meaning: "A formula depends on its own result, directly or through other cells, so it can never finish.".into(),
        why: format!("These cells refer to each other in a loop: {}. They show 0 until the loop is broken.", names.join(", ")),
        fixes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book_with(cells: &[(&str, &str)]) -> Workbook {
        let mut book = Workbook::new();
        for (address, input) in cells {
            let at = CellRef::parse(address).unwrap();
            crate::edit::apply(
                &mut book,
                &Operation::SetCell {
                    sheet: 0,
                    row: at.row,
                    col: at.col,
                    input: input.to_string(),
                },
            )
            .unwrap();
        }
        crate::calc::recalculate(&mut book);
        book
    }

    #[test]
    fn errors_are_explained_for_the_cell() {
        let book = book_with(&[
            ("A1", "10"),
            ("C1", "=A1/B1"),
            ("D1", "=VLOKUP(1,A1:B1,2)"),
            ("E1", "=A1+F1"),
            ("F1", "abc"),
            ("G1", "=VLOOKUP(5,A1:B1,2,FALSE)"),
            ("H1", "=SEQUENCE(3)"),
            ("H3", "x"),
        ]);
        let div = diagnose(&book, 0, CellRef::parse("C1").unwrap()).unwrap();
        assert_eq!(div.why, "The formula divides by cell B1, which is empty.");
        assert!(div.fixes[0].title.contains("IFERROR"));
        let name = diagnose(&book, 0, CellRef::parse("D1").unwrap()).unwrap();
        assert_eq!(
            name.why,
            "There is no function called VLOKUP. Did you mean VLOOKUP?"
        );
        let value = diagnose(&book, 0, CellRef::parse("E1").unwrap()).unwrap();
        assert!(
            value.why.starts_with("cell F1 holds the text \"abc\""),
            "{}",
            value.why
        );
        let missing = diagnose(&book, 0, CellRef::parse("G1").unwrap()).unwrap();
        assert!(missing.why.contains("looked for 5"), "{}", missing.why);
        let spill = diagnose(&book, 0, CellRef::parse("H1").unwrap()).unwrap();
        assert_eq!(
            spill.why,
            "The result needs H1:H3 (3 rows by 1 columns), but H3 is not empty."
        );
        assert!(diagnose(&book, 0, CellRef::parse("A1").unwrap()).is_none());
    }
}
