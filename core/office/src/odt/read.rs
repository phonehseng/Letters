//! reading an odt package (opendocument 1.3 text) into the model.

use std::collections::{BTreeSet, HashMap};

use crate::error::OfficeError;
use crate::model::*;
use crate::package::{detect_package, FileKind, Package};
use crate::units::parse_length;
use crate::xml::{Element, Node, XmlDocument};

use super::props::{read_style, unquote, OdtStyle};

/// odt has no limit on repeated rows and cells; spreadsheets saved as
/// text can claim a million. anything past this is treated as one.
const MAX_REPEAT: usize = 1000;

/// reads a .odt (or .ott) file.
pub fn read_odt(bytes: &[u8]) -> Result<Document, OfficeError> {
    crate::package::detect_ole(bytes)?;
    let package = Package::read(bytes)?;
    if detect_package(&package)? != FileKind::Odt {
        return Err(OfficeError::NotAPackage(
            "it is a Word file, not an OpenDocument file".into(),
        ));
    }
    OdtReader::new(&package).read()
}

/// a tracked change from `text:tracked-changes`.
struct ChangeRegion {
    deletion: bool,
    change: Change,
    deleted: Vec<Inline>,
}

/// a master page: page setup plus the ids of its headers and footers.
#[derive(Clone)]
struct MasterPage {
    section: Section,
}

struct OdtReader<'p> {
    package: &'p Package,
    doc: Document,
    fonts: HashMap<String, String>,
    common: HashMap<String, OdtStyle>,
    /// automatic styles of content.xml and of styles.xml; headers and
    /// footers use the second
    content_auto: HashMap<String, OdtStyle>,
    styles_auto: HashMap<String, OdtStyle>,
    in_styles_part: bool,
    layouts: HashMap<String, Section>,
    masters: HashMap<String, MasterPage>,
    changes: HashMap<String, ChangeRegion>,
    open_insertions: Vec<Change>,
    open_comments: Vec<String>,
    /// list style -> instance of the last list using it
    last_instance: HashMap<String, String>,
    instances_by_xml_id: HashMap<String, String>,
    media_by_path: HashMap<String, String>,
    consumed: BTreeSet<String>,
    after_space: bool,
    counter: usize,
}

