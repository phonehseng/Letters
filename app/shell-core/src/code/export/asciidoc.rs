//! an AsciiDoc version of a document: "=" headings, lists, emphasis,
//! links, tables, code listings and pictures.

use super::simple::{flatten, Piece, SimpleBlock, Span};
use super::WrittenText;
use letters_office::model::Document;

pub fn to_asciidoc(document: &Document) -> WrittenText {
    let flat = flatten(document);
    let mut out = String::new();
    let mut previous_was_list = false;
    for block in &flat.blocks {
        let is_list = matches!(block, SimpleBlock::ListItem { .. });
        if previous_was_list && !is_list {
            out.push('\n');
        }
        write_block(block, &mut out);
        previous_was_list = is_list;
    }
    WrittenText {
        text: out,
        files: flat.files,
    }
}

fn write_block(block: &SimpleBlock, out: &mut String) {
    match block {
        SimpleBlock::Heading { level, pieces } => {
            // "=" alone is the document title; sections start at "=="
            let marks = "=".repeat(usize::from(*level).clamp(1, 5) + 1);
            out.push_str(&format!("{marks} {}\n\n", inline_text(pieces)));
        }
        SimpleBlock::Paragraph { pieces } => out.push_str(&format!("{}\n\n", inline_text(pieces))),
        SimpleBlock::ListItem {
            level,
            ordered,
            pieces,
        } => {
            let symbol = if *ordered { "." } else { "*" };
            let marker = symbol.repeat(usize::from(*level) + 1);
            out.push_str(&format!("{marker} {}\n", inline_text(pieces)));
        }
        SimpleBlock::Code { text } => out.push_str(&format!("----\n{text}\n----\n\n")),
        SimpleBlock::Table { rows } => write_table(rows, out),
        SimpleBlock::PageBreak => out.push_str("<<<\n\n"),
    }
}

fn write_table(rows: &[Vec<Vec<Piece>>], out: &mut String) {
    out.push_str("[options=\"header\"]\n|===\n");
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| format!("|{}", inline_text(cell).replace('|', "\\|")))
            .collect();
        out.push_str(&format!("{}\n", cells.join(" ")));
    }
    out.push_str("|===\n\n");
}

fn inline_text(pieces: &[Piece]) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(span) => out.push_str(&styled(span)),
            Piece::LineBreak => out.push_str(" +\n"),
            Piece::Picture { file, description } => {
                out.push_str(&format!("image:{file}[{}]", description.replace(']', "")))
            }
        }
    }
    out.trim().to_owned()
}

fn styled(span: &Span) -> String {
    let trimmed = span.text.trim();
    if trimmed.is_empty() {
        return span.text.clone();
    }
    // a "+" passthrough keeps * and _ in the text from turning into emphasis
    let mut text = if trimmed.contains(['*', '_', '`', '#']) {
        format!("+{trimmed}+")
    } else {
        trimmed.to_owned()
    };
    if span.code {
        text = format!("`{text}`");
    }
    if span.italic {
        text = format!("_{text}_");
    }
    if span.bold {
        text = format!("*{text}*");
    }
    if let Some(link) = &span.link {
        text = format!("{link}[{}]", text.replace(']', ""));
    }
    let leading = if span.text.starts_with(char::is_whitespace) {
        " "
    } else {
        ""
    };
    let trailing = if span.text.ends_with(char::is_whitespace) {
        " "
    } else {
        ""
    };
    format!("{leading}{text}{trailing}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::export::test_document::sample;

    #[test]
    fn headings_lists_and_emphasis_use_asciidoc_marks() {
        let text = to_asciidoc(&sample()).text;
        assert!(text.starts_with("== Lab Report\n\n"), "{text}");
        assert!(text.contains("=== Steps"));
        assert!(text.contains("* first\n* second\n"), "{text}");
        assert!(text.contains(". one\n. two\n"));
        assert!(text.contains("*bold*"));
        assert!(text.contains("_italic_"));
        assert!(text.contains("https://example.org[a link]"));
    }

    #[test]
    fn tables_have_a_header_row() {
        let text = to_asciidoc(&sample()).text;
        assert!(
            text.contains("[options=\"header\"]\n|===\n|name |value\n"),
            "{text}"
        );
    }
}
