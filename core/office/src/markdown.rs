//! markdown export (commonmark with github tables and footnotes).
//! pictures are written as separate files next to the text.

use crate::model::*;
use crate::numbering::ListCounter;

/// markdown text and the picture files it refers to.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownExport {
    pub text: String,
    /// (relative path, bytes) for each picture, under "images/"
    pub files: Vec<(String, Vec<u8>)>,
}

pub fn to_markdown(document: &Document) -> MarkdownExport {
    let mut writer = Markdown {
        document,
        counter: ListCounter::new(document),
        notes: Vec::new(),
        files: Vec::new(),
    };
    let mut text = String::new();
    for block in &document.blocks {
        writer.block(block, &mut text);
    }
    for (index, id) in writer.notes.clone().iter().enumerate() {
        if let Some(note) = document.note(id) {
            let body: Vec<String> = note
                .blocks
                .iter()
                .filter_map(|b| b.paragraph())
                .map(|p| writer.inlines(&p.content))
                .collect();
            text.push_str(&format!("[^{}]: {}\n", index + 1, body.join(" ").trim()));
        }
    }
    MarkdownExport {
        text,
        files: writer.files,
    }
}

struct Markdown<'a> {
    document: &'a Document,
    counter: ListCounter<'a>,
    notes: Vec<String>,
    files: Vec<(String, Vec<u8>)>,
}

impl Markdown<'_> {
    fn block(&mut self, block: &Block, out: &mut String) {
        match block {
            Block::Heading(heading) => {
                let content = self.inlines(&heading.paragraph.content);
                out.push_str(&format!(
                    "{} {}\n\n",
                    "#".repeat(usize::from(heading.level.clamp(1, 6))),
                    content.trim()
                ));
            }
            Block::Paragraph(paragraph) => {
                let content = self.inlines(&paragraph.content);
                match (
                    paragraph.props.list.as_ref(),
                    self.counter.label_for_block(block),
                ) {
                    (Some(list), Some(label)) => {
                        let indent = "    ".repeat(usize::from(list.level));
                        let bullet = if label.chars().any(|c| c.is_alphanumeric()) {
                            "1."
                        } else {
                            "-"
                        };
                        out.push_str(&format!("{indent}{bullet} {}\n", content.trim()));
                    }
                    _ => {
                        if out.ends_with('\n') && !out.ends_with("\n\n") && !out.is_empty() {
                            out.push('\n');
                        }
                        if !content.trim().is_empty() {
                            out.push_str(&format!("{}\n\n", content.trim()));
                        }
                    }
                }
            }
            Block::Table(table) => self.table(table, out),
            Block::PageBreak => out.push_str("---\n\n"),
            Block::TextBox(_) | Block::Shape(_) | Block::Group(_) => {
                for inner in block.drawing_blocks() {
                    let mut part = String::new();
                    self.block(inner, &mut part);
                    for line in part.lines() {
                        out.push_str(&format!("> {line}\n"));
                    }
                }
                out.push('\n');
            }
            Block::SectionBreak | Block::Preserved { .. } => {}
        }
    }

    fn table(&mut self, table: &Table, out: &mut String) {
        let width = crate::tables::grid_width(table).max(1);
        for (index, row) in table.rows.iter().enumerate() {
            let mut cells: Vec<String> = row
                .cells
                .iter()
                .map(|cell| {
                    let texts: Vec<String> = cell
                        .blocks
                        .iter()
                        .filter_map(|b| b.paragraph())
                        .map(|p| self.inlines(&p.content))
                        .collect();
                    texts.join(" ").replace('|', "\\|").trim().to_string()
                })
                .collect();
            cells.resize(width, String::new());
            out.push_str(&format!("| {} |\n", cells.join(" | ")));
            if index == 0 {
                out.push_str(&format!("|{}\n", " --- |".repeat(width)));
            }
        }
        out.push('\n');
    }

    fn inlines(&mut self, inlines: &[Inline]) -> String {
        let mut out = String::new();
        for inline in inlines {
            if inline.is_deleted() {
                continue;
            }
            let content = match inline {
                Inline::Text { text, .. } => escape(text),
                Inline::Tab { .. } => " ".into(),
                Inline::Break { .. } => "  \n".into(),
                Inline::Field(field) => escape(&field.result),
                Inline::NoteReference { note_id, .. } => {
                    self.notes.push(note_id.clone());
                    format!("[^{}]", self.notes.len())
                }
                Inline::Image(image) => self.image(image),
                _ => String::new(),
            };
            out.push_str(&wrap(inline.marks(), content));
        }
        out
    }

    fn image(&mut self, image: &Image) -> String {
        let Some(item) = self.document.media_item(&image.media_id) else {
            return String::new();
        };
        let path = format!("images/{}", item.id.replace(['/', '\\', ' '], "_"));
        if !self.files.iter().any(|(p, _)| *p == path) {
            self.files.push((path.clone(), item.bytes.clone()));
        }
        format!(
            "![{}]({path})",
            escape(image.description.as_deref().unwrap_or(""))
        )
    }
}

fn wrap(marks: &[Mark], content: String) -> String {
    if content.trim().is_empty() {
        return content;
    }
    let mut text = content;
    if marks
        .iter()
        .any(|m| matches!(m, Mark::Strike | Mark::DoubleStrike))
    {
        text = format!("~~{text}~~");
    }
    if marks.contains(&Mark::Italic) {
        text = format!("*{text}*");
    }
    if marks.contains(&Mark::Bold) {
        text = format!("**{text}**");
    }
    if let Some(href) = marks.iter().find_map(|m| match m {
        Mark::Link(target) => target
            .href
            .clone()
            .or_else(|| target.anchor.as_ref().map(|a| format!("#{a}"))),
        _ => None,
    }) {
        text = format!("[{text}]({href})");
    }
    text
}

/// backslashes the characters markdown would read as formatting.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\`*_[]#<>|~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_lists_tables_and_marks() {
        let document = crate::import::from_html(
            "<h1>Title</h1><p>Some <b>bold</b> and <a href=\"https://example.org\">a link</a>.</p>\
             <ul><li>one</li><li>two</li></ul><table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>",
        );
        let text = to_markdown(&document).text;
        assert!(text.starts_with("# Title\n\n"), "{text}");
        assert!(text.contains("Some **bold** and [a link](https://example.org)."));
        assert!(text.contains("- one\n- two\n"));
        assert!(text.contains("| a | b |\n| --- | --- |\n| c | d |"));
    }
}