impl<'p> OdtReader<'p> {
    fn new(package: &'p Package) -> OdtReader<'p> {
        let mut doc = Document::default();
        doc.carry.origin = Some(SourceFormat::Odt);
        let mimetype = package
            .bytes("mimetype")
            .map(|bytes| String::from_utf8_lossy(bytes).trim().to_string())
            .unwrap_or_else(|| "application/vnd.oasis.opendocument.text".into());
        doc.carry.odt = Some(OdtCarry {
            mimetype,
            ..Default::default()
        });
        let consumed = [
            "mimetype",
            "content.xml",
            "styles.xml",
            "meta.xml",
            "meta-inf/manifest.xml",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        OdtReader {
            package,
            doc,
            fonts: HashMap::new(),
            common: HashMap::new(),
            content_auto: HashMap::new(),
            styles_auto: HashMap::new(),
            in_styles_part: false,
            layouts: HashMap::new(),
            masters: HashMap::new(),
            changes: HashMap::new(),
            open_insertions: Vec::new(),
            open_comments: Vec::new(),
            last_instance: HashMap::new(),
            instances_by_xml_id: HashMap::new(),
            media_by_path: HashMap::new(),
            consumed,
            after_space: true,
            counter: 0,
        }
    }

    fn odt_carry(&mut self) -> &mut OdtCarry {
        self.doc.carry.odt.get_or_insert_with(OdtCarry::default)
    }

    fn read(mut self) -> Result<Document, OfficeError> {
        let styles = self.package.xml("styles.xml")?;
        let content = self
            .package
            .xml("content.xml")?
            .ok_or_else(|| OfficeError::Damaged("its content.xml is missing".into()))?;
        if let Some(styles) = &styles {
            self.read_styles_part(styles);
        }
        self.read_content_part(&content)?;
        self.read_meta()?;
        self.read_manifest()?;
        self.carry_unread_parts();
        self.keep_only_referenced_automatic_styles();
        self.doc.normalize_sections();
        Ok(self.doc)
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}{}", self.counter)
    }

    // styles --------------------------------------------------------------

    fn read_font_faces(&mut self, element: &Element, xml: &XmlDocument) {
        for face in element.children_named("style:font-face") {
            if let Some(name) = face.attr("style:name") {
                let family = face
                    .attr("svg:font-family")
                    .map(unquote)
                    .unwrap_or_else(|| name.to_string());
                self.fonts.insert(name.to_string(), family);
            }
            let raw = xml.raw(face).to_string();
            if !self.odt_carry().font_faces.contains(&raw) {
                self.odt_carry().font_faces.push(raw);
            }
        }
    }

    fn read_styles_part(&mut self, xml: &XmlDocument) {
        self.doc.carry.part_roots.push(root_of("styles.xml", xml));
        for section in xml.root.elements() {
            match section.name.as_str() {
                "office:font-face-decls" => self.read_font_faces(section, xml),
                "office:styles" => self.read_common_styles(section, xml),
                "office:automatic-styles" => self.read_automatic_styles(section, xml, true),
                _ => {}
            }
        }
        // master pages need the page layouts and styles read above
        if let Some(masters) = xml.root.child("office:master-styles") {
            self.in_styles_part = true;
            for master in masters.children_named("style:master-page") {
                self.read_master_page(master, xml);
            }
            self.in_styles_part = false;
        }
    }

    fn read_common_styles(&mut self, element: &Element, xml: &XmlDocument) {
        for child in element.elements() {
            match child.name.as_str() {
                "style:default-style" if child.attr("style:family") == Some("paragraph") => {
                    let style = read_style(child, xml, &self.fonts);
                    self.doc.styles.defaults.paragraph = style.paragraph;
                    self.doc.styles.defaults.run = style.run;
                }
                "style:style"
                    if matches!(child.attr("style:family"), Some("paragraph") | Some("text")) =>
                {
                    let style = read_style(child, xml, &self.fonts);
                    self.doc.styles.styles.push(model_style(&style));
                    self.common.insert(style.name.clone(), style);
                }
                "text:list-style" => self.read_list_style(child),
                _ => self.doc.styles.preserved.push(xml.raw(child).to_string()),
            }
        }
    }

    fn read_automatic_styles(&mut self, element: &Element, xml: &XmlDocument, in_styles: bool) {
        for child in element.elements() {
            let raw = xml.raw(child).to_string();
            if in_styles {
                self.odt_carry().styles_automatic_styles.push(raw);
            } else {
                self.odt_carry().content_automatic_styles.push(raw);
            }
            match child.name.as_str() {
                "style:style" => {
                    let style = read_style(child, xml, &self.fonts);
                    let map = if in_styles {
                        &mut self.styles_auto
                    } else {
                        &mut self.content_auto
                    };
                    map.insert(style.name.clone(), style);
                }
                "text:list-style" => self.read_list_style(child),
                "style:page-layout" => {
                    if let Some(name) = child.attr("style:name") {
                        self.layouts.insert(name.to_string(), page_layout(child));
                    }
                }
                _ => {}
            }
        }
    }

    fn read_list_style(&mut self, element: &Element) {
        let Some(name) = element.attr("style:name") else {
            return;
        };
        if self.doc.numbering.definition(name).is_some() {
            return;
        }
        let mut levels = Vec::new();
        for level_element in element.elements() {
            if let Some(level) = list_level(level_element) {
                levels.push(level);
            }
        }
        levels.sort_by_key(|level| level.level);
        self.doc.numbering.definitions.push(ListDefinition {
            id: name.to_string(),
            name: element.attr("style:display-name").map(str::to_string),
            levels,
            preserved: Preserved::default(),
        });
    }

    fn read_master_page(&mut self, master: &Element, xml: &XmlDocument) {
        let Some(name) = master.attr("style:name") else {
            return;
        };
        let mut section = master
            .attr("style:page-layout-name")
            .and_then(|layout| self.layouts.get(layout))
            .cloned()
            .unwrap_or_default();
        section
            .preserved
            .attributes
            .push(Attribute::new("style:master-page", name));
        let slots = [
            ("style:header", HeaderFooterKind::Header, "default"),
            ("style:header-first", HeaderFooterKind::Header, "first"),
            ("loext:header-first", HeaderFooterKind::Header, "first"),
            ("style:header-left", HeaderFooterKind::Header, "even"),
            ("style:footer", HeaderFooterKind::Footer, "default"),
            ("style:footer-first", HeaderFooterKind::Footer, "first"),
            ("loext:footer-first", HeaderFooterKind::Footer, "first"),
            ("style:footer-left", HeaderFooterKind::Footer, "even"),
        ];
        for (element_name, kind, slot) in slots {
            let Some(part) = master.child(element_name) else {
                continue;
            };
            if part.attr("style:display") == Some("false") {
                continue;
            }
            let id = format!(
                "{name}-{}-{slot}",
                if kind == HeaderFooterKind::Header {
                    "header"
                } else {
                    "footer"
                }
            );
            self.after_space = true;
            let blocks = self.read_blocks(part, xml);
            self.doc.headers_footers.push(HeaderFooter {
                id: id.clone(),
                kind,
                blocks,
            });
            let refs = if kind == HeaderFooterKind::Header {
                &mut section.headers
            } else {
                &mut section.footers
            };
            match slot {
                "first" => {
                    refs.first = Some(id);
                    section.different_first_page = true;
                }
                "even" => {
                    refs.even = Some(id);
                    self.doc.settings.even_and_odd_headers = true;
                }
                _ => refs.default = Some(id),
            }
        }
        self.masters
            .insert(name.to_string(), MasterPage { section });
    }

    /// a style by name from the right automatic map, else the common ones.
    fn style(&self, name: &str) -> Option<(&OdtStyle, bool)> {
        let auto = if self.in_styles_part {
            &self.styles_auto
        } else {
            &self.content_auto
        };
        if let Some(style) = auto.get(name) {
            return Some((style, true));
        }
        self.common.get(name).map(|style| (style, false))
    }

    // content -------------------------------------------------------------

    fn read_content_part(&mut self, xml: &XmlDocument) -> Result<(), OfficeError> {
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
        self.read_body(text, xml);
        Ok(())
    }

    fn read_body(&mut self, text: &Element, xml: &XmlDocument) {
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
            if child.name == "text:tracked-changes" {
                continue;
            }
            // a new master page on the first block of a run starts a section
            if let Some(master) = self.master_page_of(child) {
                if let Some(page) = self.masters.get(&master).cloned() {
                    if blocks.is_empty() {
                        sections[0] = page.section;
                    } else {
                        blocks.push(Block::SectionBreak);
                        sections.push(page.section);
                    }
                }
            }
            self.read_block(child, xml, &mut blocks);
        }
        self.doc.blocks = blocks;
        self.doc.sections = sections;
    }

