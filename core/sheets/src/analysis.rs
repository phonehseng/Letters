//! the numbers a status bar shows for a selection (sum, average, count,
//! minimum, maximum, distinct values) and a small distribution for a
//! numeric column. a `StatsCache` keeps running totals per column, so
//! moving the selection over 200,000 rows answers sum, count and average
//! from two lookups instead of a scan.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::address::Range;
use crate::model::{Sheet, Value};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionStats {
    /// cells that are not empty
    pub count: u64,
    pub numeric_count: u64,
    pub sum: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// different values (text ignoring case, as excel's COUNTIF sees them)
    pub distinct: u64,
    /// for selections of numbers: counts in equal-width bins
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub histogram: Vec<Bin>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bin {
    pub from: f64,
    pub to: f64,
    pub count: u64,
}

/// how many bins the distribution has (sturges' rule would give about
/// 18 for 200,000 values; ten is easier to read in a small popup)
const BINS: usize = 10;

/// running totals of one column: for each stored row, the totals up to it
#[derive(Default)]
struct ColumnTotals {
    rows: Vec<u32>,
    sums: Vec<f64>,
    counts: Vec<u64>,
    numbers: Vec<u64>,
    /// sparse tables of minimum and maximum: level k covers 2^k numbers
    min_table: Vec<Vec<f64>>,
    max_table: Vec<Vec<f64>>,
    /// the numbers in row order, for the tables
    number_rows: Vec<u32>,
}

impl ColumnTotals {
    fn build(cells: &[(u32, &Value)]) -> ColumnTotals {
        let mut totals = ColumnTotals::default();
        let (mut sum, mut count, mut numbers) = (0.0, 0u64, 0u64);
        let mut values = Vec::new();
        for (row, value) in cells {
            if value.is_blank() {
                continue;
            }
            count += 1;
            if let Value::Number(n) = value {
                sum += n;
                numbers += 1;
                values.push(*n);
                totals.number_rows.push(*row);
            }
            totals.rows.push(*row);
            totals.sums.push(sum);
            totals.counts.push(count);
            totals.numbers.push(numbers);
        }
        totals.min_table = sparse_table(&values, f64::min);
        totals.max_table = sparse_table(&values, f64::max);
        totals
    }

    /// totals of the stored rows from `first` to `last`
    fn between(&self, first: u32, last: u32) -> (f64, u64, u64) {
        let start = self.rows.partition_point(|r| *r < first);
        let end = self.rows.partition_point(|r| *r <= last);
        if end == 0 || start >= end {
            return (0.0, 0, 0);
        }
        let before = |list: &[u64]| if start == 0 { 0 } else { list[start - 1] };
        let sum_before = if start == 0 {
            0.0
        } else {
            self.sums[start - 1]
        };
        (
            self.sums[end - 1] - sum_before,
            self.counts[end - 1] - before(&self.counts),
            self.numbers[end - 1] - before(&self.numbers),
        )
    }

    fn extreme(
        &self,
        first: u32,
        last: u32,
        table: &[Vec<f64>],
        pick: fn(f64, f64) -> f64,
    ) -> Option<f64> {
        let start = self.number_rows.partition_point(|r| *r < first);
        let end = self.number_rows.partition_point(|r| *r <= last);
        if start >= end {
            return None;
        }
        let level = (usize::BITS - (end - start).leading_zeros() - 1) as usize;
        let span = 1 << level;
        Some(pick(table[level][start], table[level][end - span]))
    }
}

fn sparse_table(values: &[f64], pick: fn(f64, f64) -> f64) -> Vec<Vec<f64>> {
    let mut table = vec![values.to_vec()];
    let mut span = 1;
    while span * 2 <= values.len() {
        let previous = table.last().expect("level 0 exists");
        let next: Vec<f64> = (0..=values.len() - span * 2)
            .map(|i| pick(previous[i], previous[i + span]))
            .collect();
        table.push(next);
        span *= 2;
    }
    table
}

/// running totals for every column of a sheet; rebuild after edits
pub struct StatsCache {
    columns: HashMap<u32, ColumnTotals>,
}

impl StatsCache {
    pub fn new(sheet: &Sheet) -> StatsCache {
        let mut by_column: HashMap<u32, Vec<(u32, &Value)>> = HashMap::new();
        for (row, col, cell) in sheet.cells.iter() {
            by_column.entry(col).or_default().push((row, &cell.value));
        }
        let columns = by_column
            .into_iter()
            .map(|(col, cells)| (col, ColumnTotals::build(&cells)))
            .collect();
        StatsCache { columns }
    }

    /// sum, counts, average, minimum and maximum from the totals; distinct
    /// values and the distribution scan the selection's cells
    pub fn stats(&self, sheet: &Sheet, ranges: &[Range]) -> SelectionStats {
        let mut stats = SelectionStats::default();
        let mut min: Option<f64> = None;
        let mut max: Option<f64> = None;
        for range in ranges {
            for col in range.start.col..=range.end.col.min(range.start.col + 16_383) {
                let Some(totals) = self.columns.get(&col) else {
                    continue;
                };
                let (sum, count, numbers) = totals.between(range.start.row, range.end.row);
                stats.sum += sum;
                stats.count += count;
                stats.numeric_count += numbers;
                let low =
                    totals.extreme(range.start.row, range.end.row, &totals.min_table, f64::min);
                let high =
                    totals.extreme(range.start.row, range.end.row, &totals.max_table, f64::max);
                min = combine(min, low, f64::min);
                max = combine(max, high, f64::max);
            }
        }
        finish(&mut stats, min, max);
        add_distribution(sheet, ranges, &mut stats);
        stats
    }
}

fn combine(a: Option<f64>, b: Option<f64>, pick: fn(f64, f64) -> f64) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(pick(a, b)),
        (a, b) => a.or(b),
    }
}

