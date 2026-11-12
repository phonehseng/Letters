//! writing the model as an odt package.
//!
//! formatting goes into styles, as odt wants: named styles for the
//! model's styles, and automatic styles (made here, named "LP1", "LT1"...
//! so they cannot clash with the ones a file brought along) for direct
//! formatting. automatic styles read from an odt are written back as
//! they were, because preserved xml may use them.

use std::collections::{BTreeSet, HashMap};

use crate::error::OfficeError;
use crate::model::*;
use crate::package::{write_zip, Part};
use crate::tables::{grid, Slot};
use crate::units::twips_to_inches;
use crate::xml::{escape_attribute, escape_text, namespace_uri, parse, XmlWriter};

use super::props::{cell_properties, paragraph_properties, text_properties};

const MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

/// the prefixes every written part declares.
const PREFIXES: &[&str] = &[
    "office", "style", "text", "table", "draw", "fo", "xlink", "dc", "meta", "number", "svg",
    "loext",
];

/// writes a document as an .odt file.
pub fn write_odt(document: &Document) -> Result<Vec<u8>, OfficeError> {
    let mut document = document.clone();
    document.normalize_sections();
    let mut writer = OdtWriter::new(&document);
    writer.write()
}

/// which part an automatic style goes into.
#[derive(Clone, Copy, PartialEq)]
enum Pool {
    Content,
    Styles,
}

struct OdtWriter<'d> {
    doc: &'d Document,
    native: bool,
    pool: Pool,
    content_styles: Vec<(String, String)>,
    styles_styles: Vec<(String, String)>,
    /// automatic styles already made, by what they contain
    style_names: HashMap<String, String>,
    taken_names: BTreeSet<String>,
    counter: usize,
    fonts: Vec<String>,
    regions: Vec<String>,
    used_change_ids: BTreeSet<String>,
    comment_remaining: HashMap<String, usize>,
    comment_open: BTreeSet<String>,
    comment_ranged: BTreeSet<String>,
    bookmark_names: HashMap<String, String>,
    media_paths: HashMap<String, String>,
    note_numbers: (usize, usize),
    tables: usize,
    after_space: bool,
    seen_instances: BTreeSet<String>,
    seen_definitions: BTreeSet<String>,
}

impl<'d> OdtWriter<'d> {
    fn new(doc: &'d Document) -> OdtWriter<'d> {
        let native = doc.carry.origin == Some(SourceFormat::Odt);
        let mut writer = OdtWriter {
            doc,
            native,
            pool: Pool::Content,
            content_styles: Vec::new(),
            styles_styles: Vec::new(),
            style_names: HashMap::new(),
            taken_names: BTreeSet::new(),
            counter: 0,
            fonts: Vec::new(),
            regions: Vec::new(),
            used_change_ids: BTreeSet::new(),
            comment_remaining: HashMap::new(),
            comment_open: BTreeSet::new(),
            comment_ranged: BTreeSet::new(),
            bookmark_names: HashMap::new(),
            media_paths: HashMap::new(),
            note_numbers: (0, 0),
            tables: 0,
            after_space: true,
            seen_instances: BTreeSet::new(),
            seen_definitions: BTreeSet::new(),
        };
        if let Some(odt) = writer.odt_carry() {
            for raw in odt
                .content_automatic_styles
                .iter()
                .chain(&odt.styles_automatic_styles)
            {
                if let Ok(parsed) = parse(raw) {
                    if let Some(name) = parsed.root.attr("style:name") {
                        writer.taken_names.insert(name.to_string());
                    }
                }
            }
        }
        for item in &doc.media {
            let path = item
                .source_path
                .clone()
                .filter(|path| native && path.starts_with("Pictures/"))
                .unwrap_or_else(|| format!("Pictures/{}", safe_name(&item.id)));
            writer.media_paths.insert(item.id.clone(), path);
        }
        writer.count_comment_marks();
        writer
    }

    /// the name an annotation is written under. ids from odt are kept;
    /// word's numbers are not valid names, so they get a prefix.
    fn comment_name(&self, id: &str) -> String {
        if self.native {
            id.to_string()
        } else {
            format!("c{id}")
        }
    }

    fn odt_carry(&self) -> Option<&'d OdtCarry> {
        if self.native {
            self.doc.carry.odt.as_ref()
        } else {
            None
        }
    }

    fn count_comment_marks(&mut self) {
        let mut visit = |inline: &Inline| {
            for mark in inline.marks() {
                if let Mark::Comment { id } = mark {
                    *self.comment_remaining.entry(id.clone()).or_default() += 1;
                    self.comment_ranged.insert(id.clone());
                }
            }
        };
        crate::docx::for_each_inline(&self.doc.blocks, &mut visit);
    }

    /// a fresh automatic style name that no carried style uses.
    fn fresh_name(&mut self, prefix: &str) -> String {
        loop {
            self.counter += 1;
            let name = format!("{prefix}{}", self.counter);
            if !self.taken_names.contains(&name) {
                return name;
            }
        }
    }

    /// adds an automatic style (body is the xml without the name) or finds
    /// an identical one, and returns its name.
    fn automatic_style(
        &mut self,
        prefix: &str,
        family: &str,
        attributes: &str,
        body: &str,
    ) -> String {
        let key = format!(
            "{}|{family}|{attributes}|{body}",
            self.pool == Pool::Content
        );
        if let Some(name) = self.style_names.get(&key) {
            return name.clone();
        }
        let name = self.fresh_name(prefix);
        self.style_names.insert(key.clone(), name.clone());
        let xml = format!(
            "<style:style style:name=\"{name}\" style:family=\"{family}\"{attributes}>{body}</style:style>"
        );
        let pool = if self.pool == Pool::Content {
            &mut self.content_styles
        } else {
            &mut self.styles_styles
        };
        pool.push((key, xml));
        name
    }

