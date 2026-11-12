//! the one spreadsheet model both xlsx and ods are read into and written
//! from. the grid ui works on it (as json or through the viewport
//! queries in `view`); it never holds anything format specific except
//! the `carry` that makes saving lossless.

pub mod carry;
pub mod cell;
pub mod features;
pub mod style;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use carry::{CarriedPart, Carry, Section, SheetCarry, SourceFormat};
pub use cell::{Cell, CellExtra, CellGrid, ErrorValue, Formula, FormulaKind, Value};
pub use features::*;
pub use style::{
    Alignment, Border, BorderEdge, CellStyle, Color, Fill, Font, Protection, StyleTable,
};

/// excel's default column width in characters and row height in points
pub const DEFAULT_COLUMN_WIDTH: f64 = 8.43;
pub const DEFAULT_ROW_HEIGHT: f64 = 15.0;

/// one worksheet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sheet {
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
    pub cells: CellGrid,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<ColumnInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rows: BTreeMap<u32, RowInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_column_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_row_height: Option<f64>,
    #[serde(default)]
    pub view: SheetView,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<crate::address::Range>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditional_formats: Vec<ConditionalFormat>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validations: Vec<DataValidation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hyperlinks: Vec<Hyperlink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_filter: Option<AutoFilter>,
    #[serde(default)]
    pub print: PrintSetup,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawings: Vec<DrawingObject>,
    /// true when the sheet is protected against changes
    #[serde(default)]
    pub protected: bool,
    #[serde(default)]
    pub carry: SheetCarry,
}

impl Sheet {
    pub fn new(name: impl Into<String>) -> Sheet {
        Sheet {
            name: name.into(),
            visibility: Visibility::Visible,
            cells: CellGrid::new(),
            columns: Vec::new(),
            rows: BTreeMap::new(),
            default_column_width: None,
            default_row_height: None,
            view: SheetView::default(),
            merged: Vec::new(),
            conditional_formats: Vec::new(),
            validations: Vec::new(),
            hyperlinks: Vec::new(),
            comments: Vec::new(),
            tables: Vec::new(),
            auto_filter: None,
            print: PrintSetup::default(),
            drawings: Vec::new(),
            protected: false,
            carry: SheetCarry::default(),
        }
    }

    /// the width of a column in characters.
    pub fn column_width(&self, col: u32) -> f64 {
        self.columns
            .iter()
            .find(|info| col >= info.first && col <= info.last)
            .and_then(|info| info.width)
            .or(self.default_column_width)
            .unwrap_or(DEFAULT_COLUMN_WIDTH)
    }

    pub fn column_hidden(&self, col: u32) -> bool {
        self.columns
            .iter()
            .any(|info| col >= info.first && col <= info.last && info.hidden)
    }

    /// the height of a row in points.
    pub fn row_height(&self, row: u32) -> f64 {
        self.rows
            .get(&row)
            .and_then(|info| info.height)
            .or(self.default_row_height)
            .unwrap_or(DEFAULT_ROW_HEIGHT)
    }

    pub fn row_hidden(&self, row: u32) -> bool {
        self.rows.get(&row).is_some_and(|info| info.hidden)
    }

    /// the merged range a cell belongs to.
    pub fn merge_at(&self, row: u32, col: u32) -> Option<crate::address::Range> {
        let cell = crate::address::CellRef::new(row, col);
        self.merged
            .iter()
            .copied()
            .find(|range| range.contains(cell))
    }

    /// sets one column's width, splitting runs as needed.
    pub fn set_column_width(&mut self, col: u32, width: f64) {
        self.edit_column(col, |info| {
            info.width = Some(width);
            info.custom_width = true;
        });
    }

    /// changes one column's settings, splitting the run it is in.
    pub fn edit_column(&mut self, col: u32, change: impl FnOnce(&mut ColumnInfo)) {
        let mut runs = Vec::new();
        let mut found = None;
        for info in std::mem::take(&mut self.columns) {
            if col >= info.first && col <= info.last {
                if info.first < col {
                    runs.push(ColumnInfo {
                        last: col - 1,
                        ..info.clone()
                    });
                }
                found = Some(ColumnInfo {
                    first: col,
                    last: col,
                    ..info.clone()
                });
                if info.last > col {
                    runs.push(ColumnInfo {
                        first: col + 1,
                        ..info
                    });
                }
            } else {
                runs.push(info);
            }
        }
        let mut target = found.unwrap_or(ColumnInfo {
            first: col,
            last: col,
            ..ColumnInfo::default()
        });
        change(&mut target);
        runs.push(target);
        runs.sort_by_key(|info| info.first);
        self.columns = runs;
    }
}

/// a name for a range, a value or a formula.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinedName {
    pub name: String,
    /// what it refers to, without "=", e.g. "Sheet1!$A$1:$A$10"
    pub formula: String,
    /// the sheet it belongs to, or none for the whole workbook
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<usize>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// other attributes the model does not read
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
}

/// where day zero is. windows excel uses 1900 (with lotus 1-2-3's
/// pretend 29 february 1900); old mac files use 1904.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateSystem {
    #[default]
    Excel1900,
    Excel1904,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettings {
    /// recalculate after every change (false: only when asked)
    pub automatic: bool,
    /// iterative calculation is not supported: circular references are
    /// reported instead. kept so the setting survives a save
    pub iterate: bool,
    pub iterate_count: u32,
    pub iterate_delta: f64,
    /// asks excel and libreoffice to recalculate everything on opening
    pub full_calc_on_load: bool,
    /// excel's calculation engine id from the file (calcId)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calc_id: Option<String>,
}

