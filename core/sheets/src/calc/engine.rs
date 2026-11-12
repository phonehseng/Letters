//! the calculation engine: keeps every formula parsed with its
//! dependencies, works out which formulas an edit affects, calculates
//! them in an order where every formula comes after the ones it reads,
//! spills array results, and reports circular references and formulas it
//! could not calculate.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;

use crate::address::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::formula::{BinaryOp, Expr};
use crate::model::{ErrorValue, FormulaKind, Value, Workbook};

use super::context::{local_now_serial, CellKey, Context, ParsedFormula};
use super::deps::DependencyIndex;
use super::eval::Evaluator;
use super::precedents::parse_formula;
use super::value::{to_number, Array, CalcValue};

/// a cell the engine could not calculate, and why
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotRecalculated {
    pub sheet: usize,
    pub row: u32,
    pub col: u32,
    /// functions letters does not have, or empty when the formula could
    /// not be read at all
    pub functions: Vec<String>,
}

/// what a calculation did
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcReport {
    /// cells whose value changed (spilled cells included)
    pub changed: Vec<CellKey>,
    /// cells that are part of a circular reference; excel shows 0 there
    pub circular: Vec<CellKey>,
    /// formula cells showing the value stored in the file, unchanged
    pub not_recalculated: Vec<NotRecalculated>,
    /// how many formulas were calculated
    pub calculated: usize,
    pub milliseconds: f64,
}

#[derive(Default)]
pub struct Engine {
    formulas: HashMap<CellKey, ParsedFormula>,
    index: DependencyIndex,
    /// where each formula spilled last time (the anchor's own cell included)
    spills: HashMap<CellKey, Range>,
    /// where formulas showing #SPILL! want to spill: clearing a cell
    /// there must calculate them again
    blocked: HashMap<CellKey, Range>,
    volatile: HashSet<CellKey>,
    runs: u64,
}

impl Engine {
    /// reads every formula of the workbook.
    pub fn new(book: &Workbook) -> Engine {
        let mut engine = Engine::default();
        for (sheet_index, sheet) in book.sheets.iter().enumerate() {
            for (row, col, cell) in sheet.cells.iter() {
                if let Some(formula) = &cell.formula {
                    let key = (sheet_index, row, col);
                    engine.insert(
                        key,
                        parse_formula(book, sheet_index, CellRef::new(row, col), formula),
                    );
                    if let Some(range) = stored_spill(&formula.kind) {
                        engine.spills.insert(key, range);
                    }
                }
            }
        }
        engine
    }

    fn insert(&mut self, key: CellKey, parsed: ParsedFormula) {
        self.index.add(key, &parsed.precedents);
        if parsed.volatile {
            self.volatile.insert(key);
        }
        self.formulas.insert(key, parsed);
    }

    fn forget(&mut self, key: CellKey) {
        if let Some(old) = self.formulas.remove(&key) {
            self.index.remove(key, &old.precedents);
        }
        self.volatile.remove(&key);
    }

    pub fn formula_count(&self) -> usize {
        self.formulas.len()
    }

    /// calculates every formula.
    pub fn recalculate_all(&mut self, book: &mut Workbook) -> CalcReport {
        let dirty: HashSet<CellKey> = self.formulas.keys().copied().collect();
        self.run(book, dirty)
    }

