//! one font, fill, border or colour to and from spreadsheetml xml. the
//! style table reader and writer use these for every entry.

use crate::model::style::{Border, BorderEdge, Color, Fill, Font};
use crate::xml::{Fragment, XmlWriter};

/// a colour from the attributes of a `<color>`, `<fgColor>`, `<tabColor>`...
pub fn color_from_attributes(attributes: &[(String, String)]) -> Color {
    let get = |name: &str| {
        attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    Color {
        rgb: get("rgb").map(str::to_string),
        theme: get("theme").and_then(|value| value.parse().ok()),
        tint: get("tint").and_then(|value| value.parse().ok()),
        indexed: get("indexed").and_then(|value| value.parse().ok()),
        auto: get("auto") == Some("1") || get("auto") == Some("true"),
    }
}

pub fn color_attributes(color: &Color) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    if color.auto {
        attributes.push(("auto".to_string(), "1".to_string()));
    }
    if let Some(index) = color.indexed {
        attributes.push(("indexed".to_string(), index.to_string()));
    }
    if let Some(rgb) = &color.rgb {
        // excel wants alpha first: "FFRRGGBB"
        let full = if rgb.len() == 6 {
            format!("FF{rgb}")
        } else {
            rgb.clone()
        };
        attributes.push(("rgb".to_string(), full));
    }
    if let Some(theme) = color.theme {
        attributes.push(("theme".to_string(), theme.to_string()));
    }
    if let Some(tint) = color.tint {
        attributes.push(("tint".to_string(), tint.to_string()));
    }
    attributes
}

/// a child's `val`, or "1" for a bare switch like `<b/>`
fn child_value(xml: &str) -> Option<String> {
    let fragment = Fragment::parse(xml)?;
    Some(fragment.attribute("val").unwrap_or("1").to_string())
}

/// `<b/>` and `<b val="1"/>` are on; `<b val="0"/>` is off
fn is_on(xml: &str) -> bool {
    !matches!(child_value(xml).as_deref(), Some("0") | Some("false"))
}

pub fn parse_font(xml: &str) -> Font {
    let mut font = Font::default();
    let Some(fragment) = Fragment::parse(xml) else {
        return font;
    };
    for (name, child) in &fragment.children {
        let value = child_value(child);
        match name.as_str() {
            "b" => font.bold = is_on(child),
            "i" => font.italic = is_on(child),
            "strike" => font.strike = is_on(child),
            "u" => font.underline = value.map(|v| if v == "1" { "single".to_string() } else { v }),
            "sz" => font.size = value.and_then(|v| v.parse().ok()),
            "name" | "rFont" => font.name = value,
            "family" => font.family = value.and_then(|v| v.parse().ok()),
            "charset" => font.charset = value.and_then(|v| v.parse().ok()),
            "scheme" => font.scheme = value,
            "vertAlign" => font.vert_align = value,
            "color" => {
                font.color = Fragment::parse(child).map(|f| color_from_attributes(&f.attributes))
            }
            _ => {}
        }
    }
    font
}

/// excel's own default font, written when a style leaves the font unset
pub fn default_font() -> Font {
    Font {
        name: Some("Calibri".into()),
        size: Some(11.0),
        family: Some(2),
        scheme: Some("minor".into()),
        ..Font::default()
    }
}

/// a font in the child order the schema requires
pub fn write_font(font: &Font, element: &str) -> String {
    let font = if *font == Font::default() {
        default_font()
    } else {
        font.clone()
    };
    let mut writer = XmlWriter::new();
    writer.open(element, &[]);
    if font.bold {
        writer.empty("b", &[]);
    }
    if font.italic {
        writer.empty("i", &[]);
    }
    if font.strike {
        writer.empty("strike", &[]);
    }
    if let Some(align) = &font.vert_align {
        writer.empty("vertAlign", &[("val", align)]);
    }
    if let Some(size) = font.size {
        writer.empty("sz", &[("val", &size.to_string())]);
    }
    if let Some(color) = &font.color {
        writer.empty_owned("color", &color_attributes(color));
    }
    if let Some(name) = &font.name {
        writer.empty(
            if element == "rPr" { "rFont" } else { "name" },
            &[("val", name)],
        );
    }
    if let Some(family) = font.family {
        writer.empty("family", &[("val", &family.to_string())]);
    }
    if let Some(charset) = font.charset {
        writer.empty("charset", &[("val", &charset.to_string())]);
    }
    if let Some(underline) = &font.underline {
        let value = if underline == "single" {
            vec![]
        } else {
            vec![("val", underline.as_str())]
        };
        writer.empty("u", &value);
    }
    if let Some(scheme) = &font.scheme {
        writer.empty("scheme", &[("val", scheme)]);
    }
    writer.close(element);
    writer.finish()
}