    fn write(&mut self) -> Result<Vec<u8>, OfficeError> {
        let body = self.body_xml();
        let content = self.content_xml(&body);
        let styles = self.styles_xml();
        let meta = self.meta_xml();
        let mut parts = vec![
            Part {
                name: "mimetype".into(),
                bytes: self.mimetype().into_bytes(),
            },
            Part {
                name: "content.xml".into(),
                bytes: content.into_bytes(),
            },
            Part {
                name: "styles.xml".into(),
                bytes: styles.into_bytes(),
            },
            Part {
                name: "meta.xml".into(),
                bytes: meta.into_bytes(),
            },
        ];
        for item in &self.doc.media {
            parts.push(Part {
                name: self.media_paths[&item.id].clone(),
                bytes: item.bytes.clone(),
            });
        }
        if self.native {
            let written: BTreeSet<String> =
                parts.iter().map(|p| p.name.to_ascii_lowercase()).collect();
            for carried in &self.doc.carry.parts {
                if !written.contains(&carried.name.to_ascii_lowercase()) {
                    parts.push(Part {
                        name: carried.name.clone(),
                        bytes: carried.bytes.clone(),
                    });
                }
            }
        }
        let manifest = self.manifest_xml(&parts);
        parts.push(Part {
            name: "META-INF/manifest.xml".into(),
            bytes: manifest.into_bytes(),
        });
        write_zip(&parts, &["mimetype"])
    }

    fn mimetype(&self) -> String {
        self.odt_carry()
            .map(|odt| odt.mimetype.clone())
            .unwrap_or_else(|| MIMETYPE.into())
    }

    fn root(&self, part: &str, name: &str) -> String {
        let mut attributes: Vec<(String, String)> = Vec::new();
        if self.native {
            if let Some(carried) = self.doc.carry.root_attributes(part) {
                attributes.extend(carried.iter().map(|a| (a.name.clone(), a.value.clone())));
            }
        }
        for prefix in PREFIXES {
            let key = format!("xmlns:{prefix}");
            if !attributes.iter().any(|(existing, _)| *existing == key) {
                attributes.push((key, namespace_uri(prefix).to_string()));
            }
        }
        if !attributes.iter().any(|(key, _)| key == "office:version") {
            attributes.push(("office:version".into(), "1.3".into()));
        }
        let mut xml = XmlWriter::new();
        xml.open_owned(name, &attributes);
        xml.finish()
    }

    fn font_faces(&self) -> String {
        let mut xml = String::from("<office:font-face-decls>");
        let mut declared = BTreeSet::new();
        if let Some(odt) = self.odt_carry() {
            for raw in &odt.font_faces {
                if let Ok(parsed) = parse(raw) {
                    if let Some(name) = parsed.root.attr("style:name") {
                        declared.insert(name.to_string());
                    }
                }
                xml.push_str(raw);
            }
        }
        for font in &self.fonts {
            if declared.insert(font.clone()) {
                xml.push_str(&format!(
                    "<style:font-face style:name=\"{0}\" svg:font-family=\"'{0}'\"/>",
                    escape_attribute(font)
                ));
            }
        }
        xml.push_str("</office:font-face-decls>");
        xml
    }

