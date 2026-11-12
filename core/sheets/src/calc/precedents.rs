//! reading a formula once for the engine: its tree, whether it can be
//! calculated here, whether it is volatile, and which areas it reads.

use crate::address::{CellRef, Range};
use crate::formula::{Expr, SheetRef};
use crate::model::{Formula, FormulaKind, Workbook};

use super::context::ParsedFormula;

/// functions whose result can change without any cell changing, so they
/// are calculated on every run (excel's volatile functions)
pub const VOLATILE: [&str; 8] = [
    "RAND",
    "RANDBETWEEN",
    "RANDARRAY",
    "NOW",
    "TODAY",
    "OFFSET",
    "INDIRECT",
    "CELL",
];

pub fn parse_formula(
    book: &Workbook,
    sheet: usize,
    at: CellRef,
    formula: &Formula,
) -> ParsedFormula {
    let legacy = matches!(
        formula.kind,
        FormulaKind::Normal | FormulaKind::Shared { .. } | FormulaKind::Array { .. }
    );
    let array_range = match formula.kind {
        FormulaKind::Array { range } => Some(range),
        _ => None,
    };
    let mut parsed = ParsedFormula {
        expr: None,
        legacy,
        array_range,
        unsupported: Vec::new(),
        volatile: false,
        precedents: Vec::new(),
    };
    if let FormulaKind::DataTable { .. } = formula.kind {
        parsed.unsupported.push("TABLE".into());
        return parsed;
    }
    let Ok(expr) = crate::formula::parse(&formula.text) else {
        return parsed;
    };
    for name in expr.function_names() {
        if VOLATILE.contains(&name.as_str()) {
            parsed.volatile = true;
        }
        if !super::functions::is_supported(&name) {
            parsed.unsupported.push(name);
        }
    }
    collect(book, sheet, at, &expr, &mut parsed.precedents, 0);
    parsed.expr = Some(expr);
    parsed
}

/// names can refer to names; this many levels are followed
const MAX_NAME_DEPTH: usize = 8;

fn collect(
    book: &Workbook,
    sheet: usize,
    at: CellRef,
    expr: &Expr,
    out: &mut Vec<(usize, Range)>,
    depth: usize,
) {
    expr.walk(&mut |node| match node {
        Expr::Ref { reference } => {
            for target in sheets_of(book, sheet, &reference.sheet) {
                out.push((target, reference.area.to_range()));
            }
        }
        Expr::Name { name } if depth < MAX_NAME_DEPTH => {
            name_precedents(book, sheet, at, name, out, depth)
        }
        Expr::Structured { reference } => {
            let table = match &reference.table {
                Some(name) => book.find_table(name),
                None => book
                    .table_at(sheet, at.row, at.col)
                    .map(|table| (sheet, table)),
            };
            if let Some((table_sheet, table)) = table {
                out.push((table_sheet, table.range));
            }
        }
        _ => {}
    });
}

fn name_precedents(
    book: &Workbook,
    sheet: usize,
    at: CellRef,
    name: &str,
    out: &mut Vec<(usize, Range)>,
    depth: usize,
) {
    let local = name.rsplit_once('!').map(|(_, n)| n).unwrap_or(name);
    if let Some(defined) = book.find_name(local, Some(sheet)) {
        if let Ok(expr) = crate::formula::parse(&defined.formula) {
            collect(book, sheet, at, &expr, out, depth + 1);
        }
    } else if let Some((table_sheet, table)) = book.find_table(name) {
        out.push((table_sheet, table.range));
    }
}

fn sheets_of(book: &Workbook, current: usize, sheet: &Option<SheetRef>) -> Vec<usize> {
    match sheet {
        None => vec![current],
        Some(SheetRef::One { name }) => book.sheet_index(name).into_iter().collect(),
        Some(SheetRef::Span { first, last }) => {
            match (book.sheet_index(first), book.sheet_index(last)) {
                (Some(a), Some(b)) => (a.min(b)..=a.max(b)).collect(),
                _ => Vec::new(),
            }
        }
        Some(SheetRef::External { .. }) => Vec::new(),
    }
}
