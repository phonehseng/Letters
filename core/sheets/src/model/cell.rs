//! what one cell holds, and the sparse grid of cells a sheet keeps.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::address::{CellRef, Range};

/// the error values a formula can produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorValue {
    Null,
    Div0,
    Value,
    Ref,
    Name,
    Num,
    NA,
    GettingData,
    Spill,
    Calc,
}

impl ErrorValue {
    /// how the error is written in cells and files.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorValue::Null => "#NULL!",
            ErrorValue::Div0 => "#DIV/0!",
            ErrorValue::Value => "#VALUE!",
            ErrorValue::Ref => "#REF!",
            ErrorValue::Name => "#NAME?",
            ErrorValue::Num => "#NUM!",
            ErrorValue::NA => "#N/A",
            ErrorValue::GettingData => "#GETTING_DATA",
            ErrorValue::Spill => "#SPILL!",
            ErrorValue::Calc => "#CALC!",
        }
    }

    pub fn parse(text: &str) -> Option<ErrorValue> {
        let upper = text.trim().to_ascii_uppercase();
        ErrorValue::ALL
            .iter()
            .copied()
            .find(|error| error.as_str() == upper)
            .or(match upper.as_str() {
                // libreoffice's spelling of the same thing
                "#N/A!" => Some(ErrorValue::NA),
                _ => None,
            })
    }

    /// the number ERROR.TYPE gives.
    pub fn code(&self) -> u32 {
        match self {
            ErrorValue::Null => 1,
            ErrorValue::Div0 => 2,
            ErrorValue::Value => 3,
            ErrorValue::Ref => 4,
            ErrorValue::Name => 5,
            ErrorValue::Num => 6,
            ErrorValue::NA => 7,
            ErrorValue::GettingData => 8,
            ErrorValue::Spill => 9,
            ErrorValue::Calc => 14,
        }
    }

    pub const ALL: [ErrorValue; 10] = [
        ErrorValue::Null,
        ErrorValue::Div0,
        ErrorValue::Value,
        ErrorValue::Ref,
        ErrorValue::Name,
        ErrorValue::Num,
        ErrorValue::NA,
        ErrorValue::GettingData,
        ErrorValue::Spill,
        ErrorValue::Calc,
    ];
}

/// a cell's value: what it shows before formatting.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Value {
    #[default]
    Blank,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(ErrorValue),
}

impl Value {
    pub fn is_blank(&self) -> bool {
        matches!(self, Value::Blank)
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn text(text: impl Into<String>) -> Value {
        Value::Text(text.into())
    }
}

/// how a formula is stored in the file it came from.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FormulaKind {
    /// a formula from a file written without dynamic arrays: where it
    /// expects one value and gets a range, it takes the cell in line with
    /// it (excel's implicit intersection), as the program that wrote it did
    #[default]
    Normal,
    /// a formula typed in letters or saved by excel 365 as a dynamic
    /// array: a result with many values spills into the cells beside it.
    /// `spill` is where it spilled at the last calculation
    Dynamic {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spill: Option<Range>,
    },
    /// an array formula (ctrl shift enter in older excel) covering `range`
    Array { range: Range },
    /// one cell of a group sharing one formula text. the master cell
    /// (`range` set) holds the original text; every cell's own text is
    /// already translated to its position
    Shared {
        group: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range: Option<Range>,
    },
    /// a what-if data table; recalculation leaves it alone
    DataTable { attributes: Vec<(String, String)> },
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Formula {
    /// excel syntax without the leading "=", e.g. "SUM(A1:A3)"
    pub text: String,
    #[serde(default)]
    pub kind: FormulaKind,
}

impl Formula {
    /// a formula as people type it: dynamic, so arrays spill
    pub fn new(text: impl Into<String>) -> Formula {
        Formula {
            text: text.into(),
            kind: FormulaKind::Dynamic { spill: None },
        }
    }

    /// a formula from a file without dynamic arrays
    pub fn legacy(text: impl Into<String>) -> Formula {
        Formula {
            text: text.into(),
            kind: FormulaKind::Normal,
        }
    }
}

/// things few cells have, kept out of line so ordinary cells stay small.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellExtra {
    /// the original rich text xml of a text cell (runs with their own
    /// fonts), reused while the text is unchanged
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rich_text: Option<String>,
    /// attributes of the cell element the model does not read (excel's
    /// dynamic array marker `cm`, value metadata `vm`, phonetic `ph`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
    /// formula element attributes the model does not read (`aca`, `ca`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formula_attributes: Vec<(String, String)>,
}

