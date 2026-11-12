//! table geometry: which grid column every cell starts in, and which
//! grid positions are covered by a cell spanning rows or columns.
//!
//! the model leaves covered cells out (see `model::Table`). formats that
//! write covered cells explicitly (word's continued merges, odt's
//! covered cells) use this grid to know where they go.

use crate::model::Table;

/// what sits at one row and grid column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// a cell of this row; `first` is its leftmost column
    Cell { cell: usize, first: bool },
    /// covered by a cell from a row above that spans down
    Covered {
        row: usize,
        cell: usize,
        first: bool,
    },
    /// nothing: the row is shorter than the widest row
    Empty,
}

/// lays every cell of the table on a grid of rows and columns.
pub fn grid(table: &Table) -> Vec<Vec<Slot>> {
    let rows = table.rows.len();
    let mut grid: Vec<Vec<Slot>> = vec![Vec::new(); rows];
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut column = 0;
        for (cell_index, cell) in row.cells.iter().enumerate() {
            while matches!(grid[row_index].get(column), Some(slot) if *slot != Slot::Empty) {
                column += 1;
            }
            let span = cell.col_span.max(1) as usize;
            let last_row = (row_index + cell.row_span.max(1) as usize).min(rows);
            for (target_row, slots) in grid.iter_mut().enumerate().take(last_row).skip(row_index) {
                if slots.len() < column + span {
                    slots.resize(column + span, Slot::Empty);
                }
                for offset in 0..span {
                    slots[column + offset] = if target_row == row_index {
                        Slot::Cell {
                            cell: cell_index,
                            first: offset == 0,
                        }
                    } else {
                        Slot::Covered {
                            row: row_index,
                            cell: cell_index,
                            first: offset == 0,
                        }
                    };
                }
            }
            column += span;
        }
    }
    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    for slots in &mut grid {
        slots.resize(width, Slot::Empty);
    }
    grid
}

/// the number of grid columns.
pub fn grid_width(table: &Table) -> usize {
    grid(table).first().map(Vec::len).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TableCell, TableRow};

    fn row(cells: Vec<TableCell>) -> TableRow {
        TableRow {
            cells,
            ..Default::default()
        }
    }

    #[test]
    fn spans_cover_the_right_slots() {
        // | A (2 cols)  | B (2 rows) |
        // | C   | D    |            |
        let a = TableCell {
            col_span: 2,
            ..TableCell::with_text("A")
        };
        let b = TableCell {
            row_span: 2,
            ..TableCell::with_text("B")
        };
        let table = Table {
            rows: vec![
                row(vec![a, b]),
                row(vec![TableCell::with_text("C"), TableCell::with_text("D")]),
            ],
            ..Default::default()
        };
        let grid = grid(&table);
        assert_eq!(grid[0].len(), 3);
        assert_eq!(
            grid[0][1],
            Slot::Cell {
                cell: 0,
                first: false
            }
        );
        assert_eq!(
            grid[1][1],
            Slot::Cell {
                cell: 1,
                first: true
            }
        );
        assert_eq!(
            grid[1][2],
            Slot::Covered {
                row: 0,
                cell: 1,
                first: true
            }
        );
    }
}