    /// after cells were edited: reads their new formulas and calculates
    /// only what depends on them (plus volatile formulas).
    pub fn cells_changed(&mut self, book: &mut Workbook, cells: &[CellKey]) -> CalcReport {
        let mut areas = Vec::new();
        let mut dirty = HashSet::new();
        for key in cells {
            self.forget(*key);
            let formula = book
                .sheets
                .get(key.0)
                .and_then(|s| s.cells.get(key.1, key.2))
                .and_then(|c| c.formula.clone());
            if let Some(formula) = formula {
                self.insert(
                    *key,
                    parse_formula(book, key.0, CellRef::new(key.1, key.2), &formula),
                );
                dirty.insert(*key);
            }
            areas.push((key.0, Range::cell(CellRef::new(key.1, key.2))));
            // typing into a spill area makes that formula show #SPILL!,
            // and clearing a blocking cell lets it spill again
            for (anchor, range) in self.spills.iter().chain(&self.blocked) {
                if anchor.0 == key.0 && range.contains(CellRef::new(key.1, key.2)) {
                    dirty.insert(*anchor);
                }
            }
        }
        for key in &self.volatile {
            dirty.insert(*key);
            areas.push((key.0, Range::cell(CellRef::new(key.1, key.2))));
        }
        let spills = &self.spills;
        let affected = self.index.affected(areas, &|key| spills.get(&key).copied());
        dirty.extend(affected);
        dirty.retain(|key| self.formulas.contains_key(key));
        self.run(book, dirty)
    }

