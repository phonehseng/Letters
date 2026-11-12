//! the office model flattened into the few things a markup language has:
//! headings, paragraphs, list items, code blocks, tables, page breaks.
//!
//! inline text keeps bold, italic, strike, code (a monospaced font) and
//! links. everything else (colours, sizes, tracked deletions) is dropped,
//! as markdown export does, because these formats have no way to say it.

use letters_office::model::{Block, Document, Image, Inline, Mark, Paragraph, Table};
use letters_office::numbering::ListCounter;

/// fonts that mean "this is code" when a run or a paragraph style uses them.
const MONOSPACED_FONTS: [&str; 14] = [
    "consolas",
    "courier",
    "courier new",
    "menlo",
    "monaco",
    "lucida console",
    "cascadia code",
    "cascadia mono",
    "source code pro",
    "fira code",
    "fira mono",
    "dejavu sans mono",
    "liberation mono",
    "jetbrains mono",
];

/// true for a font name that is one of the common monospaced faces.
pub fn is_monospaced_font(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    MONOSPACED_FONTS.contains(&name.as_str()) || name.ends_with(" mono") || name == "monospace"
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(Span),
    LineBreak,
    /// a picture: its file under "images/" and its description
    Picture {
        file: String,
        description: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimpleBlock {
    Heading {
        level: u8,
        pieces: Vec<Piece>,
    },
    Paragraph {
        pieces: Vec<Piece>,
    },
    ListItem {
        level: u8,
        ordered: bool,
        pieces: Vec<Piece>,
    },
    /// a paragraph set entirely in a monospaced font, or in a "Code" style
    Code {
        text: String,
    },
    /// rows of cells, each cell as plain pieces; the first row is the header
    Table {
        rows: Vec<Vec<Vec<Piece>>>,
    },
    PageBreak,
}

/// the document's blocks, and the picture files they refer to.
pub struct Flattened {
    pub blocks: Vec<SimpleBlock>,
    pub files: Vec<(String, Vec<u8>)>,
}

pub fn flatten(document: &Document) -> Flattened {
    let mut flattener = Flattener {
        document,
        counter: ListCounter::new(document),
        files: Vec::new(),
    };
    let mut blocks = Vec::new();
    for block in &document.blocks {
        flattener.block(block, &mut blocks);
    }
    Flattened {
        blocks: merge_code_lines(blocks),
        files: flattener.files,
    }
}

struct Flattener<'a> {
    document: &'a Document,
    counter: ListCounter<'a>,
    files: Vec<(String, Vec<u8>)>,
}

impl Flattener<'_> {
    fn block(&mut self, block: &Block, out: &mut Vec<SimpleBlock>) {
        match block {
            Block::Heading(heading) => {
                let pieces = self.pieces(&heading.paragraph.content);
                out.push(SimpleBlock::Heading {
                    level: heading.level.clamp(1, 6),
                    pieces,
                });
            }
            Block::Paragraph(paragraph) => self.paragraph(block, paragraph, out),
            Block::Table(table) => out.push(self.table(table)),
            Block::PageBreak => out.push(SimpleBlock::PageBreak),
            Block::TextBox(text_box) => {
                for inner in &text_box.blocks {
                    self.block(inner, out);
                }
            }
            _ => {}
        }
    }

    fn paragraph(&mut self, block: &Block, paragraph: &Paragraph, out: &mut Vec<SimpleBlock>) {
        if is_code_paragraph(self.document, paragraph) {
            out.push(SimpleBlock::Code {
                text: plain_text(&paragraph.content),
            });
            return;
        }
        let label = self.counter.label_for_block(block);
        let pieces = self.pieces(&paragraph.content);
        match (paragraph.props.list.as_ref(), label) {
            (Some(list), Some(label)) => out.push(SimpleBlock::ListItem {
                level: list.level,
                // a label with a digit or letter ("1.", "a)") is a numbered list
                ordered: label.chars().any(char::is_alphanumeric),
                pieces,
            }),
            _ => match self.style_list(paragraph) {
                Some((level, ordered)) => out.push(SimpleBlock::ListItem {
                    level,
                    ordered,
                    pieces,
                }),
                None if has_visible_text(&pieces) => out.push(SimpleBlock::Paragraph { pieces }),
                None => {}
            },
        }
    }

    /// a list that comes from the paragraph's style ("List Bullet",
    /// "List Number"), which word uses as often as numbering set on the
    /// paragraph itself. returns the level and whether it is numbered;
    /// the style's name is the only cheap sign of which kind it is.
    fn style_list(&self, paragraph: &Paragraph) -> Option<(u8, bool)> {
        let style = self.document.style(paragraph.style_id.as_deref()?)?;
        let list = style.paragraph.list.as_ref()?;
        // "0" means "explicitly not in a list" (see ListRef)
        if list.list_id == "0" {
            return None;
        }
        let ordered = style.name.to_ascii_lowercase().contains("number");
        Some((list.level, ordered))
    }

    fn table(&mut self, table: &Table) -> SimpleBlock {
        let width = letters_office::tables::grid_width(table).max(1);
        let rows = table
            .rows
            .iter()
            .map(|row| {
                let mut cells: Vec<Vec<Piece>> = row
                    .cells
                    .iter()
                    .map(|cell| {
                        let inlines: Vec<Inline> = cell
                            .blocks
                            .iter()
                            .filter_map(Block::paragraph)
                            .flat_map(|paragraph| paragraph.content.clone())
                            .collect();
                        self.pieces(&inlines)
                    })
                    .collect();
                cells.resize(width, Vec::new());
                cells
            })
            .collect();
        SimpleBlock::Table { rows }
    }

    fn pieces(&mut self, inlines: &[Inline]) -> Vec<Piece> {
        let mut pieces = Vec::new();
        for inline in inlines.iter().filter(|inline| !inline.is_deleted()) {
            match inline {
                Inline::Text { text, marks } => pieces.push(Piece::Text(span(text, marks))),
                Inline::Tab { marks } => pieces.push(Piece::Text(span(" ", marks))),
                Inline::Field(field) => pieces.push(Piece::Text(span(&field.result, &[]))),
                Inline::Break { .. } => pieces.push(Piece::LineBreak),
                Inline::Image(image) => pieces.extend(self.picture(image)),
                _ => {}
            }
        }
        pieces
    }

    fn picture(&mut self, image: &Image) -> Option<Piece> {
        let item = self.document.media_item(&image.media_id)?;
        // the same safe name markdown export uses, so both exports agree
        let file = format!("images/{}", item.id.replace(['/', '\\', ' '], "_"));
        if !self.files.iter().any(|(path, _)| *path == file) {
            self.files.push((file.clone(), item.bytes.clone()));
        }
        Some(Piece::Picture {
            file,
            description: image.description.clone().unwrap_or_default(),
        })
    }
}

