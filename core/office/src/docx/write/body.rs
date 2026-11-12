//! the main document body and the blocks and runs inside any story.

use crate::model::{
    AnchorWith, Block, BreakKind, Change, Field, Inline, LinkTarget, Mark, ObjectAnchor, Paragraph,
    RunProps, Section,
};
use crate::xml::{escape_text, XmlWriter};

use super::props::{empty, val};
use super::{DocxWriter, StoryOut};
use crate::docx::names::CT_MAIN;

/// where a tracked change wraps content.
#[derive(Clone, PartialEq)]
enum ChangeWrap<'a> {
    Inserted(&'a Change),
    Deleted(&'a Change),
}

impl DocxWriter<'_> {
    pub(super) fn write_main(&mut self) {
        let part = self.main_part.clone();
        let carried = self.carried_rels(&part);
        let mut story = StoryOut::new(&part, &carried, &[&self.doc.blocks]);
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root_tag(&part, "w:document"));
        if let Some(docx) = self.docx_carry() {
            for extra in &docx.document_extras {
                xml.raw(extra);
            }
        }
        xml.open("w:body", &[]);
        let doc = self.doc;
        self.write_blocks(&doc.blocks, &mut story, &mut xml, None, true);
        let last = doc.sections.last().cloned().unwrap_or_default();
        let section = self.section_xml(&last, &mut story);
        xml.raw(&section);
        xml.close("w:body");
        xml.close("w:document");
        self.write_story_parts(&mut story);
        let content_type = self
            .docx_carry()
            .map(|docx| docx.main_content_type.clone())
            .unwrap_or_else(|| CT_MAIN.to_string());
        self.add_part(&part, Some(&content_type), xml.finish().into_bytes());
        self.add_rels(&story);
    }

    /// writes blocks. `lead` is run xml put at the start of the first
    /// paragraph (the number inside a footnote). `main` is true only for
    /// the body, where section breaks belong.
    pub(crate) fn write_blocks(
        &mut self,
        blocks: &[Block],
        story: &mut StoryOut,
        out: &mut XmlWriter,
        mut lead: Option<String>,
        main: bool,
    ) {
        let mut section_index = 0;
        let mut index = 0;
        // drawings waiting for the paragraph of text that anchors them,
        // with the inline index each came before
        let mut waiting: Vec<(usize, String)> = Vec::new();
        while index < blocks.len() {
            // word cannot float a drawing inside a text box or a shape's
            // text (it reports a damaged file or hangs printing it), so a
            // nested drawing keeps only its text, as paragraphs
            if is_drawing(&blocks[index]) && self.drawing_depth > 0 {
                let inner: Vec<Block> = blocks[index]
                    .drawing_blocks()
                    .into_iter()
                    .cloned()
                    .collect();
                self.write_blocks(&inner, story, out, lead.take(), false);
                index += 1;
                continue;
            }
            if is_drawing(&blocks[index]) {
                let anchor = blocks[index].object_anchor().cloned().unwrap_or_default();
                let run = self.object_run(&blocks[index], &anchor, story);
                if anchor.with == AnchorWith::Next && next_takes_drawings(blocks, index + 1) {
                    waiting.push((anchor.inline_index.unwrap_or(0), run));
                    index += 1;
                    continue;
                }
                // an empty paragraph of its own, shared with the drawings
                // after it that say so
                let mut runs = std::mem::take(&mut waiting);
                runs.push((0, run));
                let mut end = index + 1;
                while let Some(next) = blocks.get(end) {
                    let shares = is_drawing(next)
                        && next.object_anchor().map(|a| a.with) == Some(AnchorWith::Previous);
                    if !shares {
                        break;
                    }
                    let anchor = next.object_anchor().cloned().unwrap_or_default();
                    runs.push((0, self.object_run(next, &anchor, story)));
                    end += 1;
                }
                let carries_break = main && matches!(blocks.get(end), Some(Block::SectionBreak));
                let section = if carries_break {
                    self.doc.sections.get(section_index).cloned()
                } else {
                    None
                };
                let paragraph = anchor.paragraph.clone().unwrap_or_default();
                self.write_paragraph_with(
                    &paragraph,
                    None,
                    section.as_ref(),
                    story,
                    out,
                    lead.take(),
                    runs,
                );
                if carries_break {
                    section_index += 1;
                    end += 1;
                }
                index = end;
                continue;
            }
            // a section ends with the paragraph just before its break
            let is_paragraph = matches!(blocks[index], Block::Paragraph(_) | Block::Heading(_));
            let carries_break =
                main && is_paragraph && matches!(blocks.get(index + 1), Some(Block::SectionBreak));
            let section = if carries_break {
                self.doc.sections.get(section_index).cloned()
            } else {
                None
            };
            match &blocks[index] {
                Block::Paragraph(paragraph) => {
                    let runs = std::mem::take(&mut waiting);
                    self.write_paragraph_with(
                        paragraph,
                        None,
                        section.as_ref(),
                        story,
                        out,
                        lead.take(),
                        runs,
                    );
                }
                Block::Heading(heading) => {
                    let runs = std::mem::take(&mut waiting);
                    self.write_paragraph_with(
                        &heading.paragraph,
                        Some(heading.level),
                        section.as_ref(),
                        story,
                        out,
                        lead.take(),
                        runs,
                    )
                }
                Block::Table(table) => {
                    if let Some(lead) = lead.take() {
                        self.write_paragraph(
                            &Paragraph::default(),
                            None,
                            None,
                            story,
                            out,
                            Some(lead),
                        );
                    }
                    self.write_table(table, story, out);
                }
                Block::PageBreak => {
                    out.raw("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
                }
                Block::SectionBreak => {
                    // reached only when no paragraph came just before it
                    if main {
                        let section = self
                            .doc
                            .sections
                            .get(section_index)
                            .cloned()
                            .unwrap_or_default();
                        let runs = std::mem::take(&mut waiting);
                        self.write_paragraph_with(
                            &Paragraph::default(),
                            None,
                            Some(&section),
                            story,
                            out,
                            None,
                            runs,
                        );
                        section_index += 1;
                    }
                }
                Block::Preserved { xml } => self.write_preserved_block(xml, out),
                // drawings were written above
                Block::TextBox(_) | Block::Shape(_) | Block::Group(_) => {}
            }
            if carries_break {
                // the break went out with the paragraph, so skip it
                section_index += 1;
                index += 1;
            }
            index += 1;
        }
        if !waiting.is_empty() || lead.is_some() {
            self.write_paragraph_with(&Paragraph::default(), None, None, story, out, lead, waiting);
        }
    }

    /// the run holding a drawing, with the formatting its run had.
    fn object_run(&mut self, block: &Block, anchor: &ObjectAnchor, story: &mut StoryOut) -> String {
        self.drawing_depth += 1;
        let content = self.object_xml(block, story);
        self.drawing_depth -= 1;
        self.run(&anchor.marks, &content)
    }

    fn write_preserved_block(&mut self, xml: &str, out: &mut XmlWriter) {
        if self.native {
            out.raw(xml);
        } else {
            // xml from another format cannot go in; its text can
            let text = foreign_text(xml);
            out.raw(&format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                escape_text(&text)
            ));
        }
    }

    pub(crate) fn write_paragraph(
        &mut self,
        paragraph: &Paragraph,
        heading: Option<u8>,
        section: Option<&Section>,
        story: &mut StoryOut,
        out: &mut XmlWriter,
        lead: Option<String>,
    ) {
        self.write_paragraph_with(paragraph, heading, section, story, out, lead, Vec::new());
    }

    /// a paragraph with drawing runs put in among its content: each
    /// `(index, run)` goes just before inline `index`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_paragraph_with(
        &mut self,
        paragraph: &Paragraph,
        heading: Option<u8>,
        section: Option<&Section>,
        story: &mut StoryOut,
        out: &mut XmlWriter,
        lead: Option<String>,
        mut drawings: Vec<(usize, String)>,
    ) {
        let mut extra = Vec::new();
        if let Some(level) = heading {
            let style_level = crate::styles::heading_level(self.doc, paragraph);
            if style_level != Some(level) && paragraph.props.outline_level.is_none() {
                extra.push((
                    "w:outlineLvl",
                    val("w:outlineLvl", &(level - 1).to_string()),
                ));
            }
        }
        if let Some(section) = section {
            extra.push(("w:sectPr", self.section_xml(section, story)));
        }
        let props = self.paragraph_props(paragraph.style_id.as_deref(), &paragraph.props, extra);
        let attributes: Vec<(String, String)> = if self.native {
            paragraph
                .attributes
                .iter()
                .map(|a| (a.name.clone(), a.value.clone()))
                .collect()
        } else {
            Vec::new()
        };
        out.open_owned("w:p", &attributes);
        if !props.is_empty() {
            out.raw(&format!("<w:pPr>{props}</w:pPr>"));
        }
        if let Some(lead) = lead {
            out.raw(&lead);
        }
        let content = &paragraph.content;
        drawings.sort_by_key(|(at, _)| *at);
        let mut start = 0;
        for (at, run) in drawings {
            let at = at.clamp(start, content.len());
            self.write_inlines(&content[start..at], story, out);
            out.raw(&run);
            start = at;
        }
        self.write_inlines(&content[start..], story, out);
        out.close("w:p");
    }

    /// writes inline content, wrapping links and tracked changes around
    /// the runs they cover.
    fn write_inlines(&mut self, inlines: &[Inline], story: &mut StoryOut, out: &mut XmlWriter) {
        let mut start = 0;
        while start < inlines.len() {
            let link = link_of(&inlines[start]);
            let end = start
                + inlines[start..]
                    .iter()
                    .take_while(|inline| link_of(inline) == link)
                    .count();
            if let Some(target) = link {
                out.raw(&self.hyperlink_open(target, story));
            }
            self.write_changes(&inlines[start..end], story, out);
            if link.is_some() {
                out.close("w:hyperlink");
            }
            start = end;
        }
    }

    fn hyperlink_open(&self, target: &LinkTarget, story: &mut StoryOut) -> String {
        let mut attributes: Vec<(String, String)> = Vec::new();
        if let Some(href) = &target.href {
            attributes.push(("r:id".into(), story.link_rel(href)));
        }
        if let Some(anchor) = &target.anchor {
            attributes.push(("w:anchor".into(), anchor.clone()));
        }
        if let Some(tooltip) = &target.tooltip {
            attributes.push(("w:tooltip".into(), tooltip.clone()));
        }
        attributes.push(("w:history".into(), "1".into()));
        let mut xml = XmlWriter::new();
        xml.open_owned("w:hyperlink", &attributes);
        xml.finish()
    }

    fn write_changes(&mut self, inlines: &[Inline], story: &mut StoryOut, out: &mut XmlWriter) {
        let mut start = 0;
        while start < inlines.len() {
            let change = change_of(&inlines[start]);
            let end = start
                + inlines[start..]
                    .iter()
                    .take_while(|inline| change_of(inline) == change)
                    .count();
            let wrapper = change.as_ref().map(|wrap| self.change_open(wrap));
            if let Some((open, _)) = &wrapper {
                out.raw(open);
            }
            let deleted = matches!(change, Some(ChangeWrap::Deleted(_)));
            for inline in &inlines[start..end] {
                self.write_inline(inline, deleted, story, out);
            }
            if let Some((_, name)) = wrapper {
                out.close(name);
            }
            start = end;
        }
    }

    fn change_open(&mut self, wrap: &ChangeWrap) -> (String, &'static str) {
        let (name, change) = match wrap {
            ChangeWrap::Inserted(change) => ("w:ins", *change),
            ChangeWrap::Deleted(change) => ("w:del", *change),
        };
        let id = self.change_id(&change.id);
        let mut attributes = vec![
            ("w:id".to_string(), id),
            ("w:author".into(), change.author.clone()),
        ];
        if let Some(date) = &change.date {
            attributes.push(("w:date".into(), date.clone()));
        }
        let mut xml = XmlWriter::new();
        xml.open_owned(name, &attributes);
        (xml.finish(), name)
    }

    fn write_inline(
        &mut self,
        inline: &Inline,
        deleted: bool,
        story: &mut StoryOut,
        out: &mut XmlWriter,
    ) {
        self.open_comment_ranges(inline, story, out);
        let marks = inline.marks();
        match inline {
            Inline::Text { text, .. } => {
                let content = text_content(text, deleted);
                out.raw(&self.run(marks, &content));
            }
            Inline::Tab { .. } => out.raw(&self.run(marks, "<w:tab/>")),
            Inline::Break { kind, .. } => {
                let content = match kind {
                    BreakKind::Line => "<w:br/>",
                    BreakKind::Page => "<w:br w:type=\"page\"/>",
                    BreakKind::Column => "<w:br w:type=\"column\"/>",
                };
                out.raw(&self.run(marks, content));
            }
            Inline::Image(image) => {
                let drawing = self.drawing_xml(image, story);
                out.raw(&self.run(marks, &drawing));
            }
            Inline::NoteReference { note_id, .. } => self.write_note_reference(note_id, marks, out),
            Inline::Field(field) => self.write_field(field, deleted, out),
            Inline::BookmarkStart { id, name } => out.raw(&empty(
                "w:bookmarkStart",
                &[("w:id", &number_or_hash(id)), ("w:name", name)],
            )),
            Inline::BookmarkEnd { id } => {
                out.raw(&empty("w:bookmarkEnd", &[("w:id", &number_or_hash(id))]))
            }
            Inline::CommentReference { comment_id } => {
                if let Some(id) = self.comment_ids.get(comment_id).cloned() {
                    out.raw(&self.comment_reference_run(&id));
                }
            }
            Inline::PreservedRun { xml, .. } => {
                if self.native {
                    out.raw(&self.run(marks, xml));
                } else {
                    let content = text_content(&foreign_text(xml), deleted);
                    out.raw(&self.run(marks, &content));
                }
            }
            Inline::Preserved { xml } => {
                if self.native {
                    out.raw(xml);
                } else {
                    let content = text_content(&foreign_text(xml), deleted);
                    out.raw(&self.run(&[], &content));
                }
            }
        }
        self.close_comment_ranges(inline, story, out);
    }

    /// a `w:r` with the formatting of `marks` around `content`.
    pub(crate) fn run(&self, marks: &[Mark], content: &str) -> String {
        let props = self.run_props(&RunProps::from_marks(marks));
        if props.is_empty() {
            format!("<w:r>{content}</w:r>")
        } else {
            format!("<w:r><w:rPr>{props}</w:rPr>{content}</w:r>")
        }
    }

    fn write_note_reference(&mut self, note_id: &str, marks: &[Mark], out: &mut XmlWriter) {
        // a reference to a note that does not exist would make the file
        // unreadable, so it is left out
        let (Some(note), Some(number)) = (self.doc.note(note_id), self.note_ids.get(note_id))
        else {
            return;
        };
        let element = match note.kind {
            crate::model::NoteKind::Footnote => "w:footnoteReference",
            crate::model::NoteKind::Endnote => "w:endnoteReference",
        };
        let content = empty(element, &[("w:id", &number.to_string())]);
        let mut marks = marks.to_vec();
        if !self.native
            && !marks
                .iter()
                .any(|m| matches!(m, Mark::Superscript | Mark::CharacterStyle { .. }))
        {
            marks.push(Mark::Superscript);
        }
        out.raw(&self.run(&marks, &content));
    }

    /// fields are always written in word's long form (begin, code,
    /// separator, result, end), which is allowed everywhere, including
    /// inside a tracked deletion.
    fn write_field(&mut self, field: &Field, deleted: bool, out: &mut XmlWriter) {
        let code_element = if deleted {
            "w:delInstrText"
        } else {
            "w:instrText"
        };
        let marks = &field.marks;
        out.raw(&self.run(marks, "<w:fldChar w:fldCharType=\"begin\"/>"));
        out.raw(&self.run(
            marks,
            &format!(
                "<{code_element} xml:space=\"preserve\"> {} </{code_element}>",
                escape_text(&field.instruction)
            ),
        ));
        out.raw(&self.run(marks, "<w:fldChar w:fldCharType=\"separate\"/>"));
        if !field.result.is_empty() {
            out.raw(&self.run(marks, &text_content(&field.result, deleted)));
        }
        out.raw(&self.run(marks, "<w:fldChar w:fldCharType=\"end\"/>"));
    }

    fn open_comment_ranges(&mut self, inline: &Inline, story: &mut StoryOut, out: &mut XmlWriter) {
        for mark in inline.marks() {
            let Mark::Comment { id } = mark else { continue };
            let Some(number) = self.comment_ids.get(id) else {
                continue;
            };
            if story.comment_open.insert(id.clone()) {
                out.raw(&empty("w:commentRangeStart", &[("w:id", number)]));
            }
        }
    }

    fn close_comment_ranges(&mut self, inline: &Inline, story: &mut StoryOut, out: &mut XmlWriter) {
        for mark in inline.marks() {
            let Mark::Comment { id } = mark else { continue };
            let Some(number) = self.comment_ids.get(id).cloned() else {
                continue;
            };
            let remaining = story.comment_remaining.entry(id.clone()).or_default();
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 && story.comment_open.remove(id) {
                out.raw(&empty("w:commentRangeEnd", &[("w:id", &number)]));
                if !story.comment_referenced.contains(id) {
                    out.raw(&self.comment_reference_run(&number));
                }
            }
        }
    }

    fn comment_reference_run(&self, number: &str) -> String {
        let reference = empty("w:commentReference", &[("w:id", number)]);
        if self.has_style("CommentReference") {
            format!("<w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr>{reference}</w:r>")
        } else {
            format!("<w:r>{reference}</w:r>")
        }
    }

    /// the parts a story's content needs that are written once for the
    /// whole document: styles, numbering, settings, notes and comments.
    fn write_story_parts(&mut self, story: &mut StoryOut) {
        self.write_styles(story);
        self.write_numbering(story);
        self.write_settings(story);
        self.write_notes(story);
        self.write_comments(story);
    }
}

