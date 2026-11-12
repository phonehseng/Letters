//! the document-wide parts: styles, numbering, settings, notes,
//! comments, headers and footers, and the core properties.

use crate::model::{
    ListDefinition, ListInstance, ListLevel, Note, NoteKind, NumberFormat, Style, StyleKind,
};
use crate::xml::{escape_text, parse, Element, XmlDocument, XmlWriter};

use super::props::{alignment_value, empty, val};
use super::{DocxWriter, StoryOut};
use crate::docx::names::{self, KIND_CORE};
use crate::docx::order::{self, Ordered};

impl DocxWriter<'_> {
    /// the part name for a part kind: where it was, or word's usual place.
    fn part_for(&self, kind: &str, default: &str) -> String {
        self.docx_carry()
            .and_then(|docx| docx.part_name(kind))
            .map(str::to_string)
            .unwrap_or_else(|| default.to_string())
    }

    pub(super) fn write_styles(&mut self, main: &mut StoryOut) {
        let styles = &self.doc.styles;
        if styles.styles.is_empty()
            && styles.defaults.run.is_empty()
            && styles.defaults.paragraph.is_empty()
        {
            return;
        }
        let part = self.part_for(names::KIND_STYLES, "word/styles.xml");
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root_tag(&part, "w:styles"));
        xml.raw("<w:docDefaults><w:rPrDefault><w:rPr>");
        xml.raw(&self.run_props(&styles.defaults.run));
        xml.raw("</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>");
        xml.raw(&self.paragraph_props(None, &styles.defaults.paragraph, Vec::new()));
        xml.raw("</w:pPr></w:pPrDefault></w:docDefaults>");
        if self.native {
            for preserved in &styles.preserved {
                xml.raw(preserved);
            }
        }
        for style in &styles.styles {
            xml.raw(&self.style_xml(style));
        }
        xml.close("w:styles");
        let story = StoryOut::new(&part, &self.carried_rels(&part), &[]);
        self.add_part(&part, Some(names::CT_STYLES), xml.finish().into_bytes());
        self.add_rels(&story);
        main.add_part_rel(names::KIND_STYLES, &part);
    }

    fn style_xml(&self, style: &Style) -> String {
        let kind = match style.kind {
            StyleKind::Paragraph => "paragraph",
            StyleKind::Character => "character",
            StyleKind::Table => "table",
            StyleKind::Numbering => "numbering",
        };
        let mut attributes = vec![
            ("w:type".to_string(), kind.to_string()),
            ("w:styleId".to_string(), style.id.clone()),
        ];
        if style.is_default {
            attributes.push(("w:default".into(), "1".into()));
        }
        let mut ordered = Ordered::new(order::STYLE);
        ordered.push("w:name", val("w:name", &style.name));
        if let Some(based_on) = &style.based_on {
            ordered.push("w:basedOn", val("w:basedOn", based_on));
        }
        if let Some(next) = &style.next {
            ordered.push("w:next", val("w:next", next));
        }
        if self.native {
            for attribute in &style.preserved.attributes {
                attributes.push((attribute.name.clone(), attribute.value.clone()));
            }
            for preserved in &style.preserved.children {
                ordered.push_raw(preserved);
            }
        } else if crate::styles::style_heading_level(self.doc, &style.id).is_some()
            || style.kind == StyleKind::Paragraph
        {
            // word shows a style in its gallery only when told to
            ordered.push("w:qFormat", empty("w:qFormat", &[]));
        }
        let paragraph = self.paragraph_props(None, &style.paragraph, Vec::new());
        if !paragraph.is_empty() {
            ordered.push("w:pPr", format!("<w:pPr>{paragraph}</w:pPr>"));
        }
        let run = self.run_props(&style.run);
        if !run.is_empty() {
            ordered.push("w:rPr", format!("<w:rPr>{run}</w:rPr>"));
        }
        let mut xml = XmlWriter::new();
        xml.open_owned("w:style", &attributes);
        xml.raw(&ordered.finish());
        xml.close("w:style");
        xml.finish()
    }

    pub(super) fn write_numbering(&mut self, main: &mut StoryOut) {
        let numbering = &self.doc.numbering;
        if numbering.definitions.is_empty() && numbering.instances.is_empty() {
            return;
        }
        let part = self.part_for(names::KIND_NUMBERING, "word/numbering.xml");
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root_tag(&part, "w:numbering"));
        let (pictures, others): (Vec<&String>, Vec<&String>) = numbering
            .preserved
            .iter()
            .partition(|xml| order::first_tag_name(xml).ends_with("numPicBullet"));
        if self.native {
            pictures.iter().for_each(|preserved| xml.raw(preserved));
        }
        for definition in &numbering.definitions {
            xml.raw(&self.list_definition_xml(definition));
        }
        for instance in &numbering.instances {
            xml.raw(&self.list_instance_xml(instance));
        }
        if self.native {
            others.iter().for_each(|preserved| xml.raw(preserved));
        }
        xml.close("w:numbering");
        let story = StoryOut::new(&part, &self.carried_rels(&part), &[]);
        self.add_part(&part, Some(names::CT_NUMBERING), xml.finish().into_bytes());
        self.add_rels(&story);
        main.add_part_rel(names::KIND_NUMBERING, &part);
    }

    fn list_definition_xml(&self, definition: &ListDefinition) -> String {
        let id = &self.definition_ids[&definition.id];
        let mut attributes = vec![("w:abstractNumId".to_string(), id.clone())];
        let mut ordered = Ordered::new(order::LIST_DEFINITION);
        if self.native {
            for attribute in &definition.preserved.attributes {
                attributes.push((attribute.name.clone(), attribute.value.clone()));
            }
            for preserved in &definition.preserved.children {
                ordered.push_raw(preserved);
            }
        } else {
            let multi = if definition.levels.len() > 1 {
                "hybridMultilevel"
            } else {
                "singleLevel"
            };
            ordered.push("w:multiLevelType", val("w:multiLevelType", multi));
        }
        if let Some(name) = &definition.name {
            ordered.push("w:name", val("w:name", name));
        }
        for level in &definition.levels {
            ordered.push("w:lvl", self.list_level_xml(level));
        }
        let mut xml = XmlWriter::new();
        xml.open_owned("w:abstractNum", &attributes);
        xml.raw(&ordered.finish());
        xml.close("w:abstractNum");
        xml.finish()
    }

    fn list_level_xml(&self, level: &ListLevel) -> String {
        let mut attributes = vec![("w:ilvl".to_string(), level.level.to_string())];
        let mut ordered = Ordered::new(order::LIST_LEVEL);
        ordered.push("w:start", val("w:start", &level.start.to_string()));
        ordered.push(
            "w:numFmt",
            val("w:numFmt", number_format_value(&level.format)),
        );
        ordered.push("w:lvlText", val("w:lvlText", &level.text));
        if let Some(alignment) = level.alignment {
            ordered.push("w:lvlJc", val("w:lvlJc", alignment_value(alignment)));
        }
        let paragraph = self.paragraph_props(None, &level.paragraph, Vec::new());
        if !paragraph.is_empty() {
            ordered.push("w:pPr", format!("<w:pPr>{paragraph}</w:pPr>"));
        }
        let run = self.run_props(&level.run);
        if !run.is_empty() {
            ordered.push("w:rPr", format!("<w:rPr>{run}</w:rPr>"));
        }
        if self.native {
            for attribute in &level.preserved.attributes {
                attributes.push((attribute.name.clone(), attribute.value.clone()));
            }
            for preserved in &level.preserved.children {
                ordered.push_raw(preserved);
            }
        }
        let mut xml = XmlWriter::new();
        xml.open_owned("w:lvl", &attributes);
        xml.raw(&ordered.finish());
        xml.close("w:lvl");
        xml.finish()
    }

    fn list_instance_xml(&self, instance: &ListInstance) -> String {
        let id = &self.list_ids[&instance.id];
        let definition = self
            .definition_ids
            .get(&instance.definition_id)
            .cloned()
            .unwrap_or_else(|| "0".into());
        let mut attributes = vec![("w:numId".to_string(), id.clone())];
        let mut body = val("w:abstractNumId", &definition);
        for item in &instance.overrides {
            body.push_str(&format!("<w:lvlOverride w:ilvl=\"{}\">", item.level));
            if let Some(start) = item.start {
                body.push_str(&val("w:startOverride", &start.to_string()));
            }
            if let (true, Some(preserved)) = (self.native, &item.preserved) {
                body.push_str(preserved);
            }
            body.push_str("</w:lvlOverride>");
        }
        if self.native {
            for attribute in &instance.preserved.attributes {
                attributes.push((attribute.name.clone(), attribute.value.clone()));
            }
            for preserved in &instance.preserved.children {
                body.push_str(preserved);
            }
        }
        let mut xml = XmlWriter::new();
        xml.open_owned("w:num", &attributes);
        xml.raw(&body);
        xml.close("w:num");
        xml.finish()
    }

    /// settings.xml: the carried part with the model's few values put
    /// back in place, or a small new one.
    pub(super) fn write_settings(&mut self, main: &mut StoryOut) {
        let carried_name = self
            .docx_carry()
            .and_then(|docx| docx.part_name(names::KIND_SETTINGS));
        let carried = carried_name
            .and_then(|name| self.doc.carry.part(name))
            .and_then(|part| crate::xml::decode_text(&part.bytes).ok())
            .and_then(|text| parse(&text).ok());
        let (part, xml) = match (carried_name, carried) {
            (Some(name), Some(document)) => (name.to_string(), self.patched_settings(&document)),
            _ => {
                let part = "word/settings.xml".to_string();
                let xml = self.new_settings();
                main.add_part_rel(names::KIND_SETTINGS, &part);
                (part, xml)
            }
        };
        self.add_part(&part, Some(names::CT_SETTINGS), xml.into_bytes());
    }

    fn modelled_settings(&self, ordered: &mut Ordered) {
        let settings = &self.doc.settings;
        if settings.track_changes {
            ordered.push("w:trackRevisions", empty("w:trackRevisions", &[]));
        }
        if let Some(stop) = settings.default_tab_stop {
            ordered.push(
                "w:defaultTabStop",
                val("w:defaultTabStop", &stop.to_string()),
            );
        }
        if settings.even_and_odd_headers {
            ordered.push("w:evenAndOddHeaders", empty("w:evenAndOddHeaders", &[]));
        }
    }

    fn patched_settings(&self, document: &XmlDocument) -> String {
        let modelled = [
            "w:trackRevisions",
            "w:defaultTabStop",
            "w:evenAndOddHeaders",
        ];
        let mut ordered = Ordered::new(order::SETTINGS);
        for child in document.root.elements() {
            if !modelled.contains(&child.name.as_str()) {
                ordered.push(&child.name, document.raw(child).to_string());
            }
        }
        self.modelled_settings(&mut ordered);
        let (open, close) = root_tags(document);
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&open);
        xml.raw(&ordered.finish());
        xml.raw(&close);
        xml.finish()
    }

    fn new_settings(&self) -> String {
        let mut ordered = Ordered::new(order::SETTINGS);
        ordered.push("w:zoom", empty("w:zoom", &[("w:percent", "100")]));
        self.modelled_settings(&mut ordered);
        ordered.push(
            "w:characterSpacingControl",
            val("w:characterSpacingControl", "doNotCompress"),
        );
        if self
            .doc
            .notes
            .iter()
            .any(|note| note.kind == NoteKind::Footnote)
        {
            ordered.push(
                "w:footnotePr",
                "<w:footnotePr><w:footnote w:id=\"-1\"/><w:footnote w:id=\"0\"/></w:footnotePr>"
                    .into(),
            );
        }
        if self
            .doc
            .notes
            .iter()
            .any(|note| note.kind == NoteKind::Endnote)
        {
            ordered.push(
                "w:endnotePr",
                "<w:endnotePr><w:endnote w:id=\"-1\"/><w:endnote w:id=\"0\"/></w:endnotePr>".into(),
            );
        }
        // without this word opens the file in compatibility mode
        ordered.push(
            "w:compat",
            "<w:compat><w:compatSetting w:name=\"compatibilityMode\" \
             w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/></w:compat>"
                .into(),
        );
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&format!(
            "<w:settings xmlns:w=\"{}\">",
            super::w_namespace()
        ));
        xml.raw(&ordered.finish());
        xml.close("w:settings");
        xml.finish()
    }

    pub(super) fn write_notes(&mut self, main: &mut StoryOut) {
        for (kind, rel_kind, element, root, default, content_type) in [
            (
                NoteKind::Footnote,
                names::KIND_FOOTNOTES,
                "w:footnote",
                "w:footnotes",
                "word/footnotes.xml",
                names::CT_FOOTNOTES,
            ),
            (
                NoteKind::Endnote,
                names::KIND_ENDNOTES,
                "w:endnote",
                "w:endnotes",
                "word/endnotes.xml",
                names::CT_ENDNOTES,
            ),
        ] {
            let doc = self.doc;
            let notes: Vec<&Note> = doc.notes.iter().filter(|note| note.kind == kind).collect();
            if notes.is_empty() {
                continue;
            }
            let part = self.part_for(rel_kind, default);
            let blocks: Vec<&[crate::model::Block]> =
                notes.iter().map(|n| n.blocks.as_slice()).collect();
            let mut story = StoryOut::new(&part, &self.carried_rels(&part), &blocks);
            let mut xml = XmlWriter::with_declaration();
            xml.raw(&self.root_tag(&part, root));
            if !notes.iter().any(|note| note.special.is_some()) {
                xml.raw(&separator_notes(element));
            }
            for note in notes {
                let id = self.note_ids[&note.id].to_string();
                let mut attributes = vec![("w:id".to_string(), id)];
                if let Some(special) = &note.special {
                    attributes.insert(0, ("w:type".to_string(), special.clone()));
                }
                xml.open_owned(element, &attributes);
                let lead = note.special.is_none().then(|| self.note_marker(kind));
                self.write_blocks(&note.blocks, &mut story, &mut xml, lead, false);
                xml.close(element);
            }
            xml.close(root);
            self.add_part(&part, Some(content_type), xml.finish().into_bytes());
            self.add_rels(&story);
            main.add_part_rel(rel_kind, &part);
        }
    }

    /// the number shown at the start of a note's own text.
    fn note_marker(&self, kind: NoteKind) -> String {
        let (style, element) = match kind {
            NoteKind::Footnote => ("FootnoteReference", "<w:footnoteRef/>"),
            NoteKind::Endnote => ("EndnoteReference", "<w:endnoteRef/>"),
        };
        if self.has_style(style) {
            format!("<w:r><w:rPr><w:rStyle w:val=\"{style}\"/></w:rPr>{element}</w:r>")
        } else {
            format!("<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr>{element}</w:r>")
        }
    }

    pub(super) fn write_comments(&mut self, main: &mut StoryOut) {
        let doc = self.doc;
        if doc.comments.is_empty() {
            return;
        }
        let part = self.part_for(names::KIND_COMMENTS, "word/comments.xml");
        let blocks: Vec<&[crate::model::Block]> =
            doc.comments.iter().map(|c| c.blocks.as_slice()).collect();
        let mut story = StoryOut::new(&part, &self.carried_rels(&part), &blocks);
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root_tag(&part, "w:comments"));
        for comment in &doc.comments {
            let mut attributes = vec![
                ("w:id".to_string(), self.comment_ids[&comment.id].clone()),
                ("w:author".to_string(), comment.author.clone()),
            ];
            if let Some(date) = &comment.date {
                attributes.push(("w:date".into(), date.clone()));
            }
            if let Some(initials) = &comment.initials {
                attributes.push(("w:initials".into(), initials.clone()));
            }
            xml.open_owned("w:comment", &attributes);
            let marker = if self.has_style("CommentReference") {
                "<w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr><w:annotationRef/></w:r>"
            } else {
                "<w:r><w:annotationRef/></w:r>"
            };
            self.write_blocks(
                &comment.blocks,
                &mut story,
                &mut xml,
                Some(marker.to_string()),
                false,
            );
            xml.close("w:comment");
        }
        xml.close("w:comments");
        self.add_part(&part, Some(names::CT_COMMENTS), xml.finish().into_bytes());
        self.add_rels(&story);
        main.add_part_rel(names::KIND_COMMENTS, &part);
    }

    pub(super) fn write_headers_footers(&mut self) {
        let doc = self.doc;
        for part_model in &doc.headers_footers {
            let path = self.header_paths[&part_model.id].clone();
            let (root, content_type) = match part_model.kind {
                crate::model::HeaderFooterKind::Header => ("w:hdr", names::CT_HEADER),
                crate::model::HeaderFooterKind::Footer => ("w:ftr", names::CT_FOOTER),
            };
            let mut story = StoryOut::new(&path, &self.carried_rels(&path), &[&part_model.blocks]);
            let mut xml = XmlWriter::with_declaration();
            xml.raw(&self.root_tag(&path, root));
            self.write_blocks(&part_model.blocks, &mut story, &mut xml, None, false);
            if !matches!(
                part_model.blocks.last(),
                Some(crate::model::Block::Paragraph(_)) | Some(crate::model::Block::Heading(_))
            ) {
                xml.raw("<w:p/>");
            }
            xml.close(root);
            self.add_part(&path, Some(content_type), xml.finish().into_bytes());
            self.add_rels(&story);
        }
    }

    pub(super) fn write_core_and_app(&mut self) {
        let part = self.part_for(KIND_CORE, "docProps/core.xml");
        let metadata = &self.doc.metadata;
        let mut xml = XmlWriter::with_declaration();
        xml.raw(
            "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
             xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" \
             xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">",
        );
        for (name, value) in [
            ("dc:title", &metadata.title),
            ("dc:subject", &metadata.subject),
            ("dc:creator", &metadata.author),
            ("cp:keywords", &metadata.keywords),
            ("dc:description", &metadata.description),
            ("cp:lastModifiedBy", &metadata.last_modified_by),
            ("dc:language", &metadata.language),
        ] {
            if let Some(value) = value {
                xml.raw(&format!("<{name}>{}</{name}>", escape_text(value)));
            }
        }
        for (name, value) in [
            ("dcterms:created", &metadata.created),
            ("dcterms:modified", &metadata.modified),
        ] {
            if let Some(value) = value {
                xml.raw(&format!(
                    "<{name} xsi:type=\"dcterms:W3CDTF\">{}</{name}>",
                    escape_text(value)
                ));
            }
        }
        if let Some(docx) = self.docx_carry() {
            for extra in &docx.core_extras {
                xml.raw(extra);
            }
        }
        xml.raw("</cp:coreProperties>");
        self.add_part(&part, Some(names::CT_CORE), xml.finish().into_bytes());
        let has_app = self.native
            && self
                .doc
                .carry
                .parts
                .iter()
                .any(|carried| carried.content_type.as_deref() == Some(names::CT_APP));
        if !has_app {
            let app = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
                <Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\">\
                <Application>Letters</Application></Properties>";
            self.add_part(
                "docProps/app.xml",
                Some(names::CT_APP),
                app.as_bytes().to_vec(),
            );
        }
    }
}