    fn master_page_of(&self, element: &Element) -> Option<String> {
        let name = element
            .attr("text:style-name")
            .or_else(|| element.attr("table:style-name"))?;
        self.style(name)
            .and_then(|(style, _)| style.master_page.clone())
    }

    fn read_blocks(&mut self, container: &Element, xml: &XmlDocument) -> Vec<Block> {
        let mut blocks = Vec::new();
        for child in container.elements() {
            self.read_block(child, xml, &mut blocks);
        }
        blocks
    }

    fn read_block(&mut self, element: &Element, xml: &XmlDocument, blocks: &mut Vec<Block>) {
        match element.name.as_str() {
            "text:p" | "text:h" => blocks.push(self.read_paragraph(element, xml, None)),
            "text:list" => self.read_list(element, xml, 0, None, blocks),
            "table:table" => blocks.push(Block::Table(self.read_table(element, xml))),
            // sections only group blocks here; their content is kept
            "text:section" => {
                for child in element.elements() {
                    self.read_block(child, xml, blocks);
                }
            }
            "text:soft-page-break" => {}
            _ => blocks.push(Block::Preserved {
                xml: xml.raw(element).to_string(),
            }),
        }
    }

    fn read_paragraph(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        list: Option<ListRef>,
    ) -> Block {
        let mut paragraph = Paragraph::default();
        let mut base_marks = Vec::new();
        let mut break_after = false;
        if let Some(name) = element.attr("text:style-name") {
            match self.style(name) {
                Some((style, true)) => {
                    paragraph.style_id = style.parent.clone();
                    paragraph.props = style.paragraph.clone();
                    paragraph.props.outline_level = None;
                    base_marks = style.run.to_marks();
                    break_after = style.break_after_page;
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
        if break_after && paragraph.content.is_empty() && paragraph.style_id.is_none() {
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

    fn read_list(
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
                        blocks.push(self.read_paragraph(child, xml, list));
                    }
                    _ => self.read_block(child, xml, blocks),
                }
            }
        }
    }

    /// which list instance a top-level list belongs to: a continued one,
    /// or a new one that restarts the numbering.
    fn list_instance(&mut self, element: &Element) -> String {
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

    // inline content --------------------------------------------------------

    fn marks_with_context(&self, base: &[Mark]) -> Vec<Mark> {
        let mut marks = base.to_vec();
        for change in &self.open_insertions {
            marks.push(Mark::Insertion(change.clone()));
        }
        for id in &self.open_comments {
            marks.push(Mark::Comment { id: id.clone() });
        }
        normalize_marks(&mut marks);
        marks
    }

    fn push_text(&mut self, raw: &str, marks: &[Mark], out: &mut Vec<Inline>) {
        let mut text = String::new();
        for c in raw.chars() {
            let c = if matches!(c, '\t' | '\n' | '\r') {
                ' '
            } else {
                c
            };
            if c == ' ' {
                if self.after_space {
                    continue;
                }
                self.after_space = true;
            } else {
                self.after_space = false;
            }
            text.push(c);
        }
        if !text.is_empty() {
            out.push(Inline::Text {
                text,
                marks: self.marks_with_context(marks),
            });
        }
    }

    fn read_inlines(
        &mut self,
        parent: &Element,
        xml: &XmlDocument,
        marks: &[Mark],
        out: &mut Vec<Inline>,
    ) {
        for node in &parent.children {
            match node {
                Node::Text(text) => self.push_text(text, marks, out),
                Node::Element(child) => self.read_inline(child, xml, marks, out),
            }
        }
    }

    fn read_inline(
        &mut self,
        child: &Element,
        xml: &XmlDocument,
        marks: &[Mark],
        out: &mut Vec<Inline>,
    ) {
        match child.name.as_str() {
            "text:span" => {
                let mut span_marks = marks.to_vec();
                if let Some(name) = child.attr("text:style-name") {
                    span_marks.extend(self.span_marks(name));
                }
                self.read_inlines(child, xml, &span_marks, out);
            }
            "text:a" | "draw:a" => {
                let href = child.attr("xlink:href").unwrap_or_default();
                let target = match href.strip_prefix('#') {
                    Some(anchor) => LinkTarget {
                        anchor: Some(anchor.to_string()),
                        ..Default::default()
                    },
                    None => LinkTarget {
                        href: Some(href.to_string()),
                        ..Default::default()
                    },
                };
                let mut link_marks = marks.to_vec();
                link_marks.push(Mark::Link(target));
                self.read_inlines(child, xml, &link_marks, out);
            }
            "text:s" => {
                let count = child
                    .attr("text:c")
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(1)
                    .min(1000);
                out.push(Inline::Text {
                    text: " ".repeat(count),
                    marks: self.marks_with_context(marks),
                });
                self.after_space = true;
            }
            "text:tab" => {
                out.push(Inline::Tab {
                    marks: self.marks_with_context(marks),
                });
                self.after_space = false;
            }
            "text:line-break" => {
                out.push(Inline::Break {
                    kind: BreakKind::Line,
                    marks: self.marks_with_context(marks),
                });
                self.after_space = true;
            }
            "text:soft-page-break" | "text:note-citation" => {}
            "text:note" => self.read_note(child, xml, marks, out),
            "text:bookmark" => {
                let name = child.attr("text:name").unwrap_or_default().to_string();
                out.push(Inline::BookmarkStart {
                    id: name.clone(),
                    name: name.clone(),
                });
                out.push(Inline::BookmarkEnd { id: name });
            }
            "text:bookmark-start" => {
                let name = child.attr("text:name").unwrap_or_default().to_string();
                out.push(Inline::BookmarkStart {
                    id: name.clone(),
                    name,
                });
            }
            "text:bookmark-end" => {
                out.push(Inline::BookmarkEnd {
                    id: child.attr("text:name").unwrap_or_default().to_string(),
                });
            }
            "office:annotation" => self.read_annotation(child, xml, out),
            "office:annotation-end" => {
                if let Some(name) = child.attr("office:name") {
                    self.open_comments.retain(|open| open != name);
                    out.push(Inline::CommentReference {
                        comment_id: name.to_string(),
                    });
                }
            }
            "text:change-start" => {
                if let Some(region) = child
                    .attr("text:change-id")
                    .and_then(|id| self.changes.get(id))
                {
                    if !region.deletion {
                        let change = region.change.clone();
                        self.open_insertions.push(change);
                    }
                }
            }
            "text:change-end" => {
                if let Some(id) = child.attr("text:change-id") {
                    self.open_insertions.retain(|change| change.id != id);
                }
            }
            "text:change" => {
                if let Some(region) = child
                    .attr("text:change-id")
                    .and_then(|id| self.changes.get(id))
                {
                    out.extend(region.deleted.iter().cloned());
                }
            }
            "text:page-number" | "text:page-count" | "text:date" => {
                let (kind, instruction) = match child.name.as_str() {
                    "text:page-number" => (FieldKind::PageNumber, "PAGE"),
                    "text:page-count" => (FieldKind::PageCount, "NUMPAGES"),
                    _ => (FieldKind::Date, "DATE"),
                };
                out.push(Inline::Field(Field {
                    kind,
                    instruction: instruction.into(),
                    result: child.deep_text(),
                    marks: self.marks_with_context(marks),
                }));
                self.after_space = false;
            }
            "draw:frame" => {
                let inline = self.read_frame(child, xml, marks);
                out.push(inline);
                self.after_space = false;
            }
            _ => {
                out.push(Inline::Preserved {
                    xml: xml.raw(child).to_string(),
                });
                self.after_space = false;
            }
        }
    }

    fn span_marks(&self, name: &str) -> Vec<Mark> {
        match self.style(name) {
            Some((style, true)) => {
                let mut run = style.run.clone();
                run.style_id = style.parent.clone();
                run.to_marks()
            }
            Some((style, false)) => vec![Mark::CharacterStyle {
                style_id: style.name.clone(),
            }],
            None => Vec::new(),
        }
    }

    fn read_note(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        marks: &[Mark],
        out: &mut Vec<Inline>,
    ) {
        let kind = if element.attr("text:note-class") == Some("endnote") {
            NoteKind::Endnote
        } else {
            NoteKind::Footnote
        };
        let id = match element.attr("text:id") {
            Some(id) => id.to_string(),
            None => self.next_id("note"),
        };
        let saved_space = self.after_space;
        let blocks = element
            .child("text:note-body")
            .map(|body| self.read_blocks(body, xml))
            .unwrap_or_default();
        self.after_space = saved_space;
        self.doc.notes.push(Note {
            id: id.clone(),
            kind,
            special: None,
            blocks,
        });
        out.push(Inline::NoteReference {
            note_id: id,
            marks: self.marks_with_context(marks),
        });
    }

    fn read_annotation(&mut self, element: &Element, xml: &XmlDocument, out: &mut Vec<Inline>) {
        let name = element.attr("office:name").map(str::to_string);
        let id = match &name {
            Some(name) => name.clone(),
            None => self.next_id("comment"),
        };
        let author = element
            .child("dc:creator")
            .map(Element::deep_text)
            .unwrap_or_default();
        let date = element.child("dc:date").map(Element::deep_text);
        let saved_space = self.after_space;
        let saved_comments = std::mem::take(&mut self.open_comments);
        let mut blocks = Vec::new();
        for child in element.elements() {
            if matches!(child.name.as_str(), "text:p" | "text:h" | "text:list") {
                self.read_block(child, xml, &mut blocks);
            }
        }
        self.open_comments = saved_comments;
        self.after_space = saved_space;
        self.doc.comments.push(Comment {
            id: id.clone(),
            author,
            initials: None,
            date,
            blocks,
        });
        match name {
            Some(_) => self.open_comments.push(id),
            None => out.push(Inline::CommentReference { comment_id: id }),
        }
    }

    fn read_tracked_changes(&mut self, element: &Element, xml: &XmlDocument) {
        for region in element.children_named("text:changed-region") {
            let Some(id) = region.attr("text:id").or_else(|| region.attr("xml:id")) else {
                continue;
            };
            let Some(kind) = region.elements().next() else {
                continue;
            };
            let info = kind.child("office:change-info");
            let change = Change {
                id: id.to_string(),
                author: info
                    .and_then(|i| i.child("dc:creator"))
                    .map(Element::deep_text)
                    .unwrap_or_default(),
                date: info
                    .and_then(|i| i.child("dc:date"))
                    .map(Element::deep_text),
            };
            let deletion = kind.name == "text:deletion";
            let mut deleted = Vec::new();
            if deletion {
                let marks = vec![Mark::Deletion(change.clone())];
                for paragraph in kind
                    .children_named("text:p")
                    .chain(kind.children_named("text:h"))
                {
                    self.after_space = true;
                    if !deleted.is_empty() {
                        deleted.push(Inline::Break {
                            kind: BreakKind::Line,
                            marks: marks.clone(),
                        });
                    }
                    self.read_inlines(paragraph, xml, &marks, &mut deleted);
                }
            }
            if deletion || kind.name == "text:insertion" {
                self.changes.insert(
                    id.to_string(),
                    ChangeRegion {
                        deletion,
                        change,
                        deleted,
                    },
                );
            }
        }
    }

    fn read_frame(&mut self, frame: &Element, xml: &XmlDocument, marks: &[Mark]) -> Inline {
        match self.read_picture(frame, xml) {
            Some(mut image) => {
                image.marks = self.marks_with_context(marks);
                Inline::Image(image)
            }
            None => Inline::Preserved {
                xml: xml.raw(frame).to_string(),
            },
        }
    }

    fn read_picture(&mut self, frame: &Element, xml: &XmlDocument) -> Option<Image> {
        let image_element = frame.child("draw:image")?;
        if frame
            .elements()
            .any(|child| child.name == "draw:text-box" || child.name == "draw:object")
        {
            return None;
        }
        let path = image_element
            .attr("xlink:href")?
            .trim_start_matches("./")
            .to_string();
        let media_id = self.media_for(&path)?;
        let width = frame.attr("svg:width").and_then(parse_length)?;
        let height = frame.attr("svg:height").and_then(parse_length)?;
        let anchor = frame.attr("text:anchor-type").unwrap_or("paragraph");
        let placement = if anchor == "as-char" {
            Placement::Inline
        } else {
            let wrap = frame
                .attr("draw:style-name")
                .and_then(|name| self.style(name))
                .and_then(|(style, _)| style.wrap)
                .unwrap_or(Wrap::Square);
            let relative = if anchor == "page" {
                ("page", "page")
            } else {
                ("column", "paragraph")
            };
            Placement::Floating {
                horizontal: FloatPosition {
                    relative_to: relative.0.into(),
                    offset: frame.attr("svg:x").and_then(parse_length).unwrap_or(0),
                    align: None,
                },
                vertical: FloatPosition {
                    relative_to: relative.1.into(),
                    offset: frame.attr("svg:y").and_then(parse_length).unwrap_or(0),
                    align: None,
                },
                wrap,
            }
        };
        let mut image = Image {
            media_id,
            width,
            height,
            description: frame.child("svg:desc").map(Element::deep_text),
            name: frame.attr("draw:name").map(str::to_string),
            placement,
            marks: Vec::new(),
            original: None,
        };
        image.original = Some(OriginalDrawing {
            xml: xml.raw(frame).to_string(),
            as_read: image.geometry(),
        });
        Some(image)
    }

    fn media_for(&mut self, path: &str) -> Option<String> {
        if let Some(id) = self.media_by_path.get(path) {
            return Some(id.clone());
        }
        let part = self.package.get(path)?;
        let file_name = path.rsplit('/').next().unwrap_or(path).to_string();
        let mut id = file_name.clone();
        let mut counter = 1;
        while self.doc.media_item(&id).is_some() {
            counter += 1;
            id = format!("{counter}-{file_name}");
        }
        self.doc.media.push(MediaItem {
            id: id.clone(),
            content_type: MediaItem::content_type_for(&file_name).to_string(),
            source_path: Some(part.name.clone()),
            bytes: part.bytes.clone(),
        });
        self.media_by_path.insert(path.to_string(), id.clone());
        self.consumed.insert(part.name.to_ascii_lowercase());
        Some(id)
    }

    // tables ----------------------------------------------------------------

    fn read_table(&mut self, element: &Element, xml: &XmlDocument) -> Table {
        let mut table = Table::default();
        if let Some((style, _)) = element
            .attr("table:style-name")
            .and_then(|name| self.style(name))
        {
            table.width = style.width;
            table.alignment = style.alignment;
        }
        for child in element.elements() {
            match child.name.as_str() {
                "table:table-column" => {
                    let width = child
                        .attr("table:style-name")
                        .and_then(|name| self.style(name))
                        .and_then(|(style, _)| style.width)
                        .unwrap_or(0);
                    let repeat = repeat(child, "table:number-columns-repeated");
                    table
                        .column_widths
                        .extend(std::iter::repeat_n(width, repeat));
                }
                "table:table-columns" | "table:table-header-columns" => {
                    for column in child.children_named("table:table-column") {
                        let width = column
                            .attr("table:style-name")
                            .and_then(|name| self.style(name))
                            .and_then(|(style, _)| style.width)
                            .unwrap_or(0);
                        table.column_widths.extend(std::iter::repeat_n(
                            width,
                            repeat(column, "table:number-columns-repeated"),
                        ));
                    }
                }
                "table:table-row" => self.read_rows(child, xml, false, &mut table),
                "table:table-header-rows" | "table:table-rows" => {
                    let header = child.name == "table:table-header-rows";
                    for row in child.children_named("table:table-row") {
                        self.read_rows(row, xml, header, &mut table);
                    }
                }
                _ => {}
            }
        }
        table
    }

    fn read_rows(&mut self, element: &Element, xml: &XmlDocument, header: bool, table: &mut Table) {
        let mut row = TableRow {
            header,
            ..Default::default()
        };
        for cell in element.elements() {
            if cell.name != "table:table-cell" {
                continue;
            }
            let mut model = TableCell {
                col_span: cell
                    .attr("table:number-columns-spanned")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .max(1),
                row_span: cell
                    .attr("table:number-rows-spanned")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .max(1),
                ..Default::default()
            };
            if let Some((style, _)) = cell
                .attr("table:style-name")
                .and_then(|name| self.style(name))
            {
                model.borders = style.cell_borders.clone();
                model.shading = style.background.clone();
                model.vertical_align = style.vertical_align;
            }
            model.blocks = self.read_blocks(cell, xml);
            let copies = repeat(cell, "table:number-columns-repeated");
            for _ in 0..copies {
                row.cells.push(model.clone());
            }
        }
        for _ in 0..repeat(element, "table:number-rows-repeated") {
            table.rows.push(row.clone());
        }
    }

    // meta and the rest -------------------------------------------------------

    fn read_meta(&mut self) -> Result<(), OfficeError> {
        let Some(xml) = self.package.xml("meta.xml")? else {
            return Ok(());
        };
        let Some(meta) = xml.root.child("office:meta") else {
            return Ok(());
        };
        let mut extras = Vec::new();
        for element in meta.elements() {
            let metadata = &mut self.doc.metadata;
            let slot = match element.name.as_str() {
                "dc:title" => &mut metadata.title,
                "dc:subject" => &mut metadata.subject,
                "dc:description" => &mut metadata.description,
                "meta:keyword" => &mut metadata.keywords,
                "meta:initial-creator" => &mut metadata.author,
                "dc:creator" => &mut metadata.last_modified_by,
                "meta:creation-date" => &mut metadata.created,
                "dc:date" => &mut metadata.modified,
                "dc:language" => &mut metadata.language,
                _ => {
                    extras.push(xml.raw(element).to_string());
                    continue;
                }
            };
            *slot = Some(element.deep_text());
        }
        self.odt_carry().meta_extras = extras;
        Ok(())
    }

    fn read_manifest(&mut self) -> Result<(), OfficeError> {
        let Some(xml) = self.package.xml("META-INF/manifest.xml")? else {
            return Ok(());
        };
        let entries: Vec<Attribute> = xml
            .root
            .children_named("manifest:file-entry")
            .filter_map(|entry| {
                Some(Attribute::new(
                    entry.attr("manifest:full-path")?,
                    entry.attr("manifest:media-type").unwrap_or(""),
                ))
            })
            .collect();
        self.odt_carry().manifest = entries;
        Ok(())
    }

    /// the writer makes automatic styles afresh for everything the model
    /// understands, so only the ones preserved xml still names are kept.
    /// that also stops styles from piling up save after save.
    fn keep_only_referenced_automatic_styles(&mut self) {
        let mut names = BTreeSet::new();
        let mut collect = |xml: &str| collect_style_names(xml, &mut names);
        visit_preserved(&self.doc, &mut collect);
        let referenced = |raw: &String| parse_name(raw).is_some_and(|name| names.contains(&name));
        if let Some(odt) = self.doc.carry.odt.as_mut() {
            odt.content_automatic_styles.retain(referenced);
            odt.styles_automatic_styles.retain(referenced);
        }
    }

    fn carry_unread_parts(&mut self) {
        let manifest = self
            .doc
            .carry
            .odt
            .as_ref()
            .map(|odt| odt.manifest.clone())
            .unwrap_or_default();
        for part in &self.package.parts {
            if self.consumed.contains(&part.name.to_ascii_lowercase()) {
                continue;
            }
            let content_type = manifest
                .iter()
                .find(|entry| entry.name == part.name)
                .map(|entry| entry.value.clone());
            self.doc.carry.parts.push(CarriedPart {
                name: part.name.clone(),
                content_type,
                bytes: part.bytes.clone(),
            });
        }
    }
}

/// every style name a piece of xml mentions (`...style-name="X"`).
fn collect_style_names(xml: &str, names: &mut BTreeSet<String>) {
    let mut rest = xml;
    while let Some(index) = rest.find("style-name=\"") {
        let after = &rest[index + "style-name=\"".len()..];
        let end = after.find('"').unwrap_or(after.len());
        names.insert(after[..end].to_string());
        rest = &after[end..];
    }
}

fn parse_name(raw: &str) -> Option<String> {
    crate::xml::parse(raw)
        .ok()?
        .root
        .attr("style:name")
        .map(str::to_string)
}

/// calls `visit` with every piece of preserved xml in the document.
fn visit_preserved(document: &Document, visit: &mut dyn FnMut(&str)) {
    fn blocks(list: &[Block], visit: &mut dyn FnMut(&str)) {
        for block in list {
            match block {
                Block::Preserved { xml } => visit(xml),
                Block::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            blocks(&cell.blocks, visit);
                        }
                    }
                }
                _ => {
                    for inline in block
                        .paragraph()
                        .map(|p| p.content.as_slice())
                        .unwrap_or(&[])
                    {
                        match inline {
                            Inline::Preserved { xml } | Inline::PreservedRun { xml, .. } => {
                                visit(xml)
                            }
                            Inline::Image(Image {
                                original: Some(original),
                                ..
                            }) => visit(&original.xml),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    blocks(&document.blocks, visit);
    for part in &document.headers_footers {
        blocks(&part.blocks, visit);
    }
    for note in &document.notes {
        blocks(&note.blocks, visit);
    }
    for comment in &document.comments {
        blocks(&comment.blocks, visit);
    }
    for preserved in &document.styles.preserved {
        visit(preserved);
    }
}

fn root_of(part: &str, xml: &XmlDocument) -> PartRoot {
    PartRoot {
        part: part.to_string(),
        attributes: xml
            .root_attributes
            .iter()
            .map(|(name, value)| Attribute::new(name, value))
            .collect(),
    }
}

fn repeat(element: &Element, attribute: &str) -> usize {
    let count = element
        .attr(attribute)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    if count > MAX_REPEAT {
        1
    } else {
        count.max(1)
    }
}

/// a common odt style as a model style. odt encodes spaces in style names
/// as "_20_"; the display name is the readable one.
fn model_style(style: &OdtStyle) -> Style {
    let kind = if style.family == "text" {
        StyleKind::Character
    } else {
        StyleKind::Paragraph
    };
    let name = style
        .display_name
        .clone()
        .unwrap_or_else(|| style.name.replace("_20_", " "));
    Style {
        id: style.name.clone(),
        is_default: style.name == "Standard",
        name,
        kind,
        based_on: style.parent.clone(),
        next: style.next.clone(),
        paragraph: style.paragraph.clone(),
        run: style.run.clone(),
        preserved: Preserved::default(),
    }
}

fn page_layout(element: &Element) -> Section {
    let mut section = Section::default();
    let Some(props) = element.child("style:page-layout-properties") else {
        return section;
    };
    let length = |name: &str| props.attr(name).and_then(parse_length);
    let page = &mut section.page;
    page.width = length("fo:page-width").unwrap_or(page.width);
    page.height = length("fo:page-height").unwrap_or(page.height);
    if props.attr("style:print-orientation") == Some("landscape") {
        page.orientation = Orientation::Landscape;
    }
    page.margins.top = length("fo:margin-top").unwrap_or(1440);
    page.margins.bottom = length("fo:margin-bottom").unwrap_or(1440);
    page.margins.left = length("fo:margin-left").unwrap_or(1440);
    page.margins.right = length("fo:margin-right").unwrap_or(1440);
    // in odt the header sits inside the top margin; word measures the body
    // from the top of the page and the header from the edge
    for (style_name, is_header) in [("style:header-style", true), ("style:footer-style", false)] {
        let Some(hf) = element
            .child(style_name)
            .and_then(|s| s.child("style:header-footer-properties"))
        else {
            continue;
        };
        let height = hf.attr("fo:min-height").and_then(parse_length).unwrap_or(0);
        let gap = hf
            .attr(if is_header {
                "fo:margin-bottom"
            } else {
                "fo:margin-top"
            })
            .and_then(parse_length)
            .unwrap_or(0);
        if is_header {
            page.margins.header = page.margins.top;
            page.margins.top += height + gap;
        } else {
            page.margins.footer = page.margins.bottom;
            page.margins.bottom += height + gap;
        }
    }
    if let Some(columns) = props.child("style:columns") {
        section.columns.count = columns
            .attr("fo:column-count")
            .and_then(|c| c.parse().ok())
            .unwrap_or(1)
            .max(1);
        section.columns.space = columns
            .attr("fo:column-gap")
            .and_then(parse_length)
            .unwrap_or(720);
    }
    section
}

/// one `text:list-level-style-*` as a list level.
fn list_level(element: &Element) -> Option<ListLevel> {
    let level = element.attr("text:level")?.parse::<u8>().ok()?.clamp(1, 9) - 1;
    let (format, text) = match element.name.as_str() {
        "text:list-level-style-bullet" => (
            NumberFormat::Bullet,
            element
                .attr("text:bullet-char")
                .unwrap_or("\u{2022}")
                .to_string(),
        ),
        "text:list-level-style-number" => {
            let format = match element.attr("style:num-format").unwrap_or("1") {
                "1" => NumberFormat::Decimal,
                "a" => NumberFormat::LowerLetter,
                "A" => NumberFormat::UpperLetter,
                "i" => NumberFormat::LowerRoman,
                "I" => NumberFormat::UpperRoman,
                "" => NumberFormat::None,
                other => NumberFormat::Other(other.to_string()),
            };
            let shown = element
                .attr("text:display-levels")
                .and_then(|d| d.parse::<u8>().ok())
                .unwrap_or(1)
                .clamp(1, level + 1);
            let numbers: Vec<String> = ((level + 2 - shown)..=(level + 1))
                .map(|l| format!("%{l}"))
                .collect();
            let text = format!(
                "{}{}{}",
                element.attr("style:num-prefix").unwrap_or(""),
                numbers.join("."),
                element.attr("style:num-suffix").unwrap_or("")
            );
            (format, text)
        }
        _ => (NumberFormat::None, String::new()),
    };
    let mut paragraph = ParagraphProps::default();
    if let Some(alignment) = element
        .child("style:list-level-properties")
        .and_then(|props| props.child("style:list-level-label-alignment"))
    {
        let left = alignment.attr("fo:margin-left").and_then(parse_length);
        let indent = alignment
            .attr("fo:text-indent")
            .and_then(parse_length)
            .unwrap_or(0);
        paragraph.indent.left = left;
        if indent < 0 {
            paragraph.indent.hanging = Some(-indent);
        } else if indent > 0 {
            paragraph.indent.first_line = Some(indent);
        }
    }
    Some(ListLevel {
        level,
        format,
        text,
        start: element
            .attr("text:start-value")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1),
        alignment: None,
        paragraph,
        run: RunProps::default(),
        preserved: Preserved::default(),
    })
}
