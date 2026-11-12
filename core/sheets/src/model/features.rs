//! everything on a sheet besides the cells: sizes, panes, merged
//! ranges, conditional formats, validations, links, notes, tables,
//! filters, printing and drawn objects.

use serde::{Deserialize, Serialize};

use crate::address::Range;

use super::style::Color;

/// settings for a run of columns.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    /// zero based, both included
    pub first: u32,
    pub last: u32,
    /// in characters of the default font, as excel measures it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom_width: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_level: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub best_fit: bool,
}

/// settings for one row; only rows that differ from the default are kept.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowInfo {
    /// points
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom_height: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// a style for the whole row (excel's customFormat)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_level: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    /// row attributes the model does not read
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
}

impl RowInfo {
    pub fn is_default(&self) -> bool {
        *self == RowInfo::default()
    }
}

/// rows above and columns left of the split stay in place when scrolling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenPanes {
    pub rows: u32,
    pub cols: u32,
}

/// one rule of a conditional format.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalRule {
    /// excel's rule type: "cellIs", "expression", "colorScale",
    /// "dataBar", "iconSet", "top10", "duplicateValues", "containsText"...
    pub kind: String,
    /// for "cellIs": "between", "equal", "greaterThan"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    /// formulas without "=", relative to the range's top-left cell
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formulas: Vec<String>,
    /// the differential format applied when the rule matches
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<u32>,
    pub priority: i32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop_if_true: bool,
    /// for text rules
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// other attributes ("rank", "percent", "bottom", "timePeriod"...)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
    /// colour scale, data bar or icon set details as original xml
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details_xml: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalFormat {
    pub ranges: Vec<Range>,
    pub rules: Vec<ConditionalRule>,
}

/// what a data validation allows.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataValidation {
    pub ranges: Vec<Range>,
    /// "any", "whole", "decimal", "list", "date", "time", "textLength", "custom"
    pub kind: String,
    /// "between", "notBetween", "equal", "notEqual", "greaterThan"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    /// for a list: a quoted list ("\"Yes,No\"") or a range; without "="
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula2: Option<String>,
    #[serde(default)]
    pub allow_blank: bool,
    /// excel's attribute is inverted: true hides the drop-down arrow
    #[serde(default)]
    pub hide_dropdown: bool,
    #[serde(default)]
    pub show_input_message: bool,
    #[serde(default)]
    pub show_error_message: bool,
    /// "stop", "warning", "information"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hyperlink {
    pub range: Range,
    /// a web address or file, when the link leaves the workbook
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// a place inside the workbook, e.g. "Sheet2!A1"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

/// a note on a cell (excel calls them notes now; older files "comments").
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub row: u32,
    pub col: u32,
    pub author: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumn {
    pub name: String,
    /// "sum", "average", "count", "max", "min", "custom"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totals_function: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totals_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totals_formula: Option<String>,
    /// a formula filled down the whole column
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calculated_formula: Option<String>,
}

/// a structured range: header row, data and perhaps a totals row.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub id: u32,
    /// the name formulas use ("Table1[Price]")
    pub name: String,
    pub range: Range,
    pub header_row: bool,
    pub totals_row: bool,
    pub columns: Vec<TableColumn>,
    /// e.g. "TableStyleMedium2"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default)]
    pub banded_rows: bool,
    #[serde(default)]
    pub banded_columns: bool,
    #[serde(default)]
    pub first_column: bool,
    #[serde(default)]
    pub last_column: bool,
    /// filter buttons in the header
    #[serde(default)]
    pub filter: Option<AutoFilter>,
}

impl Table {
    /// the rows holding data (no header, no totals)
    pub fn data_range(&self) -> Option<Range> {
        let mut first = self.range.start.row;
        let mut last = self.range.end.row;
        if self.header_row {
            first += 1;
        }
        if self.totals_row {
            last = last.checked_sub(1)?;
        }
        (first <= last)
            .then(|| Range::from_bounds(first, self.range.start.col, last, self.range.end.col))
    }