fn is_drawing(block: &Block) -> bool {
    matches!(block, Block::TextBox(_) | Block::Shape(_) | Block::Group(_))
}

/// whether drawings anchored to "the next paragraph" find one at `from`,
/// looking past other drawings that wait for it too.
fn next_takes_drawings(blocks: &[Block], from: usize) -> bool {
    for block in &blocks[from.min(blocks.len())..] {
        match block {
            Block::Paragraph(_) | Block::Heading(_) => return true,
            _ if is_drawing(block)
                && block.object_anchor().map(|a| a.with) == Some(AnchorWith::Next) => {}
            _ => return false,
        }
    }
    false
}

fn link_of(inline: &Inline) -> Option<&LinkTarget> {
    inline.marks().iter().find_map(|mark| match mark {
        Mark::Link(target) => Some(target),
        _ => None,
    })
}

fn change_of(inline: &Inline) -> Option<ChangeWrap<'_>> {
    inline.marks().iter().find_map(|mark| match mark {
        Mark::Insertion(change) => Some(ChangeWrap::Inserted(change)),
        Mark::Deletion(change) => Some(ChangeWrap::Deleted(change)),
        _ => None,
    })
}

/// text as run content: tabs, line breaks and special hyphens become
/// their own elements, everything else goes in `w:t` (or `w:delText`).
fn text_content(text: &str, deleted: bool) -> String {
    let element = if deleted { "w:delText" } else { "w:t" };
    let mut xml = String::new();
    let mut pending = String::new();
    let flush = |pending: &mut String, xml: &mut String| {
        if !pending.is_empty() {
            xml.push_str(&format!(
                "<{element} xml:space=\"preserve\">{}</{element}>",
                escape_text(pending)
            ));
            pending.clear();
        }
    };
    for c in text.chars() {
        let special = match c {
            '\t' => Some("<w:tab/>"),
            '\n' => Some("<w:br/>"),
            '\u{2011}' => Some("<w:noBreakHyphen/>"),
            '\u{AD}' => Some("<w:softHyphen/>"),
            _ => None,
        };
        match special {
            Some(element_xml) => {
                flush(&mut pending, &mut xml);
                xml.push_str(element_xml);
            }
            None => pending.push(c),
        }
    }
    flush(&mut pending, &mut xml);
    xml
}

/// bookmark ids must be numbers in word; anything else is turned into a
/// stable number from its text.
fn number_or_hash(id: &str) -> String {
    if id.parse::<u32>().is_ok() {
        return id.to_string();
    }
    let hash = id.bytes().fold(5381u32, |hash, byte| {
        hash.wrapping_mul(33) ^ u32::from(byte)
    });
    (hash % 1_000_000 + 1_000_000).to_string()
}

/// the text inside xml from another format, for conversions.
pub(crate) fn foreign_text(xml: &str) -> String {
    crate::xml::parse(xml)
        .map(|document| document.root.deep_text())
        .unwrap_or_default()
}
