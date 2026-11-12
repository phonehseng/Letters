//! tables (`a:tbl`): grid, rows and cells with spans, borders, fills,
//! insets and text. covered cells of a merge are kept, as pptx does.

use crate::xml::{escape_text, Element};

use super::super::dml::{
    attr, attr_bool, attr_i64, extra_after, extra_attributes, find_fill, opt_attr, read_line,
    write_fill, write_line, ReadContext, WriteContext, FILL_NAMES,
};
use super::super::text::{read_text_body, write_text_body};
use crate::model::{Anchor, CellBorders, SourceFormat, Table, TableCell, TableRow};
use crate::preserve::{reusable, source_of, unknown_attributes, unknown_children};

const TABLE_FLAGS: [&str; 6] = [
    "firstRow", "firstCol", "lastRow", "lastCol", "bandRow", "bandCol",
];
const CELL_BORDERS: [&str; 6] = [
    "a:lnL",
    "a:lnR",
    "a:lnT",
    "a:lnB",
    "a:lnTlToBr",
    "a:lnBlToTr",
];
const CELL_ATTRIBUTES: [&str; 5] = ["marL", "marR", "marT", "marB", "anchor"];

pub fn read_table(element: &Element, context: &ReadContext<'_>) -> Table {
    let properties = element.child("a:tblPr");
    let flags = properties.map(set_flags).unwrap_or_default();
    let mut extra = Vec::new();
    if let Some(properties) = properties {
        extra = unknown_attributes(properties, &TABLE_FLAGS);
        extra.extend(unknown_children(
            context.text,
            properties,
            &["a:tableStyleId"],
        ));
    }
    let columns = element
        .child("a:tblGrid")
        .map(|grid| {
            grid.children_named("a:gridCol")
                .map(|column| attr_i64(column, "w").unwrap_or(0))
                .collect()
        })
        .unwrap_or_default();
    Table {
        columns,
        rows: element
            .children_named("a:tr")
            .map(|row| read_row(row, context))
            .collect(),
        style_id: properties
            .and_then(|p| p.child("a:tableStyleId"))
            .map(|id| id.text()),
        flags,
        extra,
    }
}

fn set_flags(properties: &Element) -> Vec<String> {
    TABLE_FLAGS
        .iter()
        .filter(|flag| attr_bool(properties, flag) == Some(true))
        .map(|flag| flag.to_string())
        .collect()
}

fn read_row(element: &Element, context: &ReadContext<'_>) -> TableRow {
    let mut row = TableRow {
        height: attr_i64(element, "h").unwrap_or(0),
        cells: element
            .children_named("a:tc")
            .map(|cell| read_cell(cell, context))
            .collect(),
        source: None,
    };
    row.source = Some(source_of(
        SourceFormat::Pptx,
        &context.raw(element),
        &row,
        Vec::new(),
    ));
    row
}

fn read_borders(properties: Option<&Element>, context: &ReadContext<'_>) -> CellBorders {
    let border = |name: &str| {
        properties
            .and_then(|p| p.child(name))
            .map(|line| read_line(line, context))
    };
    CellBorders {
        left: border("a:lnL"),
        right: border("a:lnR"),
        top: border("a:lnT"),
        bottom: border("a:lnB"),
        diagonal_down: border("a:lnTlToBr"),
        diagonal_up: border("a:lnBlToTr"),
    }
}

fn read_anchor(value: &str) -> Anchor {
    match value {
        "ctr" => Anchor::Middle,
        "b" => Anchor::Bottom,
        _ => Anchor::Top,
    }
}

fn read_cell(element: &Element, context: &ReadContext<'_>) -> TableCell {
    let properties = element.child("a:tcPr");
    let margin = |name: &str| properties.and_then(|p| attr_i64(p, name));
    let margins = [
        margin("marL"),
        margin("marR"),
        margin("marT"),
        margin("marB"),
    ];
    let mut extra = Vec::new();
    if let Some(properties) = properties {
        extra = unknown_attributes(properties, &CELL_ATTRIBUTES);
        let known: Vec<&str> = [&CELL_BORDERS[..], &FILL_NAMES[..]].concat();
        extra.extend(unknown_children(context.text, properties, &known));
    }
    let span = |name: &str| element.attr(name).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut cell = TableCell {
        text: element
            .child("a:txBody")
            .map(|body| read_text_body(body, context))
            .unwrap_or_default(),
        grid_span: span("gridSpan"),
        row_span: span("rowSpan"),
        h_merge: attr_bool(element, "hMerge").unwrap_or(false),
        v_merge: attr_bool(element, "vMerge").unwrap_or(false),
        fill: properties.and_then(|p| find_fill(p, context)),
        borders: read_borders(properties, context),
        margins: margins.iter().any(Option::is_some).then_some(margins),
        anchor: properties.and_then(|p| p.attr("anchor")).map(read_anchor),
        extra,
        source: None,
    };
    cell.source = Some(source_of(
        SourceFormat::Pptx,
        &context.raw(element),
        &cell,
        Vec::new(),
    ));
    cell
}