pub fn parse_fill(xml: &str) -> Fill {
    let mut fill = Fill::default();
    let Some(fragment) = Fragment::parse(xml) else {
        return fill;
    };
    for (name, child) in &fragment.children {
        if name == "gradientFill" {
            fill.gradient_xml = Some(child.clone());
            continue;
        }
        let Some(pattern) = Fragment::parse(child) else {
            continue;
        };
        fill.pattern = pattern.attribute("patternType").map(str::to_string);
        for (part, part_xml) in &pattern.children {
            let color = Fragment::parse(part_xml).map(|f| color_from_attributes(&f.attributes));
            match part.as_str() {
                "fgColor" => fill.fg = color,
                "bgColor" => fill.bg = color,
                _ => {}
            }
        }
    }
    fill
}

pub fn write_fill(fill: &Fill) -> String {
    if let Some(gradient) = &fill.gradient_xml {
        return format!("<fill>{gradient}</fill>");
    }
    let mut writer = XmlWriter::new();
    writer.open("fill", &[]);
    let pattern = fill.pattern.clone().unwrap_or_else(|| "none".into());
    if fill.fg.is_none() && fill.bg.is_none() {
        writer.empty("patternFill", &[("patternType", &pattern)]);
    } else {
        writer.open("patternFill", &[("patternType", &pattern)]);
        if let Some(fg) = &fill.fg {
            writer.empty_owned("fgColor", &color_attributes(fg));
        }
        if let Some(bg) = &fill.bg {
            writer.empty_owned("bgColor", &color_attributes(bg));
        }
        writer.close("patternFill");
    }
    writer.close("fill");
    writer.finish()
}

fn parse_edge(xml: &str) -> Option<BorderEdge> {
    let fragment = Fragment::parse(xml)?;
    let style = fragment.attribute("style")?.to_string();
    let color = fragment
        .children
        .iter()
        .find(|(name, _)| name == "color")
        .and_then(|(_, child)| Fragment::parse(child))
        .map(|f| color_from_attributes(&f.attributes));
    Some(BorderEdge { style, color })
}

pub fn parse_border(xml: &str) -> Border {
    let mut border = Border::default();
    let Some(fragment) = Fragment::parse(xml) else {
        return border;
    };
    border.diagonal_up = fragment.attribute("diagonalUp") == Some("1");
    border.diagonal_down = fragment.attribute("diagonalDown") == Some("1");
    for (name, child) in &fragment.children {
        let edge = parse_edge(child);
        match name.as_str() {
            "left" | "start" => border.left = edge,
            "right" | "end" => border.right = edge,
            "top" => border.top = edge,
            "bottom" => border.bottom = edge,
            "diagonal" => border.diagonal = edge,
            _ => {}
        }
    }
    border
}

pub fn write_border(border: &Border) -> String {
    let mut attributes = Vec::new();
    if border.diagonal_up {
        attributes.push(("diagonalUp".to_string(), "1".to_string()));
    }
    if border.diagonal_down {
        attributes.push(("diagonalDown".to_string(), "1".to_string()));
    }
    let mut writer = XmlWriter::new();
    writer.open_owned("border", &attributes);
    // the schema's order: left, right, top, bottom, diagonal
    let edges = [
        ("left", &border.left),
        ("right", &border.right),
        ("top", &border.top),
        ("bottom", &border.bottom),
        ("diagonal", &border.diagonal),
    ];
    for (name, edge) in edges {
        match edge {
            None => writer.empty(name, &[]),
            Some(edge) => {
                writer.open(name, &[("style", &edge.style)]);
                if let Some(color) = &edge.color {
                    writer.empty_owned("color", &color_attributes(color));
                }
                writer.close(name);
            }
        }
    }
    writer.close("border");
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_fills_and_borders_survive_a_round_trip() {
        let font = parse_font(
            r#"<font><b/><i val="0"/><sz val="12"/><color theme="1"/><name val="Arial"/><u/></font>"#,
        );
        assert!(font.bold && !font.italic);
        assert_eq!(font.size, Some(12.0));
        assert_eq!(font.underline.as_deref(), Some("single"));
        assert_eq!(parse_font(&write_font(&font, "font")), font);
        let fill = parse_fill(
            r#"<fill><patternFill patternType="solid"><fgColor rgb="FFFFFF00"/><bgColor indexed="64"/></patternFill></fill>"#,
        );
        assert_eq!(fill.fg.as_ref().unwrap().rgb.as_deref(), Some("FFFFFF00"));
        assert_eq!(parse_fill(&write_fill(&fill)), fill);
        let border = parse_border(
            r#"<border><left style="thin"><color auto="1"/></left><right/><top/><bottom style="double"/><diagonal/></border>"#,
        );
        assert_eq!(border.bottom.as_ref().unwrap().style, "double");
        assert_eq!(parse_border(&write_border(&border)), border);
    }
}
