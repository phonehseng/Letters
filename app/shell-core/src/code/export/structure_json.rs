//! the document's structure as json: a list of blocks with their kind and
//! text, for students who want to process a document in their own code.
//! formatting is left out on purpose; this is an outline, not a file format.

use serde_json::{json, Value};

use super::simple::{flatten, pieces_text, SimpleBlock};
use super::WrittenText;
use letters_office::model::Document;

pub fn to_structure_json(document: &Document) -> WrittenText {
    let flat = flatten(document);
    let blocks: Vec<Value> = flat.blocks.iter().map(block_json).collect();
    let outline = json!({ "blocks": blocks });
    WrittenText {
        text: serde_json::to_string_pretty(&outline).unwrap_or_default() + "\n",
        files: Vec::new(),
    }
}

fn block_json(block: &SimpleBlock) -> Value {
    match block {
        SimpleBlock::Heading { level, pieces } => {
            json!({ "type": "heading", "level": level, "text": pieces_text(pieces) })
        }
        SimpleBlock::Paragraph { pieces } => {
            json!({ "type": "paragraph", "text": pieces_text(pieces) })
        }
        SimpleBlock::ListItem {
            level,
            ordered,
            pieces,
        } => json!({
            "type": "listItem",
            "level": level,
            "ordered": ordered,
            "text": pieces_text(pieces),
        }),
        SimpleBlock::Code { text } => json!({ "type": "code", "text": text }),
        SimpleBlock::Table { rows } => {
            let cells: Vec<Vec<String>> = rows
                .iter()
                .map(|row| row.iter().map(|cell| pieces_text(cell)).collect())
                .collect();
            json!({ "type": "table", "rows": cells })
        }
        SimpleBlock::PageBreak => json!({ "type": "pageBreak" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::export::test_document::sample;

    #[test]
    fn the_outline_lists_each_block_with_its_kind_and_text() {
        let text = to_structure_json(&sample()).text;
        let outline: Value = serde_json::from_str(&text).unwrap();
        let first = &outline["blocks"][0];
        assert_eq!(first["type"], "heading");
        assert_eq!(first["level"], 1);
        assert_eq!(first["text"], "Lab Report");
        let table = outline["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|block| block["type"] == "table")
            .unwrap();
        assert_eq!(table["rows"][1][0], "a, b");
    }
}
