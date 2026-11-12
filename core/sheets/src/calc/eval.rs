//! evaluating a formula tree in the context of one cell: references,
//! names, table references, operators with excel's coercion and array
//! broadcasting, and calculating a cell on demand when a formula needs
//! one that has not been calculated yet in this run.

use crate::address::{CellRef, Range};
use crate::formula::{Area as RefArea, BinaryOp, Expr, Reference, SheetRef, UnaryOp};
use crate::model::{DateSystem, ErrorValue, Value};

use super::context::{CellKey, Context};
use super::value::*;

/// formulas calculated on demand nest this deep at most; deeper chains
/// are calculated in order by the engine instead, so the stack is safe
const MAX_DEMAND_DEPTH: usize = 400;

pub struct Evaluator<'c, 'a> {
    pub ctx: &'c Context<'a>,
    pub sheet: usize,
    pub row: u32,
    pub col: u32,
    pub legacy: bool,
    pub depth: usize,
}

impl<'c, 'a> Evaluator<'c, 'a> {
    pub fn system(&self) -> DateSystem {
        self.ctx.book.date_system
    }

    pub fn eval(&self, expr: &Expr) -> CalcValue {
        match expr {
            Expr::Number { value } => CalcValue::number(*value),
            Expr::Text { value } => CalcValue::text(value.clone()),
            Expr::Bool { value } => CalcValue::boolean(*value),
            Expr::Error { value } => CalcValue::error(*value),
            Expr::Missing => CalcValue::Scalar(Value::Blank),
            Expr::Paren { expr } => self.eval(expr),
            Expr::Ref { reference } => self.reference(reference),
            Expr::Name { name } => self.name(name),
            Expr::Structured { reference } => match super::structured::resolve(self, reference) {
                Some(area) => CalcValue::Ref(vec![area]),
                None => CalcValue::error(ErrorValue::Ref),
            },
            Expr::Function { name, args } => super::functions::call(self, name, args),
            Expr::Unary { op, expr } => self.unary(*op, expr),
            Expr::Binary { op, left, right } => self.binary(*op, left, right),
            Expr::Array { rows } => self.array_constant(rows),
        }
    }

    fn array_constant(&self, rows: &[Vec<Expr>]) -> CalcValue {
        let cols = rows.first().map(Vec::len).unwrap_or(0);
        let items = rows
            .iter()
            .flatten()
            .map(|item| self.scalar(item))
            .collect();
        CalcValue::Array(Array::new(rows.len(), cols, items))
    }

    /// the sheet index a reference names; none for unknown or external sheets
    fn sheet_of(&self, sheet: &Option<SheetRef>) -> Option<usize> {
        match sheet {
            None => Some(self.sheet),
            Some(SheetRef::One { name }) => self.ctx.book.sheet_index(name),
            Some(SheetRef::Span { first, .. }) => self.ctx.book.sheet_index(first),
            Some(SheetRef::External { .. }) => None,
        }
    }

    fn reference(&self, reference: &Reference) -> CalcValue {
        if let Some(SheetRef::Span { first, last }) = &reference.sheet {
            return self.three_d(first, last, &reference.area);
        }
        match self.sheet_of(&reference.sheet) {
            Some(sheet) => CalcValue::Ref(vec![Area {
                sheet,
                range: reference.area.to_range(),
            }]),
            None => CalcValue::error(ErrorValue::Ref),
        }
    }

    /// "Jan:Mar!A1" is the same cells on every sheet from Jan to Mar
    fn three_d(&self, first: &str, last: &str, area: &RefArea) -> CalcValue {
        let book = self.ctx.book;
        let (Some(a), Some(b)) = (book.sheet_index(first), book.sheet_index(last)) else {
            return CalcValue::error(ErrorValue::Ref);
        };
        let range = area.to_range();
        let areas = (a.min(b)..=a.max(b))
            .map(|sheet| Area { sheet, range })
            .collect();
        CalcValue::Ref(areas)
    }

