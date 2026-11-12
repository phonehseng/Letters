//! which formulas read which cells, so that after an edit only the
//! formulas that depend on it (directly or through other formulas) are
//! calculated again.
//!
//! small references (up to `SMALL_AREA` cells) are indexed cell by cell;
//! larger ones that span few columns are indexed per column as row
//! intervals; very wide ones are kept in a short list. finding what a
//! change affects groups the changed cells by column and checks each
//! interval with a binary search, so a column of 200,000 running totals
//! is found in one pass.

use std::collections::{HashMap, HashSet};

use crate::address::Range;

use super::context::CellKey;

/// references with this many cells or fewer are indexed per cell
const SMALL_AREA: u64 = 16;
/// references wider than this many columns go in the short list
const MAX_INDEXED_COLUMNS: u32 = 64;

/// rows first..=last of one column read by a formula
type Interval = (u32, u32, CellKey);

#[derive(Default, Debug)]
pub struct DependencyIndex {
    by_cell: HashMap<CellKey, Vec<CellKey>>,
    /// (sheet, column) -> (first row, last row, dependent formula)
    by_column: HashMap<(usize, u32), Vec<Interval>>,
    wide: Vec<(usize, Range, CellKey)>,
}

impl DependencyIndex {
    pub fn add(&mut self, formula: CellKey, precedents: &[(usize, Range)]) {
        for (sheet, range) in precedents {
            if range.cell_count() <= SMALL_AREA {
                for cell in range.iter() {
                    self.by_cell
                        .entry((*sheet, cell.row, cell.col))
                        .or_default()
                        .push(formula);
                }
            } else if range.cols() <= MAX_INDEXED_COLUMNS {
                for col in range.start.col..=range.end.col {
                    let bucket = self.by_column.entry((*sheet, col)).or_default();
                    bucket.push((range.start.row, range.end.row, formula));
                }
            } else {
                self.wide.push((*sheet, *range, formula));
            }
        }
    }

    pub fn remove(&mut self, formula: CellKey, precedents: &[(usize, Range)]) {
        for (sheet, range) in precedents {
            if range.cell_count() <= SMALL_AREA {
                for cell in range.iter() {
                    if let Some(list) = self.by_cell.get_mut(&(*sheet, cell.row, cell.col)) {
                        list.retain(|key| *key != formula);
                    }
                }
            } else if range.cols() <= MAX_INDEXED_COLUMNS {
                for col in range.start.col..=range.end.col {
                    if let Some(bucket) = self.by_column.get_mut(&(*sheet, col)) {
                        bucket.retain(|(_, _, key)| *key != formula);
                    }
                }
            } else {
                self.wide.retain(|(_, _, key)| *key != formula);
            }
        }
    }

    /// the formulas reading any cell of `areas`, directly
    fn direct_dependents(&self, areas: &[(usize, Range)]) -> HashSet<CellKey> {
        let mut found = HashSet::new();
        let mut by_column: HashMap<(usize, u32), Vec<(u32, u32)>> = HashMap::new();
        for (sheet, range) in areas {
            self.cell_dependents(*sheet, range, &mut found);
            for col in range.start.col..=range.end.col.min(range.start.col + 16_383) {
                by_column
                    .entry((*sheet, col))
                    .or_default()
                    .push((range.start.row, range.end.row));
            }
        }
        for intervals in by_column.values_mut() {
            intervals.sort_unstable();
        }
        for (column, intervals) in &by_column {
            if let Some(bucket) = self.by_column.get(column) {
                for (first, last, formula) in bucket {
                    if overlaps_any(intervals, *first, *last) {
                        found.insert(*formula);
                    }
                }
            }
        }
        for (sheet, range, formula) in &self.wide {
            if areas
                .iter()
                .any(|(s, changed)| s == sheet && changed.intersects(range))
            {
                found.insert(*formula);
            }
        }
        found
    }

    fn cell_dependents(&self, sheet: usize, range: &Range, found: &mut HashSet<CellKey>) {
        if range.cell_count() <= 4096 {
            for cell in range.iter() {
                if let Some(list) = self.by_cell.get(&(sheet, cell.row, cell.col)) {
                    found.extend(list.iter().copied());
                }
            }
            return;
        }
        for (key, list) in &self.by_cell {
            if key.0 == sheet && range.contains(crate::address::CellRef::new(key.1, key.2)) {
                found.extend(list.iter().copied());
            }
        }
    }

    /// every formula affected by changes to `areas`, following chains of
    /// formulas. `spill_of` gives the cells a formula spills into, which
    /// change along with it.
    pub fn affected(
        &self,
        areas: Vec<(usize, Range)>,
        spill_of: &dyn Fn(CellKey) -> Option<Range>,
    ) -> HashSet<CellKey> {
        let mut affected = HashSet::new();
        let mut frontier = areas;
        while !frontier.is_empty() {
            let found = self.direct_dependents(&frontier);
            frontier = Vec::new();
            for formula in found {
                if affected.insert(formula) {
                    frontier.push((
                        formula.0,
                        Range::cell(crate::address::CellRef::new(formula.1, formula.2)),
                    ));
                    if let Some(spill) = spill_of(formula) {
                        frontier.push((formula.0, spill));
                    }
                }
            }
        }
        affected
    }
}

/// true when [first, last] meets one of the sorted intervals
fn overlaps_any(sorted: &[(u32, u32)], first: u32, last: u32) -> bool {
    // intervals starting after `last` cannot meet it; of the others, one
    // that ends at or after `first` does
    let candidates = sorted.partition_point(|(start, _)| *start <= last);
    sorted[..candidates]
        .iter()
        .rev()
        .any(|(_, end)| *end >= first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(text: &str) -> (usize, Range) {
        (0, Range::parse(text).unwrap())
    }

    #[test]
    fn finds_chains_of_dependents() {
        let mut index = DependencyIndex::default();
        index.add((0, 0, 1), &[area("A1")]); // B1 = A1
        index.add((0, 0, 2), &[area("B1")]); // C1 = B1
        index.add((0, 9, 3), &[area("A1:A100")]); // D10 = SUM(A1:A100)
        index.add((0, 0, 4), &[area("A1:Z1000")]); // a wide one
        let affected = index.affected(vec![area("A50")], &|_| None);
        assert!(affected.contains(&(0, 9, 3)));
        assert!(affected.contains(&(0, 0, 4)));
        assert!(!affected.contains(&(0, 0, 1)));
        let affected = index.affected(vec![area("A1")], &|_| None);
        assert!(affected.contains(&(0, 0, 2)), "C1 follows B1");
        index.remove((0, 9, 3), &[area("A1:A100")]);
        assert!(!index
            .affected(vec![area("A50")], &|_| None)
            .contains(&(0, 9, 3)));
    }

    #[test]
    fn interval_overlap() {
        let sorted = vec![(0, 5), (10, 20), (30, 40)];
        assert!(overlaps_any(&sorted, 6, 10));
        assert!(!overlaps_any(&sorted, 21, 29));
        assert!(overlaps_any(&sorted, 0, 0));
    }
}
