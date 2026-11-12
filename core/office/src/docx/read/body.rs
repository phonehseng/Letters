//! block and inline content: paragraphs, runs, links, tracked changes,
//! fields, bookmarks and comment ranges.

use crate::model::{
    merge_adjacent_text, normalize_marks, Attribute, Block, BreakKind, Change, Field, FieldKind,
    Heading, Inline, LinkTarget, Mark, Paragraph, Section,
};
use crate::xml::{Element, Node};

use super::props::{read_paragraph_props, read_run_props};
use super::{DocxReader, Story};
use crate::anchors::{place_drawings, placeholder, take_drawings};

/// a run's content before fields are recognised. a complex field is
/// spread over several runs (begin, code, separator, result, end), so
/// runs are read into pieces first and fields assembled afterwards.
#[allow(clippy::large_enum_variant)]
enum Piece {
    Inline(Inline),
    FieldBegin {
        xml: String,
        marks: Vec<Mark>,
    },
    FieldCode {
        text: String,
        xml: String,
        marks: Vec<Mark>,
    },
    FieldSeparate {
        xml: String,
        marks: Vec<Mark>,
    },
    FieldEnd {
        xml: String,
        marks: Vec<Mark>,
    },
}

impl DocxReader<'_> {
    /// reads the blocks inside a body, cell, header, note or comment.
    /// `sections` collects section properties met at paragraph ends.
    pub(super) fn read_blocks(
        &mut self,
        container: &Element,
        story: &mut Story,
        sections: &mut Vec<Section>,
    ) -> Vec<Block> {
        let mut blocks = Vec::new();
        for child in container.elements() {
            match child.name.as_str() {
                "w:p" => self.read_paragraph(child, story, sections, &mut blocks),
                "w:tbl" => {
                    let table = self.read_table(child, story);
                    blocks.push(Block::Table(table));
                }
                // the body's own section properties are read by the caller
                "w:sectPr" if story.is_main => {}
                // spelling marks carry no content; cell properties are
                // read by the table reader
                "w:proofErr" | "w:tcPr" => {}
                _ => blocks.push(Block::Preserved {
                    xml: story.keep(child),
                }),
            }
        }
        blocks
    }

    fn read_paragraph(
        &mut self,
        element: &Element,
        story: &mut Story,
        sections: &mut Vec<Section>,
        blocks: &mut Vec<Block>,
    ) {
        let mut paragraph = Paragraph::default();
        let mut section_element = None;
        if let Some(ppr) = element.child("w:pPr") {
            let read = read_paragraph_props(ppr, story.xml);
            paragraph.style_id = read.style_id;
            paragraph.props = read.props;
            section_element = read.section;
        }
        paragraph.attributes = element
            .attributes
            .iter()
            .map(|(name, value)| Attribute::new(name, value))
            .collect();
        let mut pieces = Vec::new();
        let first_drawing = self.pending_objects.len();
        self.read_inlines(element, story, &[], &mut pieces);
        let content = merge_adjacent_text(assemble_fields(pieces));
        let (content, drawings) = take_drawings(content, &mut self.pending_objects, first_drawing);
        paragraph.content = content;
        let block = self.classify(paragraph.clone());
        place_drawings(block, &paragraph, drawings, blocks);
        if let Some(section) = section_element {
            if story.is_main {
                let section = self.read_section(section, story);
                sections.push(section);
                blocks.push(Block::SectionBreak);
            }
        }
    }

    /// a paragraph becomes a heading when its outline level (its own or
    /// its style's) says so, or a page break block when that is all it is.
    fn classify(&self, paragraph: Paragraph) -> Block {
        let only_page_break = paragraph.style_id.is_none()
            && paragraph.props.is_empty()
            && matches!(
                paragraph.content.as_slice(),
                [Inline::Break { kind: BreakKind::Page, marks }] if marks.is_empty()
            );
        if only_page_break {
            return Block::PageBreak;
        }
        match crate::styles::heading_level(&self.doc, &paragraph) {
            Some(level) => Block::Heading(Heading { level, paragraph }),
            None => Block::Paragraph(paragraph),
        }
    }

    /// reads inline content. `context` holds the marks of the elements
    /// around it (a link, a tracked change).
    fn read_inlines(
        &mut self,
        parent: &Element,
        story: &mut Story,
        context: &[Mark],
        out: &mut Vec<Piece>,
    ) {
        for node in &parent.children {
            let Node::Element(child) = node else {
                continue;
            };
            match child.name.as_str() {
                "w:pPr" | "w:proofErr" => {}
                "w:r" => self.read_run(child, story, context, out),
                "w:hyperlink" => {
                    let mut marks = context.to_vec();
                    marks.push(Mark::Link(link_target(child, story)));
                    self.read_inlines(child, story, &marks, out);
                }
                "w:ins" | "w:del" => {
                    let change = Change {
                        id: child.attr("w:id").unwrap_or_default().to_string(),
                        author: child.attr("w:author").unwrap_or_default().to_string(),
                        date: child.attr("w:date").map(str::to_string),
                    };
                    let mut marks = context.to_vec();
                    marks.push(if child.name == "w:ins" {
                        Mark::Insertion(change)
                    } else {
                        Mark::Deletion(change)
                    });
                    self.read_inlines(child, story, &marks, out);
                }
                "w:fldSimple" => self.read_simple_field(child, story, context, out),
                "w:bookmarkStart" if bookmark_is_simple(child) => {
                    out.push(Piece::Inline(Inline::BookmarkStart {
                        id: child.attr("w:id").unwrap_or_default().to_string(),
                        name: child.attr("w:name").unwrap_or_default().to_string(),
                    }))
                }
                "w:bookmarkEnd" if child.attributes.len() == 1 => {
                    out.push(Piece::Inline(Inline::BookmarkEnd {
                        id: child.attr("w:id").unwrap_or_default().to_string(),
                    }))
                }
                "w:commentRangeStart" => {
                    if let Some(id) = child.attr("w:id") {
                        story.open_comments.push(id.to_string());
                    }
                }
                "w:commentRangeEnd" => {
                    if let Some(id) = child.attr("w:id") {
                        story.open_comments.retain(|open| open != id);
                    }
                }
                _ => out.push(Piece::Inline(Inline::Preserved {
                    xml: story.keep(child),
                })),
            }
        }
    }

    fn read_run(
        &mut self,
        run: &Element,
        story: &mut Story,
        context: &[Mark],
        out: &mut Vec<Piece>,
    ) {
        let mut marks = match run.child("w:rPr") {
            Some(rpr) => read_run_props(rpr, story.xml, &self.theme).to_marks(),
            None => Vec::new(),
        };
        marks.extend_from_slice(context);
        for id in &story.open_comments {
            marks.push(Mark::Comment { id: id.clone() });
        }
        normalize_marks(&mut marks);
        for child in run.elements() {
            if let Some(piece) = self.read_run_child(child, story, &marks) {
                out.push(piece);
            }
        }
    }

    fn read_run_child(
        &mut self,
        child: &Element,
        story: &mut Story,
        marks: &[Mark],
    ) -> Option<Piece> {
        let marks = marks.to_vec();
        let inline = match child.name.as_str() {
            "w:rPr"
            | "w:lastRenderedPageBreak"
            | "w:footnoteRef"
            | "w:endnoteRef"
            | "w:annotationRef" => return None,
            "w:t" | "w:delText" => Inline::Text {
                text: child.text(),
                marks,
            },
            "w:tab" => Inline::Tab { marks },
            "w:br" if child.attr("w:clear").is_none() => Inline::Break {
                kind: match child.attr("w:type") {
                    Some("page") => BreakKind::Page,
                    Some("column") => BreakKind::Column,
                    _ => BreakKind::Line,
                },
                marks,
            },
            "w:cr" => Inline::Break {
                kind: BreakKind::Line,
                marks,
            },
            "w:noBreakHyphen" => Inline::Text {
                text: "\u{2011}".into(),
                marks,
            },
            "w:softHyphen" => Inline::Text {
                text: "\u{AD}".into(),
                marks,
            },
            "w:footnoteReference" | "w:endnoteReference" if child.attributes.len() == 1 => {
                let prefix = if child.name == "w:footnoteReference" {
                    "fn"
                } else {
                    "en"
                };
                Inline::NoteReference {
                    note_id: format!("{prefix}{}", child.attr("w:id").unwrap_or_default()),
                    marks,
                }
            }
            "w:commentReference" => Inline::CommentReference {
                comment_id: child.attr("w:id").unwrap_or_default().to_string(),
            },
            "w:drawing" | "mc:AlternateContent" | "w:pict" => {
                if let Some(object) = self.read_object(child, story) {
                    let number = self.pending_objects.len();
                    self.pending_objects.push((object, marks));
                    // a placeholder marks where the drawing sat, so the
                    // paragraph can say which inline it came before
                    return Some(Piece::Inline(placeholder(number)));
                }
                if child.name == "w:drawing" {
                    self.read_drawing(child, story, marks)
                } else {
                    Inline::PreservedRun {
                        xml: story.keep(child),
                        marks,
                    }
                }
            }
            "w:fldChar" => return Some(field_char(child, story, marks)),
            "w:instrText" | "w:delInstrText" => {
                return Some(Piece::FieldCode {
                    text: child.text(),
                    xml: story.keep(child),
                    marks,
                })
            }
            _ => Inline::PreservedRun {
                xml: story.keep(child),
                marks,
            },
        };
        Some(Piece::Inline(inline))
    }

    /// `w:fldSimple`: a field in one element. understood fields become a
    /// `Field`; others are kept as they are.
    fn read_simple_field(
        &mut self,
        element: &Element,
        story: &mut Story,
        context: &[Mark],
        out: &mut Vec<Piece>,
    ) {
        let instruction = element
            .attr("w:instr")
            .unwrap_or_default()
            .trim()
            .to_string();
        let mut result = Vec::new();
        self.read_inlines(element, story, context, &mut result);
        let kind = FieldKind::from_instruction(&instruction);
        let text_only = result
            .iter()
            .all(|piece| matches!(piece, Piece::Inline(Inline::Text { .. })));
        match kind {
            Some(kind) if text_only && element.attributes.len() == 1 => {
                let mut marks = context.to_vec();
                let mut text = String::new();
                for (index, piece) in result.into_iter().enumerate() {
                    if let Piece::Inline(Inline::Text {
                        text: part,
                        marks: own,
                    }) = piece
                    {
                        if index == 0 {
                            marks = own;
                        }
                        text.push_str(&part);
                    }
                }
                out.push(Piece::Inline(Inline::Field(Field {
                    kind,
                    instruction,
                    result: text,
                    marks,
                })));
            }
            _ => out.push(Piece::Inline(Inline::Preserved {
                xml: story.keep(element),
            })),
        }
    }
}

