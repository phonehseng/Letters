//! a reStructuredText version of a document, the format python's sphinx
//! documentation uses: underlined headings, lists, emphasis, links,
//! grid-free "list tables", code blocks and pictures.

use super::simple::{flatten, Piece, SimpleBlock, Span};
use super::WrittenText;
use letters_office::model::Document;

// rst has no fixed heading characters; this order is the one python's own
// documentation style guide recommends
const HEADING_UNDERLINES: [char; 6] = ['=', '-', '~', '^', '"', '\''];

pub fn to_rst(document: &Document) -> WrittenText {
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
            let title = inline_text(pieces);
            let underline = HEADING_UNDERLINES[usize::from(*level).clamp(1, 6) - 1];
            let width = title.chars().count().max(1);
            out.push_str(&format!(
                "{title}\n{}\n\n",
                underline.to_string().repeat(width)
            ));
        }
        SimpleBlock::Paragraph { pieces } => out.push_str(&format!("{}\n\n", inline_text(pieces))),
        SimpleBlock::ListItem {
            level,
            ordered,
            pieces,
        } => {
            let indent = "   ".repeat(usize::from(*level));
            let marker = if *ordered { "#." } else { "-" };
            out.push_str(&format!("{indent}{marker} {}\n", inline_text(pieces)));
        }
        SimpleBlock::Code { text } => {
            out.push_str("::\n\n");
            for line in text.lines() {
                out.push_str(&format!("   {line}\n"));
            }
            out.push('\n');
        }
        SimpleBlock::Table { rows } => write_list_table(rows, out),
        SimpleBlock::PageBreak => out.push_str("----\n\n"),
    }
}

// a list-table needs no column widths drawn in characters, so cells with
// long text stay readable and can't break the table
fn write_list_table(rows: &[Vec<Vec<Piece>>], out: &mut String) {
    out.push_str(".. list-table::\n   :header-rows: 1\n\n");
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let marker = if index == 0 { "* -" } else { "  -" };
            out.push_str(&format!("   {marker} {}\n", inline_text(cell)));
        }
    }
    out.push('\n');
}

fn inline_text(pieces: &[Piece]) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(span) => out.push_str(&styled(span)),
            Piece::LineBreak => out.push(' '),
            Piece::Picture { file, description } => out.push_str(&format!(
                "\n\n.. image:: {file}\n   :alt: {description}\n\n"
            )),
        }
    }
    out.trim().to_owned()
}

fn styled(span: &Span) -> String {
    let trimmed = span.text.trim();
    if trimmed.is_empty() {
        return span.text.clone();
    }
    let mut text = if span.code {
        format!("``{trimmed}``")
    } else {
        escape(trimmed)
    };
    // rst can't nest inline markup, so bold wins over italic
    if span.bold {
        text = format!("**{text}**");
    } else if span.italic {
        text = format!("*{text}*");
    }
    if let Some(link) = &span.link {
        text = format!("`{} <{link}>`_", trimmed.replace('`', ""));
    }
    keep_outer_spaces(&span.text, text)
}

// markup must touch its words, so the spaces around them are put back outside
fn keep_outer_spaces(original: &str, styled: String) -> String {
    let leading = if original.starts_with(char::is_whitespace) {
        " "
    } else {
        ""
    };
    let trailing = if original.ends_with(char::is_whitespace) {
        " "
    } else {
        ""
    };
    format!("{leading}{styled}{trailing}")
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\*`_|".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::export::test_document::sample;

    #[test]
    fn headings_are_underlined_to_their_own_length() {
        let text = to_rst(&sample()).text;
        assert!(text.starts_with("Lab Report\n==========\n\n"), "{text}");
        assert!(text.contains("Steps\n-----\n"));
    }

    #[test]
    fn lists_emphasis_links_and_tables_are_written() {
        let text = to_rst(&sample()).text;
        assert!(text.contains("- first\n- second\n"), "{text}");
        assert!(text.contains("#. one\n#. two\n"));
        assert!(text.contains("**bold**"));
        assert!(text.contains("*italic*"));
        assert!(text.contains("`a link <https://example.org>`_"));
        assert!(text.contains(".. list-table::"));
        assert!(text.contains("   * - name\n     - value\n"), "{text}");
    }
}
