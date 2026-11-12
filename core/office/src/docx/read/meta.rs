//! the theme's fonts, the settings the model uses and the core properties.

use crate::error::OfficeError;
use crate::model::Metadata;
use crate::package::resolve_target;
use crate::xml::Element;

use super::props::{on_off, twips};
use super::DocxReader;
use crate::docx::names::{self, KIND_CORE};
use crate::docx::rels;

impl DocxReader<'_> {
    /// the theme stays in the package as it is; only its two fonts are read.
    pub(super) fn read_theme(&mut self) -> Result<(), OfficeError> {
        let Some(part) = self.main_target(names::KIND_THEME) else {
            return Ok(());
        };
        let Some(xml) = self.part_xml(&part)? else {
            return Ok(());
        };
        let mut major = None;
        let mut minor = None;
        xml.root.walk(&mut |element| {
            let slot = match element.name.as_str() {
                "a:majorFont" => &mut major,
                "a:minorFont" => &mut minor,
                _ => return,
            };
            if let Some(latin) = element.child("a:latin") {
                *slot = latin.attr("typeface").map(str::to_string);
            }
        });
        self.theme.major = major;
        self.theme.minor = minor;
        self.palette = crate::docx::drawingml::Palette::from_theme(&xml.root);
        self.doc.styles.theme_fonts = self.theme.clone();
        Ok(())
    }

    /// settings.xml is carried as it is (it holds dozens of compatibility
    /// switches); the writer only updates the few values the model owns.
    pub(super) fn read_settings(&mut self) -> Result<(), OfficeError> {
        let Some(part) = self.main_target(names::KIND_SETTINGS) else {
            return Ok(());
        };
        let Some(xml) = self.part_xml(&part)? else {
            return Ok(());
        };
        self.record_part_name(names::KIND_SETTINGS, &part);
        let settings = &mut self.doc.settings;
        if let Some(element) = xml.root.child("w:evenAndOddHeaders") {
            settings.even_and_odd_headers = on_off(element);
        }
        if let Some(element) = xml.root.child("w:trackRevisions") {
            settings.track_changes = on_off(element);
        }
        settings.default_tab_stop = xml
            .root
            .child("w:defaultTabStop")
            .and_then(|element| twips(element, "w:val"));
        Ok(())
    }

    pub(super) fn read_core_properties(&mut self) -> Result<(), OfficeError> {
        let package_rels = self
            .doc
            .carry
            .docx
            .as_ref()
            .map(|docx| docx.package_relationships.clone())
            .unwrap_or_default();
        let part = rels::find(&package_rels, KIND_CORE)
            .map(|rel| resolve_target("", &rel.target))
            .unwrap_or_else(|| "docProps/core.xml".to_string());
        let Some(xml) = self.part_xml(&part)? else {
            return Ok(());
        };
        self.consume(&part);
        self.record_part_name(KIND_CORE, &part);
        let mut extras = Vec::new();
        for element in xml.root.elements() {
            if !read_core_property(element, &mut self.doc.metadata) {
                extras.push(xml.raw(element).to_string());
            }
        }
        if let Some(docx) = self.doc.carry.docx.as_mut() {
            docx.core_extras = extras;
        }
        Ok(())
    }
}

fn read_core_property(element: &Element, metadata: &mut Metadata) -> bool {
    let slot = match element.name.as_str() {
        "dc:title" => &mut metadata.title,
        "dc:subject" => &mut metadata.subject,
        "dc:creator" => &mut metadata.author,
        "dc:description" => &mut metadata.description,
        "cp:keywords" => &mut metadata.keywords,
        "cp:lastModifiedBy" => &mut metadata.last_modified_by,
        "dcterms:created" => &mut metadata.created,
        "dcterms:modified" => &mut metadata.modified,
        "dc:language" => &mut metadata.language,
        _ => return false,
    };
    // keywords may hold child elements in rare files; keep those whole
    if element.elements().next().is_some() {
        return false;
    }
    *slot = Some(element.text());
    true
}