    fn run(&mut self, book: &mut Workbook, dirty: HashSet<CellKey>) -> CalcReport {
        let started = Instant::now();
        self.runs += 1;
        let now = local_now_serial(book.date_system);
        let seed = (now.to_bits() ^ self.runs.wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1;
        let outcome = {
            let ctx = Context::new(book, &self.formulas, &self.spills, now, seed);
            for key in &dirty {
                ctx.state.borrow_mut().mark_pending(*key);
            }
            let mut order: Vec<CellKey> = dirty.iter().copied().collect();
            order.sort_unstable();
            for key in order {
                calculate_in_order(&ctx, key, &self.formulas);
            }
            ctx.state.into_inner()
        };
        let mut report = CalcReport {
            calculated: outcome.results.len(),
            circular: outcome.circular.clone(),
            ..CalcReport::default()
        };
        self.write_back(book, outcome, &mut report);
        report.milliseconds = started.elapsed().as_secs_f64() * 1000.0;
        report
    }

    fn write_back(
        &mut self,
        book: &mut Workbook,
        outcome: super::context::RunState,
        report: &mut CalcReport,
    ) {
        for (key, value) in &outcome.results {
            let parsed = &self.formulas[key];
            let Some(cell) = book.sheets[key.0].cells.get_mut(key.1, key.2) else {
                continue;
            };
            cell.not_recalculated = !parsed.can_calculate() && parsed.legacy;
            if cell.not_recalculated {
                report.not_recalculated.push(NotRecalculated {
                    sheet: key.0,
                    row: key.1,
                    col: key.2,
                    functions: parsed.unsupported.clone(),
                });
                continue;
            }
            if cell.value != *value {
                cell.value = value.clone();
                report.changed.push(*key);
            }
            if let Some(formula) = cell.formula.as_mut() {
                if let FormulaKind::Dynamic { spill } = &mut formula.kind {
                    *spill = outcome.new_spills.get(key).copied();
                }
            }
            self.clear_old_spill(book, *key, outcome.new_spills.get(key), report);
            match outcome.new_spills.get(key) {
                Some(range) => self.spills.insert(*key, *range),
                None => self.spills.remove(key),
            };
            match outcome.blocked_spills.get(key) {
                Some(range) => self.blocked.insert(*key, *range),
                None => self.blocked.remove(key),
            };
        }
        for (key, value) in outcome.spill_values {
            let cell = book.sheets[key.0].cells.entry(key.1, key.2);
            if cell.value != value {
                cell.value = value;
                report.changed.push(key);
            }
        }
    }

    /// empties cells a formula filled last time but not this time
    fn clear_old_spill(
        &self,
        book: &mut Workbook,
        anchor: CellKey,
        new: Option<&Range>,
        report: &mut CalcReport,
    ) {
        let Some(old) = self.spills.get(&anchor) else {
            return;
        };
        for cell in old.iter() {
            let key = (anchor.0, cell.row, cell.col);
            let still = new.is_some_and(|range| range.contains(cell));
            if key == anchor || still {
                continue;
            }
            let sheet = &mut book.sheets[anchor.0];
            let remove = sheet
                .cells
                .get(cell.row, cell.col)
                .is_some_and(|c| c.formula.is_none() && c.style == 0);
            if remove {
                sheet.cells.remove(cell.row, cell.col);
            } else if let Some(c) = sheet.cells.get_mut(cell.row, cell.col) {
                if c.formula.is_none() {
                    c.value = Value::Blank;
                }
            }
            report.changed.push(key);
        }
    }
}

/// where a formula from a file says it spilled
fn stored_spill(kind: &FormulaKind) -> Option<Range> {
    match kind {
        FormulaKind::Dynamic { spill } => *spill,
        FormulaKind::Array { range } => Some(*range),
        _ => None,
    }
}

/// calculates `start` after every pending formula it reads, without
/// recursion: a depth-first walk with an explicit stack, so a chain of
/// 200,000 formulas each reading the one below is safe
fn calculate_in_order(ctx: &Context, start: CellKey, formulas: &HashMap<CellKey, ParsedFormula>) {
    let mut stack = vec![(start, false)];
    let mut on_path: HashSet<CellKey> = HashSet::new();
    while let Some((key, expanded)) = stack.pop() {
        if !ctx.state.borrow().pending.contains(&key) {
            on_path.remove(&key);
            continue;
        }
        if expanded {
            calculate_cell(ctx, key, 0);
            on_path.remove(&key);
            continue;
        }
        on_path.insert(key);
        stack.push((key, true));
        let Some(parsed) = formulas.get(&key) else {
            continue;
        };
        for (sheet, range) in &parsed.precedents {
            let waiting = ctx.state.borrow().pending_in(*sheet, range);
            for precedent in waiting {
                if precedent != key && !on_path.contains(&precedent) {
                    stack.push((precedent, false));
                }
            }
        }
    }
}

/// calculates one formula cell and records its value (and spill)
pub fn calculate_cell(ctx: &Context, key: CellKey, depth: usize) -> Value {
    let Some(parsed) = ctx.formulas.get(&key) else {
        return ctx.book.value(key.0, key.1, key.2);
    };
    let value = match (&parsed.expr, parsed.can_calculate()) {
        (Some(expr), true) => {
            ctx.state.borrow_mut().in_progress.insert(key);
            let ev = Evaluator {
                ctx,
                sheet: key.0,
                row: key.1,
                col: key.2,
                legacy: parsed.legacy,
                depth,
            };
            let result = evaluate_top(&ev, expr);
            let value = place(&ev, key, parsed, result);
            ctx.state.borrow_mut().in_progress.remove(&key);
            value
        }
        // a formula from a file keeps the value the file stored, so it
        // never shows a wrong number; one typed here shows excel's #NAME?
        _ if parsed.legacy => ctx.book.value(key.0, key.1, key.2),
        _ => Value::Error(ErrorValue::Name),
    };
    let mut state = ctx.state.borrow_mut();
    state.results.insert(key, value.clone());
    state.finish(key);
    value
}

/// the formula's result, with excel's snapping of a last addition or
/// subtraction that leaves only rounding noise
fn evaluate_top(ev: &Evaluator, expr: &Expr) -> CalcValue {
    if let Expr::Binary {
        op: op @ (BinaryOp::Add | BinaryOp::Subtract),
        left,
        right,
    } = expr
    {
        let a = ev.operand(ev.eval(left));
        let b = ev.operand(ev.eval(right));
        if let (CalcValue::Scalar(x), CalcValue::Scalar(y)) = (&a, &b) {
            let result = super::operators::apply(*op, x, y, ev.system());
            if let (Value::Number(r), Ok(x), Ok(y)) = (
                &result,
                to_number(x, ev.system()),
                to_number(y, ev.system()),
            ) {
                return CalcValue::Scalar(Value::Number(super::operators::snap_to_zero(*r, x, y)));
            }
            return CalcValue::Scalar(result);
        }
        return ev.eval(expr);
    }
    ev.eval(expr)
}

/// a result as cell values: one value, a fixed array formula's range,
/// or a spill into the cells beside it
fn place(ev: &Evaluator, key: CellKey, parsed: &ParsedFormula, result: CalcValue) -> Value {
    if let Some(range) = parsed.array_range {
        let array = as_array(ev, result);
        return fill_range(ev, key, range, &array);
    }
    if parsed.legacy {
        return top_value(ev.intersect(result));
    }
    match result {
        CalcValue::Scalar(value) => top_value(value),
        CalcValue::Ref(areas) if areas.len() == 1 && areas[0].range.cell_count() == 1 => {
            top_value(ev.intersect(CalcValue::Ref(areas)))
        }
        other => spill(ev, key, &as_array(ev, other)),
    }
}

/// a formula showing a blank cell shows 0, as in excel
fn top_value(value: Value) -> Value {
    if value.is_blank() {
        Value::Number(0.0)
    } else {
        value
    }
}

fn as_array(ev: &Evaluator, value: CalcValue) -> Array {
    match value {
        CalcValue::Array(array) => array,
        CalcValue::Scalar(value) => Array::new(1, 1, vec![value]),
        CalcValue::Ref(areas) if areas.len() == 1 => ev.area_array(areas[0]),
        CalcValue::Ref(_) => Array::new(1, 1, vec![Value::Error(ErrorValue::Value)]),
    }
}

fn fill_range(ev: &Evaluator, key: CellKey, range: Range, array: &Array) -> Value {
    let mut state = ev.ctx.state.borrow_mut();
    for cell in range.iter() {
        let r = (cell.row - range.start.row) as usize;
        let c = (cell.col - range.start.col) as usize;
        if (cell.row, cell.col) != (key.1, key.2) {
            state.spill_values.insert(
                (key.0, cell.row, cell.col),
                top_value(array.broadcast(r, c)),
            );
        }
    }
    state.new_spills.insert(key, range);
    top_value(array.broadcast(0, 0))
}

fn spill(ev: &Evaluator, key: CellKey, array: &Array) -> Value {
    if array.items.is_empty() {
        return Value::Error(ErrorValue::Calc);
    }
    if array.rows == 1 && array.cols == 1 {
        return top_value(array.items[0].clone());
    }
    let last_row = u64::from(key.1) + array.rows as u64 - 1;
    let last_col = u64::from(key.2) + array.cols as u64 - 1;
    if last_row >= u64::from(MAX_ROWS) || last_col >= u64::from(MAX_COLS) {
        return Value::Error(ErrorValue::Spill);
    }
    let range = Range::from_bounds(key.1, key.2, last_row as u32, last_col as u32);
    if !spill_room(ev, key, range) {
        ev.ctx.state.borrow_mut().blocked_spills.insert(key, range);
        return Value::Error(ErrorValue::Spill);
    }
    fill_range(ev, key, range, array)
}

/// a spill needs every cell it covers (except its own) to be empty, or
/// filled by its own previous spill
fn spill_room(ev: &Evaluator, key: CellKey, range: Range) -> bool {
    let sheet = &ev.ctx.book.sheets[key.0];
    let own = ev.ctx.old_spills.get(&key);
    let state = ev.ctx.state.borrow();
    let taken_by_other = |cell: CellRef| {
        state
            .new_spills
            .iter()
            .any(|(anchor, other)| *anchor != key && anchor.0 == key.0 && other.contains(cell))
    };
    for cell in range.iter() {
        if (cell.row, cell.col) == (key.1, key.2) {
            continue;
        }
        let ours = own.is_some_and(|own| own.contains(cell));
        let occupied = sheet
            .cells
            .get(cell.row, cell.col)
            .is_some_and(|c| c.formula.is_some() || (!c.value.is_blank() && !ours));
        if occupied || taken_by_other(cell) {
            return false;
        }
    }
    true
}

/// calculates every formula of a workbook once.
pub fn recalculate(book: &mut Workbook) -> CalcReport {
    let mut engine = Engine::new(book);
    engine.recalculate_all(book)
}
