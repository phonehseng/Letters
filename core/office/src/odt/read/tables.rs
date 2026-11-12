//! tables, their columns, rows and cells.

use super::*;

impl OdtReader<'_> {
    // tables ----------------------------------------------------------------

    pub(super) fn read_table(&mut self, element: &Element, xml: &XmlDocument) -> Table {
        let mut table = Table::default();
        if let Some((style, _)) = element
            .attr("table:style-name")
            .and_then(|name| self.style(name))
        {
            table.width = style.width;
            table.alignment = style.alignment;
            table.indent = style.table_layout.margin_left.filter(|m| *m != 0);
        }
        for child in element.elements() {
            match child.name.as_str() {
                "table:table-column" => {
                    let width = child
                        .attr("table:style-name")
                        .and_then(|name| self.style(name))
                        .and_then(|(style, _)| style.width)
                        .unwrap_or(0);
                    let repeat = repeat(child, "table:number-columns-repeated");
                    table
                        .column_widths
                        .extend(std::iter::repeat_n(width, repeat));
                }
                "table:table-columns" | "table:table-header-columns" => {
                    for column in child.children_named("table:table-column") {
                        let width = column
                            .attr("table:style-name")
                            .and_then(|name| self.style(name))
                            .and_then(|(style, _)| style.width)
                            .unwrap_or(0);
                        table.column_widths.extend(std::iter::repeat_n(
                            width,
                            repeat(column, "table:number-columns-repeated"),
                        ));
                    }
                }
                "table:table-row" => self.read_rows(child, xml, false, &mut table),
                "table:table-header-rows" | "table:table-rows" => {
                    let header = child.name == "table:table-header-rows";
                    for row in child.children_named("table:table-row") {
                        self.read_rows(row, xml, header, &mut table);
                    }
                }
                _ => {}
            }
        }
        table
    }

    pub(super) fn read_rows(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        header: bool,
        table: &mut Table,
    ) {
        let mut row = TableRow {
            header,
            ..Default::default()
        };
        if let Some((style, _)) = element
            .attr("table:style-name")
            .and_then(|name| self.style(name))
        {
            if let Some((height, rule)) = style.table_layout.row_height {
                row.height = Some(height);
                row.height_rule = Some(rule);
            }
        }
        for cell in element.elements() {
            if cell.name != "table:table-cell" {
                continue;
            }
            let mut model = TableCell {
                col_span: cell
                    .attr("table:number-columns-spanned")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .max(1),
                row_span: cell
                    .attr("table:number-rows-spanned")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .max(1),
                ..Default::default()
            };
            if let Some((style, _)) = cell
                .attr("table:style-name")
                .and_then(|name| self.style(name))
            {
                model.borders = style.cell_borders.clone();
                model.shading = style.background.clone();
                model.vertical_align = style.vertical_align;
                model.margins = style.table_layout.padding;
                model.text_direction = style.table_layout.text_direction;
            }
            model.blocks = self.read_blocks(cell, xml);
            let copies = repeat(cell, "table:number-columns-repeated");
            for _ in 0..copies {
                row.cells.push(model.clone());
            }
        }
        for _ in 0..repeat(element, "table:number-rows-repeated") {
            table.rows.push(row.clone());
        }
    }
}

pub(super) fn repeat(element: &Element, attribute: &str) -> usize {
    let count = element
        .attr(attribute)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    if count > MAX_REPEAT {
        1
    } else {
        count.max(1)
    }
}
