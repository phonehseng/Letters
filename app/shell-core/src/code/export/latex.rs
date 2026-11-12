//! a LaTeX version of a document: sections, lists, emphasis, links,
//! tables, code and pictures (saved as files beside it).
//!
//! it is deliberately basic: a standard `article` with the three packages
//! most LaTeX installations have (graphicx, hyperref, listings), so it
//! compiles anywhere and is easy to read and tidy by hand.

use super::simple::{flatten, Piece, SimpleBlock, Span};
use super::WrittenText;
use letters_office::model::Document;

const PREAMBLE: &str = "\\documentclass{article}\n\\usepackage[utf8]{inputenc}\n\\usepackage{graphicx}\n\\usepackage{hyperref}\n\\usepackage{listings}\n\n\\begin{document}\n\n";

const ENDING: &str = "\\end{document}\n";

// latex has three numbered levels in an article; deeper headings become
// paragraphs with a bold run-in title, the usual fallback
const SECTION_COMMANDS: [&str; 6] = [
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
    "subparagraph",
    "subparagraph",
];

pub fn to_latex(document: &Document) -> WrittenText {
    let flat = flatten(document);
    let mut out = String::from(PREAMBLE);
    let mut open_lists: Vec<bool> = Vec::new();
    for block in &flat.blocks {
        write_block(block, &mut open_lists, &mut out);
    }
    close_lists_to(0, &mut open_lists, &mut out);
    out.push_str(ENDING);
    WrittenText {
        text: out,
        files: flat.files,
    }
}

fn write_block(block: &SimpleBlock, open_lists: &mut Vec<bool>, out: &mut String) {
    if let SimpleBlock::ListItem {
        level,
        ordered,
        pieces,
    } = block
    {
        open_lists_to(usize::from(*level) + 1, *ordered, open_lists, out);
        out.push_str(&format!("\\item {}\n", inline_text(pieces)));
        return;
    }
    close_lists_to(0, open_lists, out);
    match block {
        SimpleBlock::Heading { level, pieces } => {
            let command = SECTION_COMMANDS[usize::from(*level).clamp(1, 6) - 1];
            out.push_str(&format!("\\{command}{{{}}}\n\n", inline_text(pieces)));
        }
        SimpleBlock::Paragraph { pieces } => out.push_str(&format!("{}\n\n", inline_text(pieces))),
        SimpleBlock::Code { text } => out.push_str(&format!(
            "\\begin{{lstlisting}}\n{text}\n\\end{{lstlisting}}\n\n"
        )),
        SimpleBlock::Table { rows } => write_table(rows, out),
        SimpleBlock::PageBreak => out.push_str("\\newpage\n\n"),
        SimpleBlock::ListItem { .. } => {}
    }
}

fn open_lists_to(depth: usize, ordered: bool, open_lists: &mut Vec<bool>, out: &mut String) {
    close_lists_to(depth, open_lists, out);
    // a numbered list right after a bulleted one at the same level is a new list
    let kind_changes = open_lists.len() == depth && open_lists.last() != Some(&ordered);
    if kind_changes {
        close_lists_to(depth - 1, open_lists, out);
    }
    while open_lists.len() < depth {
        out.push_str(if ordered {
            "\\begin{enumerate}\n"
        } else {
            "\\begin{itemize}\n"
        });
        open_lists.push(ordered);
    }
}

fn close_lists_to(depth: usize, open_lists: &mut Vec<bool>, out: &mut String) {
    while open_lists.len() > depth {
        let ordered = open_lists.pop().unwrap_or(false);
        out.push_str(if ordered {
            "\\end{enumerate}\n"
        } else {
            "\\end{itemize}\n"
        });
        if open_lists.is_empty() {
            out.push('\n');
        }
    }
}

fn write_table(rows: &[Vec<Vec<Piece>>], out: &mut String) {
    let columns = rows.first().map(Vec::len).unwrap_or(1).max(1);
    out.push_str(&format!(
        "\\begin{{tabular}}{{|{}}}\n\\hline\n",
        "l|".repeat(columns)
    ));
    for row in rows {
        let cells: Vec<String> = row.iter().map(|cell| inline_text(cell)).collect();
        out.push_str(&format!("{} \\\\\n\\hline\n", cells.join(" & ")));
    }
    out.push_str("\\end{tabular}\n\n");
}

fn inline_text(pieces: &[Piece]) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(span) => out.push_str(&styled(span)),
            Piece::LineBreak => out.push_str("\\\\\n"),
            Piece::Picture { file, .. } => {
                out.push_str(&format!("\\includegraphics[width=\\linewidth]{{{file}}}"))
            }
        }
    }
    out.trim().to_owned()
}

fn styled(span: &Span) -> String {
    if span.text.trim().is_empty() {
        return escape(&span.text);
    }
    let mut text = escape(&span.text);
    if span.code {
        text = format!("\\texttt{{{text}}}");
    }
    if span.italic {
        text = format!("\\emph{{{text}}}");
    }
    if span.bold {
        text = format!("\\textbf{{{text}}}");
    }
    if let Some(link) = &span.link {
        text = format!("\\href{{{}}}{{{text}}}", escape_url(link));
    }
    text
}

/// the ten characters latex treats as commands, written so they print.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' | '}' | '$' | '&' | '#' | '_' | '%' => {
                out.push('\\');
                out.push(c);
            }
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            _ => out.push(c),
        }
    }
    out
}

// inside \href only % and # need escaping; the rest of a url is taken as is
fn escape_url(url: &str) -> String {
    url.replace('%', "\\%").replace('#', "\\#")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::export::test_document::sample;

    #[test]
    fn headings_become_sections_and_lists_become_environments() {
        let text = to_latex(&sample()).text;
        assert!(text.starts_with("\\documentclass{article}"));
        assert!(text.contains("\\section{Lab Report}"), "{text}");
        assert!(text.contains("\\subsection{Steps}"));
        assert!(
            text.contains("\\begin{itemize}\n\\item first\n\\item second\n\\end{itemize}"),
            "{text}"
        );
        assert!(text.contains("\\begin{enumerate}\n\\item one"), "{text}");
        assert!(text.trim_end().ends_with("\\end{document}"));
    }

    #[test]
    fn emphasis_links_and_tables_are_written() {
        let text = to_latex(&sample()).text;
        assert!(text.contains("\\textbf{bold}"));
        assert!(text.contains("\\emph{italic}"));
        assert!(text.contains("\\href{https://example.org}{a link}"));
        assert!(text.contains("\\begin{tabular}{|l|l|}"));
        assert!(text.contains("name & value \\\\"));
    }

    #[test]
    fn special_characters_are_escaped() {
        let text = to_latex(&sample()).text;
        assert!(text.contains("50\\% done \\& \\$5 \\#1"), "{text}");
        assert_eq!(
            escape("a_b~c\\"),
            "a\\_b\\textasciitilde{}c\\textbackslash{}"
        );
    }
}