impl Default for CalcSettings {
    fn default() -> CalcSettings {
        CalcSettings {
            automatic: true,
            iterate: false,
            iterate_count: 100,
            iterate_delta: 0.001,
            full_calc_on_load: false,
            calc_id: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
    /// iso 8601 text as the file has it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// a whole spreadsheet file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defined_names: Vec<DefinedName>,
    #[serde(default)]
    pub styles: StyleTable,
    #[serde(default)]
    pub date_system: DateSystem,
    #[serde(default)]
    pub calculation: CalcSettings,
    #[serde(default)]
    pub metadata: Metadata,
    /// the sheet shown first
    #[serde(default)]
    pub active_sheet: usize,
    /// true when the file came with macros (an .xlsm); they are kept
    /// byte for byte and never run
    #[serde(default)]
    pub has_macros: bool,
    #[serde(default)]
    pub carry: Carry,
}

impl Default for Workbook {
    fn default() -> Workbook {
        Workbook::new()
    }
}

impl Workbook {
    /// a workbook with one empty sheet, "Sheet1".
    pub fn new() -> Workbook {
        Workbook {
            sheets: vec![Sheet::new("Sheet1")],
            defined_names: Vec::new(),
            styles: StyleTable::default(),
            date_system: DateSystem::Excel1900,
            calculation: CalcSettings::default(),
            metadata: Metadata::default(),
            active_sheet: 0,
            has_macros: false,
            carry: Carry::default(),
        }
    }

    /// the sheet with this name, ignoring case as excel does.
    pub fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets
            .iter()
            .position(|sheet| sheet.name.eq_ignore_ascii_case(name))
    }

    pub fn sheet(&self, name: &str) -> Option<&Sheet> {
        self.sheet_index(name).map(|index| &self.sheets[index])
    }

    pub fn sheet_mut(&mut self, name: &str) -> Option<&mut Sheet> {
        let index = self.sheet_index(name)?;
        Some(&mut self.sheets[index])
    }

    /// the defined name visible from `sheet`: a sheet's own name wins
    /// over a workbook-wide one.
    pub fn find_name(&self, name: &str, sheet: Option<usize>) -> Option<&DefinedName> {
        let matches = |defined: &&DefinedName| defined.name.eq_ignore_ascii_case(name);
        self.defined_names
            .iter()
            .filter(matches)
            .find(|defined| defined.sheet.is_some() && defined.sheet == sheet)
            .or_else(|| {
                self.defined_names
                    .iter()
                    .filter(matches)
                    .find(|d| d.sheet.is_none())
            })
    }

    /// the table with this name, and the sheet it is on.
    pub fn find_table(&self, name: &str) -> Option<(usize, &Table)> {
        self.sheets.iter().enumerate().find_map(|(index, sheet)| {
            sheet
                .tables
                .iter()
                .find(|table| table.name.eq_ignore_ascii_case(name))
                .map(|table| (index, table))
        })
    }

    /// the table that covers a cell.
    pub fn table_at(&self, sheet: usize, row: u32, col: u32) -> Option<&Table> {
        let cell = crate::address::CellRef::new(row, col);
        self.sheets
            .get(sheet)?
            .tables
            .iter()
            .find(|table| table.range.contains(cell))
    }

    /// a name for a new sheet that no sheet has yet ("Sheet2").
    pub fn unused_sheet_name(&self, base: &str) -> String {
        (1..)
            .map(|n| format!("{base}{n}"))
            .find(|name| self.sheet_index(name).is_none())
            .unwrap_or_else(|| base.to_string())
    }

    /// sets a constant value (not a formula) in a cell, keeping its style.
    pub fn set_value(&mut self, sheet: usize, row: u32, col: u32, value: Value) {
        if let Some(sheet) = self.sheets.get_mut(sheet) {
            let cell = sheet.cells.entry(row, col);
            cell.value = value;
            cell.formula = None;
            cell.not_recalculated = false;
        }
    }

    /// sets a formula (excel syntax without "="); its value is worked out
    /// by the calculation engine.
    pub fn set_formula(&mut self, sheet: usize, row: u32, col: u32, text: &str) {
        if let Some(sheet) = self.sheets.get_mut(sheet) {
            let cell = sheet.cells.entry(row, col);
            cell.formula = Some(Formula::new(text.trim_start_matches('=')));
            cell.not_recalculated = false;
        }
    }

    /// the value in a cell, blank when there is none.
    pub fn value(&self, sheet: usize, row: u32, col: u32) -> Value {
        self.sheets
            .get(sheet)
            .map(|sheet| sheet.cells.value(row, col).clone())
            .unwrap_or_default()
    }

    /// shortcut for tests and examples: the value at "Sheet1!B3" or "B3"
    /// (first sheet).
    pub fn value_at(&self, address: &str) -> Value {
        let (sheet, cell) = match address.rsplit_once('!') {
            Some((name, cell)) => (self.sheet_index(name.trim_matches('\'')), cell),
            None => (Some(0), address),
        };
        match (sheet, crate::address::CellRef::parse(cell)) {
            (Some(sheet), Some(cell)) => self.value(sheet, cell.row, cell.col),
            _ => Value::Blank,
        }
    }
}
