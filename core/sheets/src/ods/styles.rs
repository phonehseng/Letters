//! opendocument cell styles to and from the model's cell styles: font,
//! fill, borders, alignment, wrapping and the data style (number format).

use std::collections::HashMap;

use crate::model::style::{Alignment, Border, BorderEdge, CellStyle, Color, Fill};
use crate::xml::{escape_attribute, Fragment};

/// the style elements of a file by name: cell, column, row, table and
/// data styles, each kept as its xml
#[derive(Default)]
pub struct StyleSheet {
    pub styles: HashMap<String, String>,
    pub data_styles: HashMap<String, String>,
}

impl StyleSheet {
    /// files every style element of a `<office:styles>` or
    /// `<office:automatic-styles>` element
    pub fn add_all(&mut self, container_xml: &str) {
        let Some(container) = Fragment::parse(container_xml) else {
            return;
        };
        for (name, xml) in &container.children {
            let Some(style) = Fragment::parse(xml) else {
                continue;
            };
            let Some(style_name) = style.attribute("style:name").map(str::to_string) else {
                continue;
            };
            if name.ends_with("-style") && name != "default-style" {
                self.data_styles.insert(style_name, xml.clone());
            } else if name == "style" {
                self.styles.insert(style_name, xml.clone());
            }
        }
    }

    /// a property of a style or, failing that, of the styles it is based on
    pub fn property(&self, style: &str, group: &str, name: &str) -> Option<String> {
        let mut current = Some(style.to_string());
        for _ in 0..16 {
            let fragment = Fragment::parse(self.styles.get(current.as_ref()?)?)?;
            if let Some(value) = fragment
                .children
                .iter()
                .filter(|(child, _)| child == group)
                .find_map(|(_, xml)| Fragment::parse(xml)?.attribute(name).map(str::to_string))
            {
                return Some(value);
            }
            if group.is_empty() {
                if let Some(value) = fragment.attribute(name) {
                    return Some(value.to_string());
                }
            }
            current = fragment
                .attribute("style:parent-style-name")
                .map(str::to_string);
        }
        None
    }

    /// the model style for an opendocument cell style
    pub fn cell_style(&self, name: &str) -> CellStyle {
        let get = |group: &str, property: &str| self.property(name, group, property);
        let cell = "table-cell-properties";
        let text = "text-properties";
        let mut style = CellStyle::default();
        if let Some(data) = get("", "style:data-style-name") {
            if let Some(xml) = self.data_styles.get(&data) {
                style.number_format = crate::numfmt::odf::odf_to_excel(xml, &|other| {
                    self.data_styles.get(other).cloned()
                });
            }
        }
        style.font.bold = get(text, "fo:font-weight")
            .is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600));
        style.font.italic =
            get(text, "fo:font-style").is_some_and(|s| s == "italic" || s == "oblique");
        style.font.underline = get(text, "style:text-underline-style")
            .filter(|u| u != "none")
            .map(|_| "single".to_string());
        style.font.strike = get(text, "style:text-line-through-style").is_some_and(|s| s != "none");
        style.font.color = get(text, "fo:color").map(|c| Color::rgb(&c));
        style.font.size = get(text, "fo:font-size").and_then(|s| super::units::points(&s));
        style.font.name = get(text, "style:font-name");
        if let Some(background) = get(cell, "fo:background-color").filter(|c| c != "transparent") {
            style.fill = Fill::solid(Color::rgb(&background));
        }
        style.border = borders(&get);
        style.alignment = Alignment {
            horizontal: get("paragraph-properties", "fo:text-align").and_then(|a| horizontal(&a)),
            vertical: get(cell, "style:vertical-align")
                .filter(|v| v != "automatic")
                .map(|v| if v == "middle" { "center".into() } else { v }),
            wrap_text: get(cell, "fo:wrap-option").is_some_and(|w| w == "wrap"),
            shrink_to_fit: get(cell, "style:shrink-to-fit").is_some_and(|s| s == "true"),
            rotation: get(cell, "style:rotation-angle")
                .and_then(|r| r.parse::<f64>().ok())
                .filter(|r| *r != 0.0)
                .map(|r| r as u32),
            ..Alignment::default()
        };
        style
    }
}

fn horizontal(align: &str) -> Option<String> {
    Some(
        match align {
            "start" | "left" => "left",
            "center" => "center",
            "end" | "right" => "right",
            "justify" => "justify",
            _ => return None,
        }
        .into(),
    )
}

/// "0.06pt solid #000000" as an excel border edge
fn edge(text: &str) -> Option<BorderEdge> {
    if text == "none" || text.is_empty() {
        return None;
    }
    let mut width = 0.75;
    let mut kind = "solid";
    let mut color = None;
    for piece in text.split_whitespace() {
        if let Some(points) = super::units::points(piece) {
            width = points;
        } else if piece.starts_with('#') {
            color = Some(Color::rgb(piece));
        } else {
            kind = match piece {
                "double" => "double",
                "dashed" => "dashed",
                "dotted" => "dotted",
                _ => "solid",
            };
        }
    }
    // excel's border weights: hairline to 1 point is thin, to 2 medium
    let style = match (kind, width) {
        ("double", _) => "double",
        ("dashed", _) => "dashed",
        ("dotted", _) => "dotted",
        (_, w) if w <= 1.0 => "thin",
        (_, w) if w <= 2.0 => "medium",
        _ => "thick",
    };
    Some(BorderEdge {
        style: style.into(),
        color,
    })
}

