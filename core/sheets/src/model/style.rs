//! cell formatting: number format, font, fill, borders, alignment and
//! protection, kept once in a table and shared by index.

use serde::{Deserialize, Serialize};

/// a colour as files give it: an rgb value, a theme slot with an
/// optional tint, or one of the old indexed colours.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Color {
    /// "FF0000" or with alpha "FFFF0000" as excel writes it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto: bool,
}

impl Color {
    pub fn rgb(hex: &str) -> Color {
        Color {
            rgb: Some(hex.trim_start_matches('#').to_ascii_uppercase()),
            ..Color::default()
        }
    }

    /// "#RRGGBB" for drawing, given the workbook's theme colours.
    pub fn to_css(&self, theme: &[String]) -> Option<String> {
        let base = if let Some(rgb) = &self.rgb {
            let hex = if rgb.len() == 8 {
                &rgb[2..]
            } else {
                rgb.as_str()
            };
            hex.to_string()
        } else if let Some(slot) = self.theme {
            theme.get(slot as usize)?.clone()
        } else {
            indexed_color(self.indexed?)?.to_string()
        };
        let channel =
            |i: usize| u8::from_str_radix(base.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
        let (mut r, mut g, mut b) = (channel(0), channel(2), channel(4));
        if let Some(tint) = self.tint {
            // excel tints in hsl lightness; the rgb mix below is close
            // enough for a screen
            let apply = |c: u8| -> u8 {
                let c = f64::from(c);
                let mixed = if tint < 0.0 {
                    c * (1.0 + tint)
                } else {
                    c + (255.0 - c) * tint
                };
                mixed.round().clamp(0.0, 255.0) as u8
            };
            r = apply(r);
            g = apply(g);
            b = apply(b);
        }
        Some(format!("#{r:02X}{g:02X}{b:02X}"))
    }
}

/// the first colours of excel's legacy palette.
fn indexed_color(index: u32) -> Option<&'static str> {
    const PALETTE: [&str; 64] = [
        "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF", "000000",
        "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF", "800000", "008000",
        "000080", "808000", "800080", "008080", "C0C0C0", "808080", "9999FF", "993366", "FFFFCC",
        "CCFFFF", "660066", "FF8080", "0066CC", "CCCCFF", "000080", "FF00FF", "FFFF00", "00FFFF",
        "800080", "800000", "008080", "0000FF", "00CCFF", "CCFFFF", "CCFFCC", "FFFF99", "99CCFF",
        "FF99CC", "CC99FF", "FFCC99", "3366FF", "33CCCC", "99CC00", "FFCC00", "FF9900", "FF6600",
        "666699", "969696", "003366", "339966", "003300", "333300", "993300", "993366", "333399",
        "333333",
    ];
    PALETTE.get(index as usize).copied()
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Font {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// points
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// "single", "double", "singleAccounting", "doubleAccounting"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strike: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// "superscript" or "subscript"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vert_align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<u32>,
    /// "minor" or "major": the theme's body or heading font
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    /// "none", "solid", "gray125" and the other excel patterns
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color>,
    /// a gradient fill kept as its original xml
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_xml: Option<String>,
}

impl Fill {
    /// a plain background colour
    pub fn solid(color: Color) -> Fill {
        Fill {
            pattern: Some("solid".into()),
            fg: Some(color),
            bg: None,
            gradient_xml: None,
        }
    }

