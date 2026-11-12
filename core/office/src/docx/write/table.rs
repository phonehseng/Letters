//! tables, with word's explicit covered cells for vertical merges.

use crate::model::{
    CellMargins, HeightRule, Table, TableCell, TablePosition, TableRow, TextDirection,
    VerticalAlign,
};
use crate::tables::{grid, Slot};
use crate::xml::XmlWriter;

use super::props::{alignment_value, borders_xml, empty, val};
use super::{DocxWriter, StoryOut};
use crate::docx::order::{self, Ordered};

/// a table needs some width per column; 6.5 inches shared out is word's
/// own choice for a new table on a letter page.
const DEFAULT_TABLE_WIDTH: i32 = 9360;

impl DocxWriter<'_> {
    pub(crate) fn write_table(&mut self, table: &Table, story: &mut StoryOut, out: &mut XmlWriter) {
        let layout = grid(table);
        let columns = layout.first().map(Vec::len).unwrap_or(0).max(1);
        let attributes = self.preserved_attributes(&table.preserved.attributes);
        out.open_owned("w:tbl", &attributes);
        out.raw(&format!("<w:tblPr>{}</w:tblPr>", self.table_props(table)));
        out.raw("<w:tblGrid>");
        for width in column_widths(table, columns) {
            out.raw(&empty("w:gridCol", &[("w:w", &width.to_string())]));
        }
        out.raw("</w:tblGrid>");
        // the reader keeps unknown table properties and unknown table
        // children in one list; the properties went into w:tblPr above
        if self.native {
            for xml in table
                .preserved
                .children
                .iter()
                .filter(|xml| !is_table_property(xml))
            {
                out.raw(xml);
            }
        }
        for (row_index, row) in table.rows.iter().enumerate() {
            self.write_row(table, row, &layout[row_index], story, out);
        }
        out.close("w:tbl");
    }

    fn table_props(&self, table: &Table) -> String {
        let mut ordered = Ordered::new(order::TABLE);
        if let Some(style) = &table.style_id {
            ordered.push("w:tblStyle", val("w:tblStyle", style));
        }
        match table.width {
            Some(width) => ordered.push(
                "w:tblW",
                empty("w:tblW", &[("w:w", &width.to_string()), ("w:type", "dxa")]),
            ),
            None if !self.native => ordered.push(
                "w:tblW",
                empty("w:tblW", &[("w:w", "0"), ("w:type", "auto")]),
            ),
            None => {}
        }
        if let Some(alignment) = table.alignment {
            ordered.push("w:jc", val("w:jc", alignment_value(alignment)));
        }
        if let Some(borders) = &table.borders {
            ordered.push("w:tblBorders", borders_xml("w:tblBorders", borders));
        }
        if let Some(position) = &table.position {
            ordered.push("w:tblpPr", table_position_xml(position));
            if !position.overlap {
                ordered.push("w:tblOverlap", val("w:tblOverlap", "never"));
            }
        }
        if let Some(indent) = table.indent {
            ordered.push(
                "w:tblInd",
                empty(
                    "w:tblInd",
                    &[("w:w", &indent.to_string()), ("w:type", "dxa")],
                ),
            );
        }
        if let Some(fixed) = table.fixed_layout {
            let kind = if fixed { "fixed" } else { "autofit" };
            ordered.push("w:tblLayout", empty("w:tblLayout", &[("w:type", kind)]));
        }
        if let Some(margins) = &table.cell_margins {
            ordered.push("w:tblCellMar", margins_xml("w:tblCellMar", margins));
        }
        if self.native {
            for xml in table
                .preserved
                .children
                .iter()
                .filter(|xml| is_table_property(xml))
            {
                ordered.push_raw(xml);
            }
        }
        ordered.finish()
    }

    fn write_row(
        &mut self,
        table: &Table,
        row: &TableRow,
        slots: &[Slot],
        story: &mut StoryOut,
        out: &mut XmlWriter,
    ) {
        let attributes = self.preserved_attributes(&row.preserved.attributes);
        out.open_owned("w:tr", &attributes);
        // a row's exceptions to the table's properties come before the
        // row's own properties, outside them
        if self.native {
            for xml in row
                .preserved
                .children
                .iter()
                .filter(|xml| is_row_level(xml))
            {
                out.raw(xml);
            }
        }
        let row_props = self.row_props(row);
        if !row_props.is_empty() {
            out.raw(&format!("<w:trPr>{row_props}</w:trPr>"));
        }
        let mut wrote_cell = false;
        for slot in slots {
            match *slot {
                Slot::Cell { cell, first: true } => {
                    self.write_cell(&row.cells[cell], story, out);
                    wrote_cell = true;
                }
                Slot::Covered {
                    row: from,
                    cell,
                    first: true,
                } => {
                    let covering = &table.rows[from].cells[cell];
                    out.raw(&covered_cell(covering));
                    wrote_cell = true;
                }
                _ => {}
            }
        }
        // word needs at least one cell in every row
        if !wrote_cell {
            out.raw("<w:tc><w:p/></w:tc>");
        }
        out.close("w:tr");
    }

    fn row_props(&self, row: &TableRow) -> String {
        let mut ordered = Ordered::new(order::ROW);
        if row.cant_split {
            ordered.push("w:cantSplit", empty("w:cantSplit", &[]));
        }
        if let Some(height) = row.height {
            let height = height.to_string();
            let mut attributes = vec![("w:val", height.as_str())];
            match row.height_rule {
                Some(HeightRule::Exact) => attributes.insert(0, ("w:hRule", "exact")),
                Some(HeightRule::AtLeast) => attributes.insert(0, ("w:hRule", "atLeast")),
                None => {}
            }
            ordered.push("w:trHeight", empty("w:trHeight", &attributes));
        }
        if row.header {
            ordered.push("w:tblHeader", empty("w:tblHeader", &[]));
        }
        if self.native {
            for xml in row
                .preserved
                .children
                .iter()
                .filter(|xml| !is_row_level(xml))
            {
                ordered.push_raw(xml);
            }
        }
        ordered.finish()
    }

    fn write_cell(&mut self, cell: &TableCell, story: &mut StoryOut, out: &mut XmlWriter) {
        let attributes = self.preserved_attributes(&cell.preserved.attributes);
        out.open_owned("w:tc", &attributes);
        let props = self.cell_props(cell, false);
        if !props.is_empty() {
            out.raw(&format!("<w:tcPr>{props}</w:tcPr>"));
        }
        self.write_blocks(&cell.blocks, story, out, None, false);
        // a cell has to end with a paragraph
        let ends_with_paragraph = matches!(
            cell.blocks.last(),
            Some(crate::model::Block::Paragraph(_)) | Some(crate::model::Block::Heading(_))
        );
        if !ends_with_paragraph {
            out.raw("<w:p/>");
        }
        out.close("w:tc");
    }

    fn cell_props(&self, cell: &TableCell, covered: bool) -> String {
        let mut ordered = Ordered::new(order::CELL);
        if let Some(width) = cell.width {
            ordered.push(
                "w:tcW",
                empty("w:tcW", &[("w:w", &width.to_string()), ("w:type", "dxa")]),
            );
        }
        if cell.col_span > 1 {
            ordered.push("w:gridSpan", val("w:gridSpan", &cell.col_span.to_string()));
        }
        if covered {
            ordered.push("w:vMerge", empty("w:vMerge", &[]));
        } else if cell.row_span > 1 {
            ordered.push("w:vMerge", val("w:vMerge", "restart"));
        }
        if let Some(borders) = &cell.borders {
            ordered.push("w:tcBorders", borders_xml("w:tcBorders", borders));
        }
        if let Some(fill) = &cell.shading {
            ordered.push(
                "w:shd",
                empty(
                    "w:shd",
                    &[("w:val", "clear"), ("w:color", "auto"), ("w:fill", fill)],
                ),
            );
        }
        if let Some(margins) = &cell.margins {
            ordered.push("w:tcMar", margins_xml("w:tcMar", margins));
        }
        if let Some(direction) = cell.text_direction {
            let value = match direction {
                TextDirection::Horizontal => "lrTb",
                TextDirection::Upward => "btLr",
                TextDirection::Downward => "tbRl",
            };
            ordered.push("w:textDirection", val("w:textDirection", value));
        }
        if let Some(align) = cell.vertical_align {
            let value = match align {
                VerticalAlign::Top => "top",
                VerticalAlign::Center => "center",
                VerticalAlign::Bottom => "bottom",
            };
            ordered.push("w:vAlign", val("w:vAlign", value));
        }
        if self.native && !covered {
            for xml in &cell.preserved.children {
                ordered.push_raw(xml);
            }
        }
        ordered.finish()
    }

    fn preserved_attributes(
        &self,
        attributes: &[crate::model::Attribute],
    ) -> Vec<(String, String)> {
        if !self.native {
            return Vec::new();
        }
        attributes
            .iter()
            .map(|attribute| (attribute.name.clone(), attribute.value.clone()))
            .collect()
    }
}

