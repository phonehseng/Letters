//! styles: font faces, named and automatic styles, list styles and master pages.

use super::*;

impl OdtReader<'_> {
    // styles --------------------------------------------------------------

    pub(super) fn read_font_faces(&mut self, element: &Element, xml: &XmlDocument) {
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

    pub(super) fn read_styles_part(&mut self, xml: &XmlDocument) {
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

    pub(super) fn read_common_styles(&mut self, element: &Element, xml: &XmlDocument) {
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
                _ => {
                    self.read_drawing_style(child, xml);
                    self.doc.styles.preserved.push(xml.raw(child).to_string());
                }
            }
        }
    }

    pub(super) fn read_automatic_styles(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        in_styles: bool,
    ) {
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

    pub(super) fn read_list_style(&mut self, element: &Element) {
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

    pub(super) fn read_master_page(&mut self, master: &Element, xml: &XmlDocument) {
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
    pub(super) fn style(&self, name: &str) -> Option<(&OdtStyle, bool)> {
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
}

/// a common odt style as a model style. odt encodes spaces in style names
/// as "_20_"; the display name is the readable one.
pub(super) fn model_style(style: &OdtStyle) -> Style {
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

pub(super) fn page_layout(element: &Element) -> Section {
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
        let text_width = section.page.text_width();
        let (count_and_gap, widths) = crate::odt::table_props::read_columns(columns, text_width);
        section.columns = count_and_gap;
        section.column_widths = widths;
    }
    section
}

/// one `text:list-level-style-*` as a list level.
pub(super) fn list_level(element: &Element) -> Option<ListLevel> {
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
