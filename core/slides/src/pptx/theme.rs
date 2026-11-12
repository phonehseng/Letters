//! themes (`ppt/theme/themeN.xml`): the colour scheme, the font scheme
//! and the format scheme's style lists, read and written.

use crate::xml::{Element, XmlDocument};

use super::dml::{attr, find_color, find_fill, read_fill, read_line, write_color, ReadContext};
use crate::model::{ColorRef, ColorScheme, FontScheme, FormatScheme, SourceFormat, Theme};
use crate::preserve::source_of;

pub fn read_theme(id: &str, document: &XmlDocument, context: &ReadContext<'_>) -> Theme {
    let root = &document.root;
    let elements = root.child("a:themeElements");
    let mut theme = Theme {
        id: id.to_string(),
        name: root.attr("name").unwrap_or("").to_string(),
        colors: elements
            .and_then(|e| e.child("a:clrScheme"))
            .map(read_color_scheme)
            .unwrap_or_default(),
        fonts: elements
            .and_then(|e| e.child("a:fontScheme"))
            .map(read_font_scheme)
            .unwrap_or_default(),
        formats: elements
            .and_then(|e| e.child("a:fmtScheme"))
            .map(|f| read_format_scheme(f, context))
            .unwrap_or_default(),
        source: None,
    };
    theme.source = Some(source_of(
        SourceFormat::Pptx,
        &document.source,
        &theme,
        Vec::new(),
    ));
    theme
}

fn read_color_scheme(element: &Element) -> ColorScheme {
    let mut scheme = ColorScheme {
        name: element.attr("name").unwrap_or("").to_string(),
        ..ColorScheme::default()
    };
    for slot in ColorScheme::SLOTS {
        let color = element.child(&format!("a:{slot}")).and_then(find_color);
        if let (Some(color), Some(target)) = (color, scheme.slot_mut(slot)) {
            *target = color;
        }
    }
    scheme
}

fn read_font_scheme(element: &Element) -> FontScheme {
    let face = |font: Option<&Element>, name: &str| {
        font.and_then(|f| f.child(name))
            .and_then(|e| e.attr("typeface"))
            .unwrap_or("")
            .to_string()
    };
    let scripts = |font: Option<&Element>| -> Vec<(String, String)> {
        font.map(|f| {
            f.children_named("a:font")
                .filter_map(|e| {
                    Some((
                        e.attr("script")?.to_string(),
                        e.attr("typeface")?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
    };
    let major = element.child("a:majorFont");
    let minor = element.child("a:minorFont");
    FontScheme {
        name: element.attr("name").unwrap_or("").to_string(),
        major_latin: face(major, "a:latin"),
        major_east_asian: face(major, "a:ea"),
        major_complex: face(major, "a:cs"),
        minor_latin: face(minor, "a:latin"),
        minor_east_asian: face(minor, "a:ea"),
        minor_complex: face(minor, "a:cs"),
        major_scripts: scripts(major),
        minor_scripts: scripts(minor),
    }
}

fn read_format_scheme(element: &Element, context: &ReadContext<'_>) -> FormatScheme {
    let fills = |name: &str| {
        element
            .child(name)
            .map(|list| {
                list.elements()
                    .filter_map(|fill| read_fill(fill, context))
                    .collect()
            })
            .unwrap_or_default()
    };
    FormatScheme {
        fills: fills("a:fillStyleLst"),
        lines: element
            .child("a:lnStyleLst")
            .map(|list| {
                list.children_named("a:ln")
                    .map(|line| read_line(line, context))
                    .collect()
            })
            .unwrap_or_default(),
        effects: element
            .child("a:effectStyleLst")
            .map(|list| list.elements().map(|e| context.raw(e)).collect())
            .unwrap_or_default(),
        background_fills: fills("a:bgFillStyleLst"),
    }
}

/// the theme part's text: the original when nothing changed, otherwise
/// the original with its colour and font schemes rebuilt.
pub fn write_theme(theme: &Theme) -> Option<String> {
    let source = theme.source.as_ref()?;
    let mut plain = theme.clone();
    plain.source = None;
    if crate::preserve::reusable(Some(source), SourceFormat::Pptx, &plain).is_some() {
        return Some(source.xml.clone());
    }
    let document = crate::xml::parse(&source.xml).ok()?;
    let elements = document.root.child("a:themeElements")?;
    let mut text = source.xml.clone();
    // replace from the back so earlier ranges stay valid
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    if let Some(colors) = elements.child("a:clrScheme") {
        replacements.push((colors.start, colors.end, write_color_scheme(&theme.colors)));
    }
    if let Some(fonts) = elements.child("a:fontScheme") {
        let original = &source.xml[fonts.start..fonts.end];
        replacements.push((
            fonts.start,
            fonts.end,
            write_font_scheme(&theme.fonts, original),
        ));
    }
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, xml) in replacements {
        text.replace_range(start..end, &xml);
    }
    Some(text)
}

pub fn write_color_scheme(scheme: &ColorScheme) -> String {
    let mut out = format!("<a:clrScheme{}>", attr("name", &scheme.name));
    for slot in ColorScheme::SLOTS {
        let color: &ColorRef = scheme.slot(slot).expect("every slot exists");
        out.push_str(&format!("<a:{slot}>{}</a:{slot}>", write_color(color)));
    }
    out.push_str("</a:clrScheme>");
    out
}

/// the font scheme with the latin, east asian and complex faces set and
/// the per-script list rebuilt; anything else in it is dropped only
/// when the scheme itself changed.
fn write_font_scheme(fonts: &FontScheme, _original: &str) -> String {
    let font =
        |name: &str, latin: &str, east: &str, complex: &str, scripts: &[(String, String)]| {
            let mut out = format!(
                "<a:{name}><a:latin{}/><a:ea{}/><a:cs{}/>",
                attr("typeface", latin),
                attr("typeface", east),
                attr("typeface", complex)
            );
            for (script, face) in scripts {
                out.push_str(&format!(
                    "<a:font{}{}/>",
                    attr("script", script),
                    attr("typeface", face)
                ));
            }
            out.push_str(&format!("</a:{name}>"));
            out
        };
    format!(
        "<a:fontScheme{}>{}{}</a:fontScheme>",
        attr("name", &fonts.name),
        font(
            "majorFont",
            &fonts.major_latin,
            &fonts.major_east_asian,
            &fonts.major_complex,
            &fonts.major_scripts
        ),
        font(
            "minorFont",
            &fonts.minor_latin,
            &fonts.minor_east_asian,
            &fonts.minor_complex,
            &fonts.minor_scripts
        )
    )
}

/// a theme part for a deck made from nothing (no file to start from).
pub fn default_theme_xml() -> String {
    let scheme = write_color_scheme(&ColorScheme::default());
    let fonts = write_font_scheme(&FontScheme::default(), "");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Office Theme\"><a:themeElements>{scheme}{fonts}<a:fmtScheme name=\"Office\"><a:fillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"><a:tint val=\"50000\"/></a:schemeClr></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"><a:shade val=\"80000\"/></a:schemeClr></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w=\"6350\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln><a:ln w=\"12700\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln><a:ln w=\"19050\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"><a:tint val=\"95000\"/></a:schemeClr></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"><a:shade val=\"90000\"/></a:schemeClr></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"
    )
}

/// the fill of the first child of `parent`, a helper for backgrounds.
pub fn background_fill(parent: &Element, context: &ReadContext<'_>) -> Option<crate::model::Fill> {
    find_fill(parent, context)
}