    fn name(&self, name: &str) -> CalcValue {
        let parsed = self.parsed_name(name);
        if let Some(expr) = parsed {
            if self.depth > 32 {
                // a name defined through itself
                return CalcValue::error(ErrorValue::Ref);
            }
            let inner = Evaluator {
                depth: self.depth + 1,
                ..*self
            };
            return inner.eval(&expr);
        }
        if let Some((sheet, table)) = self.ctx.book.find_table(name) {
            if let Some(range) = table.data_range() {
                return CalcValue::Ref(vec![Area { sheet, range }]);
            }
        }
        CalcValue::error(ErrorValue::Name)
    }

    /// a defined name's formula, parsed once per run
    pub fn parsed_name(&self, name: &str) -> Option<Expr> {
        let key = format!("{}\u{0}{name}", self.sheet).to_ascii_lowercase();
        if let Some(cached) = self.ctx.names.borrow().get(&key) {
            return cached.clone();
        }
        let local_name = name.rsplit_once('!').map(|(_, n)| n).unwrap_or(name);
        let defined = self.ctx.book.find_name(local_name, Some(self.sheet));
        let parsed = defined.and_then(|d| crate::formula::parse(&d.formula).ok());
        self.ctx.names.borrow_mut().insert(key, parsed.clone());
        parsed
    }

    fn unary(&self, op: UnaryOp, inner: &Expr) -> CalcValue {
        match op {
            UnaryOp::At => CalcValue::Scalar(self.intersect(self.eval(inner))),
            UnaryOp::Spill => self.spill_reference(inner),
            UnaryOp::Plus => self.eval(inner),
            UnaryOp::Minus => {
                self.map_operand(self.eval(inner), &|v| match to_number(v, self.system()) {
                    Ok(n) => number_or_error(-n),
                    Err(e) => Value::Error(e),
                })
            }
            UnaryOp::Percent => {
                self.map_operand(self.eval(inner), &|v| match to_number(v, self.system()) {
                    Ok(n) => number_or_error(n / 100.0),
                    Err(e) => Value::Error(e),
                })
            }
        }
    }

    /// "A1#": the range a formula in A1 spilled into
    fn spill_reference(&self, inner: &Expr) -> CalcValue {
        let Expr::Ref { reference } = inner else {
            return CalcValue::error(ErrorValue::Ref);
        };
        let Some(sheet) = self.sheet_of(&reference.sheet) else {
            return CalcValue::error(ErrorValue::Ref);
        };
        let at = reference.area.to_range().start;
        let key = (sheet, at.row, at.col);
        // make sure the anchor has been calculated in this run
        self.cell_value(sheet, at.row, at.col);
        let state = self.ctx.state.borrow();
        let range = state
            .new_spills
            .get(&key)
            .or_else(|| self.ctx.old_spills.get(&key))
            .copied();
        match range {
            Some(range) => CalcValue::Ref(vec![Area { sheet, range }]),
            None => CalcValue::Ref(vec![Area {
                sheet,
                range: Range::cell(at),
            }]),
        }
    }

    /// applies `f` to a value or to every item of an array or range
    fn map_operand(&self, value: CalcValue, f: &dyn Fn(&Value) -> Value) -> CalcValue {
        match self.operand(value) {
            CalcValue::Array(array) => {
                let items = array.items.iter().map(f).collect();
                CalcValue::Array(Array::new(array.rows, array.cols, items))
            }
            CalcValue::Scalar(v) => CalcValue::Scalar(f(&v)),
            CalcValue::Ref(_) => CalcValue::error(ErrorValue::Value),
        }
    }

    /// a reference turned into what an operator works on: one cell's
    /// value, the cell in line with the formula (legacy files), or the
    /// whole range as an array
    pub fn operand(&self, value: CalcValue) -> CalcValue {
        match value {
            CalcValue::Ref(areas) if areas.len() == 1 && areas[0].range.cell_count() == 1 => {
                let at = areas[0].range.start;
                CalcValue::Scalar(self.cell_value(areas[0].sheet, at.row, at.col))
            }
            CalcValue::Ref(areas) if areas.len() == 1 => {
                if self.legacy {
                    CalcValue::Scalar(self.intersect(CalcValue::Ref(areas)))
                } else {
                    CalcValue::Array(self.area_array(areas[0]))
                }
            }
            CalcValue::Ref(_) => CalcValue::error(ErrorValue::Value),
            other => other,
        }
    }

