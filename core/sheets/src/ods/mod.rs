//! opendocument spreadsheets (.ods): read into the shared model and
//! written back. pictures, charts (embedded objects), forms, settings and
//! the styles part are carried when the file came from ods.

mod read;
mod read_conditional;
mod read_table;
mod styles;
mod units;
mod write;
mod write_styles;
mod write_table;

use crate::address::Range;

pub use read::read_ods;
pub use write::write_ods;

/// joins single cells into as few rectangles as possible: runs down each
/// column first, then side by side runs of the same rows
pub fn merge_cells_into_ranges(mut cells: Vec<Range>) -> Vec<Range> {
    cells.sort_by_key(|r| (r.start.col, r.start.row));
    let mut columns: Vec<Range> = Vec::new();
    for cell in cells {
        match columns.last_mut() {
            Some(last)
                if last.start.col == cell.start.col
                    && last.end.col == cell.end.col
                    && last.end.row + 1 == cell.start.row =>
            {
                last.end.row = cell.end.row;
            }
            _ => columns.push(cell),
        }
    }
    columns.sort_by_key(|r| (r.start.row, r.end.row, r.start.col));
    let mut merged: Vec<Range> = Vec::new();
    for range in columns {
        match merged.last_mut() {
            Some(last)
                if last.start.row == range.start.row
                    && last.end.row == range.end.row
                    && last.end.col + 1 == range.start.col =>
            {
                last.end.col = range.end.col;
            }
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_join_into_rectangles() {
        let cells = ["A1", "A2", "A3", "B1", "B2", "B3", "D5"]
            .iter()
            .map(|a| Range::parse(a).unwrap())
            .collect();
        let merged = merge_cells_into_ranges(cells);
        assert_eq!(Range::list_to_a1(&merged), "A1:B3 D5");
    }
}
