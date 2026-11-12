//! what the rules that compare a cell with the rest of its range need to
//! know about that range: its numbers (top 10, average, colour scales,
//! data bars) and how often each value appears (duplicates). each is
//! worked out once per request for the whole range, never per cell, so a
//! rule over a whole column costs one pass over the column.

use std::collections::HashMap;

use crate::address::Range;
use crate::calc::value::round_significant;
use crate::model::{Sheet, Value};

/// the parts of `ranges` that hold cells: a whole-column range is cut at
/// the sheet's last used row, since empty cells never count
fn used_parts(ranges: &[Range], used: Option<Range>) -> Vec<Range> {
    let Some(used) = used else {
        return Vec::new();
    };
    ranges
        .iter()
        .filter_map(|range| range.intersection(&used))
        .collect()
}

/// the numbers of a range. text, logical values and errors are left out,
/// as excel's rules leave them out. they are put in order only when a
/// percentile asks for it: a colour scale from the least to the most
/// needs no sorting, and sorting 200,000 numbers is most of the cost.
pub struct Numbers {
    values: Vec<f64>,
    sorted: bool,
    least: f64,
    most: f64,
}

impl Numbers {
    pub fn new(values: Vec<f64>) -> Numbers {
        let least = values.iter().copied().fold(f64::INFINITY, f64::min);
        let most = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        Numbers {
            values,
            sorted: false,
            least,
            most,
        }
    }

