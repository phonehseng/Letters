//! functions that work with references and positions: INDEX, CHOOSE,
//! OFFSET, INDIRECT, ROW(S), COLUMN(S) and TRANSPOSE. INDEX and OFFSET
//! give references, so they can be used inside ranges (A1:INDEX(...)).

use crate::address::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::eval::Evaluator;
use super::super::value::{Area, Array, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &[
    "INDEX",
    "CHOOSE",
    "OFFSET",
    "INDIRECT",
    "ROW",
    "ROWS",
    "COLUMN",
    "COLUMNS",
    "TRANSPOSE",
];

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    Some(match name {
        "INDEX" => index(ev, args),
        "CHOOSE" => choose(ev, args),
        "OFFSET" => offset(ev, args),
        "INDIRECT" => indirect(ev, args),
        "ROW" | "COLUMN" => position(ev, args, name == "ROW"),
        "ROWS" | "COLUMNS" => done_number(size(ev, args, name == "ROWS")),
        "TRANSPOSE" => match grid(ev, args.first().unwrap_or(&Expr::Missing)) {
            Ok(array) => CalcValue::Array(array.transpose()),
            Err(e) => CalcValue::error(e),
        },
        _ => return None,
    })
}

/// INDEX(array or range, row, [column]); on a range it gives a reference,
/// so A1:INDEX(B:B,5) works. 0 for row or column means all of them
fn index(ev: &Evaluator, args: &[Expr]) -> CalcValue {
    if let Err(e) = arity(args, 2, 4) {
        return CalcValue::error(e);
    }
    let (row, col) = match (integer(ev, &args[1]), integer_or(ev, args, 2, -1)) {
        (Ok(r), Ok(c)) => (r, c),
        (Err(e), _) | (_, Err(e)) => return CalcValue::error(e),
    };
    match ev.eval(&args[0]) {
        CalcValue::Ref(areas) => {
            let area_number = integer_or(ev, args, 3, 1).unwrap_or(1);
            match areas.get((area_number - 1).max(0) as usize) {
                Some(area) => index_area(*area, row, col),
                None => CalcValue::error(ErrorValue::Ref),
            }
        }
        CalcValue::Array(array) => index_array(&array, row, col),
        CalcValue::Scalar(Value::Error(e)) => CalcValue::error(e),
        CalcValue::Scalar(value) => index_array(&Array::new(1, 1, vec![value]), row, col),
    }
}

/// (row, col) of INDEX on a shape: a single row or column takes one
/// number as the position along it
fn index_position(rows: u32, cols: u32, row: i64, col: i64) -> Option<(Option<u32>, Option<u32>)> {
    let (row, col) = if col < 0 && rows == 1 && cols > 1 {
        (1, row)
    } else {
        (row, col.max(0))
    };
    if row < 0 || col < 0 || row > i64::from(rows) || col > i64::from(cols) {
        return None;
    }
    let pick = |n: i64| (n > 0).then(|| (n - 1) as u32);
    let col = if cols == 1 && col == 0 {
        Some(0)
    } else {
        pick(col)
    };
    let row = if rows == 1 && row == 0 {
        Some(0)
    } else {
        pick(row)
    };
    Some((row, col))
}

fn index_area(area: Area, row: i64, col: i64) -> CalcValue {
    let range = area.range;
    let Some((r, c)) = index_position(range.rows(), range.cols(), row, col) else {
        return CalcValue::error(ErrorValue::Ref);
    };
    let (r1, r2) = match r {
        Some(r) => (range.start.row + r, range.start.row + r),
        None => (range.start.row, range.end.row),
    };
    let (c1, c2) = match c {
        Some(c) => (range.start.col + c, range.start.col + c),
        None => (range.start.col, range.end.col),
    };
    CalcValue::Ref(vec![Area {
        sheet: area.sheet,
        range: Range::from_bounds(r1, c1, r2, c2),
    }])
}

fn index_array(array: &Array, row: i64, col: i64) -> CalcValue {
    let Some((r, c)) = index_position(array.rows as u32, array.cols as u32, row, col) else {
        return CalcValue::error(ErrorValue::Ref);
    };
    match (r, c) {
        (Some(r), Some(c)) => CalcValue::Scalar(array.get(r as usize, c as usize).clone()),
        (Some(r), None) => CalcValue::Array(Array::new(
            1,
            array.cols,
            array.rows_iter().nth(r as usize).unwrap_or(&[]).to_vec(),
        )),
        (None, Some(c)) => CalcValue::Array(Array::column(
            (0..array.rows)
                .map(|r| array.get(r, c as usize).clone())
                .collect(),
        )),
        (None, None) => CalcValue::Array(array.clone()),
    }
}

