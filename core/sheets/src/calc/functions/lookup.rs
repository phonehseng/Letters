//! looking things up: VLOOKUP, HLOOKUP, XLOOKUP, MATCH, XMATCH and
//! LOOKUP. INDEX, OFFSET and the other functions that work with
//! references are in references.rs.

use std::borrow::Borrow;
use std::cmp::Ordering;

use crate::address::Range;
use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::criteria::wildcard_match;
use super::super::eval::Evaluator;
use super::super::value::{compare, Area, Array, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &["VLOOKUP", "HLOOKUP", "XLOOKUP", "MATCH", "XMATCH", "LOOKUP"];

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    Some(match name {
        "VLOOKUP" => done(table_lookup(ev, args, false)),
        "HLOOKUP" => done(table_lookup(ev, args, true)),
        "LOOKUP" => done(vector_lookup(ev, args)),
        "XLOOKUP" => xlookup(ev, args),
        "MATCH" => done_number(match_function(ev, args)),
        "XMATCH" => done_number(xmatch(ev, args)),

        _ => return None,
    })
}

/// how a lookup compares
#[derive(Clone, Copy, PartialEq)]
pub enum Matching {
    Exact,
    /// "*" and "?" in the looked-up text are wildcards
    Wildcard,
    /// exact, else the next smaller item
    NextSmaller,
    /// exact, else the next larger item
    NextLarger,
}