fn field_char(element: &Element, story: &mut Story, marks: Vec<Mark>) -> Piece {
    let xml = story.keep(element);
    // a field char with extra content (form field data...) is kept whole
    let simple = element.attributes.len() == 1 && element.elements().next().is_none();
    match element.attr("w:fldCharType") {
        Some("begin") if simple => Piece::FieldBegin { xml, marks },
        Some("separate") if simple => Piece::FieldSeparate { xml, marks },
        Some("end") if simple => Piece::FieldEnd { xml, marks },
        _ => Piece::Inline(Inline::PreservedRun { xml, marks }),
    }
}

fn bookmark_is_simple(element: &Element) -> bool {
    element
        .attributes
        .iter()
        .all(|(name, _)| name == "w:id" || name == "w:name")
}

fn link_target(element: &Element, story: &Story) -> LinkTarget {
    let href = element
        .attr("r:id")
        .and_then(|id| story.rel(id))
        .map(|rel| rel.target.clone());
    LinkTarget {
        href,
        anchor: element.attr("w:anchor").map(str::to_string),
        tooltip: element.attr("w:tooltip").map(str::to_string),
    }
}

/// turns the pieces of a paragraph into inline content, recognising
/// complex fields that begin and end inside it. a field the model does
/// not understand (or one running across paragraphs, like a long table of
/// contents) stays as preserved runs around its visible result.
fn assemble_fields(pieces: Vec<Piece>) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < pieces.len() {
        if let Piece::FieldBegin { .. } = pieces[index] {
            if let Some((field, next)) = understood_field(&pieces, index) {
                out.push(Inline::Field(field));
                index = next;
                continue;
            }
        }
        out.push(piece_to_inline(&pieces[index]));
        index += 1;
    }
    out
}

