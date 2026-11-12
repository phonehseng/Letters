//! the text body: blocks, paragraphs and their automatic styles, and lists.

use super::*;

impl OdtWriter<'_> {
    // the body --------------------------------------------------------------

    pub(super) fn body_xml(&mut self) -> String {
        let doc = self.doc;
        let mut out = String::new();
        let mut section = 0;
        let mut pending_master: Option<String> = Some(self.master_name(0));
        let mut sections = SectionState::new();
        let blocks = &doc.blocks;
        let mut index = 0;
        while index < blocks.len() {
            if let Block::SectionBreak = blocks[index] {
                section += 1;
                if let Some(master) = self.break_section(section, &mut sections, &mut out) {
                    pending_master = Some(master);
                }
                index += 1;
                continue;
            }
            if is_drawing(&blocks[index]) {
                let page_anchored = blocks[index]
                    .object_anchor()
                    .is_some_and(|a| a.with == AnchorWith::Page);
                // a drawing on the page has no paragraph to start the section
                let master = if page_anchored {
                    None
                } else {
                    pending_master.take()
                };
                index += self.drawings_xml(blocks, index, master.as_deref(), &mut out);
                continue;
            }
            let master = pending_master.take();
            if is_list_paragraph(&blocks[index]) {
                let end = index
                    + blocks[index..]
                        .iter()
                        .take_while(|b| is_list_paragraph(b))
                        .count();
                out.push_str(&self.list_run(&blocks[index..end], master.as_deref()));
                index = end;
                continue;
            }
            out.push_str(&self.block_xml(&blocks[index], master.as_deref()));
            index += 1;
        }
        self.close_column_section(&mut sections, &mut out);
        out
    }

    pub(super) fn master_name(&self, section: usize) -> String {
        let carried = self
            .doc
            .sections
            .get(section)
            .filter(|_| self.native)
            .and_then(|s| {
                s.preserved
                    .attributes
                    .iter()
                    .find(|a| a.name == "style:master-page")
            })
            .map(|a| a.value.clone());
        carried.unwrap_or_else(|| {
            if section == 0 {
                "Standard".into()
            } else {
                format!("LMaster{section}")
            }
        })
    }

    pub(super) fn blocks_xml(&mut self, blocks: &[Block]) -> String {
        let mut out = String::new();
        let mut index = 0;
        while index < blocks.len() {
            if is_drawing(&blocks[index]) {
                index += self.drawings_xml(blocks, index, None, &mut out);
            } else if is_list_paragraph(&blocks[index]) {
                let end = index
                    + blocks[index..]
                        .iter()
                        .take_while(|b| is_list_paragraph(b))
                        .count();
                out.push_str(&self.list_run(&blocks[index..end], None));
                index = end;
            } else {
                out.push_str(&self.block_xml(&blocks[index], None));
                index += 1;
            }
        }
        out
    }

    pub(super) fn block_xml(&mut self, block: &Block, master: Option<&str>) -> String {
        match block {
            Block::Paragraph(paragraph) => self.paragraph_xml(paragraph, None, master, false),
            Block::Heading(heading) => {
                self.paragraph_xml(&heading.paragraph, Some(heading.level), master, false)
            }
            Block::Table(table) => self.table_xml(table, master),
            Block::PageBreak => self.paragraph_xml(&Paragraph::default(), None, master, true),
            Block::SectionBreak => String::new(),
            // drawings are written by `drawings_xml`, which the block loops call
            Block::TextBox(_) | Block::Shape(_) | Block::Group(_) => String::new(),
            Block::Preserved { xml } => {
                if self.native {
                    xml.clone()
                } else {
                    let text = crate::docx::foreign_text(xml);
                    self.paragraph_xml(&Paragraph::with_text(&text), None, master, false)
                }
            }
        }
    }

    pub(super) fn paragraph_xml(
        &mut self,
        paragraph: &Paragraph,
        heading: Option<u8>,
        master: Option<&str>,
        break_after: bool,
    ) -> String {
        // odt has no column break inside a line; one at the start of a
        // paragraph is the paragraph's own "starts a new column"
        let column_before = matches!(
            paragraph.content.first(),
            Some(Inline::Break {
                kind: BreakKind::Column,
                ..
            })
        );
        let breaks = Breaks {
            page_after: break_after,
            column_before,
        };
        let style_name = self.paragraph_style(paragraph, master, breaks);
        let element = if heading.is_some() {
            "text:h"
        } else {
            "text:p"
        };
        let mut attributes = String::new();
        if let Some(name) = &style_name {
            attributes.push_str(&format!(" text:style-name=\"{}\"", escape_attribute(name)));
        }
        if let Some(level) = heading {
            attributes.push_str(&format!(" text:outline-level=\"{level}\""));
        }
        self.after_space = true;
        let content = if column_before {
            for (at, _) in &mut self.waiting_drawings {
                *at = at.saturating_sub(1);
            }
            self.content_with_drawings(&paragraph.content[1..])
        } else {
            self.content_with_drawings(&paragraph.content)
        };
        format!("<{element}{attributes}>{content}</{element}>")
    }

    /// the style name a paragraph is written with: its named style, or an
    /// automatic style when it has its own formatting or starts a page.
    pub(super) fn paragraph_style(
        &mut self,
        paragraph: &Paragraph,
        master: Option<&str>,
        breaks: Breaks,
    ) -> Option<String> {
        let style = paragraph
            .style_id
            .clone()
            // a word paragraph without a style uses the default one; odt
            // has no such rule, so the style is named
            .or_else(|| {
                (!self.native)
                    .then(|| crate::styles::default_paragraph_style(self.doc).map(|s| s.id.clone()))
                    .flatten()
            })
            .map(|id| safe_name(&id));
        let mut props = paragraph.props.clone();
        props.list = None;
        if props.is_empty() && master.is_none() && !breaks.page_after && !breaks.column_before {
            return style;
        }
        let mut attributes = String::new();
        if let Some(style) = &style {
            attributes.push_str(&format!(
                " style:parent-style-name=\"{}\"",
                escape_attribute(style)
            ));
        }
        if let Some(master) = master {
            attributes.push_str(&format!(
                " style:master-page-name=\"{}\"",
                escape_attribute(master)
            ));
        }
        let body = paragraph_properties(&props, breaks, self.native);
        Some(self.automatic_style("LP", "paragraph", &attributes, &body))
    }

    /// consecutive list paragraphs as nested `text:list` elements.
    pub(super) fn list_run(&mut self, blocks: &[Block], master: Option<&str>) -> String {
        let mut out = String::new();
        // one entry per open text:list: whether it has an open item
        let mut stack: Vec<bool> = Vec::new();
        let mut current: Option<String> = None;
        let mut master = master.map(str::to_string);
        for block in blocks {
            let Some(paragraph) = block.paragraph() else {
                continue;
            };
            let Some(list) = paragraph.props.list.clone() else {
                continue;
            };
            if current.as_deref() != Some(list.list_id.as_str()) {
                close_lists(&mut stack, &mut out, 0);
                out.push_str(&self.list_open(&list.list_id));
                stack.push(false);
                current = Some(list.list_id.clone());
            }
            let target = usize::from(list.level) + 1;
            close_lists(&mut stack, &mut out, target);
            while stack.len() < target {
                if let Some(open) = stack.last_mut() {
                    if !*open {
                        out.push_str("<text:list-item>");
                        *open = true;
                    }
                }
                out.push_str("<text:list>");
                stack.push(false);
            }
            if let Some(open) = stack.last_mut() {
                if *open {
                    out.push_str("</text:list-item>");
                }
                out.push_str("<text:list-item>");
                *open = true;
            }
            let heading = match block {
                Block::Heading(heading) => Some(heading.level),
                _ => None,
            };
            out.push_str(&self.paragraph_xml(paragraph, heading, master.take().as_deref(), false));
        }
        close_lists(&mut stack, &mut out, 0);
        out
    }

    /// the opening tag of a top-level list. word's rule that lists of the
    /// same definition count on unless restarted maps onto odt's
    /// "continue numbering".
    pub(super) fn list_open(&mut self, instance_id: &str) -> String {
        let numbering = &self.doc.numbering;
        let instance = numbering.instance(instance_id);
        let definition = instance
            .map(|i| i.definition_id.clone())
            .unwrap_or_default();
        let restarts = instance.is_some_and(|i| {
            i.overrides
                .iter()
                .any(|o| o.level == 0 && o.start.is_some())
        });
        let seen = !self.seen_instances.insert(instance_id.to_string());
        let earlier_list = !self.seen_definitions.insert(definition.clone());
        let continues = seen || (earlier_list && !restarts);
        let mut tag = String::from("<text:list");
        if !definition.is_empty() {
            tag.push_str(&format!(
                " text:style-name=\"{}\"",
                escape_attribute(&safe_name(&definition))
            ));
        }
        if continues {
            tag.push_str(" text:continue-numbering=\"true\"");
        }
        tag.push('>');
        tag
    }
}

pub(super) fn is_list_paragraph(block: &Block) -> bool {
    block
        .paragraph()
        .and_then(|p| p.props.list.as_ref())
        .is_some_and(|list| list.list_id != "0")
}

pub(super) fn close_lists(stack: &mut Vec<bool>, out: &mut String, keep: usize) {
    while stack.len() > keep {
        if stack.pop() == Some(true) {
            out.push_str("</text:list-item>");
        }
        out.push_str("</text:list>");
    }
}