fn finish(stats: &mut SelectionStats, min: Option<f64>, max: Option<f64>) {
    stats.min = min;
    stats.max = max;
    stats.average = (stats.numeric_count > 0).then(|| stats.sum / stats.numeric_count as f64);
}

/// distinct values and, for numbers, the bins
fn add_distribution(sheet: &Sheet, ranges: &[Range], stats: &mut SelectionStats) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut numbers = Vec::new();
    for range in ranges {
        for (_, _, cell) in sheet.cells.iter_range(*range) {
            let key = match &cell.value {
                Value::Blank => continue,
                Value::Number(n) => {
                    numbers.push(*n);
                    format!("n{n}")
                }
                Value::Text(t) => format!("t{}", t.to_lowercase()),
                other => format!("{other:?}"),
            };
            seen.insert(key);
        }
    }
    stats.distinct = seen.len() as u64;
    if let (Some(min), Some(max)) = (stats.min, stats.max) {
        if numbers.len() >= 2 && max > min {
            let width = (max - min) / BINS as f64;
            let mut counts = [0u64; BINS];
            for n in numbers {
                let index = (((n - min) / width) as usize).min(BINS - 1);
                counts[index] += 1;
            }
            stats.histogram = (0..BINS)
                .map(|i| Bin {
                    from: min + width * i as f64,
                    to: min + width * (i + 1) as f64,
                    count: counts[i],
                })
                .collect();
        }
    }
}

/// the same numbers without a cache, for one-off selections
pub fn selection_stats(sheet: &Sheet, ranges: &[Range]) -> SelectionStats {
    let mut stats = SelectionStats::default();
    let mut min: Option<f64> = None;
    let mut max: Option<f64> = None;
    for range in ranges {
        for (_, _, cell) in sheet.cells.iter_range(*range) {
            if cell.value.is_blank() {
                continue;
            }
            stats.count += 1;
            if let Value::Number(n) = cell.value {
                stats.numeric_count += 1;
                stats.sum += n;
                min = combine(min, Some(n), f64::min);
                max = combine(max, Some(n), f64::max);
            }
        }
    }
    finish(&mut stats, min, max);
    add_distribution(sheet, ranges, &mut stats);
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Cell;

    #[test]
    fn cached_and_direct_answers_agree() {
        let mut sheet = Sheet::new("S");
        for row in 0..1000u32 {
            sheet
                .cells
                .set(row, 0, Cell::value(Value::Number(f64::from(row % 37))));
            if row % 3 == 0 {
                sheet.cells.set(
                    row,
                    1,
                    Cell::value(Value::text(if row % 2 == 0 { "a" } else { "B" })),
                );
            }
        }
        let cache = StatsCache::new(&sheet);
        for range in ["A1:A1000", "A10:B500", "A999:A999", "C1:C10", "A100:A100"] {
            let range = Range::parse(range).unwrap();
            let cached = cache.stats(&sheet, &[range]);
            let direct = selection_stats(&sheet, &[range]);
            assert_eq!(cached.count, direct.count);
            assert_eq!(cached.min, direct.min);
            assert_eq!(cached.max, direct.max);
            assert!((cached.sum - direct.sum).abs() < 1e-9);
        }
        let all = cache.stats(&sheet, &[Range::parse("A1:B1000").unwrap()]);
        assert_eq!(all.distinct, 37 + 2);
        assert_eq!(all.histogram.iter().map(|b| b.count).sum::<u64>(), 1000);
    }
}