/// a preserved table child that is one of the table's properties (it goes
/// inside `w:tblPr`, like `w:tblW` or `w:tblLook`).
fn is_table_property(xml: &str) -> bool {
    let name = crate::docx::first_tag_name(xml);
    name.strip_prefix("w:")
        .is_some_and(|local| order::TABLE.contains(&local))
}

/// preserved row children that belong to the row itself (`w:tblPrEx`),
/// not inside its `w:trPr`. the reader keeps both kinds in one list.
fn is_row_level(xml: &str) -> bool {
    crate::docx::first_tag_name(xml) == "w:tblPrEx"
}

/// the cell word expects where a cell from above continues downwards.
fn covered_cell(covering: &TableCell) -> String {
    let mut props = String::new();
    if let Some(width) = covering.width {
        props.push_str(&empty(
            "w:tcW",
            &[("w:w", &width.to_string()), ("w:type", "dxa")],
        ));
    }
    if covering.col_span > 1 {
        props.push_str(&val("w:gridSpan", &covering.col_span.to_string()));
    }
    props.push_str("<w:vMerge/>");
    if let Some(borders) = &covering.borders {
        props.push_str(&borders_xml("w:tcBorders", borders));
    }
    format!("<w:tc><w:tcPr>{props}</w:tcPr><w:p/></w:tc>")
}

