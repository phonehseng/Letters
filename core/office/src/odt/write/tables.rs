//! tables, rows and cells, with their automatic styles.

use super::*;

impl OdtWriter<'_> {
    // tables ------------------------------------------------------------------

    pub(super) fn table_xml(&mut self, table: &Table, master: Option<&str>) -> String {
        self.tables += 1;
        let layout = grid(table);
        let columns = layout.first().map(Vec::len).unwrap_or(0).max(1);
        let mut attributes = String::new();
        if let Some(master) = master {
            attributes.push_str(&format!(
                " style:master-page-name=\"{}\"",
                escape_attribute(master)
            ));
        }
        let mut properties = String::new();
        if let Some(width) = table.width {
            properties.push_str(&format!(" style:width=\"{}\"", twips_to_inches(width)));
        }
        let align = match (table.alignment, table.indent) {
            (Some(Alignment::Center), _) => "center",
            (Some(Alignment::Right), _) => "right",
            // an indent only counts for a table aligned to the left
            (Some(_), _) | (None, Some(_)) => "left",
            (None, None) => "margins",
        };
        properties.push_str(&format!(" table:align=\"{align}\""));
        if let Some(indent) = table.indent {
            properties.push_str(&format!(" fo:margin-left=\"{}\"", twips_to_inches(indent)));
        }
        let style = self.automatic_style(
            "LTab",
            "table",
            &attributes,
            &format!("<style:table-properties{properties}/>"),
        );
        let mut xml = format!(
            "<table:table table:name=\"Table{}\" table:style-name=\"{style}\">",
            self.tables
        );
        for column in 0..columns {
            let width = table.column_widths.get(column).copied().filter(|w| *w > 0);
            match width {
                Some(width) => {
                    let body = format!(
                        "<style:table-column-properties style:column-width=\"{}\"/>",
                        twips_to_inches(width)
                    );
                    let name = self.automatic_style("LCol", "table-column", "", &body);
                    xml.push_str(&format!(
                        "<table:table-column table:style-name=\"{name}\"/>"
                    ));
                }
                None => xml.push_str("<table:table-column/>"),
            }
        }
        let header_rows = table.rows.iter().take_while(|row| row.header).count();
        for (index, row) in table.rows.iter().enumerate() {
            if index == 0 && header_rows > 0 {
                xml.push_str("<table:table-header-rows>");
            }
            xml.push_str(&self.row_xml(row, &layout[index], table.cell_margins.as_ref()));
            if header_rows > 0 && index + 1 == header_rows {
                xml.push_str("</table:table-header-rows>");
            }
        }
        xml.push_str("</table:table>");
        xml
    }

    pub(super) fn row_xml(
        &mut self,
        row: &TableRow,
        slots: &[Slot],
        margins: Option<&CellMargins>,
    ) -> String {
        let mut xml = match crate::odt::table_props::row_properties(row) {
            Some(body) => {
                let name = self.automatic_style("LRow", "table-row", "", &body);
                format!("<table:table-row table:style-name=\"{name}\">")
            }
            None => String::from("<table:table-row>"),
        };
        for slot in slots {
            match *slot {
                Slot::Cell { cell, first: true } => {
                    xml.push_str(&self.cell_xml(&row.cells[cell], margins))
                }
                Slot::Empty => xml.push_str("<table:table-cell><text:p/></table:table-cell>"),
                _ => xml.push_str("<table:covered-table-cell/>"),
            }
        }
        xml.push_str("</table:table-row>");
        xml
    }

    pub(super) fn cell_xml(&mut self, cell: &TableCell, margins: Option<&CellMargins>) -> String {
        let mut attributes = String::from(" office:value-type=\"string\"");
        let layout = crate::odt::table_props::cell_attributes(cell, margins);
        if cell.borders.is_some()
            || cell.shading.is_some()
            || cell.vertical_align.is_some()
            || !layout.is_empty()
        {
            let body = cell_properties(
                cell.borders.as_ref(),
                cell.shading.as_deref(),
                cell.vertical_align,
            )
            .replacen(
                "<style:table-cell-properties",
                &format!("<style:table-cell-properties{layout}"),
                1,
            );
            let name = self.automatic_style("LCell", "table-cell", "", &body);
            attributes.push_str(&format!(" table:style-name=\"{name}\""));
        }
        if cell.col_span > 1 {
            attributes.push_str(&format!(
                " table:number-columns-spanned=\"{}\"",
                cell.col_span
            ));
        }
        if cell.row_span > 1 {
            attributes.push_str(&format!(" table:number-rows-spanned=\"{}\"", cell.row_span));
        }
        let content = self.blocks_xml(&cell.blocks);
        let content = if content.is_empty() {
            "<text:p/>".to_string()
        } else {
            content
        };
        format!("<table:table-cell{attributes}>{content}</table:table-cell>")
    }
}