/// writes a table; rows and cells that did not change keep their xml.
pub fn write_table(
    table: &Table,
    original: Option<(&str, &Element)>,
    context: &mut WriteContext<'_>,
) -> String {
    let mut out = String::from("<a:tbl>");
    let original_properties =
        original.and_then(|(source, tbl)| tbl.child("a:tblPr").map(|p| (source, p)));
    match original_properties {
        // the original properties stay while the switches and style are the same
        Some((source, element)) if properties_match(table, element) => {
            out.push_str(&source[element.start..element.end])
        }
        _ => out.push_str(&table_properties(table)),
    }
    out.push_str("<a:tblGrid>");
    for width in &table.columns {
        out.push_str(&format!("<a:gridCol{}/>", attr("w", width)));
    }
    out.push_str("</a:tblGrid>");
    for row in &table.rows {
        out.push_str(&write_row(row, context));
    }
    out.push_str("</a:tbl>");
    out
}

fn table_properties(table: &Table) -> String {
    let flags: String = table.flags.iter().map(|flag| attr(flag, "1")).collect();
    let style = table
        .style_id
        .as_ref()
        .map(|id| format!("<a:tableStyleId>{}</a:tableStyleId>", escape_text(id)))
        .unwrap_or_default();
    let children: String = table
        .extra
        .iter()
        .filter(|c| c.xml.starts_with('<'))
        .map(|c| c.xml.as_str())
        .collect();
    format!(
        "<a:tblPr{flags}{}>{children}{style}</a:tblPr>",
        extra_attributes(&table.extra)
    )
}

fn properties_match(table: &Table, element: &Element) -> bool {
    let style = element.child("a:tableStyleId").map(|id| id.text());
    set_flags(element) == table.flags && style == table.style_id
}

fn write_row(row: &TableRow, context: &mut WriteContext<'_>) -> String {
    let mut plain = row.clone();
    plain.source = None;
    if let Some(xml) = reusable(row.source.as_ref(), SourceFormat::Pptx, &plain) {
        return xml.to_string();
    }
    let mut out = format!("<a:tr{}>", attr("h", row.height));
    for cell in &row.cells {
        out.push_str(&write_cell(cell, context));
    }
    out.push_str("</a:tr>");
    out
}

fn write_cell(cell: &TableCell, context: &mut WriteContext<'_>) -> String {
    let mut plain = cell.clone();
    plain.source = None;
    if let Some(xml) = reusable(cell.source.as_ref(), SourceFormat::Pptx, &plain) {
        return xml.to_string();
    }
    let flag = |name: &str, on: bool| if on { attr(name, "1") } else { String::new() };
    let span = |name: &str, value: u32| {
        if value > 1 {
            attr(name, value)
        } else {
            String::new()
        }
    };
    format!(
        "<a:tc{}{}{}{}>{}{}</a:tc>",
        span("gridSpan", cell.grid_span),
        span("rowSpan", cell.row_span),
        flag("hMerge", cell.h_merge),
        flag("vMerge", cell.v_merge),
        write_text_body(&cell.text, "a:txBody", context),
        cell_properties(cell, context)
    )
}

fn anchor_name(anchor: Anchor) -> &'static str {
    match anchor {
        Anchor::Middle => "ctr",
        Anchor::Bottom => "b",
        _ => "t",
    }
}

fn cell_properties(cell: &TableCell, context: &mut WriteContext<'_>) -> String {
    let margins = cell.margins.unwrap_or([None; 4]);
    let mut out = format!(
        "<a:tcPr{}{}{}{}{}{}>",
        opt_attr("marL", margins[0]),
        opt_attr("marR", margins[1]),
        opt_attr("marT", margins[2]),
        opt_attr("marB", margins[3]),
        opt_attr("anchor", cell.anchor.map(anchor_name)),
        extra_attributes(&cell.extra),
    );
    let borders = &cell.borders;
    let lines = [
        (&borders.left, "a:lnL"),
        (&borders.right, "a:lnR"),
        (&borders.top, "a:lnT"),
        (&borders.bottom, "a:lnB"),
        (&borders.diagonal_down, "a:lnTlToBr"),
        (&borders.diagonal_up, "a:lnBlToTr"),
    ];
    for (line, name) in lines {
        if let Some(line) = line {
            out.push_str(&write_line(line, name, context));
        }
    }
    if let Some(fill) = &cell.fill {
        out.push_str(&write_fill(fill, context));
    }
    out.push_str(&extra_after(&cell.extra, None));
    for name in CELL_BORDERS.iter().chain(FILL_NAMES.iter()) {
        out.push_str(&extra_after(&cell.extra, Some(name)));
    }
    out.push_str("</a:tcPr>");
    out
}
