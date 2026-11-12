//! what the grid needs to draw a sheet, in small answers: the tabs, one
//! sheet's sizes and features, the look of each style, and the cells of
//! the rows on screen. nothing here walks the whole sheet except the
//! line sizes, which only list the rows and columns that differ.

use letters_sheets::model::{DrawingObject, SheetView, Visibility};
use letters_sheets::numfmt::Locale;
use letters_sheets::view::{cells_in, own_cells_in, ViewCell};
use letters_sheets::{CellRef, Range, Workbook};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetTab {
    pub name: String,
    pub hidden: bool,
    /// "#RRGGBB"
    pub color: Option<String>,
}

pub fn sheet_tabs(book: &Workbook) -> Vec<SheetTab> {
    let theme = &book.styles.theme_colors;
    book.sheets
        .iter()
        .map(|sheet| SheetTab {
            name: sheet.name.clone(),
            hidden: sheet.visibility != Visibility::Visible,
            color: sheet.view.tab_color.as_ref().and_then(|c| c.to_css(theme)),
        })
        .collect()
}

/// a column run or a row whose size or visibility isn't the default.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineSize {
    pub first: u32,
    pub last: u32,
    /// characters for columns, points for rows; none is the default
    pub size: Option<f64>,
    pub hidden: bool,
}

/// a chart, picture or shape: the grid shows a placeholder where it sits.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placeholder {
    /// "chart", "picture" or "shape"
    pub kind: &'static str,
    pub title: String,
    pub from: CellRef,
    pub to: CellRef,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetInfo {
    /// rows and columns up to the last used cell (or drawing), at least one
    pub used_rows: u32,
    pub used_cols: u32,
    /// points
    pub default_row_height: f64,
    /// characters
    pub default_column_width: f64,
    pub rows: Vec<LineSize>,
    pub columns: Vec<LineSize>,
    pub frozen_rows: u32,
    pub frozen_cols: u32,
    pub merged: Vec<Range>,
    pub placeholders: Vec<Placeholder>,
    pub filter: Option<Range>,
    pub tables: Vec<(String, Range)>,
    pub show_grid_lines: bool,
    pub active_cell: Option<CellRef>,
    pub protected: bool,
}

pub fn sheet_info(book: &Workbook, sheet: usize) -> Option<SheetInfo> {
    let target = book.sheets.get(sheet)?;
    let placeholders = placeholders(book, sheet);
    let (used_rows, used_cols) = used_extent(book, sheet, &placeholders);
    let SheetView {
        frozen,
        show_grid_lines,
        active_cell,
        ..
    } = target.view.clone();
    Some(SheetInfo {
        used_rows,
        used_cols,
        default_row_height: target.row_height(u32::MAX),
        default_column_width: target
            .default_column_width
            .unwrap_or(letters_sheets::model::DEFAULT_COLUMN_WIDTH),
        rows: row_sizes(target),
        columns: column_sizes(target),
        frozen_rows: frozen.map(|f| f.rows).unwrap_or(0),
        frozen_cols: frozen.map(|f| f.cols).unwrap_or(0),
        merged: target.merged.clone(),
        placeholders,
        filter: target.auto_filter.as_ref().map(|filter| filter.range),
        tables: target
            .tables
            .iter()
            .map(|table| (table.name.clone(), table.range))
            .collect(),
        show_grid_lines,
        active_cell,
        protected: target.protected,
    })
}

fn used_extent(book: &Workbook, sheet: usize, placeholders: &[Placeholder]) -> (u32, u32) {
    let target = &book.sheets[sheet];
    let mut rows = 1;
    let mut cols = 1;
    if let Some(used) = target.cells.used_range() {
        rows = used.end.row + 1;
        cols = used.end.col + 1;
    }
    for corner in target.merged.iter().map(|range| range.end) {
        rows = rows.max(corner.row + 1);
        cols = cols.max(corner.col + 1);
    }
    for placeholder in placeholders {
        rows = rows.max(placeholder.to.row + 1);
        cols = cols.max(placeholder.to.col + 1);
    }
    (rows, cols)
}

fn row_sizes(sheet: &letters_sheets::Sheet) -> Vec<LineSize> {
    sheet
        .rows
        .iter()
        .filter(|(_, info)| info.height.is_some() || info.hidden)
        .map(|(row, info)| LineSize {
            first: *row,
            last: *row,
            size: info.height,
            hidden: info.hidden,
        })
        .collect()
}

fn column_sizes(sheet: &letters_sheets::Sheet) -> Vec<LineSize> {
    sheet
        .columns
        .iter()
        .filter(|info| info.width.is_some() || info.hidden)
        .map(|info| LineSize {
            first: info.first,
            last: info.last,
            size: info.width,
            hidden: info.hidden,
        })
        .collect()
}