    fn binary(&self, op: BinaryOp, left: &Expr, right: &Expr) -> CalcValue {
        match op {
            BinaryOp::Range => return self.range_operator(left, right),
            BinaryOp::Intersect => return self.intersect_operator(left, right),
            BinaryOp::Union => return self.union_operator(left, right),
            _ => {}
        }
        let a = self.operand(self.eval(left));
        let b = self.operand(self.eval(right));
        match (a, b) {
            (CalcValue::Scalar(x), CalcValue::Scalar(y)) => {
                CalcValue::Scalar(super::operators::apply(op, &x, &y, self.system()))
            }
            (x, y) => self.broadcast(op, x, y),
        }
    }

    fn broadcast(&self, op: BinaryOp, a: CalcValue, b: CalcValue) -> CalcValue {
        let as_array = |v: CalcValue| match v {
            CalcValue::Array(array) => array,
            CalcValue::Scalar(value) => Array::new(1, 1, vec![value]),
            CalcValue::Ref(_) => Array::new(1, 1, vec![Value::Error(ErrorValue::Value)]),
        };
        let (a, b) = (as_array(a), as_array(b));
        let rows = a.rows.max(b.rows);
        let cols = a.cols.max(b.cols);
        let mut items = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                items.push(super::operators::apply(
                    op,
                    &a.broadcast(r, c),
                    &b.broadcast(r, c),
                    self.system(),
                ));
            }
        }
        CalcValue::Array(Array::new(rows, cols, items))
    }

    fn areas_of(&self, expr: &Expr) -> Result<Vec<Area>, ErrorValue> {
        match self.eval(expr) {
            CalcValue::Ref(areas) => Ok(areas),
            CalcValue::Scalar(Value::Error(e)) => Err(e),
            _ => Err(ErrorValue::Value),
        }
    }

    /// "A1:B2:C5", "A1:INDEX(...)": the smallest range holding both
    fn range_operator(&self, left: &Expr, right: &Expr) -> CalcValue {
        let (a, b) = match (self.areas_of(left), self.areas_of(right)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return CalcValue::error(e),
        };
        let all: Vec<&Area> = a.iter().chain(&b).collect();
        let sheet = all[0].sheet;
        if all.iter().any(|area| area.sheet != sheet) {
            return CalcValue::error(ErrorValue::Value);
        }
        let mut bounds = all[0].range;
        for area in &all[1..] {
            bounds = Range::new(
                CellRef::new(
                    bounds.start.row.min(area.range.start.row),
                    bounds.start.col.min(area.range.start.col),
                ),
                CellRef::new(
                    bounds.end.row.max(area.range.end.row),
                    bounds.end.col.max(area.range.end.col),
                ),
            );
        }
        CalcValue::Ref(vec![Area {
            sheet,
            range: bounds,
        }])
    }

    fn intersect_operator(&self, left: &Expr, right: &Expr) -> CalcValue {
        let (a, b) = match (self.areas_of(left), self.areas_of(right)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return CalcValue::error(e),
        };
        let mut found = Vec::new();
        for x in &a {
            for y in b.iter().filter(|y| y.sheet == x.sheet) {
                if let Some(range) = x.range.intersection(&y.range) {
                    found.push(Area {
                        sheet: x.sheet,
                        range,
                    });
                }
            }
        }
        if found.is_empty() {
            CalcValue::error(ErrorValue::Null)
        } else {
            CalcValue::Ref(found)
        }
    }

    fn union_operator(&self, left: &Expr, right: &Expr) -> CalcValue {
        match (self.areas_of(left), self.areas_of(right)) {
            (Ok(mut a), Ok(b)) => {
                a.extend(b);
                CalcValue::Ref(a)
            }
            (Err(e), _) | (_, Err(e)) => CalcValue::error(e),
        }
    }

    /// one value from anything: a single cell, the cell in line with the
    /// formula for a range (implicit intersection), an array's first item
    pub fn intersect(&self, value: CalcValue) -> Value {
        match value {
            CalcValue::Scalar(v) => v,
            CalcValue::Array(array) if array.items.is_empty() => Value::Error(ErrorValue::Value),
            CalcValue::Array(array) => array.items[0].clone(),
            CalcValue::Ref(areas) if areas.len() != 1 => Value::Error(ErrorValue::Value),
            CalcValue::Ref(areas) => {
                let area = areas[0];
                let range = area.range;
                if range.cell_count() == 1 {
                    return self.cell_value(area.sheet, range.start.row, range.start.col);
                }
                if range.cols() == 1 && (range.start.row..=range.end.row).contains(&self.row) {
                    return self.cell_value(area.sheet, self.row, range.start.col);
                }
                if range.rows() == 1 && (range.start.col..=range.end.col).contains(&self.col) {
                    return self.cell_value(area.sheet, range.start.row, self.col);
                }
                Value::Error(ErrorValue::Value)
            }
        }
    }

    /// one value for a function argument
    pub fn scalar(&self, expr: &Expr) -> Value {
        self.intersect(self.eval(expr))
    }

    /// a cell's current value, calculating it first when it is a formula
    /// still waiting in this run
    pub fn cell_value(&self, sheet: usize, row: u32, col: u32) -> Value {
        let key: CellKey = (sheet, row, col);
        {
            let state = self.ctx.state.borrow();
            if let Some(value) = state.results.get(&key) {
                return value.clone();
            }
            if let Some(value) = state.spill_values.get(&key) {
                return value.clone();
            }
        }
        let pending = self.ctx.state.borrow().pending.contains(&key);
        if pending {
            return calculate_on_demand(self.ctx, key, self.depth + 1);
        }
        if let Some(anchor) = self.pending_spill_anchor(key) {
            calculate_on_demand(self.ctx, anchor, self.depth + 1);
            if let Some(value) = self.ctx.state.borrow().spill_values.get(&key) {
                return value.clone();
            }
            return Value::Blank;
        }
        let in_old_spill = self.ctx.old_spills.iter().any(|(anchor, range)| {
            anchor.0 == sheet && *anchor != key && range.contains(CellRef::new(row, col))
        });
        if in_old_spill {
            // the formula that filled this cell no longer spills here
            return Value::Blank;
        }
        match self.ctx.book.sheets.get(sheet) {
            Some(s) => s.cells.value(row, col).clone(),
            None => Value::Error(ErrorValue::Ref),
        }
    }

    fn pending_spill_anchor(&self, key: CellKey) -> Option<CellKey> {
        if self.ctx.old_spills.is_empty() {
            return None;
        }
        let state = self.ctx.state.borrow();
        self.ctx
            .old_spills
            .iter()
            .find(|(anchor, range)| {
                anchor.0 == key.0
                    && range.contains(CellRef::new(key.1, key.2))
                    && state.pending.contains(anchor)
            })
            .map(|(anchor, _)| *anchor)
    }

    /// a range cut at the sheet's last used row and column, so whole
    /// column references cost only the cells that exist
    pub fn trimmed(&self, area: Area) -> Area {
        let Some(used) = self.ctx.used_range(area.sheet) else {
            return Area {
                sheet: area.sheet,
                range: Range::cell(area.range.start),
            };
        };
        let end_row = area
            .range
            .end
            .row
            .min(used.end.row.max(area.range.start.row));
        let end_col = area
            .range
            .end
            .col
            .min(used.end.col.max(area.range.start.col));
        Area {
            sheet: area.sheet,
            range: Range::from_bounds(area.range.start.row, area.range.start.col, end_row, end_col),
        }
    }

    /// every value of a range, row by row (whole columns trimmed)
    pub fn area_array(&self, area: Area) -> Array {
        let area = if area.range.is_whole_columns() || area.range.is_whole_rows() {
            self.trimmed(area)
        } else {
            area
        };
        let range = area.range;
        let mut items = Vec::with_capacity(range.cell_count().min(10_000_000) as usize);
        for row in range.start.row..=range.end.row {
            for col in range.start.col..=range.end.col {
                items.push(self.cell_value(area.sheet, row, col));
            }
        }
        Array::new(range.rows() as usize, range.cols() as usize, items)
    }

    /// a cell's value without copying it when it comes straight from the
    /// workbook: scanning 200,000 text cells for COUNTIF or VLOOKUP then
    /// copies nothing
    pub fn value_cow(&self, sheet: usize, row: u32, col: u32) -> std::borrow::Cow<'a, Value> {
        let book: &'a crate::model::Workbook = self.ctx.book;
        let Some(target) = book.sheets.get(sheet) else {
            return std::borrow::Cow::Owned(Value::Error(ErrorValue::Ref));
        };
        let cell = target.cells.get(row, col);
        let no_spills =
            self.ctx.old_spills.is_empty() && self.ctx.state.borrow().spill_values.is_empty();
        // a formula cell not touched by this run still holds its value
        let settled = match cell {
            Some(c) if c.formula.is_some() => {
                let state = self.ctx.state.borrow();
                let key = (sheet, row, col);
                !state.pending.contains(&key) && !state.results.contains_key(&key)
            }
            _ => true,
        };
        let plain = no_spills && settled;
        match cell {
            Some(cell) if plain => std::borrow::Cow::Borrowed(&cell.value),
            None if plain => std::borrow::Cow::Owned(Value::Blank),
            _ => std::borrow::Cow::Owned(self.cell_value(sheet, row, col)),
        }
    }

    /// every value of a range (whole columns cut at the used range), row
    /// by row, borrowed where possible
    pub fn area_values(&self, area: Area) -> (Vec<std::borrow::Cow<'a, Value>>, usize, usize) {
        let area = if area.range.is_whole_columns() || area.range.is_whole_rows() {
            self.trimmed(area)
        } else {
            area
        };
        let range = area.range;
        let mut values = Vec::with_capacity(range.cell_count().min(10_000_000) as usize);
        for row in range.start.row..=range.end.row {
            for col in range.start.col..=range.end.col {
                values.push(self.value_cow(area.sheet, row, col));
            }
        }
        (values, range.rows() as usize, range.cols() as usize)
    }

    /// calls `visit` with every value in a range that is not blank, in
    /// row order; fast on large sparse ranges
    pub fn for_each_value(&self, area: Area, visit: &mut dyn FnMut(u32, u32, &Value)) {
        let Some(sheet) = self.ctx.book.sheets.get(area.sheet) else {
            return;
        };
        let formula_cells: Vec<(u32, u32)> = sheet
            .cells
            .iter_range(area.range)
            .filter(|(_, _, cell)| cell.formula.is_some())
            .map(|(r, c, _)| (r, c))
            .collect();
        // formula cells first make sure they are calculated
        for (r, c) in &formula_cells {
            self.cell_value(area.sheet, *r, *c);
        }
        let spills = self.spilled_into(area);
        let state = self.ctx.state.borrow();
        for (r, c, cell) in sheet.cells.iter_range(area.range) {
            // only formula cells can have a newer value in this run
            let newer = if cell.formula.is_some() {
                state.results.get(&(area.sheet, r, c))
            } else {
                None
            };
            let value = newer.unwrap_or(&cell.value);
            if !value.is_blank() {
                visit(r, c, value);
            }
        }
        for (key, value) in spills {
            let exists = sheet.cells.get(key.1, key.2).is_some();
            if !exists && !value.is_blank() {
                visit(key.1, key.2, &value);
            }
        }
    }

    /// values spilled this run into cells of the range
    fn spilled_into(&self, area: Area) -> Vec<(CellKey, Value)> {
        let state = self.ctx.state.borrow();
        if state.spill_values.is_empty() {
            return Vec::new();
        }
        let low = (area.sheet, area.range.start.row, 0);
        let high = (area.sheet, area.range.end.row, u32::MAX);
        state
            .spill_values
            .range(low..=high)
            .filter(|(key, _)| (area.range.start.col..=area.range.end.col).contains(&key.2))
            .map(|(key, value)| (*key, value.clone()))
            .collect()
    }
}

/// calculates one formula cell now, because another formula needs it
pub fn calculate_on_demand(ctx: &Context, key: CellKey, depth: usize) -> Value {
    let looping = ctx.state.borrow().in_progress.contains(&key);
    if looping {
        let mut state = ctx.state.borrow_mut();
        if !state.circular.contains(&key) {
            state.circular.push(key);
        }
        // excel shows 0 in a cell that is part of a circular reference
        return Value::Number(0.0);
    }
    if depth > MAX_DEMAND_DEPTH {
        return ctx.book.value(key.0, key.1, key.2);
    }
    super::engine::calculate_cell(ctx, key, depth)
}