fn number_format_value(format: &NumberFormat) -> &str {
    match format {
        NumberFormat::Bullet => "bullet",
        NumberFormat::Decimal => "decimal",
        NumberFormat::DecimalZero => "decimalZero",
        NumberFormat::LowerLetter => "lowerLetter",
        NumberFormat::UpperLetter => "upperLetter",
        NumberFormat::LowerRoman => "lowerRoman",
        NumberFormat::UpperRoman => "upperRoman",
        NumberFormat::None => "none",
        NumberFormat::Other(other) => other,
    }
}

/// the separator lines word draws above notes, which it expects to find
/// as notes -1 and 0.
fn separator_notes(element: &str) -> String {
    let paragraph = |content: &str| {
        format!(
            "<w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r>{content}</w:r></w:p>"
        )
    };
    format!(
        "<{element} w:type=\"separator\" w:id=\"-1\">{}</{element}>\
         <{element} w:type=\"continuationSeparator\" w:id=\"0\">{}</{element}>",
        paragraph("<w:separator/>"),
        paragraph("<w:continuationSeparator/>")
    )
}

/// the root element's own start and end tags, exactly as written, so a
/// rewritten part keeps every namespace and attribute of its root.
pub(crate) fn root_tags(document: &XmlDocument) -> (String, String) {
    let root: &Element = &document.root;
    let source = &document.source[root.start..root.end];
    let open_end = start_tag_length(source);
    let open = &source[..open_end];
    let written_name = crate::docx::order::first_tag_name(open);
    if let Some(empty) = open.strip_suffix("/>") {
        // an empty root: reopen it so children can go inside
        return (
            format!("{}>", empty.trim_end()),
            format!("</{written_name}>"),
        );
    }
    (open.to_string(), format!("</{written_name}>"))
}

/// the length of the start tag at the beginning of `source`, skipping
/// any `>` inside quoted attribute values.
fn start_tag_length(source: &str) -> usize {
    let mut quote = None;
    for (index, c) in source.char_indices() {
        match (quote, c) {
            (None, '"') | (None, '\'') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (None, '>') => return index + 1,
            _ => {}
        }
    }
    source.len()
}
