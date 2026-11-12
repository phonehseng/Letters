//! opendocument tables inside frames (`table:table`) read into the
//! model's table: columns, rows, cells with spans and fills, text.

use super::read_paint::read_fill;
use super::read_shapes::{placeholder_of, read_transform, ShapeContext};
use super::read_text::{read_text_body, TextContext};
use super::styles::{get, Style};
use super::units::length;
use crate::model::{GraphicFrame, GraphicKind, Table, TableCell, TableRow};
use crate::xml::Element;

pub fn read_table_frame(
    frame: &Element,
    table: &Element,
    context: &ShapeContext<'_>,
) -> GraphicFrame {
    let columns: Vec<i64> = table
        .children_named("table:table-column")
        .flat_map(|column| {
            let style = context
                .styles
                .resolve("table-column", column.attr("table:style-name"));
            let width = style_width(&style);
            let repeat: usize = column
                .attr("table:number-columns-repeated")
                .and_then(|r| r.parse().ok())
                .unwrap_or(1);
            std::iter::repeat_n(width, repeat.min(1000))
        })
        .collect();
    let rows = table
        .children_named("table:table-row")
        .map(|row| read_row(row, context))
        .collect();
    GraphicFrame {
        id: context.id(),
        name: frame.attr("draw:name").unwrap_or("").to_string(),
        description: None,
        hidden: false,
        placeholder: placeholder_of(frame),
        transform: Some(read_transform(frame)),
        kind: GraphicKind::Table(Table {
            columns,
            rows,
            ..Default::default()
        }),
        fallback_media_id: None,
        source: None,
    }
}

/// a column's or row's size from its style's `style:column-width` or
/// `style:row-height` (both live in the `table-column`/`table-row`
/// properties, which `Style` keeps with the graphic ones).
fn style_width(style: &Style) -> i64 {
    get(&style.graphic, "style:column-width")
        .or_else(|| get(&style.graphic, "style:row-height"))
        .and_then(length)
        .unwrap_or(0)
}

fn read_row(row: &Element, context: &ShapeContext<'_>) -> TableRow {
    let style = context
        .styles
        .resolve("table-row", row.attr("table:style-name"));
    let cells = row
        .elements()
        .filter(|c| c.name == "table:table-cell" || c.name == "table:covered-table-cell")
        .map(|cell| read_cell(cell, context))
        .collect();
    TableRow {
        height: style_width(&style),
        cells,
        source: None,
    }
}

fn read_cell(cell: &Element, context: &ShapeContext<'_>) -> TableCell {
    let style = context
        .styles
        .resolve("table-cell", cell.attr("table:style-name"));
    let text_context = TextContext {
        styles: context.styles,
        paragraph: style.paragraph.clone(),
        text: style.text.clone(),
    };
    let span = |name: &str| cell.attr(name).and_then(|s| s.parse().ok()).unwrap_or(1);
    TableCell {
        text: read_text_body(cell, &style.graphic, &text_context),
        grid_span: span("table:number-columns-spanned"),
        row_span: span("table:number-rows-spanned"),
        h_merge: cell.name == "table:covered-table-cell",
        fill: read_fill(&style.graphic, context),
        ..Default::default()
    }
}
