//! paragraph, run and section properties, written in schema order.

use crate::model::{
    Alignment, Border, CellBorders, LineRule, ParagraphProps, RunProps, Script, Section,
    SectionStart, TabKind, TabLeader,
};
use crate::xml::{escape_attribute, XmlWriter};

use super::{DocxWriter, StoryOut};
use crate::docx::names::{KIND_FOOTER, KIND_HEADER};
use crate::docx::order::{self, Ordered};

/// `<name a="1" b="2"/>` as a string.
pub fn empty(name: &str, attributes: &[(&str, &str)]) -> String {
    let mut xml = XmlWriter::new();
    xml.empty(name, attributes);
    xml.finish()
}

pub fn val(name: &str, value: &str) -> String {
    empty(name, &[("w:val", value)])
}

pub fn toggle(name: &str, on: bool) -> String {
    if on {
        empty(name, &[])
    } else {
        val(name, "0")
    }
}

pub fn alignment_value(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Left => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
        Alignment::Justify => "both",
    }
}

impl DocxWriter<'_> {
    /// the children of a `w:pPr`, sorted. `extra` holds elements the
    /// caller adds (a section, an outline level for a heading).
    pub(crate) fn paragraph_props(
        &self,
        style_id: Option<&str>,
        props: &ParagraphProps,
        extra: Vec<(&str, String)>,
    ) -> String {
        let mut ordered = Ordered::new(order::PARAGRAPH);
        if let Some(id) = style_id {
            ordered.push("w:pStyle", val("w:pStyle", id));
        }
        if let Some(on) = props.keep_with_next {
            ordered.push("w:keepNext", toggle("w:keepNext", on));
        }
        if let Some(on) = props.keep_lines_together {
            ordered.push("w:keepLines", toggle("w:keepLines", on));
        }
        if let Some(on) = props.page_break_before {
            ordered.push("w:pageBreakBefore", toggle("w:pageBreakBefore", on));
        }
        if let Some(list) = &props.list {
            let id = self
                .list_ids
                .get(&list.list_id)
                .cloned()
                .unwrap_or_else(|| "0".into());
            ordered.push(
                "w:numPr",
                format!(
                    "<w:numPr>{}{}</w:numPr>",
                    val("w:ilvl", &list.level.to_string()),
                    val("w:numId", &id)
                ),
            );
        }
        if !props.tabs.is_empty() {
            ordered.push("w:tabs", tabs_xml(props));
        }
        if !props.spacing.is_empty() {
            ordered.push("w:spacing", spacing_xml(props));
        }
        if !props.indent.is_empty() {
            ordered.push("w:ind", indent_xml(props));
        }
        if let Some(alignment) = props.alignment {
            ordered.push("w:jc", val("w:jc", alignment_value(alignment)));
        }
        if let Some(level) = props.outline_level {
            ordered.push("w:outlineLvl", val("w:outlineLvl", &level.to_string()));
        }
        if self.native {
            for xml in &props.preserved {
                ordered.push_raw(xml);
            }
        }
        for (name, xml) in extra {
            ordered.push(name, xml);
        }
        ordered.finish()
    }

    /// the children of a `w:rPr`, sorted.
    pub(crate) fn run_props(&self, props: &RunProps) -> String {
        let mut ordered = Ordered::new(order::RUN);
        if let Some(id) = &props.style_id {
            ordered.push("w:rStyle", val("w:rStyle", id));
        }
        if let Some(font) = &props.font {
            let mut attributes = vec![
                ("w:ascii".to_string(), font.name.clone()),
                ("w:hAnsi".to_string(), font.name.clone()),
            ];
            if let Some(theme) = &font.theme {
                attributes.push(("w:asciiTheme".into(), theme.clone()));
                attributes.push(("w:hAnsiTheme".into(), theme.clone()));
            }
            if self.native {
                for extra in &font.extra {
                    attributes.push((extra.name.clone(), extra.value.clone()));
                }
            }
            let mut xml = XmlWriter::new();
            xml.empty_owned("w:rFonts", &attributes);
            ordered.push("w:rFonts", xml.finish());
        }
        for (name, value) in [
            ("w:b", props.bold),
            ("w:i", props.italic),
            ("w:caps", props.all_caps),
            ("w:smallCaps", props.small_caps),
            ("w:strike", props.strike),
            ("w:dstrike", props.double_strike),
            ("w:vanish", props.hidden),
        ] {
            if let Some(on) = value {
                ordered.push(name, toggle(name, on));
            }
        }
        if let Some(color) = &props.color {
            ordered.push("w:color", val("w:color", color));
        }
        if let Some(size) = props.size {
            ordered.push("w:sz", val("w:sz", &size.to_string()));
        }
        if let Some(highlight) = &props.highlight {
            ordered.push(
                "w:highlight",
                val("w:highlight", crate::compat::word_highlight(highlight)),
            );
        }
        if let Some(underline) = &props.underline {
            ordered.push("w:u", val("w:u", underline));
        }
        if let Some(script) = props.script {
            let value = match script {
                Script::Superscript => "superscript",
                Script::Subscript => "subscript",
                Script::Baseline => "baseline",
            };
            ordered.push("w:vertAlign", val("w:vertAlign", value));
        }
        if let Some(language) = &props.language {
            ordered.push("w:lang", val("w:lang", language));
        }
        if self.native {
            for xml in &props.preserved {
                ordered.push_raw(xml);
            }
        }
        ordered.finish()
    }

    /// a whole `w:sectPr`, with references to its headers and footers.
    pub(crate) fn section_xml(&self, section: &Section, story: &mut StoryOut) -> String {
        let mut ordered = Ordered::new(order::SECTION);
        for (refs, element, kind) in [
            (&section.headers, "w:headerReference", KIND_HEADER),
            (&section.footers, "w:footerReference", KIND_FOOTER),
        ] {
            for (slot, id) in [
                ("default", &refs.default),
                ("first", &refs.first),
                ("even", &refs.even),
            ] {
                let Some(path) = id.as_ref().and_then(|id| self.header_paths.get(id)) else {
                    continue;
                };
                let rel = story.add_part_rel(kind, path);
                ordered.push(element, empty(element, &[("w:type", slot), ("r:id", &rel)]));
            }
        }
        let start = match section.start {
            SectionStart::NextPage => None,
            SectionStart::Continuous => Some("continuous"),
            SectionStart::EvenPage => Some("evenPage"),
            SectionStart::OddPage => Some("oddPage"),
            SectionStart::NextColumn => Some("nextColumn"),
        };
        if let Some(start) = start {
            ordered.push("w:type", val("w:type", start));
        }
        ordered.push("w:pgSz", page_size_xml(section));
        let m = &section.page.margins;
        let margins: Vec<(String, String)> = [
            ("w:top", m.top),
            ("w:right", m.right),
            ("w:bottom", m.bottom),
            ("w:left", m.left),
            ("w:header", m.header),
            ("w:footer", m.footer),
            ("w:gutter", m.gutter),
        ]
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
        let mut xml = XmlWriter::new();
        xml.empty_owned("w:pgMar", &margins);
        ordered.push("w:pgMar", xml.finish());
        if let Some(start) = section.page_number_start {
            ordered.push(
                "w:pgNumType",
                empty("w:pgNumType", &[("w:start", &start.to_string())]),
            );
        }
        ordered.push("w:cols", columns_xml(section));
        if section.different_first_page {
            ordered.push("w:titlePg", empty("w:titlePg", &[]));
        }
        let mut attributes = Vec::new();
        if self.native {
            for xml in &section.preserved.children {
                ordered.push_raw(xml);
            }
            for attribute in &section.preserved.attributes {
                attributes.push((attribute.name.clone(), attribute.value.clone()));
            }
        }
        let mut out = XmlWriter::new();
        out.open_owned("w:sectPr", &attributes);
        out.raw(&ordered.finish());
        out.close("w:sectPr");
        out.finish()
    }
}

