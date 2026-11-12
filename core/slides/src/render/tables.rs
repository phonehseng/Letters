//! tables drawn: cell fills from the cell or the table style, borders,
//! merged cells, and rows that grow to fit their text as they do in
//! powerpoint.

use crate::color::{resolve, ColorContext};
use crate::inherit::Chain;
use crate::model::{ColorRef, Fill, Line, Object, Table, TableCell, TextBody};

use super::paint::{fill_paint, stroke_style};
use super::scene::{multiply, rectangle, segment_path, translate, Item, Matrix, StrokeStyle};
use super::text::{draw_text, TextTarget};
use super::{points, RenderContext};

/// powerpoint's default table style, "medium style 2 - accent 1"; its
/// look is built into powerpoint rather than written to the file
const MEDIUM_STYLE_2_ACCENT_1: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";

/// cell insets when a cell sets none (ecma-376 part 1, 21.1.3.17)
const CELL_INSET_SIDE: i64 = 91_440;
const CELL_INSET_TOP: i64 = 45_720;

/// the built-in style's cell colours and text colour for a cell.
struct StyleCell {
    fill: Option<ColorRef>,
    text: Option<ColorRef>,
    bold: bool,
}

fn style_cell(table: &Table, row: usize) -> StyleCell {
    let uses_default = table.style_id.as_deref() == Some(MEDIUM_STYLE_2_ACCENT_1);
    if !uses_default {
        return StyleCell {
            fill: None,
            text: None,
            bold: false,
        };
    }
    let header = row == 0 && table.flags.iter().any(|f| f == "firstRow");
    let banded = table.flags.iter().any(|f| f == "bandRow");
    let body_row = if header || !table.flags.iter().any(|f| f == "firstRow") {
        row
    } else {
        row - 1
    };
    if header {
        return StyleCell {
            fill: Some(ColorRef::scheme("accent1")),
            text: Some(ColorRef::scheme("lt1")),
            bold: true,
        };
    }
    // odd banded rows are 40 % accent, the rest of the table 20 %
    let tint = if banded && body_row % 2 == 0 {
        40_000
    } else {
        20_000
    };
    StyleCell {
        fill: Some(ColorRef::scheme("accent1").with("tint", tint)),
        text: Some(ColorRef::scheme("dk1")),
        bold: false,
    }
}

pub fn draw_table(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    object: &Object,
    table: &Table,
    matrix: Matrix,
    items: &mut Vec<Item>,
) {
    let columns: Vec<f64> = table.columns.iter().map(|w| points(*w)).collect();
    let lefts = running_sums(&columns);
    let heights = row_heights(context, chain, object, table, &columns);
    let tops = running_sums(&heights);
    let mut cells = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        for (column_index, cell) in row.cells.iter().enumerate() {
            if cell.h_merge || cell.v_merge || column_index >= columns.len() {
                continue;
            }
            let span_end = (column_index + cell.grid_span.max(1) as usize).min(columns.len());
            let row_end = (row_index + cell.row_span.max(1) as usize).min(heights.len());
            let rect = [
                lefts[column_index],
                tops[row_index],
                lefts[span_end],
                tops[row_end],
            ];
            let style = style_cell(table, row_index);
            draw_cell(context, chain, object, cell, &style, rect, matrix, items);
            cells.push(rect);
        }
    }
    if table.style_id.as_deref() == Some(MEDIUM_STYLE_2_ACCENT_1) {
        draw_style_grid(context, &cells, matrix, items);
    }
}

fn running_sums(values: &[f64]) -> Vec<f64> {
    let mut sums = vec![0.0];
    for value in values {
        sums.push(sums.last().copied().unwrap_or(0.0) + value);
    }
    sums
}

/// each row's height: what the file says, or more when its text needs it.
fn row_heights(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    object: &Object,
    table: &Table,
    columns: &[f64],
) -> Vec<f64> {
    let mut heights = Vec::new();
    for row in &table.rows {
        let mut height = points(row.height);
        for (index, cell) in row.cells.iter().enumerate() {
            if cell.h_merge || cell.v_merge || cell.row_span > 1 || index >= columns.len() {
                continue;
            }
            let end = (index + cell.grid_span.max(1) as usize).min(columns.len());
            let width: f64 = columns[index..end].iter().sum();
            let insets = cell_insets(cell);
            let body = cell_body(cell, false);
            let target = TextTarget {
                object,
                body: &body,
                rect: [0.0, 0.0, width - insets[0] - insets[2], 0.0],
                matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                color: None,
            };
            let laid = draw_text(context, chain, &target, &mut Vec::new());
            height = height.max(laid.content_height + insets[1] + insets[3]);
        }
        heights.push(height);
    }
    heights
}