    pub fn column_index(&self, name: &str) -> Option<u32> {
        self.columns
            .iter()
            .position(|column| column.name.eq_ignore_ascii_case(name))
            .map(|index| index as u32)
    }
}

/// one column's filter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ColumnFilter {
    /// show rows whose value is one of these (shown text); `blank` keeps empty cells
    Values { values: Vec<String>, blank: bool },
    /// one or two conditions such as (">", "10")
    Custom {
        conditions: Vec<(String, String)>,
        and: bool,
    },
    /// top or bottom n items or percent
    Top {
        count: f64,
        percent: bool,
        bottom: bool,
    },
    /// a filter the model does not read, as original xml
    Other { xml: String },
}

/// filter buttons over a range, with what each column filters on and
/// how the range was last sorted.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFilter {
    pub range: Range,
    /// (column offset inside the range, filter)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<(u32, ColumnFilter)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<SortState>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortState {
    pub range: Range,
    /// (column or row being sorted on, descending)
    pub keys: Vec<(Range, bool)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    #[default]
    Default,
    Portrait,
    Landscape,
}

/// page margins in inches, as excel keeps them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Margins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub header: f64,
    pub footer: f64,
}

impl Default for Margins {
    fn default() -> Margins {
        // excel's "normal" margins
        Margins {
            left: 0.7,
            right: 0.7,
            top: 0.75,
            bottom: 0.75,
            header: 0.3,
            footer: 0.3,
        }
    }
}

/// how the sheet prints.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintSetup {
    /// what to print; the used range when empty
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub area: Vec<Range>,
    /// rows repeated at the top of every page, zero based, both included
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_rows: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_cols: Option<(u32, u32)>,
    /// manual page breaks: a new page starts at each of these rows / columns
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_breaks: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub col_breaks: Vec<u32>,
    #[serde(default)]
    pub orientation: Orientation,
    /// excel's paper size number: 1 letter, 9 a4, 5 legal...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper_size: Option<u32>,
    /// percent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
    /// fit to this many pages wide / tall (0 means "as many as needed")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit_to: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margins: Option<Margins>,
    /// excel header and footer codes such as "&C&P of &N"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<String>,
    #[serde(default)]
    pub center_horizontally: bool,
    #[serde(default)]
    pub center_vertically: bool,
    #[serde(default)]
    pub grid_lines: bool,
    #[serde(default)]
    pub headings: bool,
}

/// the kinds of chart the model understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartKind {
    Bar,
    Column,
    Line,
    Pie,
    Doughnut,
    Scatter,
    Area,
    Other,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    /// formulas such as "Sheet1!$B$1"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<String>,
}

/// what a chart shows. it is read from the chart part, which itself is
/// carried byte for byte; a chart made in letters is written from this.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub kind: ChartKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub series: Vec<ChartSeries>,
    pub legend: bool,
}

/// where an object sits: from one cell corner to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub from_row: u32,
    pub from_col: u32,
    pub to_row: u32,
    pub to_col: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DrawingObject {
    Chart {
        anchor: Anchor,
        chart: Chart,
        /// the chart part inside the package, when it came from a file
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<String>,
    },
    Image {
        anchor: Anchor,
        name: String,
        /// the picture's part inside the package
        part: String,
    },
    Shape {
        anchor: Anchor,
        name: String,
    },
}

/// excel's three sheet states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    VeryHidden,
}

/// a sheet's tab colour and view settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_color: Option<Color>,
    /// percent
    pub zoom: u32,
    pub show_grid_lines: bool,
    pub show_formulas: bool,
    pub right_to_left: bool,
    /// the active cell
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_cell: Option<crate::address::CellRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen: Option<FrozenPanes>,
    #[serde(default)]
    pub selected: bool,
}

impl Default for SheetView {
    fn default() -> SheetView {
        SheetView {
            tab_color: None,
            zoom: 100,
            show_grid_lines: true,
            show_formulas: false,
            right_to_left: false,
            active_cell: None,
            frozen: None,
            selected: false,
        }
    }
}
