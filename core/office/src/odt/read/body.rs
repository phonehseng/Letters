//! the text body: blocks, paragraphs and lists.

use super::*;

impl OdtReader<'_> {
    // content -------------------------------------------------------------

    pub(super) fn read_content_part(&mut self, xml: &XmlDocument) -> Result<(), OfficeError> {
        if xml.root.name != "office:document-content" {
            return Err(OfficeError::bad_xml(
                "content.xml",
                "it is not an OpenDocument content part",
            ));
        }
        self.doc.carry.part_roots.push(root_of("content.xml", xml));
        for section in xml.root.elements() {
            match section.name.as_str() {
                "office:font-face-decls" => self.read_font_faces(section, xml),
                "office:automatic-styles" => self.read_automatic_styles(section, xml, false),
                _ => {}
            }
        }
        let text = xml
            .root
            .child("office:body")
            .and_then(|body| body.child("office:text"))
            .ok_or_else(|| OfficeError::bad_xml("content.xml", "it has no text body"))?;
        if let Some(changes) = text.child("text:tracked-changes") {
            self.read_tracked_changes(changes, xml);
        }
        self.find_chains(text);
        self.read_body(text, xml);
        Ok(())
    }

    pub(super) fn read_body(&mut self, text: &Element, xml: &XmlDocument) {
        // the document starts on the standard master page unless its
        // first block names another
        let standard = self
            .masters
            .get("Standard")
            .or_else(|| self.masters.values().next());
        let mut sections: Vec<Section> = vec![standard
            .map(|master| master.section.clone())
            .unwrap_or_default()];
        let mut blocks = Vec::new();
        let mut children = Vec::new();
        for child in text.elements() {
            if matches!(
                child.name.as_str(),
                "text:sequence-decls"
                    | "text:variable-decls"
                    | "text:user-field-decls"
                    | "text:dde-connection-decls"
                    | "office:forms"
            ) {
                let raw = xml.raw(child).to_string();
                self.odt_carry().body_prelude.push(raw);
                continue;
            }
            if child.name != "text:tracked-changes" {
                children.push(child);
            }
        }
        self.read_body_children(&children, xml, &mut blocks, &mut sections);
        self.doc.blocks = blocks;
        self.doc.sections = sections;
    }

    pub(super) fn master_page_of(&self, element: &Element) -> Option<String> {
        let name = element
            .attr("text:style-name")
            .or_else(|| element.attr("table:style-name"))?;
        self.style(name)
            .and_then(|(style, _)| style.master_page.clone())
    }

    pub(super) fn read_blocks(&mut self, container: &Element, xml: &XmlDocument) -> Vec<Block> {
        let mut blocks = Vec::new();
        for child in container.elements() {
            self.read_block(child, xml, &mut blocks);
        }
        blocks
    }

    pub(super) fn read_block(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        blocks: &mut Vec<Block>,
    ) {
        match element.name.as_str() {
            "text:p" | "text:h" => self.read_paragraph_with_drawings(element, xml, None, blocks),
            "text:list" => self.read_list(element, xml, 0, None, blocks),
            "table:table" => blocks.push(Block::Table(self.read_table(element, xml))),
            // sections only group blocks here; their content is kept
            "text:section" => {
                for child in element.elements() {
                    self.read_block(child, xml, blocks);
                }
            }
            "text:soft-page-break" => {}
            // a drawing anchored to the page sits outside any paragraph
            _ if drawing::is_drawing_element(element) => match self.read_drawing(element, xml) {
                Some(mut block) => {
                    if let Some(slot) = block.object_anchor_mut() {
                        *slot = Some(ObjectAnchor {
                            with: AnchorWith::Page,
                            page_number: element
                                .attr("text:anchor-page-number")
                                .and_then(|n| n.parse().ok()),
                            ..Default::default()
                        });
                    }
                    blocks.push(block);
                }
                None => blocks.push(Block::Preserved {
                    xml: xml.raw(element).to_string(),
                }),
            },
            _ => blocks.push(Block::Preserved {
                xml: xml.raw(element).to_string(),
            }),
        }
    }

    /// a paragraph and the drawings anchored in it, as blocks.
    pub(super) fn read_paragraph_with_drawings(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        list: Option<ListRef>,
        blocks: &mut Vec<Block>,
    ) {
        let first_drawing = self.pending_objects.len();
        let mut block = self.read_paragraph(element, xml, list);
        let Some(paragraph) = block.paragraph_mut() else {
            blocks.push(block);
            return;
        };
        let content = std::mem::take(&mut paragraph.content);
        let (content, drawings) = take_drawings(content, &mut self.pending_objects, first_drawing);
        paragraph.content = content;
        let own = paragraph.clone();
        place_drawings(block, &own, drawings, blocks);
    }

    pub(super) fn read_paragraph(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        list: Option<ListRef>,
    ) -> Block {
        let mut paragraph = Paragraph::default();
        let mut base_marks = Vec::new();
        let mut break_after = false;
        let mut column_before = false;
        if let Some(name) = element.attr("text:style-name") {
            match self.style(name) {
                Some((style, true)) => {
                    paragraph.style_id = style.parent.clone();
                    paragraph.props = style.paragraph.clone();
                    paragraph.props.outline_level = None;
                    base_marks = style.run.to_marks();
                    break_after = style.break_after_page;
                    column_before = style.break_before_column;
                }
                Some((style, false)) => paragraph.style_id = Some(style.name.clone()),
                None => paragraph.style_id = Some(name.to_string()),
            }
        }
        if list.is_some() {
            paragraph.props.list = list;
        }
        self.after_space = true;
        let mut content = Vec::new();
        self.read_inlines(element, xml, &base_marks, &mut content);
        paragraph.content = merge_adjacent_text(content);
        if column_before {
            let column = Inline::Break {
                kind: BreakKind::Column,
                marks: Vec::new(),
            };
            paragraph.content.insert(0, column);
        }
        // an empty paragraph there only to break the page is a page break:
        // letters writes one for each (odt/write/body.rs), and its automatic
        // style always names a parent ("Normal"), so the style is no test
        // (every page break was lost on reopening a saved .odt)
        if break_after && paragraph.content.is_empty() && element.name == "text:p" {
            return Block::PageBreak;
        }
        if element.name == "text:h" {
            let level = element
                .attr("text:outline-level")
                .and_then(|level| level.parse::<u8>().ok())
                .unwrap_or(1)
                .clamp(1, 6);
            if crate::styles::heading_level(&self.doc, &paragraph) != Some(level) {
                paragraph.props.outline_level = Some(level - 1);
            }
            return Block::Heading(Heading { level, paragraph });
        }
        match crate::styles::heading_level(&self.doc, &paragraph) {
            Some(level) => Block::Heading(Heading { level, paragraph }),
            None => Block::Paragraph(paragraph),
        }
    }

    pub(super) fn read_list(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        level: u8,
        instance: Option<String>,
        blocks: &mut Vec<Block>,
    ) {
        let instance = match instance {
            Some(instance) => instance,
            None => self.list_instance(element),
        };
        for item in element.elements() {
            let numbered = item.name == "text:list-item";
            let mut first = true;
            for child in item.elements() {
                match child.name.as_str() {
                    "text:list" => self.read_list(
                        child,
                        xml,
                        (level + 1).min(8),
                        Some(instance.clone()),
                        blocks,
                    ),
                    "text:p" | "text:h" => {
                        let list = (numbered && first).then(|| ListRef {
                            list_id: instance.clone(),
                            level,
                        });
                        first = false;
                        self.read_paragraph_with_drawings(child, xml, list, blocks);
                    }
                    _ => self.read_block(child, xml, blocks),
                }
            }
        }
    }

    /// which list instance a top-level list belongs to: a continued one,
    /// or a new one that restarts the numbering.
    pub(super) fn list_instance(&mut self, element: &Element) -> String {
        let style = element.attr("text:style-name").unwrap_or("").to_string();
        let definition = if self.doc.numbering.definition(&style).is_some() {
            style.clone()
        } else {
            let id = if style.is_empty() {
                "LettersBullets".to_string()
            } else {
                style.clone()
            };
            if self.doc.numbering.definition(&id).is_none() {
                self.doc
                    .numbering
                    .definitions
                    .push(ListDefinition::standard(&id, false));
            }
            id
        };
        let continued = element
            .attr("text:continue-list")
            .and_then(|id| self.instances_by_xml_id.get(id).cloned())
            .or_else(|| {
                (element.attr("text:continue-numbering") == Some("true"))
                    .then(|| self.last_instance.get(&definition).cloned())
                    .flatten()
            });
        let instance = continued.unwrap_or_else(|| self.doc.numbering.restart(&definition));
        if let Some(id) = element.attr("xml:id") {
            self.instances_by_xml_id
                .insert(id.to_string(), instance.clone());
        }
        self.last_instance.insert(definition, instance.clone());
        instance
    }
}
