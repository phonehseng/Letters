//! section properties (`w:sectPr`): page setup, columns, headers and
//! footers, and the body that holds the document's main text.

use crate::error::OfficeError;
use crate::model::{
    Attribute, HeaderFooter, HeaderFooterKind, HeaderFooterRefs, Orientation, Section, SectionStart,
};
use crate::xml::Element;

use super::props::{number, on_off, only_attributes, twips};
use super::{DocxReader, Story};
use crate::docx::names::{relationship_kind, KIND_FOOTER, KIND_HEADER};
use crate::docx::rels;

impl DocxReader<'_> {
    /// reads the main document part: its body, every section, and the
    /// headers and footers those sections use.
    pub(super) fn read_main(&mut self) -> Result<(), OfficeError> {
        let part = self.main_part.clone();
        let xml = self
            .part_xml(&part)?
            .ok_or_else(|| OfficeError::Damaged("the main document part is missing".into()))?;
        if xml.root.name != "w:document" {
            return Err(OfficeError::bad_xml(
                &part,
                "it is not a Word document body",
            ));
        }
        self.consume(&part);
        let mut story = Story::new(&part, &xml, self.main_rels.clone());
        story.is_main = true;
        let mut extras = Vec::new();
        let mut sections = Vec::new();
        for child in xml.root.elements() {
            if child.name == "w:body" {
                self.doc.blocks = self.read_blocks(child, &mut story, &mut sections);
                let last = match child.child("w:sectPr") {
                    Some(sect) => self.read_section(sect, &mut story),
                    None => Section::default(),
                };
                sections.push(last);
            } else {
                extras.push(story.keep(child));
            }
        }
        if let Some(docx) = self.doc.carry.docx.as_mut() {
            docx.document_extras = extras;
        }
        self.doc.sections = sections;
        self.finish_story(story, true);
        Ok(())
    }

    pub(super) fn read_section(&mut self, element: &Element, story: &mut Story) -> Section {
        let mut section = Section::default();
        section.preserved.attributes = element
            .attributes
            .iter()
            .map(|(name, value)| Attribute::new(name, value))
            .collect();
        for child in element.elements() {
            if !self.read_section_child(child, story, &mut section) {
                section.preserved.children.push(story.keep(child));
            }
        }
        section
    }

    fn read_section_child(
        &mut self,
        child: &Element,
        story: &mut Story,
        section: &mut Section,
    ) -> bool {
        match child.name.as_str() {
            "w:headerReference" | "w:footerReference" => {
                self.read_header_reference(child, story, section)
            }
            "w:type" => {
                section.start = match child.attr("w:val") {
                    Some("continuous") => SectionStart::Continuous,
                    Some("evenPage") => SectionStart::EvenPage,
                    Some("oddPage") => SectionStart::OddPage,
                    Some("nextColumn") => SectionStart::NextColumn,
                    _ => SectionStart::NextPage,
                };
                true
            }
            "w:pgSz" if only_attributes(child, &["w:w", "w:h", "w:orient", "w:code"]) => {
                let page = &mut section.page;
                page.width = twips(child, "w:w").unwrap_or(page.width);
                page.height = twips(child, "w:h").unwrap_or(page.height);
                page.orientation = match child.attr("w:orient") {
                    Some("landscape") => Orientation::Landscape,
                    _ => Orientation::Portrait,
                };
                true
            }
            "w:pgMar" => {
                let margins = &mut section.page.margins;
                for (slot, name) in [
                    (&mut margins.top, "w:top"),
                    (&mut margins.right, "w:right"),
                    (&mut margins.bottom, "w:bottom"),
                    (&mut margins.left, "w:left"),
                    (&mut margins.header, "w:header"),
                    (&mut margins.footer, "w:footer"),
                    (&mut margins.gutter, "w:gutter"),
                ] {
                    *slot = twips(child, name).unwrap_or(0);
                }
                true
            }
            "w:cols"
                if child.elements().next().is_none()
                    && only_attributes(child, &["w:num", "w:space", "w:sep"]) =>
            {
                section.columns.count = number(child, "w:num").unwrap_or(1).max(1);
                section.columns.space = twips(child, "w:space").unwrap_or(720);
                section.columns.separator = child.attr("w:sep").is_some_and(|v| v == "1");
                true
            }
            "w:titlePg" => {
                section.different_first_page = on_off(child);
                true
            }
            "w:pgNumType" if only_attributes(child, &["w:start"]) => {
                section.page_number_start = number(child, "w:start");
                true
            }
            _ => false,
        }
    }

    fn read_header_reference(
        &mut self,
        element: &Element,
        story: &Story,
        section: &mut Section,
    ) -> bool {
        let is_header = element.name == "w:headerReference";
        let Some(rel) = element.attr("r:id").and_then(|id| story.rel(id)).cloned() else {
            return false;
        };
        let expected = if is_header { KIND_HEADER } else { KIND_FOOTER };
        if relationship_kind(&rel.kind) != expected {
            return false;
        }
        let path = rels::target_path(&story.part, &rel);
        let kind = if is_header {
            HeaderFooterKind::Header
        } else {
            HeaderFooterKind::Footer
        };
        let Ok(Some(id)) = self.read_header_part(&path, kind) else {
            return false;
        };
        let refs: &mut HeaderFooterRefs = if is_header {
            &mut section.headers
        } else {
            &mut section.footers
        };
        match element.attr("w:type") {
            Some("first") => refs.first = Some(id),
            Some("even") => refs.even = Some(id),
            _ => refs.default = Some(id),
        }
        true
    }

    /// reads a header or footer part once, however many sections use it.
    /// its id is its file name without the extension ("header1").
    fn read_header_part(
        &mut self,
        path: &str,
        kind: HeaderFooterKind,
    ) -> Result<Option<String>, OfficeError> {
        if let Some(id) = self.headers_by_path.get(&path.to_ascii_lowercase()) {
            return Ok(Some(id.clone()));
        }
        let Some(xml) = self.part_xml(path)? else {
            return Ok(None);
        };
        let stem = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".xml")
            .to_string();
        let mut id = stem.clone();
        let mut counter = 1;
        while self.doc.header_footer(&id).is_some() {
            counter += 1;
            id = format!("{stem}-{counter}");
        }
        self.headers_by_path
            .insert(path.to_ascii_lowercase(), id.clone());
        self.consume(path);
        self.record_part_name(&format!("header:{id}"), path);
        let mut story = Story::new(path, &xml, rels::read(self.package, path)?);
        let mut ignored_sections = Vec::new();
        let blocks = self.read_blocks(&xml.root, &mut story, &mut ignored_sections);
        self.finish_story(story, true);
        self.doc.headers_footers.push(HeaderFooter {
            id: id.clone(),
            kind,
            blocks,
        });
        Ok(Some(id))
    }
}
