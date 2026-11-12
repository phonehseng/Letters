//! plain text and html versions of a document, for the conversion stage
//! and for copying out of the editor.

use crate::base64;
use crate::model::*;
use crate::numbering::ListCounter;
use crate::xml::{escape_attribute, escape_text};

/// the document as plain text: one line per paragraph, list labels in
/// front of list items, table cells separated by tabs, notes at the end.
pub fn to_plain_text(document: &Document) -> String {
    let mut out = String::new();
    let mut counter = ListCounter::new(document);
    let mut notes = Vec::new();
    for block in &document.blocks {
        text_block(block, &mut counter, &mut notes, &mut out);
    }
    if !notes.is_empty() {
        out.push_str("\nNotes\n");
        for (number, id) in notes.iter().enumerate() {
            if let Some(note) = document.note(id) {
                let mut text = String::new();
                for block in &note.blocks {
                    if let Some(paragraph) = block.paragraph() {
                        text.push_str(&paragraph_text(paragraph, &mut Vec::new()));
                    }
                }
                out.push_str(&format!("[{}] {}\n", number + 1, text.trim()));
            }
        }
    }
    out
}

fn text_block(block: &Block, counter: &mut ListCounter, notes: &mut Vec<String>, out: &mut String) {
    match block {
        Block::Paragraph(_) | Block::Heading(_) => {
            if let Some(label) = counter.label_for_block(block) {
                out.push_str(&label);
                out.push(' ');
            }
            out.push_str(&paragraph_text(
                block.paragraph().unwrap_or(&Paragraph::default()),
                notes,
            ));
            out.push('\n');
        }
        Block::Table(table) => {
            for row in &table.rows {
                let cells: Vec<String> = row
                    .cells
                    .iter()
                    .map(|cell| {
                        let mut text = String::new();
                        for block in &cell.blocks {
                            text_block(block, counter, notes, &mut text);
                        }
                        text.trim_end().replace('\n', " ")
                    })
                    .collect();
                out.push_str(&cells.join("\t"));
                out.push('\n');
            }
        }
        Block::PageBreak => out.push('\u{C}'),
        Block::TextBox(_) | Block::Shape(_) | Block::Group(_) => {
            for inner in block.drawing_blocks() {
                text_block(inner, counter, notes, out);
            }
        }
        Block::SectionBreak => {}
        Block::Preserved { xml } => {
            let text = xml_text(xml);
            if !text.trim().is_empty() {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
}

fn paragraph_text(paragraph: &Paragraph, notes: &mut Vec<String>) -> String {
    let mut text = String::new();
    for inline in &paragraph.content {
        if inline.is_deleted() {
            continue;
        }
        match inline {
            Inline::NoteReference { note_id, .. } => {
                notes.push(note_id.clone());
                text.push_str(&format!("[{}]", notes.len()));
            }
            Inline::Image(image) => {
                text.push_str(&format!(
                    "[Image{}]",
                    image
                        .description
                        .as_deref()
                        .map(|d| format!(": {d}"))
                        .unwrap_or_default()
                ));
            }
            Inline::Preserved { xml } | Inline::PreservedRun { xml, .. } => {
                text.push_str(&xml_text(xml))
            }
            other => other.push_plain_text(&mut text),
        }
    }
    text
}

fn xml_text(xml: &str) -> String {
    crate::xml::parse(xml)
        .map(|d| d.root.deep_text())
        .unwrap_or_default()
}

/// the document as a self-contained html page: headings, paragraphs,
/// lists, tables, links, pictures (inline, as data), notes at the end,
/// and tracked changes as `ins` and `del`.
pub fn to_html(document: &Document) -> String {
    let mut writer = HtmlWriter {
        document,
        counter: ListCounter::new(document),
        notes: Vec::new(),
        out: String::new(),
    };
    writer.document_html();
    writer.out
}

struct HtmlWriter<'a> {
    document: &'a Document,
    counter: ListCounter<'a>,
    notes: Vec<String>,
    out: String,
}

impl HtmlWriter<'_> {
    fn document_html(&mut self) {
        let title = self.document.metadata.title.clone().unwrap_or_default();
        let language = self
            .document
            .metadata
            .language
            .clone()
            .unwrap_or_else(|| "en".into());
        self.out.push_str(&format!(
            "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n",
            escape_attribute(&language),
            escape_text(&title)
        ));
        let blocks = &self.document.blocks;
        self.blocks_html(blocks);
        self.notes_html();
        self.out.push_str("</body>\n</html>\n");
    }

    fn blocks_html(&mut self, blocks: &[Block]) {
        let mut index = 0;
        while index < blocks.len() {
            if list_of(&blocks[index]).is_some() {
                let end = index
                    + blocks[index..]
                        .iter()
                        .take_while(|b| list_of(b).is_some())
                        .count();
                self.list_html(&blocks[index..end]);
                index = end;
            } else {
                self.block_html(&blocks[index]);
                index += 1;
            }
        }
    }

    fn block_html(&mut self, block: &Block) {
        match block {
            Block::Paragraph(paragraph) => {
                let style = paragraph_style(&paragraph.props);
                let content = self.inlines_html(&paragraph.content);
                self.out.push_str(&format!("<p{style}>{content}</p>\n"));
            }
            Block::Heading(heading) => {
                let level = heading.level.clamp(1, 6);
                let content = self.inlines_html(&heading.paragraph.content);
                self.out
                    .push_str(&format!("<h{level}>{content}</h{level}>\n"));
            }
            Block::Table(table) => self.table_html(table),
            Block::PageBreak => self
                .out
                .push_str("<hr style=\"page-break-after: always\">\n"),
            Block::SectionBreak => {}
            Block::TextBox(text_box) => {
                self.out.push_str(&format!(
                    "<div class=\"text-box\" style=\"width: {}pt\">\n",
                    text_box.width / 20
                ));
                self.blocks_html(&text_box.blocks);
                self.out.push_str("</div>\n");
            }
            Block::Shape(_) | Block::Group(_) => {
                let inner: Vec<Block> = block.drawing_blocks().into_iter().cloned().collect();
                if !inner.is_empty() {
                    self.out.push_str("<div class=\"shape\">\n");
                    self.blocks_html(&inner);
                    self.out.push_str("</div>\n");
                }
            }
            Block::Preserved { xml } => {
                let text = xml_text(xml);
                if !text.trim().is_empty() {
                    self.out
                        .push_str(&format!("<p>{}</p>\n", escape_text(&text)));
                }
            }
        }
    }

    /// list paragraphs as nested ul/ol; the label kind decides which.
    fn list_html(&mut self, blocks: &[Block]) {
        let mut open: Vec<&'static str> = Vec::new();
        for block in blocks {
            let Some(list) = list_of(block) else { continue };
            let _ = self.counter.label_for_block(block);
            let numbering = &self.document.numbering;
            let bullet = numbering
                .level(&list.list_id, list.level)
                .is_none_or(|level| level.format == NumberFormat::Bullet);
            let tag = if bullet { "ul" } else { "ol" };
            let depth = usize::from(list.level) + 1;
            while open.len() > depth {
                let tag = open.pop().unwrap_or("ul");
                self.out.push_str(&format!("</li></{tag}>\n"));
            }
            if open.len() == depth {
                self.out.push_str("</li>\n");
            }
            while open.len() < depth {
                self.out.push_str(&format!("<{tag}>\n"));
                open.push(tag);
            }
            let content = self.inlines_html(
                &block
                    .paragraph()
                    .map(|p| p.content.clone())
                    .unwrap_or_default(),
            );
            self.out.push_str(&format!("<li>{content}"));
        }
        while let Some(tag) = open.pop() {
            self.out.push_str(&format!("</li></{tag}>\n"));
        }
    }

    fn table_html(&mut self, table: &Table) {
        self.out.push_str("<table>\n");
        for row in &table.rows {
            self.out.push_str("<tr>");
            let tag = if row.header { "th" } else { "td" };
            for cell in &row.cells {
                let mut attributes = String::new();
                if cell.col_span > 1 {
                    attributes.push_str(&format!(" colspan=\"{}\"", cell.col_span));
                }
                if cell.row_span > 1 {
                    attributes.push_str(&format!(" rowspan=\"{}\"", cell.row_span));
                }
                if let Some(fill) = &cell.shading {
                    attributes.push_str(&format!(
                        " style=\"background-color: #{}\"",
                        escape_attribute(fill)
                    ));
                }
                let saved = std::mem::take(&mut self.out);
                self.blocks_html(&cell.blocks);
                let content = std::mem::replace(&mut self.out, saved);
                self.out.push_str(&format!(
                    "<{tag}{attributes}>{}</{tag}>",
                    content.trim_end()
                ));
            }
            self.out.push_str("</tr>\n");
        }
        self.out.push_str("</table>\n");
    }

    fn inlines_html(&mut self, inlines: &[Inline]) -> String {
        let mut html = String::new();
        for inline in inlines {
            let content = match inline {
                Inline::Text { text, .. } => escape_text(text).replace('\n', "<br>"),
                Inline::Tab { .. } => "\t".into(),
                Inline::Break { .. } => "<br>".into(),
                Inline::Image(image) => self.image_html(image),
                Inline::NoteReference { note_id, .. } => {
                    self.notes.push(note_id.clone());
                    let number = self.notes.len();
                    format!(
                        "<sup><a href=\"#note-{number}\" id=\"ref-{number}\">{number}</a></sup>"
                    )
                }
                Inline::Field(field) => escape_text(&field.result),
                Inline::BookmarkStart { name, .. } => {
                    format!("<a id=\"{}\"></a>", escape_attribute(name))
                }
                Inline::Preserved { xml } | Inline::PreservedRun { xml, .. } => {
                    escape_text(&xml_text(xml))
                }
                Inline::BookmarkEnd { .. } | Inline::CommentReference { .. } => String::new(),
            };
            html.push_str(&wrap_marks(inline.marks(), content));
        }
        html
    }

    fn image_html(&self, image: &Image) -> String {
        let Some(item) = self.document.media_item(&image.media_id) else {
            return String::new();
        };
        format!(
            "<img src=\"data:{};base64,{}\" width=\"{}\" height=\"{}\" alt=\"{}\">",
            escape_attribute(&item.content_type),
            base64::encode(&item.bytes),
            image.width / 15,
            image.height / 15,
            escape_attribute(image.description.as_deref().unwrap_or(""))
        )
    }

    fn notes_html(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        self.out.push_str("<section class=\"notes\">\n<ol>\n");
        let ids = self.notes.clone();
        for (index, id) in ids.iter().enumerate() {
            let number = index + 1;
            let Some(note) = self.document.note(id) else {
                continue;
            };
            let mut content = String::new();
            for block in &note.blocks {
                if let Some(paragraph) = block.paragraph() {
                    content.push_str(&self.inlines_html(&paragraph.content));
                }
            }
            self.out.push_str(&format!(
                "<li id=\"note-{number}\">{} <a href=\"#ref-{number}\">\u{21A9}</a></li>\n",
                content.trim()
            ));
        }
        self.out.push_str("</ol>\n</section>\n");
    }
}

fn list_of(block: &Block) -> Option<ListRef> {
    block
        .paragraph()?
        .props
        .list
        .clone()
        .filter(|list| list.list_id != "0")
}

fn paragraph_style(props: &ParagraphProps) -> String {
    let mut style = Vec::new();
    match props.alignment {
        Some(Alignment::Center) => style.push("text-align: center".to_string()),
        Some(Alignment::Right) => style.push("text-align: right".to_string()),
        Some(Alignment::Justify) => style.push("text-align: justify".to_string()),
        _ => {}
    }
    if let Some(left) = props.indent.left.filter(|l| *l != 0) {
        style.push(format!("margin-left: {}pt", left / 20));
    }
    let offset = props.indent.first_line_offset();
    if offset != 0 {
        style.push(format!("text-indent: {}pt", offset / 20));
    }
    if style.is_empty() {
        String::new()
    } else {
        format!(" style=\"{}\"", style.join("; "))
    }
}

/// only links that cannot run code are written as links.
pub(crate) fn safe_href(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with('#')
}

fn wrap_marks(marks: &[Mark], content: String) -> String {
    if content.is_empty() {
        return content;
    }
    let mut html = content;
    let mut styles = Vec::new();
    for mark in marks.iter().rev() {
        html = match mark {
            Mark::Bold => format!("<strong>{html}</strong>"),
            Mark::Italic => format!("<em>{html}</em>"),
            Mark::Underline { .. } => format!("<u>{html}</u>"),
            Mark::Strike | Mark::DoubleStrike => format!("<s>{html}</s>"),
            Mark::Superscript => format!("<sup>{html}</sup>"),
            Mark::Subscript => format!("<sub>{html}</sub>"),
            Mark::Link(target) => {
                let href = match (&target.href, &target.anchor) {
                    (Some(href), _) => href.clone(),
                    (None, Some(anchor)) => format!("#{anchor}"),
                    _ => String::new(),
                };
                if safe_href(&href) {
                    format!("<a href=\"{}\">{html}</a>", escape_attribute(&href))
                } else {
                    html
                }
            }
            Mark::Insertion(change) => format!(
                "<ins title=\"{}\">{html}</ins>",
                escape_attribute(&change.author)
            ),
            Mark::Deletion(change) => format!(
                "<del title=\"{}\">{html}</del>",
                escape_attribute(&change.author)
            ),
            Mark::FontFamily { name, .. } => {
                styles.push(format!("font-family: '{}'", name.replace('\'', "")));
                html
            }
            Mark::FontSize { half_points } => {
                styles.push(format!("font-size: {}pt", f64::from(*half_points) / 2.0));
                html
            }
            Mark::Color { hex } => {
                styles.push(format!("color: #{hex}"));
                html
            }
            Mark::Highlight { color } => {
                let css = if color.len() == 6 && color.chars().all(|c| c.is_ascii_hexdigit()) {
                    format!("#{color}")
                } else {
                    color.to_lowercase()
                };
                styles.push(format!("background-color: {css}"));
                html
            }
            Mark::SmallCaps => {
                styles.push("font-variant: small-caps".into());
                html
            }
            Mark::AllCaps => {
                styles.push("text-transform: uppercase".into());
                html
            }
            _ => html,
        };
    }
    if styles.is_empty() {
        html
    } else {
        format!(
            "<span style=\"{}\">{html}</span>",
            escape_attribute(&styles.join("; "))
        )
    }
}