    pub fn in_ranges(sheet: &Sheet, ranges: &[Range], used: Option<Range>) -> Numbers {
        let mut values = Vec::new();
        for part in used_parts(ranges, used) {
            for (_, _, cell) in sheet.cells.iter_range(part) {
                if let Value::Number(n) = cell.value {
                    values.push(n);
                }
            }
        }
        Numbers::new(values)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn least(&self) -> Option<f64> {
        (!self.values.is_empty()).then_some(self.least)
    }

    pub fn most(&self) -> Option<f64> {
        (!self.values.is_empty()).then_some(self.most)
    }

    /// excel's PERCENTILE.INC, `fraction` from 0 to 1
    pub fn percentile(&mut self, fraction: f64) -> Option<f64> {
        let last = self.values.len().checked_sub(1)?;
        if !self.sorted {
            self.values.sort_unstable_by(f64::total_cmp);
            self.sorted = true;
        }
        let sorted = &self.values;
        let position = fraction.clamp(0.0, 1.0) * last as f64;
        let below = position.floor() as usize;
        let above = (below + 1).min(last);
        let weight = position - below as f64;
        Some(sorted[below] + (sorted[above] - sorted[below]) * weight)
    }

    /// the number at `index` (0 is the least) if the numbers were in
    /// order; found by selection, which is quicker than a full sort
    pub fn nth_smallest(&mut self, index: usize) -> Option<f64> {
        if index >= self.values.len() {
            return None;
        }
        if self.sorted {
            return Some(self.values[index]);
        }
        let (_, nth, _) = self.values.select_nth_unstable_by(index, f64::total_cmp);
        Some(*nth)
    }

    /// the mean and the population standard deviation (excel's STDEVP)
    pub fn mean_and_deviation(&self) -> Option<(f64, f64)> {
        if self.values.is_empty() {
            return None;
        }
        let count = self.values.len() as f64;
        let mean = self.values.iter().sum::<f64>() / count;
        let variance = self.values.iter().map(|n| (n - mean).powi(2)).sum::<f64>() / count;
        Some((mean, variance.sqrt()))
    }
}

/// a value as duplicate rules compare it: text ignores case and numbers
/// match to the 15 digits excel shows
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueKey {
    Number(u64),
    Text(String),
    Bool(bool),
}

impl ValueKey {
    /// none for blanks and errors, which duplicate rules never mark
    pub fn of(value: &Value) -> Option<ValueKey> {
        ExactKey::of(value).map(ExactKey::folded)
    }
}

/// a value exactly as stored, borrowed from its cell: counting these
/// first needs no copy of each text and no rounding of each number (the
/// rounding goes through text), only one per different value
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ExactKey<'v> {
    Number(u64),
    Text(&'v str),
    Bool(bool),
}

impl<'v> ExactKey<'v> {
    fn of(value: &'v Value) -> Option<ExactKey<'v>> {
        match value {
            // 0.0 and -0.0 are the same number
            Value::Number(n) => Some(ExactKey::Number((n + 0.0).to_bits())),
            Value::Text(text) if text.is_empty() => None,
            Value::Text(text) => Some(ExactKey::Text(text)),
            Value::Bool(b) => Some(ExactKey::Bool(*b)),
            Value::Blank | Value::Error(_) => None,
        }
    }

    fn folded(self) -> ValueKey {
        match self {
            ExactKey::Number(bits) => {
                ValueKey::Number(round_significant(f64::from_bits(bits)).to_bits())
            }
            ExactKey::Text(text) => ValueKey::Text(text.to_lowercase()),
            ExactKey::Bool(b) => ValueKey::Bool(b),
        }
    }
}

/// how many times each value appears in the ranges
pub fn value_counts(
    sheet: &Sheet,
    ranges: &[Range],
    used: Option<Range>,
) -> HashMap<ValueKey, u32> {
    let mut exact: HashMap<ExactKey, u32> = HashMap::new();
    for part in used_parts(ranges, used) {
        for (_, _, cell) in sheet.cells.iter_range(part) {
            if let Some(key) = ExactKey::of(&cell.value) {
                *exact.entry(key).or_insert(0) += 1;
            }
        }
    }
    // values that differ only in case or past the 15th digit are one
    let mut counts = HashMap::with_capacity(exact.len());
    for (key, count) in exact {
        *counts.entry(key.folded()).or_insert(0) += count;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_match_excel() {
        let mut numbers = Numbers::new(vec![4.0, 2.0, 1.0, 3.0]);
        assert_eq!(numbers.percentile(0.5), Some(2.5));
        assert_eq!(numbers.percentile(0.0), Some(1.0));
        assert_eq!(numbers.percentile(1.0), Some(4.0));
        assert_eq!(Numbers::new(Vec::new()).percentile(0.5), None);
    }

    #[test]
    fn ranks_are_found_without_sorting_first() {
        let mut numbers = Numbers::new(vec![5.0, 1.0, 4.0, 2.0, 3.0]);
        assert_eq!(numbers.nth_smallest(1), Some(2.0));
        assert_eq!(numbers.nth_smallest(4), Some(5.0));
        assert_eq!(numbers.nth_smallest(5), None);
        assert_eq!((numbers.least(), numbers.most()), (Some(1.0), Some(5.0)));
    }

    #[test]
    fn deviation_is_the_population_one() {
        let numbers = Numbers::new(vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        let (mean, deviation) = numbers.mean_and_deviation().unwrap();
        assert_eq!(mean, 5.0);
        assert_eq!(deviation, 2.0);
    }

    #[test]
    fn whole_columns_stop_at_the_used_rows() {
        let mut sheet = Sheet::new("S");
        let values = [
            Value::Number(3.0),
            Value::text("Apple"),
            Value::text("apple"),
            Value::Number(1.0),
            Value::Number(0.1 + 0.2),
            Value::Number(0.3),
        ];
        for (row, value) in values.into_iter().enumerate() {
            sheet.cells.set(row as u32, 0, crate::Cell::value(value));
        }
        let whole = [Range::parse("A:A").unwrap()];
        let used = sheet.cells.used_range();
        let mut numbers = Numbers::in_ranges(&sheet, &whole, used);
        assert_eq!(numbers.len(), 4);
        assert_eq!(numbers.nth_smallest(0), Some(0.3));
        let counts = value_counts(&sheet, &whole, used);
        assert_eq!(counts[&ValueKey::Text("apple".into())], 2);
        // 0.1 + 0.2 is 0.3 to the 15 digits excel compares
        assert_eq!(counts[&ValueKey::of(&Value::Number(0.3)).unwrap()], 2);
    }
}