fn borders(get: &dyn Fn(&str, &str) -> Option<String>) -> Border {
    let cell = "table-cell-properties";
    let all = get(cell, "fo:border").and_then(|b| edge(&b));
    let side = |name: &str| {
        get(cell, name)
            .and_then(|b| edge(&b))
            .or_else(|| all.clone())
    };
    Border {
        left: side("fo:border-left"),
        right: side("fo:border-right"),
        top: side("fo:border-top"),
        bottom: side("fo:border-bottom"),
        ..Border::default()
    }
}

fn hex(color: &Color, theme: &[String]) -> Option<String> {
    color.to_css(theme).map(|c| c.to_ascii_lowercase())
}

fn edge_text(edge: &Option<BorderEdge>, theme: &[String]) -> String {
    match edge {
        None => "none".into(),
        Some(edge) => {
            let (width, kind) = match edge.style.as_str() {
                "medium" => ("1.5pt", "solid"),
                "thick" => ("2.5pt", "solid"),
                "double" => ("2.5pt", "double"),
                "dashed" | "mediumDashed" => ("0.75pt", "dashed"),
                "dotted" | "hair" => ("0.75pt", "dotted"),
                _ => ("0.75pt", "solid"),
            };
            let color = edge
                .color
                .as_ref()
                .and_then(|c| hex(c, theme))
                .unwrap_or_else(|| "#000000".into());
            format!("{width} {kind} {color}")
        }
    }
}

/// a model style as an automatic cell style element
pub fn cell_style_xml(
    name: &str,
    data_style: Option<&str>,
    style: &CellStyle,
    theme: &[String],
) -> String {
    let mut cell = Vec::new();
    if let Some(background) = style.fill.background().and_then(|c| hex(c, theme)) {
        cell.push(format!("fo:background-color=\"{background}\""));
    }
    let border = &style.border;
    if border.left.is_some()
        || border.right.is_some()
        || border.top.is_some()
        || border.bottom.is_some()
    {
        for (side, edge) in [
            ("left", &border.left),
            ("right", &border.right),
            ("top", &border.top),
            ("bottom", &border.bottom),
        ] {
            cell.push(format!("fo:border-{side}=\"{}\"", edge_text(edge, theme)));
        }
    }
    if style.alignment.wrap_text {
        cell.push("fo:wrap-option=\"wrap\"".into());
    }
    if let Some(vertical) = &style.alignment.vertical {
        let value = if vertical == "center" {
            "middle"
        } else {
            vertical.as_str()
        };
        cell.push(format!("style:vertical-align=\"{value}\""));
    }
    if let Some(rotation) = style.alignment.rotation {
        cell.push(format!("style:rotation-angle=\"{rotation}\""));
    }
    let mut text = Vec::new();
    let font = &style.font;
    if font.bold {
        text.push("fo:font-weight=\"bold\"".to_string());
    }
    if font.italic {
        text.push("fo:font-style=\"italic\"".into());
    }
    if font.underline.is_some() {
        text.push("style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"".into());
    }
    if font.strike {
        text.push("style:text-line-through-style=\"solid\"".into());
    }
    if let Some(color) = font.color.as_ref().and_then(|c| hex(c, theme)) {
        text.push(format!("fo:color=\"{color}\""));
    }
    if let Some(size) = font.size {
        text.push(format!("fo:font-size=\"{size}pt\""));
    }
    if let Some(family) = &font.name {
        text.push(format!("style:font-name=\"{}\"", escape_attribute(family)));
    }
    let data = data_style
        .map(|d| format!(" style:data-style-name=\"{d}\""))
        .unwrap_or_default();
    let mut out = format!("<style:style style:name=\"{name}\" style:family=\"table-cell\" style:parent-style-name=\"Default\"{data}>");
    if !cell.is_empty() {
        out.push_str(&format!(
            "<style:table-cell-properties {}/>",
            cell.join(" ")
        ));
    }
    if let Some(align) = &style.alignment.horizontal {
        let value = match align.as_str() {
            "left" => "start",
            "right" => "end",
            "center" | "centerContinuous" => "center",
            "justify" => "justify",
            _ => "start",
        };
        out.push_str(&format!(
            "<style:paragraph-properties fo:text-align=\"{value}\"/>"
        ));
    }
    if !text.is_empty() {
        out.push_str(&format!("<style:text-properties {}/>", text.join(" ")));
    }
    out.push_str("</style:style>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_survive_both_ways() {
        let mut style = CellStyle::default();
        style.font.bold = true;
        style.font.color = Some(Color::rgb("FF0000"));
        style.fill = Fill::solid(Color::rgb("FFFF00"));
        style.alignment.horizontal = Some("center".into());
        style.alignment.wrap_text = true;
        style.border.bottom = Some(BorderEdge {
            style: "medium".into(),
            color: Some(Color::rgb("000000")),
        });
        let xml = cell_style_xml(
            "ce1",
            None,
            &style,
            &crate::model::style::default_theme_colors(),
        );
        let mut sheet = StyleSheet::default();
        sheet.add_all(&format!(
            "<office:automatic-styles>{xml}</office:automatic-styles>"
        ));
        let back = sheet.cell_style("ce1");
        assert!(back.font.bold && back.alignment.wrap_text);
        assert_eq!(back.alignment.horizontal.as_deref(), Some("center"));
        assert_eq!(back.border.bottom.unwrap().style, "medium");
        assert!(back.border.top.is_none());
        assert_eq!(back.fill.fg.unwrap().rgb.as_deref(), Some("FFFF00"));
    }
}
