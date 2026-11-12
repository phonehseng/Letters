//! styles.xml and numbering.xml.

use crate::error::OfficeError;
use crate::model::{
    Attribute, LevelOverride, ListDefinition, ListInstance, ListLevel, NumberFormat, Preserved,
    Style, StyleKind,
};
use crate::xml::Element;

use super::props::{alignment, number, read_paragraph_props, read_run_props};
use super::{DocxReader, Story};
use crate::docx::{names, rels};

impl DocxReader<'_> {
    pub(super) fn read_styles(&mut self) -> Result<(), OfficeError> {
        let Some(part) = self.main_target(names::KIND_STYLES) else {
            return Ok(());
        };
        let Some(xml) = self.part_xml(&part)? else {
            return Ok(());
        };
        self.consume(&part);
        self.record_part_name(names::KIND_STYLES, &part);
        let mut story = Story::new(&part, &xml, rels::read(self.package, &part)?);
        for element in xml.root.elements() {
            match element.name.as_str() {
                "w:docDefaults" => self.read_defaults(element, &mut story),
                "w:style" => {
                    let style = self.read_style(element, &mut story);
                    self.doc.styles.styles.push(style);
                }
                _ => {
                    let kept = story.keep(element);
                    self.doc.styles.preserved.push(kept);
                }
            }
        }
        self.finish_story(story, false);
        Ok(())
    }

    fn read_defaults(&mut self, defaults: &Element, story: &mut Story) {
        let xml = story.xml;
        if let Some(rpr) = defaults
            .child("w:rPrDefault")
            .and_then(|d| d.child("w:rPr"))
        {
            self.doc.styles.defaults.run = read_run_props(rpr, xml, &self.theme);
        }
        if let Some(ppr) = defaults
            .child("w:pPrDefault")
            .and_then(|d| d.child("w:pPr"))
        {
            self.doc.styles.defaults.paragraph = read_paragraph_props(ppr, xml).props;
        }
    }

    fn read_style(&self, element: &Element, story: &mut Story) -> Style {
        let kind = match element.attr("w:type") {
            Some("character") => StyleKind::Character,
            Some("table") => StyleKind::Table,
            Some("numbering") => StyleKind::Numbering,
            _ => StyleKind::Paragraph,
        };
        let id = element.attr("w:styleId").unwrap_or_default().to_string();
        let mut style = Style {
            name: id.clone(),
            id,
            kind,
            based_on: None,
            next: None,
            is_default: element
                .attr("w:default")
                .is_some_and(|v| v == "1" || v == "true"),
            paragraph: Default::default(),
            run: Default::default(),
            preserved: Preserved::default(),
        };
        for (name, value) in &element.attributes {
            if !matches!(name.as_str(), "w:type" | "w:styleId" | "w:default") {
                style.preserved.attributes.push(Attribute::new(name, value));
            }
        }
        for child in element.elements() {
            match child.name.as_str() {
                "w:name" => style.name = child.attr("w:val").unwrap_or_default().to_string(),
                "w:basedOn" => style.based_on = child.attr("w:val").map(str::to_string),
                "w:next" => style.next = child.attr("w:val").map(str::to_string),
                // a style's own pPr cannot sensibly name a paragraph style,
                // so a pStyle there is ignored
                "w:pPr" => style.paragraph = read_paragraph_props(child, story.xml).props,
                "w:rPr" => style.run = read_run_props(child, story.xml, &self.theme),
                _ => style.preserved.children.push(story.keep(child)),
            }
        }
        style
    }

    pub(super) fn read_numbering(&mut self) -> Result<(), OfficeError> {
        let Some(part) = self.main_target(names::KIND_NUMBERING) else {
            return Ok(());
        };
        let Some(xml) = self.part_xml(&part)? else {
            return Ok(());
        };
        self.consume(&part);
        self.record_part_name(names::KIND_NUMBERING, &part);
        let mut story = Story::new(&part, &xml, rels::read(self.package, &part)?);
        for element in xml.root.elements() {
            match element.name.as_str() {
                "w:abstractNum" => {
                    let definition = self.read_list_definition(element, &mut story);
                    self.doc.numbering.definitions.push(definition);
                }
                "w:num" => {
                    let instance = read_list_instance(element, &mut story);
                    self.doc.numbering.instances.push(instance);
                }
                _ => {
                    let kept = story.keep(element);
                    self.doc.numbering.preserved.push(kept);
                }
            }
        }
        self.finish_story(story, false);
        Ok(())
    }

    fn read_list_definition(&self, element: &Element, story: &mut Story) -> ListDefinition {
        let mut definition = ListDefinition {
            id: element
                .attr("w:abstractNumId")
                .unwrap_or_default()
                .to_string(),
            name: None,
            levels: Vec::new(),
            preserved: Preserved::default(),
        };
        keep_other_attributes(element, &["w:abstractNumId"], &mut definition.preserved);
        for child in element.elements() {
            match child.name.as_str() {
                "w:lvl" => definition.levels.push(self.read_level(child, story)),
                "w:name" => definition.name = child.attr("w:val").map(str::to_string),
                _ => definition.preserved.children.push(story.keep(child)),
            }
        }
        definition
    }

    fn read_level(&self, element: &Element, story: &mut Story) -> ListLevel {
        let mut level = ListLevel {
            level: number(element, "w:ilvl").unwrap_or(0).min(8) as u8,
            format: NumberFormat::Decimal,
            text: String::new(),
            start: 1,
            alignment: None,
            paragraph: Default::default(),
            run: Default::default(),
            preserved: Preserved::default(),
        };
        keep_other_attributes(element, &["w:ilvl"], &mut level.preserved);
        for child in element.elements() {
            match child.name.as_str() {
                "w:start" => level.start = number(child, "w:val").unwrap_or(1),
                "w:numFmt" if child.attributes.len() == 1 => {
                    level.format = number_format(child.attr("w:val").unwrap_or("decimal"))
                }
                "w:lvlText" if child.attributes.len() <= 1 => {
                    level.text = child.attr("w:val").unwrap_or_default().to_string()
                }
                "w:lvlJc" => match child.attr("w:val").and_then(alignment) {
                    Some(value) => level.alignment = Some(value),
                    None => level.preserved.children.push(story.keep(child)),
                },
                "w:pPr" => level.paragraph = read_paragraph_props(child, story.xml).props,
                "w:rPr" => level.run = read_run_props(child, story.xml, &self.theme),
                _ => level.preserved.children.push(story.keep(child)),
            }
        }
        level
    }
}

