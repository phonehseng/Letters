//! parts a worksheet points to that the model rebuilds: notes (the
//! comments part plus the legacy vml drawing excel still needs to show
//! them) and tables.

use crate::model::{Comment, Table};
use crate::xml::{escape_attribute, escape_text, XmlWriter};

use super::write_sections::auto_filter;

pub fn comments_xml(comments: &[Comment]) -> String {
    let mut authors: Vec<&str> = Vec::new();
    for comment in comments {
        if !authors.contains(&comment.author.as_str()) {
            authors.push(&comment.author);
        }
    }
    let mut writer = XmlWriter::with_declaration();
    writer.open(
        "comments",
        &[(
            "xmlns",
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
        )],
    );
    writer.open("authors", &[]);
    for author in &authors {
        writer.text_element("author", &[], author);
    }
    writer.close("authors");
    writer.open("commentList", &[]);
    for comment in comments {
        let cell = crate::address::CellRef::new(comment.row, comment.col).to_a1();
        let author = authors
            .iter()
            .position(|a| *a == comment.author)
            .unwrap_or(0)
            .to_string();
        writer.open("comment", &[("ref", &cell), ("authorId", &author)]);
        writer.open("text", &[]);
        writer.open("r", &[]);
        writer.text_element("t", &[("xml:space", "preserve")], &comment.text);
        writer.close("r");
        writer.close("text");
        writer.close("comment");
    }
    writer.close("commentList");
    writer.close("comments");
    writer.finish()
}

/// the vml drawing excel needs to show notes: one hidden note shape per
/// comment, anchored next to its cell. the shape type and attributes are
/// the ones excel itself writes for a note.
pub fn notes_vml(comments: &[Comment]) -> String {
    let mut out = String::from(
        "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\
<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"1\"/></o:shapelayout>\
<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\">\
<v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>",
    );
    for (index, comment) in comments.iter().enumerate() {
        let id = 1025 + index;
        let anchor = format!(
            "{}, 15, {}, 2, {}, 15, {}, 16",
            comment.col + 1,
            comment.row,
            comment.col + 3,
            comment.row + 4
        );
        out.push_str(&format!(
            "<v:shape id=\"_x0000_s{id}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:{z};visibility:hidden\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\">\
<v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/>\
<v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox>\
<x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{anchor}</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{col}</x:Column></x:ClientData></v:shape>",
            z = index + 1,
            row = comment.row,
            col = comment.col
        ));
    }
    out.push_str("</xml>");
    out
}

pub fn table_xml(table: &Table) -> String {
    let mut attributes = vec![
        ("id".to_string(), table.id.to_string()),
        ("name".to_string(), table.name.clone()),
        ("displayName".to_string(), table.name.clone()),
        ("ref".to_string(), table.range.to_a1()),
    ];
    if !table.header_row {
        attributes.push(("headerRowCount".into(), "0".into()));
    }
    if table.totals_row {
        attributes.push(("totalsRowCount".into(), "1".into()));
    }
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"");
    for (key, value) in &attributes {
        out.push_str(&format!(" {key}=\"{}\"", escape_attribute(value)));
    }
    out.push('>');
    if let Some(filter) = &table.filter {
        out.push_str(&auto_filter(filter));
    }
    out.push_str(&format!("<tableColumns count=\"{}\">", table.columns.len()));
    for (index, column) in table.columns.iter().enumerate() {
        out.push_str(&table_column(index, column));
    }
    out.push_str("</tableColumns>");
    out.push_str(&table_style(table));
    out.push_str("</table>");
    out
}

fn table_column(index: usize, column: &crate::model::TableColumn) -> String {
    let mut out = format!(
        "<tableColumn id=\"{}\" name=\"{}\"",
        index + 1,
        escape_attribute(&column.name)
    );
    if let Some(function) = &column.totals_function {
        out.push_str(&format!(" totalsRowFunction=\"{function}\""));
    }
    if let Some(label) = &column.totals_label {
        out.push_str(&format!(" totalsRowLabel=\"{}\"", escape_attribute(label)));
    }
    let children = [
        ("calculatedColumnFormula", &column.calculated_formula),
        ("totalsRowFormula", &column.totals_formula),
    ];
    if children.iter().all(|(_, value)| value.is_none()) {
        out.push_str("/>");
        return out;
    }
    out.push('>');
    for (name, value) in children {
        if let Some(value) = value {
            out.push_str(&format!("<{name}>{}</{name}>", escape_text(value)));
        }
    }
    out.push_str("</tableColumn>");
    out
}

fn table_style(table: &Table) -> String {
    let flag = |on: bool| if on { "1" } else { "0" };
    let name = table
        .style
        .as_ref()
        .map(|name| format!(" name=\"{}\"", escape_attribute(name)))
        .unwrap_or_default();
    format!(
        "<tableStyleInfo{name} showFirstColumn=\"{}\" showLastColumn=\"{}\" showRowStripes=\"{}\" showColumnStripes=\"{}\"/>",
        flag(table.first_column),
        flag(table.last_column),
        flag(table.banded_rows),
        flag(table.banded_columns)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Range;
    use crate::model::TableColumn;

    #[test]
    fn written_parts_read_back() {
        let comments = vec![Comment {
            row: 1,
            col: 2,
            author: "Ann".into(),
            text: "Hello & bye".into(),
        }];
        let read = super::super::read_extras::read_comments(&comments_xml(&comments));
        assert_eq!(read, comments);
        assert!(notes_vml(&comments).contains("<x:Row>1</x:Row><x:Column>2</x:Column>"));
        let table = Table {
            id: 1,
            name: "Sales".into(),
            range: Range::parse("A1:B3").unwrap(),
            header_row: true,
            columns: vec![
                TableColumn {
                    name: "Item".into(),
                    ..TableColumn::default()
                },
                TableColumn {
                    name: "Price".into(),
                    ..TableColumn::default()
                },
            ],
            style: Some("TableStyleMedium2".into()),
            banded_rows: true,
            ..Table::default()
        };
        let back = super::super::read_extras::read_table(&table_xml(&table)).unwrap();
        assert_eq!(back, table);
    }
}