/// one cell.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    /// the value, or for a formula its last calculated result
    #[serde(default, skip_serializing_if = "Value::is_blank")]
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<Formula>,
    /// index into the workbook's style table; 0 is the default style
    #[serde(default, skip_serializing_if = "is_zero")]
    pub style: u32,
    /// true when the formula could not be recalculated here (it uses a
    /// function letters does not have), so `value` is the one stored in
    /// the file, shown as is
    #[serde(default, skip_serializing_if = "is_false")]
    pub not_recalculated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Box<CellExtra>>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Cell {
    pub fn value(value: Value) -> Cell {
        Cell {
            value,
            ..Cell::default()
        }
    }

    pub fn formula(text: &str) -> Cell {
        Cell {
            formula: Some(Formula::new(text)),
            ..Cell::default()
        }
    }

    /// nothing in it but perhaps a style
    pub fn is_empty(&self) -> bool {
        self.value.is_blank() && self.formula.is_none()
    }
}

/// the cells of a sheet, stored sparsely: only cells that exist take
/// memory. rows are kept in order (so a viewport finds its rows quickly)
/// and each row is a short list sorted by column.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellGrid {
    rows: BTreeMap<u32, Vec<(u32, Cell)>>,
    count: usize,
}

impl CellGrid {
    pub fn new() -> CellGrid {
        CellGrid::default()
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn get(&self, row: u32, col: u32) -> Option<&Cell> {
        let cells = self.rows.get(&row)?;
        cells
            .binary_search_by_key(&col, |(c, _)| *c)
            .ok()
            .map(|index| &cells[index].1)
    }

    pub fn get_mut(&mut self, row: u32, col: u32) -> Option<&mut Cell> {
        let cells = self.rows.get_mut(&row)?;
        match cells.binary_search_by_key(&col, |(c, _)| *c) {
            Ok(index) => Some(&mut cells[index].1),
            Err(_) => None,
        }
    }

    /// the cell's value, blank when there is no cell.
    pub fn value(&self, row: u32, col: u32) -> &Value {
        static BLANK: Value = Value::Blank;
        self.get(row, col).map(|cell| &cell.value).unwrap_or(&BLANK)
    }

    /// puts a cell in place, returning what was there.
    pub fn set(&mut self, row: u32, col: u32, cell: Cell) -> Option<Cell> {
        let cells = self.rows.entry(row).or_default();
        match cells.binary_search_by_key(&col, |(c, _)| *c) {
            Ok(index) => Some(std::mem::replace(&mut cells[index].1, cell)),
            Err(index) => {
                cells.insert(index, (col, cell));
                self.count += 1;
                None
            }
        }
    }

    /// the cell at a position, made if missing.
    pub fn entry(&mut self, row: u32, col: u32) -> &mut Cell {
        let cells = self.rows.entry(row).or_default();
        let index = match cells.binary_search_by_key(&col, |(c, _)| *c) {
            Ok(index) => index,
            Err(index) => {
                cells.insert(index, (col, Cell::default()));
                self.count += 1;
                index
            }
        };
        &mut cells[index].1
    }

    pub fn remove(&mut self, row: u32, col: u32) -> Option<Cell> {
        let cells = self.rows.get_mut(&row)?;
        let index = cells.binary_search_by_key(&col, |(c, _)| *c).ok()?;
        let (_, cell) = cells.remove(index);
        self.count -= 1;
        if cells.is_empty() {
            self.rows.remove(&row);
        }
        Some(cell)
    }

    /// every cell in row order: (row, col, cell).
    pub fn iter(&self) -> impl Iterator<Item = (u32, u32, &Cell)> {
        self.rows
            .iter()
            .flat_map(|(row, cells)| cells.iter().map(move |(col, cell)| (*row, *col, cell)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u32, u32, &mut Cell)> {
        self.rows.iter_mut().flat_map(|(row, cells)| {
            let row = *row;
            cells.iter_mut().map(move |(col, cell)| (row, *col, cell))
        })
    }

    /// the cells inside a range, in row order. fast for viewports: rows
    /// outside the range are never looked at.
    pub fn iter_range(&self, range: Range) -> impl Iterator<Item = (u32, u32, &Cell)> {
        self.rows
            .range(range.start.row..=range.end.row)
            .flat_map(move |(row, cells)| {
                let first = cells.partition_point(|(c, _)| *c < range.start.col);
                cells[first..]
                    .iter()
                    .take_while(move |(c, _)| *c <= range.end.col)
                    .map(move |(col, cell)| (*row, *col, cell))
            })
    }

    /// the rows that have at least one cell
    pub fn row_numbers(&self) -> impl Iterator<Item = u32> + '_ {
        self.rows.keys().copied()
    }

    pub fn row(&self, row: u32) -> &[(u32, Cell)] {
        self.rows
            .get(&row)
            .map(|cells| cells.as_slice())
            .unwrap_or(&[])
    }

    /// the smallest range holding every cell, or none for an empty grid.
    pub fn used_range(&self) -> Option<Range> {
        let first = *self.rows.keys().next()?;
        let last = *self.rows.keys().next_back()?;
        let mut min_col = u32::MAX;
        let mut max_col = 0;
        for cells in self.rows.values() {
            if let (Some(a), Some(b)) = (cells.first(), cells.last()) {
                min_col = min_col.min(a.0);
                max_col = max_col.max(b.0);
            }
        }
        Some(Range::from_bounds(first, min_col, last, max_col))
    }

    /// takes every cell out, leaving the grid empty.
    pub fn drain(&mut self) -> Vec<(u32, u32, Cell)> {
        let rows = std::mem::take(&mut self.rows);
        self.count = 0;
        rows.into_iter()
            .flat_map(|(row, cells)| cells.into_iter().map(move |(col, cell)| (row, col, cell)))
            .collect()
    }

    /// rebuilds the grid with every cell moved by `place` (none drops it).
    pub fn remap(&mut self, mut place: impl FnMut(u32, u32) -> Option<(u32, u32)>) {
        for (row, col, cell) in self.drain() {
            if let Some((r, c)) = place(row, col) {
                self.set(r, c, cell);
            }
        }
    }

    pub fn cell_at(&self, cell: CellRef) -> Option<&Cell> {
        self.get(cell.row, cell.col)
    }
}

/// json form: {"0": {"0": cell, "3": cell}, "5": {...}}, rows and
/// columns zero based.
impl Serialize for CellGrid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        struct RowCells<'a>(&'a [(u32, Cell)]);
        impl Serialize for RowCells<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(self.0.len()))?;
                for (col, cell) in self.0 {
                    map.serialize_entry(col, cell)?;
                }
                map.end()
            }
        }
        let mut map = serializer.serialize_map(Some(self.rows.len()))?;
        for (row, cells) in &self.rows {
            map.serialize_entry(row, &RowCells(cells))?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for CellGrid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let rows: BTreeMap<u32, BTreeMap<u32, Cell>> = BTreeMap::deserialize(deserializer)?;
        let mut grid = CellGrid::new();
        for (row, cells) in rows {
            for (col, cell) in cells {
                grid.set(row, col, cell);
            }
        }
        Ok(grid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_keeps_cells_sorted_and_counts() {
        let mut grid = CellGrid::new();
        grid.set(5, 3, Cell::value(Value::Number(1.0)));
        grid.set(5, 1, Cell::value(Value::text("a")));
        grid.set(0, 0, Cell::value(Value::Bool(true)));
        assert_eq!(grid.len(), 3);
        let order: Vec<(u32, u32)> = grid.iter().map(|(r, c, _)| (r, c)).collect();
        assert_eq!(order, vec![(0, 0), (5, 1), (5, 3)]);
        let inside: Vec<(u32, u32)> = grid
            .iter_range(Range::from_bounds(1, 2, 9, 9))
            .map(|(r, c, _)| (r, c))
            .collect();
        assert_eq!(inside, vec![(5, 3)]);
        assert_eq!(grid.used_range().unwrap().to_a1(), "A1:D6");
        grid.remove(5, 1);
        assert_eq!(grid.len(), 2);
        let json = serde_json::to_string(&grid).unwrap();
        let back: CellGrid = serde_json::from_str(&json).unwrap();
        assert_eq!(back, grid);
    }

    #[test]
    fn errors_parse_both_spellings() {
        for error in ErrorValue::ALL {
            assert_eq!(ErrorValue::parse(error.as_str()), Some(error));
        }
        assert_eq!(ErrorValue::parse("#n/a"), Some(ErrorValue::NA));
    }
}