fn read_list_instance(element: &Element, story: &mut Story) -> ListInstance {
    let mut instance = ListInstance {
        id: element.attr("w:numId").unwrap_or_default().to_string(),
        definition_id: element
            .child_val("w:abstractNumId")
            .unwrap_or_default()
            .to_string(),
        overrides: Vec::new(),
        preserved: Preserved::default(),
    };
    keep_other_attributes(element, &["w:numId"], &mut instance.preserved);
    for child in element.elements() {
        match child.name.as_str() {
            "w:abstractNumId" => {}
            "w:lvlOverride" => instance.overrides.push(LevelOverride {
                level: number(child, "w:ilvl").unwrap_or(0).min(8) as u8,
                start: child
                    .child("w:startOverride")
                    .and_then(|start| number(start, "w:val")),
                preserved: child.child("w:lvl").map(|lvl| story.keep(lvl)),
            }),
            _ => instance.preserved.children.push(story.keep(child)),
        }
    }
    instance
}

fn keep_other_attributes(element: &Element, known: &[&str], preserved: &mut Preserved) {
    for (name, value) in &element.attributes {
        if !known.contains(&name.as_str()) {
            preserved.attributes.push(Attribute::new(name, value));
        }
    }
}

pub(crate) fn number_format(value: &str) -> NumberFormat {
    match value {
        "bullet" => NumberFormat::Bullet,
        "decimal" => NumberFormat::Decimal,
        "decimalZero" => NumberFormat::DecimalZero,
        "lowerLetter" => NumberFormat::LowerLetter,
        "upperLetter" => NumberFormat::UpperLetter,
        "lowerRoman" => NumberFormat::LowerRoman,
        "upperRoman" => NumberFormat::UpperRoman,
        "none" => NumberFormat::None,
        other => NumberFormat::Other(other.to_string()),
    }
}
