//! the document's tables as csv, one file per table, quoted the way rfc
//! 4180 says so spreadsheets and python's csv module read them back.

use super::simple::{flatten, pieces_text, SimpleBlock};
use letters_office::model::Document;

/// one csv text per table, in document order. empty when there are none.
pub fn tables_to_csv(document: &Document) -> Vec<String> {
    flatten(document)
        .blocks
        .iter()
        .filter_map(|block| match block {
            SimpleBlock::Table { rows } => Some(table_csv(rows)),
            _ => None,
        })
        .collect()
}

fn table_csv(rows: &[Vec<Vec<super::simple::Piece>>]) -> String {
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row.iter().map(|cell| quote(&pieces_text(cell))).collect();
        out.push_str(&cells.join(","));
        // rfc 4180 ends every record with crlf
        out.push_str("\r\n");
    }
    out
}

/// a field in quotes when it holds a comma, a quote or a line break.
pub fn quote(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::export::test_document::sample;

    #[test]
    fn each_table_becomes_one_csv_with_quoted_commas() {
        let tables = tables_to_csv(&sample());
        assert_eq!(tables, vec!["name,value\r\n\"a, b\",2\r\n".to_owned()]);
    }

    #[test]
    fn quotes_inside_a_field_are_doubled() {
        assert_eq!(quote("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(quote("plain"), "plain");
    }
}
