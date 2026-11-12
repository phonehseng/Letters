//! styles.xml: the named styles, list styles, page layouts and master pages.

use super::*;

impl OdtWriter<'_> {
    // styles.xml ----------------------------------------------------------------

    pub(super) fn styles_xml(&mut self) -> String {
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
        xml.raw(&self.definitions.xml());
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

    pub(super) fn common_styles(&mut self) -> String {
        let doc = self.doc;
        let mut xml = String::from("<style:default-style style:family=\"paragraph\">");
        xml.push_str(&paragraph_properties(
            &doc.styles.defaults.paragraph,
            Breaks::default(),
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
                body.push_str(&paragraph_properties(
                    &props,
                    Breaks::default(),
                    self.native,
                ));
            }
            body.push_str(&text_properties(&style.run, self.native, &mut self.fonts));
            xml.push_str(&format!("<style:style{attributes}>{body}</style:style>"));
        }
        xml
    }

    pub(super) fn list_styles(&self) -> String {
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
    pub(super) fn master_pages(&mut self) -> (String, String) {
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
}

pub(super) fn inherit(current: &mut HeaderFooterRefs, section: &HeaderFooterRefs) {
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

pub(super) fn page_layout_xml(name: &str, section: &Section, header: bool, footer: bool) -> String {
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
    let columns = crate::odt::table_props::columns_xml(section);
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

pub(super) fn list_level_xml(level: &ListLevel) -> String {
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
pub(super) fn split_label(text: &str) -> (String, usize, String) {
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
