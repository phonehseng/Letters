//! moving by data, as excel's ctrl with the arrows and ctrl a do: the
//! grid only knows the cells on screen, so the shell, which has the whole
//! sheet, finds where a block of data ends.

use letters_sheets::address::{MAX_COLS, MAX_ROWS};
use letters_sheets::{CellRef, Range, Sheet};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

fn filled(sheet: &Sheet, row: u32, col: u32) -> bool {
    sheet
        .cells
        .get(row, col)
        .is_some_and(|cell| !cell.is_empty())
}

/// the filled positions along the line from `at` in `direction`, nearest
/// first (the sparse grid keeps rows, so a column is read row by row)
fn filled_along(sheet: &Sheet, at: CellRef, direction: Direction) -> Vec<u32> {
    match direction {
        Direction::Right | Direction::Left => {
            let mut cols: Vec<u32> = sheet
                .cells
                .row(at.row)
                .iter()
                .filter(|(_, cell)| !cell.is_empty())
                .map(|(col, _)| *col)
                .collect();
            if direction == Direction::Left {
                cols.reverse();
            }
            cols
        }
        Direction::Down | Direction::Up => {
            let mut rows: Vec<u32> = sheet
                .cells
                .row_numbers()
                .filter(|row| filled(sheet, *row, at.col))
                .collect();
            if direction == Direction::Up {
                rows.reverse();
            }
            rows
        }
    }
}

/// where ctrl and an arrow land: the end of the block of filled cells,
/// or the next filled cell after a gap, or the edge of the sheet.
pub fn edge(sheet: &Sheet, at: CellRef, direction: Direction) -> CellRef {
    let (here, last) = match direction {
        Direction::Down | Direction::Up => (at.row, MAX_ROWS - 1),
        Direction::Left | Direction::Right => (at.col, MAX_COLS - 1),
    };
    let forward = matches!(direction, Direction::Down | Direction::Right);
    let ahead: Vec<u32> = filled_along(sheet, at, direction)
        .into_iter()
        .filter(|&i| if forward { i > here } else { i < here })
        .collect();
    let step = |i: u32| if forward { i + 1 } else { i.wrapping_sub(1) };
    let next_is_filled = ahead.first() == Some(&step(here));
    let target = if is_filled_at(sheet, at) && next_is_filled {
        // run to the last cell of this block
        let mut end = here;
        for &i in &ahead {
            if i != step(end) {
                break;
            }
            end = i;
        }
        end
    } else {
        ahead
            .first()
            .copied()
            .unwrap_or(if forward { last } else { 0 })
    };
    match direction {
        Direction::Down | Direction::Up => CellRef::new(target, at.col),
        Direction::Left | Direction::Right => CellRef::new(at.row, target),
    }
}

fn is_filled_at(sheet: &Sheet, at: CellRef) -> bool {
    filled(sheet, at.row, at.col)
}

/// ctrl a's first press: the block of data around `at`, grown while any
/// filled cell touches its edge (diagonals included, as excel does).
pub fn current_region(sheet: &Sheet, at: CellRef) -> Range {
    /// a region is found in a few passes; this bounds a pathological sheet
    const MOST_PASSES: usize = 10_000;
    let mut region = Range::cell(at);
    for _ in 0..MOST_PASSES {
        let grown = grow_once(sheet, region);
        if grown == region {
            break;
        }
        region = grown;
    }
    region
}

fn grow_once(sheet: &Sheet, region: Range) -> Range {
    let top = region.start.row.saturating_sub(1);
    let left = region.start.col.saturating_sub(1);
    let bottom = (region.end.row + 1).min(MAX_ROWS - 1);
    let right = (region.end.col + 1).min(MAX_COLS - 1);
    let around = Range::from_bounds(top, left, bottom, right);
    let mut grown = region;
    for (row, col, cell) in sheet.cells.iter_range(around) {
        if cell.is_empty() {
            continue;
        }
        grown = Range::new(
            CellRef::new(grown.start.row.min(row), grown.start.col.min(col)),
            CellRef::new(grown.end.row.max(row), grown.end.col.max(col)),
        );
    }
    grown
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_sheets::{Cell, Value};

    /// column A filled in rows 1 to 3 and 7 to 8 (zero based 0-2, 6-7)
    fn sheet_with_two_blocks() -> Sheet {
        let mut sheet = Sheet::new("Data");
        for row in [0, 1, 2, 6, 7] {
            sheet.cells.set(row, 0, Cell::value(Value::Number(1.0)));
        }
        sheet.cells.set(1, 1, Cell::value(Value::text("x")));
        sheet
    }

    #[test]
    fn ctrl_down_runs_to_the_end_of_a_block_then_jumps_the_gap_then_the_edge() {
        let sheet = sheet_with_two_blocks();
        let first = edge(&sheet, CellRef::new(0, 0), Direction::Down);
        assert_eq!(first, CellRef::new(2, 0));
        let second = edge(&sheet, first, Direction::Down);
        assert_eq!(second, CellRef::new(6, 0));
        let third = edge(&sheet, CellRef::new(7, 0), Direction::Down);
        assert_eq!(third, CellRef::new(MAX_ROWS - 1, 0));
    }

    #[test]
    fn ctrl_up_and_left_stop_at_the_first_row_and_column() {
        let sheet = sheet_with_two_blocks();
        assert_eq!(
            edge(&sheet, CellRef::new(6, 0), Direction::Up),
            CellRef::new(2, 0)
        );
        assert_eq!(
            edge(&sheet, CellRef::new(5, 3), Direction::Left),
            CellRef::new(5, 0)
        );
        assert_eq!(
            edge(&sheet, CellRef::new(1, 5), Direction::Left),
            CellRef::new(1, 1)
        );
    }

    #[test]
    fn the_current_region_takes_in_touching_cells() {
        let sheet = sheet_with_two_blocks();
        let region = current_region(&sheet, CellRef::new(0, 0));
        assert_eq!(region, Range::parse("A1:B3").unwrap());
    }
}