fn choose(ev: &Evaluator, args: &[Expr]) -> CalcValue {
    if args.len() < 2 {
        return CalcValue::error(ErrorValue::Value);
    }
    match integer(ev, &args[0]) {
        Ok(n) if n >= 1 && (n as usize) < args.len() => ev.eval(&args[n as usize]),
        Ok(_) => CalcValue::error(ErrorValue::Value),
        Err(e) => CalcValue::error(e),
    }
}

/// OFFSET(reference, rows, cols, [height], [width])
fn offset(ev: &Evaluator, args: &[Expr]) -> CalcValue {
    let attempt = || -> Outcome<CalcValue> {
        arity(args, 3, 5)?;
        let base = area(ev, &args[0])?;
        let rows = integer(ev, &args[1])?;
        let cols = integer(ev, &args[2])?;
        let height = integer_or(ev, args, 3, i64::from(base.range.rows()))?;
        let width = integer_or(ev, args, 4, i64::from(base.range.cols()))?;
        if height < 1 || width < 1 {
            return Err(ErrorValue::Ref);
        }
        let top = i64::from(base.range.start.row) + rows;
        let left = i64::from(base.range.start.col) + cols;
        let bottom = top + height - 1;
        let right = left + width - 1;
        if top < 0 || left < 0 || bottom >= i64::from(MAX_ROWS) || right >= i64::from(MAX_COLS) {
            return Err(ErrorValue::Ref);
        }
        let range = Range::from_bounds(top as u32, left as u32, bottom as u32, right as u32);
        Ok(CalcValue::Ref(vec![Area {
            sheet: base.sheet,
            range,
        }]))
    };
    attempt().unwrap_or_else(CalcValue::error)
}

/// INDIRECT("Sheet2!B3") and INDIRECT("R2C3", FALSE)
fn indirect(ev: &Evaluator, args: &[Expr]) -> CalcValue {
    let attempt = || -> Outcome<CalcValue> {
        arity(args, 1, 2)?;
        let text = text(ev, &args[0])?;
        let a1 = boolean_or(ev, args, 1, true)?;
        let parsed = if a1 {
            crate::formula::parse(&text)
        } else {
            crate::formula::parse_r1c1(&text, CellRef::new(ev.row, ev.col))
        };
        let expr = parsed.map_err(|_| ErrorValue::Ref)?;
        match &expr {
            Expr::Ref { .. }
            | Expr::Name { .. }
            | Expr::Binary { .. }
            | Expr::Structured { .. } => {}
            _ => return Err(ErrorValue::Ref),
        }
        match ev.eval(&expr) {
            value @ CalcValue::Ref(_) => Ok(value),
            _ => Err(ErrorValue::Ref),
        }
    };
    attempt().unwrap_or_else(CalcValue::error)
}

/// ROW and COLUMN: of the formula's own cell, or of a reference (one
/// number per row or column in excel 365)
fn position(ev: &Evaluator, args: &[Expr], rows: bool) -> CalcValue {
    let Some(arg) = args.first().filter(|a| !matches!(a, Expr::Missing)) else {
        return CalcValue::number(f64::from(if rows { ev.row } else { ev.col }) + 1.0);
    };
    let area = match area(ev, arg) {
        Ok(area) => area,
        Err(e) => return CalcValue::error(e),
    };
    let range = area.range;
    let (first, last) = if rows {
        (range.start.row, range.end.row)
    } else {
        (range.start.col, range.end.col)
    };
    if first == last || ev.legacy {
        return CalcValue::number(f64::from(first) + 1.0);
    }
    let items: Vec<Value> = (first..=last)
        .map(|n| Value::Number(f64::from(n) + 1.0))
        .collect();
    if rows {
        CalcValue::Array(Array::column(items))
    } else {
        CalcValue::Array(Array::new(1, items.len(), items))
    }
}

fn size(ev: &Evaluator, args: &[Expr], rows: bool) -> Outcome<f64> {
    arity(args, 1, 1)?;
    let (r, c) = match ev.eval(&args[0]) {
        CalcValue::Ref(areas) if areas.len() == 1 => (
            areas[0].range.rows() as usize,
            areas[0].range.cols() as usize,
        ),
        CalcValue::Array(array) => (array.rows, array.cols),
        CalcValue::Scalar(Value::Error(e)) => return Err(e),
        CalcValue::Scalar(_) => (1, 1),
        CalcValue::Ref(_) => return Err(ErrorValue::Ref),
    };
    Ok(if rows { r as f64 } else { c as f64 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_positions() {
        assert_eq!(index_position(3, 1, 2, -1), Some((Some(1), Some(0))));
        assert_eq!(index_position(1, 3, 2, -1), Some((Some(0), Some(1))));
        assert_eq!(index_position(3, 3, 0, 2), Some((None, Some(1))));
        assert_eq!(index_position(3, 3, 4, 1), None);
    }
}