fn cell_insets(cell: &TableCell) -> [f64; 4] {
    let margins = cell.margins.unwrap_or([None; 4]);
    [
        margins[0].unwrap_or(CELL_INSET_SIDE),
        margins[2].unwrap_or(CELL_INSET_TOP),
        margins[1].unwrap_or(CELL_INSET_SIDE),
        margins[3].unwrap_or(CELL_INSET_TOP),
    ]
    .map(points)
}

/// the cell's text with no insets of its own: a cell's insets are its
/// margins, which the caller takes off the cell's rectangle. a style's
/// bold header makes runs bold unless they say otherwise.
fn cell_body(cell: &TableCell, bold: bool) -> TextBody {
    let mut body = cell.text.clone();
    body.body.inset_left = Some(0);
    body.body.inset_right = Some(0);
    body.body.inset_top = Some(0);
    body.body.inset_bottom = Some(0);
    if bold {
        for paragraph in &mut body.paragraphs {
            for run in &mut paragraph.runs {
                run.props_mut().bold.get_or_insert(true);
            }
        }
    }
    body
}

#[allow(clippy::too_many_arguments)]
fn draw_cell(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    object: &Object,
    cell: &TableCell,
    style: &StyleCell,
    rect: [f64; 4],
    matrix: Matrix,
    items: &mut Vec<Item>,
) {
    let [left, top, right, bottom] = rect;
    let colors = context.colors();
    let fill = cell
        .fill
        .clone()
        .or_else(|| style.fill.clone().map(|color| Fill::Solid { color }));
    if let Some(paint) = fill.and_then(|f| fill_paint(&f, &colors, right - left, bottom - top)) {
        items.push(Item::Fill {
            matrix: multiply(&matrix, &translate(left, top)),
            paths: rectangle(0.0, 0.0, right - left, bottom - top),
            paint,
        });
    }
    draw_cell_borders(&colors, cell, rect, matrix, items);
    let text_color = style.text.as_ref().map(|c| resolve(c, &colors));
    let body = cell_body(cell, style.bold);
    let insets = cell_insets(cell);
    let inner = [
        left + insets[0],
        top + insets[1],
        right - insets[2],
        bottom - insets[3],
    ];
    let target = TextTarget {
        object,
        body: &body,
        rect: inner,
        matrix,
        color: text_color,
    };
    draw_text(context, chain, &target, items);
}

/// a point in a table's own space.
type Point = (f64, f64);

fn draw_cell_borders(
    colors: &ColorContext<'_>,
    cell: &TableCell,
    rect: [f64; 4],
    matrix: Matrix,
    items: &mut Vec<Item>,
) {
    let [left, top, right, bottom] = rect;
    let sides: [(&Option<Line>, Point, Point); 4] = [
        (&cell.borders.left, (left, top), (left, bottom)),
        (&cell.borders.right, (right, top), (right, bottom)),
        (&cell.borders.top, (left, top), (right, top)),
        (&cell.borders.bottom, (left, bottom), (right, bottom)),
    ];
    for (line, from, to) in sides {
        let Some(style) = line.as_ref().and_then(|l| stroke_style(l, colors)) else {
            continue;
        };
        items.push(Item::Stroke {
            matrix,
            paths: segment_path(from, to),
            style,
        });
    }
}

/// the built-in style's white 1 pt lines around every cell (a merged
/// cell is one cell, so no line crosses it).
fn draw_style_grid(
    context: &RenderContext<'_>,
    cells: &[[f64; 4]],
    matrix: Matrix,
    items: &mut Vec<Item>,
) {
    let colors = context.colors();
    let style = StrokeStyle {
        color: resolve(&ColorRef::scheme("lt1"), &colors),
        width: 1.0,
        dashes: Vec::new(),
        cap: 0,
        join: 0,
    };
    // separate lines, as powerpoint draws them, not closed rectangles
    for [left, top, right, bottom] in cells {
        let corners = [
            (*left, *top),
            (*right, *top),
            (*right, *bottom),
            (*left, *bottom),
        ];
        for side in 0..4 {
            items.push(Item::Stroke {
                matrix,
                paths: segment_path(corners[side], corners[(side + 1) % 4]),
                style: style.clone(),
            });
        }
    }
}