fn placeholders(book: &Workbook, sheet: usize) -> Vec<Placeholder> {
    book.sheets[sheet]
        .drawings
        .iter()
        .map(|drawing| {
            let (kind, title, anchor) = match drawing {
                DrawingObject::Chart { anchor, chart, .. } => (
                    "chart",
                    chart.title.clone().unwrap_or_else(|| "Chart".into()),
                    anchor,
                ),
                DrawingObject::Image { anchor, name, .. } => ("picture", name.clone(), anchor),
                DrawingObject::Shape { anchor, name } => ("shape", name.clone(), anchor),
            };
            Placeholder {
                kind,
                title,
                from: CellRef::new(anchor.from_row, anchor.from_col),
                to: CellRef::new(anchor.to_row, anchor.to_col),
            }
        })
        .collect()
}

/// how a style looks, for drawing; colours come with each cell.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleLook {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// points; none is the workbook's default (11)
    pub size: Option<f64>,
    pub font: Option<String>,
    pub wrap: bool,
    /// "top", "center" or "bottom"
    pub vertical: String,
    pub number_format: String,
    /// css colours of the top, right, bottom and left edges, when drawn
    pub borders: [Option<BorderLook>; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BorderLook {
    pub color: String,
    /// css pixels
    pub width: f64,
    pub dashed: bool,
}

/// every style from index `from` on (the ui keeps the ones it has).
pub fn style_looks(book: &Workbook, from: usize) -> Vec<StyleLook> {
    let theme = &book.styles.theme_colors;
    book.styles
        .styles
        .iter()
        .skip(from)
        .map(|style| {
            let edge = |edge: &Option<letters_sheets::model::BorderEdge>| {
                edge.as_ref().map(|edge| border_look(edge, theme))
            };
            let border = &style.border;
            StyleLook {
                bold: style.font.bold,
                italic: style.font.italic,
                underline: style.font.underline.is_some(),
                strike: style.font.strike,
                size: style.font.size,
                font: style.font.name.clone(),
                wrap: style.alignment.wrap_text,
                vertical: style
                    .alignment
                    .vertical
                    .clone()
                    .unwrap_or_else(|| "bottom".into()),
                number_format: style.number_format.clone(),
                borders: [
                    edge(&border.top),
                    edge(&border.right),
                    edge(&border.bottom),
                    edge(&border.left),
                ],
            }
        })
        .collect()
}

fn border_look(edge: &letters_sheets::model::BorderEdge, theme: &[String]) -> BorderLook {
    let width = match edge.style.as_str() {
        "medium" | "mediumDashed" | "mediumDashDot" => 2.0,
        "thick" | "double" => 3.0,
        _ => 1.0,
    };
    BorderLook {
        color: edge
            .color
            .as_ref()
            .and_then(|color| color.to_css(theme))
            .unwrap_or_else(|| "#000000".into()),
        width,
        dashed: edge.style.contains("ash") || edge.style.contains("otted"),
    }
}

/// the cells of a block of the sheet, as the grid draws them.
pub fn cells(book: &Workbook, sheet: usize, range: Range) -> Vec<ViewCell> {
    cells_in(book, sheet, range, &Locale::default())
}

/// a copied range as tab-separated text (what other apps paste as cells)
/// and as an html table (what keeps formatting in word processors).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopiedCells {
    pub text: String,
    pub html: String,
}

pub fn copy_cells(book: &Workbook, sheet: usize, range: Range) -> CopiedCells {
    // the values only: conditional formats add empty cells that are only drawn
    let shown: std::collections::HashMap<(u32, u32), String> =
        own_cells_in(book, sheet, range, &Locale::default())
            .into_iter()
            .map(|cell| ((cell.row, cell.col), cell.text))
            .collect();
    let mut lines = Vec::with_capacity(range.rows() as usize);
    for row in range.start.row..=range.end.row {
        let fields: Vec<String> = (range.start.col..=range.end.col)
            .map(|col| {
                shown
                    .get(&(row, col))
                    .map(|text| tsv_field(text))
                    .unwrap_or_default()
            })
            .collect();
        lines.push(fields.join("\t"));
    }
    CopiedCells {
        // excel ends the copied text with a line break too
        text: lines.join("\r\n") + "\r\n",
        html: letters_sheets::export::to_html(book, sheet, Some(range)),
    }
}

/// excel quotes a field that holds a tab, a line break or a quote
fn tsv_field(text: &str) -> String {
    if text.contains(['\t', '\n', '\r', '"']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// the different values a column of a range shows, for the filter menu.
pub fn column_values(book: &Workbook, sheet: usize, range: Range, column: u32) -> Vec<String> {
    /// a filter list longer than this is searched, not scrolled
    const MOST_VALUES: usize = 1000;
    let col = range.start.col + column;
    let first = range.start.row + 1;
    if first > range.end.row {
        return Vec::new();
    }
    let block = Range::from_bounds(first, col, range.end.row, col);
    // own cells only, so empty cells a conditional format marks still count as blank
    let mut values: Vec<String> = own_cells_in(book, sheet, block, &Locale::default())
        .into_iter()
        .map(|cell| cell.text)
        .collect();
    let has_blank = (values.len() as u64) < block.cell_count();
    values.sort_by_key(|value| value.to_lowercase());
    values.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    values.truncate(MOST_VALUES);
    if has_blank {
        values.push(String::new());
    }
    values
}