/// `w:tblpPr` for a floating table.
fn table_position_xml(position: &TablePosition) -> String {
    let anchor = |relative_to: &str| match relative_to {
        "page" => "page",
        "margin" => "margin",
        _ => "text",
    };
    let mut attributes: Vec<(String, String)> = vec![
        ("w:leftFromText".into(), position.distance.left.to_string()),
        (
            "w:rightFromText".into(),
            position.distance.right.to_string(),
        ),
        ("w:topFromText".into(), position.distance.top.to_string()),
        (
            "w:bottomFromText".into(),
            position.distance.bottom.to_string(),
        ),
        (
            "w:vertAnchor".into(),
            anchor(&position.vertical.relative_to).into(),
        ),
        (
            "w:horzAnchor".into(),
            anchor(&position.horizontal.relative_to).into(),
        ),
    ];
    match &position.horizontal.align {
        Some(align) => attributes.push(("w:tblpXSpec".into(), align.clone())),
        None => attributes.push(("w:tblpX".into(), position.horizontal.offset.to_string())),
    }
    match &position.vertical.align {
        Some(align) => attributes.push(("w:tblpYSpec".into(), align.clone())),
        None => attributes.push(("w:tblpY".into(), position.vertical.offset.to_string())),
    }
    let mut xml = XmlWriter::new();
    xml.empty_owned("w:tblpPr", &attributes);
    xml.finish()
}

/// `w:tblCellMar` or `w:tcMar`: the sides that are set.
fn margins_xml(name: &str, margins: &CellMargins) -> String {
    let mut xml = format!("<{name}>");
    for (side, value) in [
        ("w:top", margins.top),
        ("w:left", margins.left),
        ("w:bottom", margins.bottom),
        ("w:right", margins.right),
    ] {
        if let Some(value) = value {
            xml.push_str(&empty(
                side,
                &[("w:w", &value.to_string()), ("w:type", "dxa")],
            ));
        }
    }
    xml.push_str(&format!("</{name}>"));
    xml
}

/// grid column widths, filled in when the model has none or too few.
fn column_widths(table: &Table, columns: usize) -> Vec<i32> {
    let mut widths = table.column_widths.clone();
    let share = table.width.unwrap_or(DEFAULT_TABLE_WIDTH) / columns as i32;
    widths.truncate(columns);
    while widths.len() < columns {
        widths.push(share.max(1));
    }
    widths
}