fn page_size_xml(section: &Section) -> String {
    let width = section.page.width.to_string();
    let height = section.page.height.to_string();
    let mut attributes = vec![("w:w", width.as_str()), ("w:h", height.as_str())];
    if section.page.orientation == crate::model::Orientation::Landscape {
        attributes.push(("w:orient", "landscape"));
    }
    empty("w:pgSz", &attributes)
}

fn columns_xml(section: &Section) -> String {
    let space = section.columns.space.to_string();
    let count = section.columns.count.to_string();
    let mut attributes = vec![("w:space", space.as_str())];
    if section.columns.count > 1 {
        attributes.push(("w:num", count.as_str()));
    }
    if section.columns.separator {
        attributes.push(("w:sep", "1"));
    }
    empty("w:cols", &attributes)
}

fn tabs_xml(props: &ParagraphProps) -> String {
    let mut xml = String::from("<w:tabs>");
    for stop in &props.tabs {
        let kind = match stop.kind {
            TabKind::Left => "left",
            TabKind::Center => "center",
            TabKind::Right => "right",
            TabKind::Decimal => "decimal",
            TabKind::Bar => "bar",
            TabKind::Clear => "clear",
        };
        let leader = match stop.leader {
            TabLeader::None => None,
            TabLeader::Dot => Some("dot"),
            TabLeader::Hyphen => Some("hyphen"),
            TabLeader::Underscore => Some("underscore"),
            TabLeader::MiddleDot => Some("middleDot"),
            TabLeader::Heavy => Some("heavy"),
        };
        let position = stop.position.to_string();
        let mut attributes = vec![("w:val", kind)];
        if let Some(leader) = leader {
            attributes.push(("w:leader", leader));
        }
        attributes.push(("w:pos", position.as_str()));
        xml.push_str(&empty("w:tab", &attributes));
    }
    xml.push_str("</w:tabs>");
    xml
}