fn span(text: &str, marks: &[Mark]) -> Span {
    let mut span = Span {
        text: text.to_owned(),
        ..Span::default()
    };
    for mark in marks {
        match mark {
            Mark::Bold => span.bold = true,
            Mark::Italic => span.italic = true,
            Mark::Strike | Mark::DoubleStrike => span.strike = true,
            Mark::FontFamily { name, .. } if is_monospaced_font(name) => span.code = true,
            Mark::Link(target) => {
                span.link = target
                    .href
                    .clone()
                    .or_else(|| target.anchor.as_ref().map(|anchor| format!("#{anchor}")));
            }
            _ => {}
        }
    }
    span
}

/// a paragraph whose style is a code style, or whose every letter is monospaced.
pub fn is_code_paragraph(document: &Document, paragraph: &Paragraph) -> bool {
    if let Some(style) = paragraph
        .style_id
        .as_deref()
        .and_then(|id| document.style(id))
    {
        let name = style.name.to_ascii_lowercase();
        let style_font_is_mono = style
            .run
            .font
            .as_ref()
            .is_some_and(|font| is_monospaced_font(&font.name));
        if name.contains("code") || name.contains("preformatted") || style_font_is_mono {
            return true;
        }
    }
    let texts: Vec<&[Mark]> = paragraph
        .content
        .iter()
        .filter_map(|inline| match inline {
            Inline::Text { text, marks } if !text.trim().is_empty() => Some(marks.as_slice()),
            _ => None,
        })
        .collect();
    !texts.is_empty()
        && texts
            .iter()
            .all(|marks| marks.iter().any(is_monospaced_mark))
}