fn same_kind(a: &Value, b: &Value) -> bool {
    let rank = |v: &Value| match v {
        Value::Number(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Error(_) => 3,
        Value::Blank => 4,
    };
    rank(a) == rank(b)
}

fn equal(target: &Value, item: &Value, wildcards: bool) -> bool {
    if let (true, Value::Text(pattern), Value::Text(text)) = (wildcards, target, item) {
        return wildcard_match(pattern, text);
    }
    same_kind(target, item) && compare(target, item) == Ordering::Equal
}

/// the position of `target` among `items`, searching from the front or
/// (reverse) the back
pub fn find<I: Borrow<Value>>(
    target: &Value,
    items: &[I],
    matching: Matching,
    reverse: bool,
) -> Option<usize> {
    let wildcards = matching == Matching::Wildcard;
    let order: Box<dyn Iterator<Item = usize>> = if reverse {
        Box::new((0..items.len()).rev())
    } else {
        Box::new(0..items.len())
    };
    let mut best: Option<usize> = None;
    for index in order {
        let item = items[index].borrow();
        if equal(target, item, wildcards) {
            return Some(index);
        }
        // "next smaller" and "next larger" compare across kinds in excel's
        // order (numbers, text, logical): XLOOKUP("x",numbers,,,-1) finds
        // the largest number (libreoffice finds nothing)
        let closer = |current: &Value| match matching {
            Matching::NextSmaller => {
                compare(item, target).is_lt() && compare(item, current).is_gt()
            }
            Matching::NextLarger => compare(item, target).is_gt() && compare(item, current).is_lt(),
            _ => false,
        };
        let wanted = match matching {
            Matching::NextSmaller => compare(item, target).is_lt(),
            Matching::NextLarger => compare(item, target).is_gt(),
            _ => false,
        };
        if wanted && best.is_none_or(|b| closer(items[b].borrow())) {
            best = Some(index);
        }
    }
    best
}

/// approximate match on sorted items: the last item not greater than the
/// target (ascending) or not smaller (descending), found by halving as
/// excel does. items of another kind than the target are passed over.
pub fn sorted_find<I: Borrow<Value>>(
    target: &Value,
    items: &[I],
    descending: bool,
) -> Option<usize> {
    let candidates: Vec<usize> = (0..items.len())
        .filter(|i| same_kind(target, items[*i].borrow()))
        .collect();
    let fits = |index: usize| {
        let order = compare(items[index].borrow(), target);
        if descending {
            order != Ordering::Less
        } else {
            order != Ordering::Greater
        }
    };
    let count = candidates.partition_point(|index| fits(*index));
    count.checked_sub(1).map(|last| candidates[last])
}

/// VLOOKUP and HLOOKUP
fn table_lookup(ev: &Evaluator, args: &[Expr], horizontal: bool) -> Outcome<Value> {
    arity(args, 3, 4)?;
    let target = ev.scalar(&args[0]);
    if let Value::Error(e) = target {
        return Err(e);
    }
    let column = integer(ev, &args[2])?;
    let approximate = boolean_or(ev, args, 3, true)?;
    if column < 1 {
        return Err(ErrorValue::Value);
    }
    if let CalcValue::Ref(areas) = ev.eval(&args[1]) {
        if areas.len() == 1 {
            return range_lookup(
                ev,
                &target,
                areas[0],
                column as u32,
                approximate,
                horizontal,
            );
        }
    }
    let table = grid(ev, &args[1])?;
    let table = if horizontal { table.transpose() } else { table };
    if column as usize > table.cols {
        return Err(ErrorValue::Ref);
    }
    let keys: Vec<Value> = (0..table.rows).map(|r| table.get(r, 0).clone()).collect();
    let found = if approximate {
        sorted_find(&target, &keys, false)
    } else {
        find(&target, &keys, Matching::Wildcard, false)
    };
    let row = found.ok_or(ErrorValue::NA)?;
    Ok(table.get(row, column as usize - 1).clone())
}

/// VLOOKUP on a range reads only the key column (borrowed, not copied)
/// and then the one result cell, so a lookup in B:D of 200,000 rows
/// touches 200,001 cells, not 600,000
fn range_lookup(
    ev: &Evaluator,
    target: &Value,
    area: Area,
    column: u32,
    approximate: bool,
    horizontal: bool,
) -> Outcome<Value> {
    let width = if horizontal {
        area.range.rows()
    } else {
        area.range.cols()
    };
    if column > width {
        return Err(ErrorValue::Ref);
    }
    let start = area.range.start;
    let keys_area = if horizontal {
        Range::from_bounds(start.row, start.col, start.row, area.range.end.col)
    } else {
        Range::from_bounds(start.row, start.col, area.range.end.row, start.col)
    };
    let (keys, _, _) = ev.area_values(Area {
        sheet: area.sheet,
        range: keys_area,
    });
    let found = if approximate {
        sorted_find(target, &keys, false)
    } else {
        find(target, &keys, Matching::Wildcard, false)
    };
    let index = found.ok_or(ErrorValue::NA)? as u32;
    let (row, col) = if horizontal {
        (start.row + column - 1, start.col + index)
    } else {
        (start.row + index, start.col + column - 1)
    };
    Ok(ev.cell_value(area.sheet, row, col))
}

/// LOOKUP(value, lookup vector, [result vector]): approximate, sorted
fn vector_lookup(ev: &Evaluator, args: &[Expr]) -> Outcome<Value> {
    arity(args, 2, 3)?;
    let target = ev.scalar(&args[0]);
    let keys = grid(ev, &args[1])?;
    let results = match args.get(2) {
        Some(expr) => grid(ev, expr)?,
        None => keys.clone(),
    };
    let row = sorted_find(&target, &keys.items, false).ok_or(ErrorValue::NA)?;
    results.items.get(row).cloned().ok_or(ErrorValue::NA)
}

/// a one-row or one-column argument as a list
fn vector(ev: &Evaluator, expr: &Expr) -> Outcome<Vec<Value>> {
    let array = grid(ev, expr)?;
    if array.rows != 1 && array.cols != 1 {
        return Err(ErrorValue::NA);
    }
    Ok(array.items)
}

fn match_function(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    arity(args, 2, 3)?;
    let target = ev.scalar(&args[0]);
    if let Value::Error(e) = target {
        return Err(e);
    }
    let items = vector(ev, &args[1])?;
    let kind = number_or(ev, args, 2, 1.0)?;
    let found = if kind > 0.0 {
        sorted_find(&target, &items, false)
    } else if kind < 0.0 {
        sorted_find(&target, &items, true)
    } else {
        find(&target, &items, Matching::Wildcard, false)
    };
    found.map(|index| index as f64 + 1.0).ok_or(ErrorValue::NA)
}

fn matching_of(mode: i64) -> Outcome<Matching> {
    Ok(match mode {
        0 => Matching::Exact,
        -1 => Matching::NextSmaller,
        1 => Matching::NextLarger,
        2 => Matching::Wildcard,
        _ => return Err(ErrorValue::Value),
    })
}

/// XLOOKUP and XMATCH's search: modes 1 and -1 scan, 2 and -2 halve
fn x_find(
    target: &Value,
    items: &[Value],
    matching: Matching,
    search: i64,
) -> Outcome<Option<usize>> {
    Ok(match search {
        1 => find(target, items, matching, false),
        -1 => find(target, items, matching, true),
        2 | -2 => {
            let descending = search == -2;
            let position = sorted_find(target, items, descending);
            let exact = position.filter(|p| equal(target, &items[*p], false));
            match matching {
                Matching::Exact | Matching::Wildcard => exact,
                Matching::NextSmaller => position,
                Matching::NextLarger => {
                    exact.or(position.map(|p| p + 1).filter(|p| *p < items.len()))
                }
            }
        }
        _ => return Err(ErrorValue::Value),
    })
}

fn xmatch(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    arity(args, 2, 4)?;
    let target = ev.scalar(&args[0]);
    let items = vector(ev, &args[1])?;
    let matching = matching_of(integer_or(ev, args, 2, 0)?)?;
    let found = x_find(&target, &items, matching, integer_or(ev, args, 3, 1)?)?;
    found.map(|index| index as f64 + 1.0).ok_or(ErrorValue::NA)
}

fn xlookup(ev: &Evaluator, args: &[Expr]) -> CalcValue {
    if let Err(e) = arity(args, 3, 6) {
        return CalcValue::error(e);
    }
    let attempt = || -> Outcome<Option<CalcValue>> {
        let target = ev.scalar(&args[0]);
        let keys = grid(ev, &args[1])?;
        if keys.rows != 1 && keys.cols != 1 {
            return Err(ErrorValue::Value);
        }
        let results = grid(ev, &args[2])?;
        let matching = matching_of(integer_or(ev, args, 4, 0)?)?;
        let Some(index) = x_find(&target, &keys.items, matching, integer_or(ev, args, 5, 1)?)?
        else {
            return Ok(None);
        };
        Ok(Some(result_slice(&keys, &results, index)?))
    };
    match attempt() {
        Ok(Some(value)) => value,
        Ok(None) => match args.get(3) {
            Some(expr) if !matches!(expr, Expr::Missing) => ev.eval(expr),
            _ => CalcValue::error(ErrorValue::NA),
        },
        Err(e) => CalcValue::error(e),
    }
}

/// the matching row (vertical keys) or column of the result array
fn result_slice(keys: &Array, results: &Array, index: usize) -> Outcome<CalcValue> {
    let vertical = keys.cols == 1 && keys.rows > 1
        || (keys.items.len() == 1 && results.rows == 1 && results.cols == 1);
    if vertical {
        if results.rows != keys.rows {
            return Err(ErrorValue::Value);
        }
        let row: Vec<Value> = (0..results.cols)
            .map(|c| results.get(index, c).clone())
            .collect();
        return Ok(single_or_array(Array::new(1, row.len(), row)));
    }
    if results.cols != keys.cols {
        return Err(ErrorValue::Value);
    }
    let column: Vec<Value> = (0..results.rows)
        .map(|r| results.get(r, index).clone())
        .collect();
    Ok(single_or_array(Array::column(column)))
}

fn single_or_array(array: Array) -> CalcValue {
    if array.items.len() == 1 {
        CalcValue::Scalar(array.items[0].clone())
    } else {
        CalcValue::Array(array)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(n: &[f64]) -> Vec<Value> {
        n.iter().map(|x| Value::Number(*x)).collect()
    }

    #[test]
    fn approximate_and_exact_finding() {
        let items = numbers(&[10.0, 20.0, 30.0, 40.0]);
        assert_eq!(sorted_find(&Value::Number(25.0), &items, false), Some(1));
        assert_eq!(sorted_find(&Value::Number(5.0), &items, false), None);
        assert_eq!(sorted_find(&Value::Number(40.0), &items, false), Some(3));
        let descending = numbers(&[40.0, 30.0, 20.0]);
        assert_eq!(
            sorted_find(&Value::Number(25.0), &descending, true),
            Some(1)
        );
        assert_eq!(
            find(&Value::Number(30.0), &items, Matching::Exact, false),
            Some(2)
        );
        assert_eq!(
            find(&Value::Number(25.0), &items, Matching::NextSmaller, false),
            Some(1)
        );
        assert_eq!(
            find(&Value::Number(25.0), &items, Matching::NextLarger, false),
            Some(2)
        );
        let words = vec![Value::text("apple"), Value::text("Banana")];
        assert_eq!(
            find(&Value::text("b*"), &words, Matching::Wildcard, false),
            Some(1)
        );
        assert_eq!(
            find(&Value::text("b*"), &words, Matching::Exact, false),
            None
        );
        assert_eq!(
            find(&Value::text("1"), &numbers(&[1.0]), Matching::Exact, false),
            None
        );
    }
}