    /// the colour a screen shows, if any
    pub fn background(&self) -> Option<&Color> {
        match self.pattern.as_deref() {
            Some("solid") => self.fg.as_ref(),
            Some("none") | None => None,
            Some(_) => self.fg.as_ref().or(self.bg.as_ref()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BorderEdge {
    /// "thin", "medium", "thick", "dashed", "dotted", "double", "hair"...
    pub style: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Border {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<BorderEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<BorderEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<BorderEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<BorderEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal: Option<BorderEdge>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub diagonal_up: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub diagonal_down: bool,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alignment {
    /// "left", "center", "right", "fill", "justify", "centerContinuous", "distributed"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<String>,
    /// "top", "center", "bottom", "justify", "distributed"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wrap_text: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shrink_to_fit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<u32>,
    /// degrees, 0-180 as excel stores it (255 is vertical text)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading_order: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Protection {
    pub locked: bool,
    pub hidden: bool,
}

impl Default for Protection {
    fn default() -> Protection {
        // excel's default: every cell is locked once the sheet is protected
        Protection {
            locked: true,
            hidden: false,
        }
    }
}

/// everything that decides how one cell looks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellStyle {
    /// an excel format code such as "#,##0.00" or "General"
    pub number_format: String,
    pub font: Font,
    pub fill: Fill,
    pub border: Border,
    pub alignment: Alignment,
    pub protection: Protection,
    /// the named cell style this one is based on (excel's xfId), kept
    /// for round trips
    #[serde(default)]
    pub named_style: u32,
}

impl Default for CellStyle {
    fn default() -> CellStyle {
        CellStyle {
            number_format: "General".into(),
            font: Font::default(),
            fill: Fill::default(),
            border: Border::default(),
            alignment: Alignment::default(),
            protection: Protection::default(),
            named_style: 0,
        }
    }
}

/// a font, fill or border exactly as the file had it, so the file's own
/// numbering of them (which named styles refer to) survives a save.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginalStyleParts {
    #[serde(default)]
    pub fonts: Vec<(Font, String)>,
    #[serde(default)]
    pub fills: Vec<(Fill, String)>,
    #[serde(default)]
    pub borders: Vec<(Border, String)>,
    /// number format codes with the ids the file gave them
    #[serde(default)]
    pub number_formats: Vec<(u32, String)>,
    /// the default font for the workbook, from the file
    #[serde(default)]
    pub default_font: Option<Font>,
}

/// the workbook's styles, each stored once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleTable {
    /// style 0 is always the default
    pub styles: Vec<CellStyle>,
    /// differential formats (what a conditional format or table style
    /// changes), kept as their original xml and referred to by index
    #[serde(default)]
    pub differential: Vec<String>,
    /// the theme's colours by the index cells use, as "RRGGBB". excel
    /// swaps the first two pairs of the theme's own order: index 0 is
    /// light 1, 1 is dark 1, 2 is light 2, 3 is dark 2, then accents 1-6,
    /// link and followed link (checked against excel: theme="1" text is black)
    #[serde(default)]
    pub theme_colors: Vec<String>,
    #[serde(default)]
    pub original: OriginalStyleParts,
}

impl Default for StyleTable {
    fn default() -> StyleTable {
        StyleTable {
            styles: vec![CellStyle::default()],
            differential: Vec::new(),
            theme_colors: default_theme_colors(),
            original: OriginalStyleParts::default(),
        }
    }
}

/// the office 2013+ theme, used when a file has none.
pub fn default_theme_colors() -> Vec<String> {
    [
        "FFFFFF", "000000", "E7E6E6", "44546A", "4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5",
        "70AD47", "0563C1", "954F72",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

impl StyleTable {
    /// the index of a style equal to `style`, adding it when new.
    pub fn intern(&mut self, style: CellStyle) -> u32 {
        if let Some(index) = self.styles.iter().position(|known| *known == style) {
            return index as u32;
        }
        self.styles.push(style);
        (self.styles.len() - 1) as u32
    }

    /// the style at an index, the default for an unknown one.
    pub fn get(&self, index: u32) -> &CellStyle {
        self.styles.get(index as usize).unwrap_or(&self.styles[0])
    }

    /// a copy of style `base` changed by `change`, interned.
    pub fn derive(&mut self, base: u32, change: impl FnOnce(&mut CellStyle)) -> u32 {
        let mut style = self.get(base).clone();
        change(&mut style);
        self.intern(style)
    }

    /// adds a differential format that sets a font colour and a fill, as
    /// conditional formats use.
    pub fn add_differential(
        &mut self,
        font_rgb: Option<&str>,
        fill_rgb: Option<&str>,
        bold: bool,
    ) -> u32 {
        let mut xml = String::from("<dxf>");
        if font_rgb.is_some() || bold {
            xml.push_str("<font>");
            if bold {
                xml.push_str("<b/>");
            }
            if let Some(rgb) = font_rgb {
                xml.push_str(&format!("<color rgb=\"FF{rgb}\"/>"));
            }
            xml.push_str("</font>");
        }
        if let Some(rgb) = fill_rgb {
            xml.push_str(&format!(
                "<fill><patternFill><bgColor rgb=\"FF{rgb}\"/></patternFill></fill>"
            ));
        }
        xml.push_str("</dxf>");
        self.differential.push(xml);
        (self.differential.len() - 1) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_are_stored_once() {
        let mut table = StyleTable::default();
        let bold = table.derive(0, |s| s.font.bold = true);
        assert_eq!(bold, 1);
        assert_eq!(table.derive(0, |s| s.font.bold = true), 1);
        assert_eq!(table.intern(CellStyle::default()), 0);
    }

    #[test]
    fn colours_resolve_for_screens() {
        let theme = default_theme_colors();
        assert_eq!(Color::rgb("ff0000").to_css(&theme).unwrap(), "#FF0000");
        let accent = Color {
            theme: Some(4),
            ..Color::default()
        };
        assert_eq!(accent.to_css(&theme).unwrap(), "#4472C4");
        let light = Color {
            theme: Some(1),
            tint: Some(0.5),
            ..Color::default()
        };
        assert_eq!(light.to_css(&theme).unwrap(), "#808080");
    }
}