    fn content_xml(&mut self, body: &str) -> String {
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root("content.xml", "office:document-content"));
        xml.raw(&self.font_faces());
        xml.raw("<office:automatic-styles>");
        if let Some(odt) = self.odt_carry() {
            for raw in odt
                .content_automatic_styles
                .iter()
                .filter(|raw| keep_carried_style(raw))
            {
                xml.raw(raw);
            }
        }
        for (_, style) in &self.content_styles {
            xml.raw(style);
        }
        xml.raw("</office:automatic-styles><office:body><office:text>");
        if !self.regions.is_empty() {
            xml.raw("<text:tracked-changes>");
            for region in &self.regions {
                xml.raw(region);
            }
            xml.raw("</text:tracked-changes>");
        }
        if let Some(odt) = self.odt_carry() {
            for raw in &odt.body_prelude {
                xml.raw(raw);
            }
        }
        xml.raw(body);
        xml.raw("</office:text></office:body></office:document-content>");
        xml.finish()
    }

    // the body --------------------------------------------------------------

    fn body_xml(&mut self) -> String {
        let doc = self.doc;
        let mut out = String::new();
        let mut section = 0;
        let mut pending_master: Option<String> = Some(self.master_name(0));
        let blocks = &doc.blocks;
        let mut index = 0;
        while index < blocks.len() {
            if let Block::SectionBreak = blocks[index] {
                section += 1;
                pending_master = Some(self.master_name(section));
                index += 1;
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
        out
    }

    fn master_name(&self, section: usize) -> String {
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

    fn blocks_xml(&mut self, blocks: &[Block]) -> String {
        let mut out = String::new();
        let mut index = 0;
        while index < blocks.len() {
            if is_list_paragraph(&blocks[index]) {
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

    fn block_xml(&mut self, block: &Block, master: Option<&str>) -> String {
        match block {
            Block::Paragraph(paragraph) => self.paragraph_xml(paragraph, None, master, false),
            Block::Heading(heading) => {
                self.paragraph_xml(&heading.paragraph, Some(heading.level), master, false)
            }
            Block::Table(table) => self.table_xml(table, master),
            Block::PageBreak => self.paragraph_xml(&Paragraph::default(), None, master, true),
            Block::SectionBreak => String::new(),
            Block::TextBox(text_box) => self.text_box_xml(text_box, master),
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

    fn paragraph_xml(
        &mut self,
        paragraph: &Paragraph,
        heading: Option<u8>,
        master: Option<&str>,
        break_after: bool,
    ) -> String {
        let style_name = self.paragraph_style(paragraph, master, break_after);
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
        let content = self.inlines_xml(&paragraph.content);
        format!("<{element}{attributes}>{content}</{element}>")
    }

    /// the style name a paragraph is written with: its named style, or an
    /// automatic style when it has its own formatting or starts a page.
    fn paragraph_style(
        &mut self,
        paragraph: &Paragraph,
        master: Option<&str>,
        break_after: bool,
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
        if props.is_empty() && master.is_none() && !break_after {
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
        let body = paragraph_properties(&props, break_after, self.native);
        Some(self.automatic_style("LP", "paragraph", &attributes, &body))
    }

    /// consecutive list paragraphs as nested `text:list` elements.
    fn list_run(&mut self, blocks: &[Block], master: Option<&str>) -> String {
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
    fn list_open(&mut self, instance_id: &str) -> String {
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

    // inline content ----------------------------------------------------------

    fn inlines_xml(&mut self, inlines: &[Inline]) -> String {
        let mut out = String::new();
        let mut link: Option<LinkTarget> = None;
        let mut insertion: Option<String> = None;
        let mut index = 0;
        while index < inlines.len() {
            let inline = &inlines[index];
            let this_link = inline.marks().iter().find_map(|m| match m {
                Mark::Link(target) => Some(target.clone()),
                _ => None,
            });
            if this_link != link {
                self.close_insertion(&mut insertion, &mut out);
                if link.is_some() {
                    out.push_str("</text:a>");
                }
                if let Some(target) = &this_link {
                    let href = match (&target.href, &target.anchor) {
                        (Some(href), _) => href.clone(),
                        (None, Some(anchor)) => format!("#{anchor}"),
                        _ => String::new(),
                    };
                    out.push_str(&format!(
                        "<text:a xlink:type=\"simple\" xlink:href=\"{}\">",
                        escape_attribute(&href)
                    ));
                }
                link = this_link;
            }
            // a run of deleted content becomes one change mark
            if inline.is_deleted() {
                self.close_insertion(&mut insertion, &mut out);
                let end = index
                    + inlines[index..]
                        .iter()
                        .take_while(|i| i.is_deleted())
                        .count();
                out.push_str(&self.deletion(&inlines[index..end]));
                index = end;
                continue;
            }
            let inserted = inline.marks().iter().find_map(|m| match m {
                Mark::Insertion(change) => Some(change.clone()),
                _ => None,
            });
            match (&insertion, &inserted) {
                (Some(_), None) => self.close_insertion(&mut insertion, &mut out),
                (None, Some(change)) => insertion = Some(self.open_insertion(change, &mut out)),
                _ => {}
            }
            self.open_comments(inline, &mut out);
            out.push_str(&self.inline_xml(inline));
            self.close_comments(inline, &mut out);
            index += 1;
        }
        self.close_insertion(&mut insertion, &mut out);
        if link.is_some() {
            out.push_str("</text:a>");
        }
        out
    }

    fn change_info(change: &Change) -> String {
        format!(
            "<office:change-info><dc:creator>{}</dc:creator><dc:date>{}</dc:date></office:change-info>",
            escape_text(&change.author),
            escape_text(change.date.as_deref().unwrap_or("1970-01-01T00:00:00"))
        )
    }

    fn region_id(&mut self, change: &Change) -> String {
        let wanted = safe_name(&change.id);
        if self.native && !change.id.is_empty() && self.used_change_ids.insert(wanted.clone()) {
            return wanted;
        }
        loop {
            self.counter += 1;
            let id = format!("ct{}", self.counter);
            if self.used_change_ids.insert(id.clone()) {
                return id;
            }
        }
    }

    fn open_insertion(&mut self, change: &Change, out: &mut String) -> String {
        let id = self.region_id(change);
        self.regions.push(format!(
            "<text:changed-region xml:id=\"{id}\" text:id=\"{id}\"><text:insertion>{}</text:insertion></text:changed-region>",
            Self::change_info(change)
        ));
        out.push_str(&format!("<text:change-start text:change-id=\"{id}\"/>"));
        id
    }

    fn close_insertion(&mut self, insertion: &mut Option<String>, out: &mut String) {
        if let Some(id) = insertion.take() {
            out.push_str(&format!("<text:change-end text:change-id=\"{id}\"/>"));
        }
    }

    /// deleted text lives in the change region; the text itself only gets
    /// a marker where it was.
    fn deletion(&mut self, inlines: &[Inline]) -> String {
        let change = inlines[0]
            .marks()
            .iter()
            .find_map(|m| match m {
                Mark::Deletion(change) => Some(change.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let id = self.region_id(&change);
        let saved = self.after_space;
        self.after_space = true;
        let mut text = String::new();
        for inline in inlines {
            let mut visible = inline.clone();
            if let Some(marks) = visible.marks_mut() {
                marks.retain(|m| {
                    !matches!(m, Mark::Deletion(_) | Mark::Comment { .. } | Mark::Link(_))
                });
            }
            text.push_str(&self.inline_xml(&visible));
        }
        self.after_space = saved;
        self.regions.push(format!(
            "<text:changed-region xml:id=\"{id}\" text:id=\"{id}\"><text:deletion>{}<text:p>{text}</text:p></text:deletion></text:changed-region>",
            Self::change_info(&change)
        ));
        format!("<text:change text:change-id=\"{id}\"/>")
    }

    fn open_comments(&mut self, inline: &Inline, out: &mut String) {
        for mark in inline.marks() {
            let Mark::Comment { id } = mark else { continue };
            if self.comment_open.insert(id.clone()) {
                if let Some(comment) = self.doc.comment(id) {
                    out.push_str(&self.annotation(comment, true));
                }
            }
        }
    }

    fn close_comments(&mut self, inline: &Inline, out: &mut String) {
        for mark in inline.marks() {
            let Mark::Comment { id } = mark else { continue };
            let remaining = self.comment_remaining.entry(id.clone()).or_default();
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 && self.comment_open.remove(id) && self.doc.comment(id).is_some() {
                out.push_str(&format!(
                    "<office:annotation-end office:name=\"{}\"/>",
                    escape_attribute(&self.comment_name(id))
                ));
            }
        }
    }

    fn annotation(&mut self, comment: &Comment, ranged: bool) -> String {
        let saved = self.after_space;
        let name = if ranged {
            format!(
                " office:name=\"{}\"",
                escape_attribute(&self.comment_name(&comment.id))
            )
        } else {
            String::new()
        };
        let mut xml = format!(
            "<office:annotation{name}><dc:creator>{}</dc:creator>",
            escape_text(&comment.author)
        );
        if let Some(date) = &comment.date {
            xml.push_str(&format!("<dc:date>{}</dc:date>", escape_text(date)));
        }
        let blocks = self.blocks_xml(&comment.blocks);
        xml.push_str(if blocks.is_empty() {
            "<text:p/>"
        } else {
            &blocks
        });
        xml.push_str("</office:annotation>");
        self.after_space = saved;
        xml
    }

    fn span(&mut self, marks: &[Mark], content: String) -> String {
        let run = RunProps::from_marks(marks);
        let mut plain = run.clone();
        plain.style_id = None;
        if !self.native {
            plain.preserved.clear();
        }
        let name = if plain.is_empty() {
            run.style_id.as_deref().map(safe_name)
        } else {
            let attributes = run
                .style_id
                .as_deref()
                .map(|id| {
                    format!(
                        " style:parent-style-name=\"{}\"",
                        escape_attribute(&safe_name(id))
                    )
                })
                .unwrap_or_default();
            let body = text_properties(&plain, self.native, &mut self.fonts);
            Some(self.automatic_style("LT", "text", &attributes, &body))
        };
        match name {
            Some(name) => format!(
                "<text:span text:style-name=\"{}\">{content}</text:span>",
                escape_attribute(&name)
            ),
            None => content,
        }
    }

    fn inline_xml(&mut self, inline: &Inline) -> String {
        let marks = inline.marks().to_vec();
        match inline {
            Inline::Text { text, .. } => {
                let content = self.encode_text(text);
                self.span(&marks, content)
            }
            Inline::Tab { .. } => {
                self.after_space = false;
                self.span(&marks, "<text:tab/>".into())
            }
            Inline::Break { .. } => {
                self.after_space = true;
                "<text:line-break/>".into()
            }
            Inline::Image(image) => {
                self.after_space = false;
                self.frame_xml(image)
            }
            Inline::NoteReference { note_id, .. } => self.note_xml(note_id, &marks),
            Inline::Field(field) => {
                self.after_space = false;
                let result = escape_text(&field.result);
                let content = match field.kind {
                    FieldKind::PageNumber => format!("<text:page-number text:select-page=\"current\">{result}</text:page-number>"),
                    FieldKind::PageCount => format!("<text:page-count>{result}</text:page-count>"),
                    FieldKind::Date => format!("<text:date>{result}</text:date>"),
                    // a table of contents is a block in odt; inline, only its text can go
                    FieldKind::TableOfContents => result,
                };
                self.span(&marks, content)
            }
            Inline::BookmarkStart { id, name } => {
                self.bookmark_names.insert(id.clone(), name.clone());
                format!(
                    "<text:bookmark-start text:name=\"{}\"/>",
                    escape_attribute(name)
                )
            }
            Inline::BookmarkEnd { id } => {
                let name = self
                    .bookmark_names
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.clone());
                format!(
                    "<text:bookmark-end text:name=\"{}\"/>",
                    escape_attribute(&name)
                )
            }
            Inline::CommentReference { comment_id } => {
                if self.comment_ranged.contains(comment_id) {
                    return String::new();
                }
                match self.doc.comment(comment_id) {
                    Some(comment) => self.annotation(comment, false),
                    None => String::new(),
                }
            }
            Inline::PreservedRun { xml, .. } => {
                let content = self.encode_text(&crate::docx::foreign_text(xml));
                self.span(&marks, content)
            }
            Inline::Preserved { xml } => {
                if self.native {
                    self.after_space = false;
                    xml.clone()
                } else {
                    self.encode_text(&crate::docx::foreign_text(xml))
                }
            }
        }
    }

    /// text with odt's white space rules: runs of spaces as `text:s`,
    /// tabs and line breaks as elements.
    fn encode_text(&mut self, text: &str) -> String {
        let mut out = String::new();
        let mut spaces = 0;
        let flush = |spaces: &mut usize, out: &mut String| {
            match *spaces {
                0 => {}
                1 => out.push_str("<text:s/>"),
                n => out.push_str(&format!("<text:s text:c=\"{n}\"/>")),
            }
            *spaces = 0;
        };
        for c in text.chars() {
            match c {
                ' ' if self.after_space => spaces += 1,
                ' ' => {
                    out.push(' ');
                    self.after_space = true;
                }
                '\t' => {
                    flush(&mut spaces, &mut out);
                    out.push_str("<text:tab/>");
                    self.after_space = false;
                }
                '\n' => {
                    flush(&mut spaces, &mut out);
                    out.push_str("<text:line-break/>");
                    self.after_space = true;
                }
                other => {
                    flush(&mut spaces, &mut out);
                    out.push_str(&escape_text(&other.to_string()));
                    self.after_space = false;
                }
            }
        }
        flush(&mut spaces, &mut out);
        out
    }

    fn note_xml(&mut self, note_id: &str, marks: &[Mark]) -> String {
        let Some(note) = self.doc.note(note_id) else {
            return String::new();
        };
        let (class, number) = match note.kind {
            NoteKind::Footnote => {
                self.note_numbers.0 += 1;
                ("footnote", self.note_numbers.0)
            }
            NoteKind::Endnote => {
                self.note_numbers.1 += 1;
                ("endnote", self.note_numbers.1)
            }
        };
        let saved = self.after_space;
        let body = self.blocks_xml(&note.blocks);
        self.after_space = saved;
        let body = if body.is_empty() {
            "<text:p/>".to_string()
        } else {
            body
        };
        let xml = format!(
            "<text:note text:id=\"{}\" text:note-class=\"{class}\"><text:note-citation>{number}</text:note-citation><text:note-body>{body}</text:note-body></text:note>",
            escape_attribute(&safe_name(note_id))
        );
        let mut marks = marks.to_vec();
        marks.retain(|m| !matches!(m, Mark::Superscript | Mark::CharacterStyle { .. }));
        self.span(&marks, xml)
    }

    /// the attributes of a floating frame: anchored to its paragraph, with
    /// the position measured from the page, the margins or the paragraph
    /// through the frame's graphic style.
    fn floating_attributes(
        &mut self,
        horizontal: &FloatPosition,
        vertical: &FloatPosition,
        wrap: Wrap,
        z_order: Option<i64>,
        extra_properties: &str,
    ) -> String {
        let (wrap_value, through) = match wrap {
            Wrap::Square => ("parallel", ""),
            Wrap::Tight | Wrap::Through => ("dynamic", " style:wrap-contour=\"true\""),
            Wrap::TopAndBottom => ("none", ""),
            Wrap::BehindText => ("run-through", " style:run-through=\"background\""),
            Wrap::InFrontOfText => ("run-through", " style:run-through=\"foreground\""),
        };
        let relation = |position: &FloatPosition| match position.relative_to.as_str() {
            "page" => "page",
            "margin" => "page-content",
            "character" => "char",
            _ => "paragraph",
        };
        let body = format!(
            "<style:graphic-properties style:wrap=\"{wrap_value}\"{through} style:horizontal-pos=\"from-left\" \
             style:horizontal-rel=\"{}\" style:vertical-pos=\"from-top\" style:vertical-rel=\"{}\"{extra_properties}/>",
            relation(horizontal),
            relation(vertical)
        );
        let style = self.automatic_style("LFr", "graphic", "", &body);
        let z = z_order
            .map(|z| format!(" draw:z-index=\"{}\"", z.max(0)))
            .unwrap_or_default();
        format!(
            " draw:style-name=\"{style}\" text:anchor-type=\"paragraph\" svg:x=\"{}\" svg:y=\"{}\"{z}",
            twips_to_inches(horizontal.offset),
            twips_to_inches(vertical.offset)
        )
    }

    /// a text box as a frame holding a `draw:text-box`, in a paragraph of
    /// its own so it has something to anchor to.
    fn text_box_xml(&mut self, text_box: &TextBox, master: Option<&str>) -> String {
        let m = &text_box.margins;
        let mut properties = format!(
            " fo:padding-left=\"{}\" fo:padding-top=\"{}\" fo:padding-right=\"{}\" fo:padding-bottom=\"{}\"",
            twips_to_inches(m.left),
            twips_to_inches(m.top),
            twips_to_inches(m.right),
            twips_to_inches(m.bottom)
        );
        match &text_box.fill {
            Some(hex) => properties.push_str(&format!(
                " draw:fill=\"solid\" draw:fill-color=\"#{0}\" fo:background-color=\"#{0}\"",
                hex.to_ascii_lowercase()
            )),
            None => properties.push_str(" draw:fill=\"none\" fo:background-color=\"transparent\""),
        }
        match &text_box.border {
            Some(border) if border.style != "none" && border.style != "nil" => {
                properties.push_str(&format!(
                    " fo:border=\"{} solid #{}\"",
                    crate::units::points(f64::from(border.size) / 8.0),
                    border
                        .color
                        .as_deref()
                        .unwrap_or("000000")
                        .to_ascii_lowercase()
                ))
            }
            _ => properties.push_str(" fo:border=\"none\""),
        }
        let floating = self.floating_attributes(
            &text_box.horizontal,
            &text_box.vertical,
            text_box.wrap,
            text_box.z_order,
            &properties,
        );
        self.counter += 1;
        let name = text_box
            .name
            .clone()
            .unwrap_or_else(|| format!("Text Box {}", self.counter));
        let height = if text_box.auto_height {
            String::new()
        } else {
            format!(" svg:height=\"{}\"", twips_to_inches(text_box.height))
        };
        let min_height = if text_box.auto_height {
            format!(" fo:min-height=\"{}\"", twips_to_inches(text_box.height))
        } else {
            String::new()
        };
        let saved = self.after_space;
        let content = self.blocks_xml(&text_box.blocks);
        self.after_space = saved;
        let content = if content.is_empty() {
            "<text:p/>".to_string()
        } else {
            content
        };
        let anchor = self.paragraph_style(&Paragraph::default(), master, false);
        let style = anchor
            .map(|name| format!(" text:style-name=\"{}\"", escape_attribute(&name)))
            .unwrap_or_default();
        format!(
            "<text:p{style}><draw:frame draw:name=\"{}\"{floating} svg:width=\"{}\"{height}>\
             <draw:text-box{min_height}>{content}</draw:text-box></draw:frame></text:p>",
            escape_attribute(&name),
            twips_to_inches(text_box.width)
        )
    }

    fn frame_xml(&mut self, image: &Image) -> String {
        if let (true, Some(original)) = (self.native, &image.original) {
            if original.as_read == image.geometry() {
                return original.xml.clone();
            }
        }
        let Some(path) = self.media_paths.get(&image.media_id).cloned() else {
            return String::new();
        };
        let content_type = self
            .doc
            .media_item(&image.media_id)
            .map(|m| m.content_type.clone())
            .unwrap_or_default();
        self.counter += 1;
        let name = image
            .name
            .clone()
            .unwrap_or_else(|| format!("Image{}", self.counter));
        let mut attributes = format!(
            " draw:name=\"{}\" svg:width=\"{}\" svg:height=\"{}\"",
            escape_attribute(&name),
            twips_to_inches(image.width),
            twips_to_inches(image.height)
        );
        match &image.placement {
            Placement::Inline => attributes.push_str(" text:anchor-type=\"as-char\""),
            Placement::Floating {
                horizontal,
                vertical,
                wrap,
            } => {
                let floating =
                    self.floating_attributes(horizontal, vertical, *wrap, image.z_order, "");
                attributes.push_str(&floating);
            }
        }
        let description = image
            .description
            .as_ref()
            .map(|d| format!("<svg:desc>{}</svg:desc>", escape_text(d)))
            .unwrap_or_default();
        format!(
            "<draw:frame{attributes}><draw:image xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\" draw:mime-type=\"{}\"/>{description}</draw:frame>",
            escape_attribute(&path),
            escape_attribute(&content_type)
        )
    }

    // tables ------------------------------------------------------------------

    fn table_xml(&mut self, table: &Table, master: Option<&str>) -> String {
        self.tables += 1;
        let layout = grid(table);
        let columns = layout.first().map(Vec::len).unwrap_or(0).max(1);
        let mut attributes = String::new();
        if let Some(master) = master {
            attributes.push_str(&format!(
                " style:master-page-name=\"{}\"",
                escape_attribute(master)
            ));
        }
        let mut properties = String::new();
        if let Some(width) = table.width {
            properties.push_str(&format!(" style:width=\"{}\"", twips_to_inches(width)));
        }
        let align = match table.alignment {
            Some(Alignment::Center) => "center",
            Some(Alignment::Right) => "right",
            Some(_) => "left",
            None => "margins",
        };
        properties.push_str(&format!(" table:align=\"{align}\""));
        let style = self.automatic_style(
            "LTab",
            "table",
            &attributes,
            &format!("<style:table-properties{properties}/>"),
        );
        let mut xml = format!(
            "<table:table table:name=\"Table{}\" table:style-name=\"{style}\">",
            self.tables
        );
        for column in 0..columns {
            let width = table.column_widths.get(column).copied().filter(|w| *w > 0);
            match width {
                Some(width) => {
                    let body = format!(
                        "<style:table-column-properties style:column-width=\"{}\"/>",
                        twips_to_inches(width)
                    );
                    let name = self.automatic_style("LCol", "table-column", "", &body);
                    xml.push_str(&format!(
                        "<table:table-column table:style-name=\"{name}\"/>"
                    ));
                }
                None => xml.push_str("<table:table-column/>"),
            }
        }
        let header_rows = table.rows.iter().take_while(|row| row.header).count();
        for (index, row) in table.rows.iter().enumerate() {
            if index == 0 && header_rows > 0 {
                xml.push_str("<table:table-header-rows>");
            }
            xml.push_str(&self.row_xml(row, &layout[index]));
            if header_rows > 0 && index + 1 == header_rows {
                xml.push_str("</table:table-header-rows>");
            }
        }
        xml.push_str("</table:table>");
        xml
    }

    fn row_xml(&mut self, row: &TableRow, slots: &[Slot]) -> String {
        let mut xml = String::from("<table:table-row>");
        for slot in slots {
            match *slot {
                Slot::Cell { cell, first: true } => xml.push_str(&self.cell_xml(&row.cells[cell])),
                Slot::Empty => xml.push_str("<table:table-cell><text:p/></table:table-cell>"),
                _ => xml.push_str("<table:covered-table-cell/>"),
            }
        }
        xml.push_str("</table:table-row>");
        xml
    }

    fn cell_xml(&mut self, cell: &TableCell) -> String {
        let mut attributes = String::from(" office:value-type=\"string\"");
        if cell.borders.is_some() || cell.shading.is_some() || cell.vertical_align.is_some() {
            let body = cell_properties(
                cell.borders.as_ref(),
                cell.shading.as_deref(),
                cell.vertical_align,
            );
            let name = self.automatic_style("LCell", "table-cell", "", &body);
            attributes.push_str(&format!(" table:style-name=\"{name}\""));
        }
        if cell.col_span > 1 {
            attributes.push_str(&format!(
                " table:number-columns-spanned=\"{}\"",
                cell.col_span
            ));
        }
        if cell.row_span > 1 {
            attributes.push_str(&format!(" table:number-rows-spanned=\"{}\"", cell.row_span));
        }
        let content = self.blocks_xml(&cell.blocks);
        let content = if content.is_empty() {
            "<text:p/>".to_string()
        } else {
            content
        };
        format!("<table:table-cell{attributes}>{content}</table:table-cell>")
    }

    // styles.xml ----------------------------------------------------------------

    fn styles_xml(&mut self) -> String {
        let common = self.common_styles();
        let lists = self.list_styles();
        self.pool = Pool::Styles;
        let (layouts, masters) = self.master_pages();
        self.pool = Pool::Content;
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root("styles.xml", "office:document-styles"));
        xml.raw(&self.font_faces());
        xml.raw("<office:styles>");
        xml.raw(&common);
        xml.raw(&lists);
        if self.native {
            for preserved in &self.doc.styles.preserved {
                xml.raw(preserved);
            }
        }
        xml.raw("</office:styles><office:automatic-styles>");
        if let Some(odt) = self.odt_carry() {
            for raw in odt
                .styles_automatic_styles
                .iter()
                .filter(|raw| keep_carried_style(raw))
            {
                xml.raw(raw);
            }
        }
        for (_, style) in &self.styles_styles {
            xml.raw(style);
        }
        xml.raw(&layouts);
        xml.raw("</office:automatic-styles><office:master-styles>");
        xml.raw(&masters);
        xml.raw("</office:master-styles></office:document-styles>");
        xml.finish()
    }

    fn common_styles(&mut self) -> String {
        let doc = self.doc;
        let mut xml = String::from("<style:default-style style:family=\"paragraph\">");
        xml.push_str(&paragraph_properties(
            &doc.styles.defaults.paragraph,
            false,
            self.native,
        ));
        xml.push_str(&text_properties(
            &doc.styles.defaults.run,
            self.native,
            &mut self.fonts,
        ));
        xml.push_str("</style:default-style>");
        for style in &doc.styles.styles {
            let family = match style.kind {
                StyleKind::Paragraph => "paragraph",
                StyleKind::Character => "text",
                // table and numbering styles have no odt counterpart
                _ => continue,
            };
            let mut attributes = format!(
                " style:name=\"{}\" style:display-name=\"{}\" style:family=\"{family}\"",
                escape_attribute(&safe_name(&style.id)),
                escape_attribute(&display_name(&style.name))
            );
            if let Some(parent) = &style.based_on {
                attributes.push_str(&format!(
                    " style:parent-style-name=\"{}\"",
                    escape_attribute(&safe_name(parent))
                ));
            }
            if let Some(next) = &style.next {
                attributes.push_str(&format!(
                    " style:next-style-name=\"{}\"",
                    escape_attribute(&safe_name(next))
                ));
            }
            if let Some(level) = style.paragraph.outline_level.filter(|l| *l < 10) {
                attributes.push_str(&format!(" style:default-outline-level=\"{}\"", level + 1));
            }
            let mut body = String::new();
            if style.kind == StyleKind::Paragraph {
                let mut props = style.paragraph.clone();
                props.outline_level = None;
                props.list = None;
                body.push_str(&paragraph_properties(&props, false, self.native));
            }
            body.push_str(&text_properties(&style.run, self.native, &mut self.fonts));
            xml.push_str(&format!("<style:style{attributes}>{body}</style:style>"));
        }
        xml
    }

    fn list_styles(&self) -> String {
        let mut xml = String::new();
        for definition in &self.doc.numbering.definitions {
            xml.push_str(&format!(
                "<text:list-style style:name=\"{}\">",
                escape_attribute(&safe_name(&definition.id))
            ));
            for level in &definition.levels {
                xml.push_str(&list_level_xml(level));
            }
            xml.push_str("</text:list-style>");
        }
        xml
    }

    /// page layouts and master pages, one per section, with headers and
    /// footers carried on from the section before when a section has none.
    fn master_pages(&mut self) -> (String, String) {
        let doc = self.doc;
        let mut layouts = String::new();
        let mut masters = String::new();
        let mut written = BTreeSet::new();
        let mut headers = HeaderFooterRefs::default();
        let mut footers = HeaderFooterRefs::default();
        for (index, section) in doc.sections.iter().enumerate() {
            inherit(&mut headers, &section.headers);
            inherit(&mut footers, &section.footers);
            let name = self.master_name(index);
            if !written.insert(name.clone()) {
                continue;
            }
            let layout = self.fresh_name("LPM");
            layouts.push_str(&page_layout_xml(
                &layout,
                section,
                headers.default.is_some(),
                footers.default.is_some(),
            ));
            let mut master = format!(
                "<style:master-page style:name=\"{}\" style:page-layout-name=\"{layout}\">",
                escape_attribute(&name)
            );
            let even = doc.settings.even_and_odd_headers;
            let first = section.different_first_page;
            for (refs, element) in [(&headers, "style:header"), (&footers, "style:footer")] {
                let slots = [
                    (refs.default.clone(), element.to_string()),
                    (
                        refs.even.clone().filter(|_| even),
                        format!("{element}-left"),
                    ),
                    (
                        refs.first.clone().filter(|_| first),
                        format!("{element}-first"),
                    ),
                ];
                for (id, tag) in slots {
                    let Some(part) = id.and_then(|id| doc.header_footer(&id)) else {
                        continue;
                    };
                    let blocks = self.blocks_xml(&part.blocks);
                    master.push_str(&format!("<{tag}>{blocks}</{tag}>"));
                }
            }
            master.push_str("</style:master-page>");
            masters.push_str(&master);
        }
        (layouts, masters)
    }

    // meta and manifest ------------------------------------------------------------

    fn meta_xml(&self) -> String {
        let metadata = &self.doc.metadata;
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root("meta.xml", "office:document-meta"));
        xml.raw("<office:meta>");
        for (name, value) in [
            ("dc:title", &metadata.title),
            ("dc:subject", &metadata.subject),
            ("dc:description", &metadata.description),
            ("meta:keyword", &metadata.keywords),
            ("meta:initial-creator", &metadata.author),
            ("dc:creator", &metadata.last_modified_by),
            ("meta:creation-date", &metadata.created),
            ("dc:date", &metadata.modified),
            ("dc:language", &metadata.language),
        ] {
            if let Some(value) = value {
                xml.raw(&format!("<{name}>{}</{name}>", escape_text(value)));
            }
        }
        if let Some(odt) = self.odt_carry() {
            for extra in &odt.meta_extras {
                xml.raw(extra);
            }
        } else {
            xml.raw("<meta:generator>Letters</meta:generator>");
        }
        xml.raw("</office:meta></office:document-meta>");
        xml.finish()
    }

    fn manifest_xml(&self, parts: &[Part]) -> String {
        let carried: HashMap<&str, &str> = self
            .odt_carry()
            .map(|odt| {
                odt.manifest
                    .iter()
                    .map(|e| (e.name.as_str(), e.value.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&format!(
            "<manifest:manifest xmlns:manifest=\"{}\" manifest:version=\"1.3\">",
            namespace_uri("manifest")
        ));
        xml.raw(&format!(
            "<manifest:file-entry manifest:full-path=\"/\" manifest:version=\"1.3\" manifest:media-type=\"{}\"/>",
            escape_attribute(&self.mimetype())
        ));
        for part in parts.iter().filter(|p| p.name != "mimetype") {
            let media_type = match part.name.as_str() {
                "content.xml" | "styles.xml" | "meta.xml" | "settings.xml" => {
                    "text/xml".to_string()
                }
                name => carried
                    .get(name)
                    .map(|t| t.to_string())
                    .or_else(|| {
                        self.doc
                            .media
                            .iter()
                            .find(|m| self.media_paths[&m.id] == name)
                            .map(|m| m.content_type.clone())
                    })
                    .unwrap_or_default(),
            };
            xml.raw(&format!(
                "<manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{}\"/>",
                escape_attribute(&part.name),
                escape_attribute(&media_type)
            ));
        }
        xml.raw("</manifest:manifest>");
        xml.finish()
    }
}

/// carried automatic styles are kept, except list styles and page layouts:
/// the model owns those and the writer makes them afresh.
fn keep_carried_style(raw: &str) -> bool {
    let name = crate::docx::first_tag_name(raw);
    name != "text:list-style" && name != "style:page-layout"
}

fn is_list_paragraph(block: &Block) -> bool {
    block
        .paragraph()
        .and_then(|p| p.props.list.as_ref())
        .is_some_and(|list| list.list_id != "0")
}

fn close_lists(stack: &mut Vec<bool>, out: &mut String, keep: usize) {
    while stack.len() > keep {
        if stack.pop() == Some(true) {
            out.push_str("</text:list-item>");
        }
        out.push_str("</text:list>");
    }
}

fn inherit(current: &mut HeaderFooterRefs, section: &HeaderFooterRefs) {
    if section.default.is_some() {
        current.default.clone_from(&section.default);
    }
    if section.first.is_some() {
        current.first.clone_from(&section.first);
    }
    if section.even.is_some() {
        current.even.clone_from(&section.even);
    }
}

fn page_layout_xml(name: &str, section: &Section, header: bool, footer: bool) -> String {
    let page = &section.page;
    let margins = &page.margins;
    let top = if header { margins.header } else { margins.top };
    let bottom = if footer {
        margins.footer
    } else {
        margins.bottom
    };
    let orientation = if page.orientation == Orientation::Landscape {
        "landscape"
    } else {
        "portrait"
    };
    let columns = if section.columns.count > 1 {
        format!(
            "<style:columns fo:column-count=\"{}\" fo:column-gap=\"{}\"/>",
            section.columns.count,
            twips_to_inches(section.columns.space)
        )
    } else {
        String::new()
    };
    let mut xml = format!(
        "<style:page-layout style:name=\"{name}\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\" \
         style:print-orientation=\"{orientation}\" fo:margin-top=\"{}\" fo:margin-bottom=\"{}\" fo:margin-left=\"{}\" fo:margin-right=\"{}\">{columns}</style:page-layout-properties>",
        twips_to_inches(page.width),
        twips_to_inches(page.height),
        twips_to_inches(top),
        twips_to_inches(bottom),
        twips_to_inches(margins.left + margins.gutter),
        twips_to_inches(margins.right)
    );
    // word measures the body from the page edge; odt puts the header inside
    // the margin, so the gap below the header makes up the difference
    if header {
        xml.push_str(&format!(
            "<style:header-style><style:header-footer-properties fo:min-height=\"0in\" fo:margin-bottom=\"{}\"/></style:header-style>",
            twips_to_inches((margins.top - margins.header).max(0))
        ));
    }
    if footer {
        xml.push_str(&format!(
            "<style:footer-style><style:header-footer-properties fo:min-height=\"0in\" fo:margin-top=\"{}\"/></style:footer-style>",
            twips_to_inches((margins.bottom - margins.footer).max(0))
        ));
    }
    xml.push_str("</style:page-layout>");
    xml
}

fn list_level_xml(level: &ListLevel) -> String {
    let left = level
        .paragraph
        .indent
        .left
        .unwrap_or(720 * (i32::from(level.level) + 1));
    let indent = -level.paragraph.indent.hanging.unwrap_or(360)
        + level.paragraph.indent.first_line.unwrap_or(0);
    let position = format!(
        "<style:list-level-properties text:list-level-position-and-space-mode=\"label-alignment\">\
         <style:list-level-label-alignment text:label-followed-by=\"listtab\" text:list-tab-stop-position=\"{left}\" fo:text-indent=\"{indent}\" fo:margin-left=\"{left}\"/></style:list-level-properties>",
        left = twips_to_inches(left),
        indent = twips_to_inches(indent)
    );
    let number = level.level + 1;
    if level.format == NumberFormat::Bullet {
        return format!(
            "<text:list-level-style-bullet text:level=\"{number}\" text:bullet-char=\"{}\">{position}</text:list-level-style-bullet>",
            escape_attribute(level.text.chars().next().map(String::from).as_deref().unwrap_or("\u{2022}"))
        );
    }
    let format = match &level.format {
        NumberFormat::LowerLetter => "a",
        NumberFormat::UpperLetter => "A",
        NumberFormat::LowerRoman => "i",
        NumberFormat::UpperRoman => "I",
        NumberFormat::None => "",
        _ => "1",
    };
    let (prefix, shown, suffix) = split_label(&level.text);
    format!(
        "<text:list-level-style-number text:level=\"{number}\" style:num-format=\"{format}\" style:num-prefix=\"{}\" style:num-suffix=\"{}\" text:display-levels=\"{}\" text:start-value=\"{}\">{position}</text:list-level-style-number>",
        escape_attribute(&prefix),
        escape_attribute(&suffix),
        shown.max(1),
        level.start
    )
}

/// "(%1.%2)" as prefix "(", two levels shown, suffix ")".
fn split_label(text: &str) -> (String, usize, String) {
    let first = text.find('%');
    let last = text.rfind('%');
    match (first, last) {
        (Some(first), Some(last)) => {
            let suffix_start = (last + 2).min(text.len());
            (
                text[..first].to_string(),
                text.matches('%').count(),
                text[suffix_start..].to_string(),
            )
        }
        _ => (String::new(), 1, text.to_string()),
    }
}

/// style and id names must be xml names: letters, digits, '-', '_', '.'.
fn safe_name(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty()
        || !cleaned
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
    {
        cleaned.insert(0, '_');
    }
    cleaned
}

/// word names its heading styles "heading 1"; odt applications look for
/// "Heading 1".
fn display_name(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("heading ") {
        return format!("Heading {rest}");
    }
    name.to_string()
}