fn is_monospaced_mark(mark: &Mark) -> bool {
    matches!(mark, Mark::FontFamily { name, .. } if is_monospaced_font(name))
}

/// the text of some inlines, with tabs kept and line breaks as "\n".
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines.iter().filter(|inline| !inline.is_deleted()) {
        match inline {
            Inline::Text { text: part, .. } => text.push_str(part),
            Inline::Tab { .. } => text.push('\t'),
            Inline::Break { .. } => text.push('\n'),
            Inline::Field(field) => text.push_str(&field.result),
            _ => {}
        }
    }
    text
}

fn has_visible_text(pieces: &[Piece]) -> bool {
    pieces.iter().any(|piece| match piece {
        Piece::Text(span) => !span.text.trim().is_empty(),
        Piece::Picture { .. } => true,
        Piece::LineBreak => false,
    })
}

// word stores each line of a code sample as its own paragraph; one block
// per sample reads far better in every markup language
fn merge_code_lines(blocks: Vec<SimpleBlock>) -> Vec<SimpleBlock> {
    let mut merged: Vec<SimpleBlock> = Vec::with_capacity(blocks.len());
    for block in blocks {
        match (merged.last_mut(), block) {
            (Some(SimpleBlock::Code { text }), SimpleBlock::Code { text: next }) => {
                text.push('\n');
                text.push_str(&next);
            }
            (_, block) => merged.push(block),
        }
    }
    merged
}

/// the plain text of some pieces, for writers that only want words (csv, json).
pub fn pieces_text(pieces: &[Piece]) -> String {
    let mut text = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(span) => text.push_str(&span.text),
            Piece::LineBreak => text.push(' '),
            Piece::Picture { description, .. } => text.push_str(description),
        }
    }
    text.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::Paragraph;

    fn mono_paragraph(text: &str) -> Block {
        let marks = vec![Mark::FontFamily {
            name: "Consolas".into(),
            theme: None,
            extra: Vec::new(),
        }];
        Block::Paragraph(Paragraph {
            content: vec![Inline::styled(text, marks)],
            ..Paragraph::default()
        })
    }

    #[test]
    fn headings_lists_and_tables_survive_flattening() {
        let flat = flatten(&crate::code::export::test_document::sample());
        assert!(matches!(
            &flat.blocks[0],
            SimpleBlock::Heading { level: 1, .. }
        ));
        let unordered = flat
            .blocks
            .iter()
            .filter(|block| matches!(block, SimpleBlock::ListItem { ordered: false, .. }))
            .count();
        assert_eq!(unordered, 2);
        assert!(flat
            .blocks
            .iter()
            .any(|block| matches!(block, SimpleBlock::Table { .. })));
    }

    #[test]
    fn consecutive_monospaced_paragraphs_become_one_code_block() {
        let mut document = Document::new();
        document.blocks = vec![mono_paragraph("int main(void) {"), mono_paragraph("}")];
        let flat = flatten(&document);
        assert_eq!(
            flat.blocks,
            vec![SimpleBlock::Code {
                text: "int main(void) {\n}".into()
            }]
        );
    }

    #[test]
    fn a_list_set_by_its_style_is_still_a_list() {
        let mut document = Document::new();
        let mut style = letters_office::model::Style::paragraph("ListBullet", "List Bullet");
        style.paragraph.list = Some(letters_office::model::ListRef {
            list_id: "3".into(),
            level: 0,
        });
        document.styles.styles.push(style);
        document.blocks = vec![Block::Paragraph(Paragraph {
            style_id: Some("ListBullet".into()),
            content: vec![Inline::text("first")],
            ..Paragraph::default()
        })];
        let flat = flatten(&document);
        assert!(matches!(
            &flat.blocks[0],
            SimpleBlock::ListItem {
                level: 0,
                ordered: false,
                ..
            }
        ));
    }

    #[test]
    fn monospaced_font_names_are_recognised() {
        assert!(is_monospaced_font("Courier New"));
        assert!(is_monospaced_font("Roboto Mono"));
        assert!(!is_monospaced_font("Calibri"));
    }
}