/// looks for `begin code [separate result] end` starting at `start` with
/// nothing nested inside, and a code the model knows.
fn understood_field(pieces: &[Piece], start: usize) -> Option<(Field, usize)> {
    let mut code = String::new();
    let mut result = String::new();
    let mut result_marks: Option<Vec<Mark>> = None;
    let mut separated = false;
    let begin_marks = match &pieces[start] {
        Piece::FieldBegin { marks, .. } => marks.clone(),
        _ => return None,
    };
    for (offset, piece) in pieces[start + 1..].iter().enumerate() {
        match piece {
            Piece::FieldCode { text, .. } if !separated => code.push_str(text),
            Piece::FieldSeparate { .. } if !separated => separated = true,
            Piece::Inline(Inline::Text { text, marks }) if separated => {
                result.push_str(text);
                result_marks.get_or_insert_with(|| marks.clone());
            }
            Piece::FieldEnd { .. } => {
                let kind = FieldKind::from_instruction(&code)?;
                let field = Field {
                    kind,
                    instruction: code.trim().to_string(),
                    result,
                    marks: result_marks.unwrap_or(begin_marks),
                };
                return Some((field, start + offset + 2));
            }
            _ => return None,
        }
    }
    None
}

fn piece_to_inline(piece: &Piece) -> Inline {
    match piece {
        Piece::Inline(inline) => inline.clone(),
        Piece::FieldBegin { xml, marks }
        | Piece::FieldCode { xml, marks, .. }
        | Piece::FieldSeparate { xml, marks }
        | Piece::FieldEnd { xml, marks } => Inline::PreservedRun {
            xml: xml.clone(),
            marks: marks.clone(),
        },
    }
}