fn spacing_xml(props: &ParagraphProps) -> String {
    let spacing = &props.spacing;
    let mut attributes: Vec<(String, String)> = Vec::new();
    if let Some(before) = spacing.before {
        attributes.push(("w:before".into(), before.to_string()));
    }
    if let Some(after) = spacing.after {
        attributes.push(("w:after".into(), after.to_string()));
    }
    if let Some(line) = spacing.line {
        attributes.push(("w:line".into(), line.value.to_string()));
        let rule = match line.rule {
            LineRule::Auto => "auto",
            LineRule::Exact => "exact",
            LineRule::AtLeast => "atLeast",
        };
        attributes.push(("w:lineRule".into(), rule.into()));
    }
    let mut xml = XmlWriter::new();
    xml.empty_owned("w:spacing", &attributes);
    xml.finish()
}

fn indent_xml(props: &ParagraphProps) -> String {
    let indent = &props.indent;
    let mut attributes: Vec<(String, String)> = Vec::new();
    for (name, value) in [
        ("w:left", indent.left),
        ("w:right", indent.right),
        ("w:firstLine", indent.first_line),
        ("w:hanging", indent.hanging),
    ] {
        if let Some(value) = value {
            attributes.push((name.into(), value.to_string()));
        }
    }
    let mut xml = XmlWriter::new();
    xml.empty_owned("w:ind", &attributes);
    xml.finish()
}

/// `w:tblBorders` or `w:tcBorders`.
pub fn borders_xml(name: &str, borders: &CellBorders) -> String {
    let mut xml = format!("<{name}>");
    for (side, border) in [
        ("w:top", &borders.top),
        ("w:left", &borders.left),
        ("w:bottom", &borders.bottom),
        ("w:right", &borders.right),
        ("w:insideH", &borders.inside_horizontal),
        ("w:insideV", &borders.inside_vertical),
    ] {
        if let Some(border) = border {
            xml.push_str(&border_xml(side, border));
        }
    }
    xml.push_str(&format!("</{name}>"));
    xml
}

fn border_xml(side: &str, border: &Border) -> String {
    format!(
        "<{side} w:val=\"{}\" w:sz=\"{}\" w:space=\"0\" w:color=\"{}\"/>",
        escape_attribute(&border.style),
        border.size,
        escape_attribute(border.color.as_deref().unwrap_or("auto"))
    )
}
