//! the styles part of a spreadsheet: fonts, fills, borders and number
//! formats, each listed once, and the combinations cells point to.

use crate::content::Rgb;
use crate::package::xml_escape;
use crate::structure::model::{CellAlign, CellBorders};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FontStyle {
    pub name: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub color: Rgb,
}

impl Default for FontStyle {
    fn default() -> Self {
        Self {
            name: "Calibri".into(),
            size: 11.0,
            bold: false,
            italic: false,
            color: Rgb::BLACK,
        }
    }
}

/// everything about how one cell looks.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CellStyle {
    pub font: FontStyle,
    pub fill: Option<Rgb>,
    pub borders: CellBorders,
    pub align: CellAlign,
    pub wrap: bool,
    pub number_format: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct Combination {
    font: usize,
    fill: usize,
    border: usize,
    number_format: u32,
    align: CellAlign,
    wrap: bool,
}

/// collects the styles cells use, giving each combination a number.
pub(crate) struct StyleSheet {
    fonts: Vec<FontStyle>,
    fills: Vec<Rgb>,
    borders: Vec<CellBorders>,
    number_formats: Vec<String>,
    combinations: Vec<Combination>,
}

/// custom number formats are numbered from 164; lower numbers are built in
const FIRST_CUSTOM_FORMAT: u32 = 164;

impl StyleSheet {
    pub fn new() -> Self {
        let mut sheet = Self {
            fonts: vec![FontStyle::default()],
            fills: Vec::new(),
            borders: vec![CellBorders::default()],
            number_formats: Vec::new(),
            combinations: Vec::new(),
        };
        // style 0 is the default every unstyled cell has
        sheet.index(&CellStyle::default());
        sheet
    }

    /// the number cells with this style refer to (their `s` attribute).
    pub fn index(&mut self, style: &CellStyle) -> usize {
        let font = position_or_push(&mut self.fonts, &style.font);
        // fills 0 and 1 are the two every file must have (none, gray125)
        let fill = match style.fill {
            Some(color) => 2 + position_or_push(&mut self.fills, &color),
            None => 0,
        };
        let border = position_or_push(&mut self.borders, &style.borders);
        let number_format = match &style.number_format {
            Some(code) => {
                FIRST_CUSTOM_FORMAT + position_or_push(&mut self.number_formats, code) as u32
            }
            None => 0,
        };
        let combination = Combination {
            font,
            fill,
            border,
            number_format,
            align: style.align,
            wrap: style.wrap,
        };
        position_or_push(&mut self.combinations, &combination)
    }

    pub fn xml(&self) -> String {
        let mut xml = String::from(crate::package::XML_HEADER);
        xml.push_str(
            "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
        );
        if !self.number_formats.is_empty() {
            xml.push_str(&format!(
                "<numFmts count=\"{}\">",
                self.number_formats.len()
            ));
            for (index, code) in self.number_formats.iter().enumerate() {
                xml.push_str(&format!(
                    "<numFmt numFmtId=\"{}\" formatCode=\"{}\"/>",
                    FIRST_CUSTOM_FORMAT + index as u32,
                    xml_escape(code)
                ));
            }
            xml.push_str("</numFmts>");
        }
        xml.push_str(&format!("<fonts count=\"{}\">", self.fonts.len()));
        for font in &self.fonts {
            xml.push_str("<font>");
            if font.bold {
                xml.push_str("<b/>");
            }
            if font.italic {
                xml.push_str("<i/>");
            }
            xml.push_str(&format!(
                "<sz val=\"{}\"/><color rgb=\"FF{}\"/><name val=\"{}\"/><family val=\"2\"/></font>",
                font_size(font.size),
                font.color.hex(),
                xml_escape(&font.name)
            ));
        }
        xml.push_str("</fonts>");
        xml.push_str(&format!(
            "<fills count=\"{}\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>",
            self.fills.len() + 2
        ));
        for color in &self.fills {
            xml.push_str(&format!(
                "<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{}\"/><bgColor indexed=\"64\"/></patternFill></fill>",
                color.hex()
            ));
        }
        xml.push_str("</fills>");
        xml.push_str(&format!("<borders count=\"{}\">", self.borders.len()));
        for border in &self.borders {
            // the order of the sides is fixed by the file format
            xml.push_str("<border>");
            for (name, on) in [
                ("left", border.left),
                ("right", border.right),
                ("top", border.top),
                ("bottom", border.bottom),
            ] {
                if on {
                    xml.push_str(&format!(
                        "<{name} style=\"thin\"><color rgb=\"FF000000\"/></{name}>"
                    ));
                } else {
                    xml.push_str(&format!("<{name}/>"));
                }
            }
            xml.push_str("<diagonal/></border>");
        }
        xml.push_str("</borders>");
        xml.push_str("<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>");
        xml.push_str(&format!("<cellXfs count=\"{}\">", self.combinations.len()));
        for c in &self.combinations {
            xml.push_str(&format!(
                "<xf numFmtId=\"{}\" fontId=\"{}\" fillId=\"{}\" borderId=\"{}\" xfId=\"0\"",
                c.number_format, c.font, c.fill, c.border
            ));
            for (attribute, used) in [
                ("applyNumberFormat", c.number_format != 0),
                ("applyFont", c.font != 0),
                ("applyFill", c.fill != 0),
                ("applyBorder", c.border != 0),
            ] {
                if used {
                    xml.push_str(&format!(" {attribute}=\"1\""));
                }
            }
            let horizontal = match c.align {
                CellAlign::Left => "left",
                CellAlign::Center => "center",
                CellAlign::Right => "right",
            };
            xml.push_str(&format!(
                " applyAlignment=\"1\"><alignment horizontal=\"{horizontal}\" vertical=\"center\"{}/></xf>",
                if c.wrap { " wrapText=\"1\"" } else { "" }
            ));
        }
        xml.push_str("</cellXfs>");
        xml.push_str("<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>");
        xml.push_str("</styleSheet>");
        xml
    }
}

/// spreadsheets keep font sizes to half points
fn font_size(size: f32) -> String {
    let half_points = (size * 2.0).round().clamp(2.0, 818.0) / 2.0;
    if half_points == half_points.trunc() {
        format!("{}", half_points as i32)
    } else {
        format!("{half_points}")
    }
}

fn position_or_push<T: PartialEq + Clone>(list: &mut Vec<T>, item: &T) -> usize {
    match list.iter().position(|existing| existing == item) {
        Some(index) => index,
        None => {
            list.push(item.clone());
            list.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_are_listed_once() {
        let mut sheet = StyleSheet::new();
        let bold = CellStyle {
            font: FontStyle {
                bold: true,
                ..Default::default()
            },
            fill: Some(Rgb(200, 200, 200)),
            number_format: Some("0.00".into()),
            ..Default::default()
        };
        assert_eq!(sheet.index(&CellStyle::default()), 0);
        assert_eq!(sheet.index(&bold), 1);
        assert_eq!(sheet.index(&bold), 1);
        let xml = sheet.xml();
        assert!(xml.contains("<numFmt numFmtId=\"164\" formatCode=\"0.00\"/>"));
        assert!(xml.contains("<fills count=\"3\">"));
        assert!(xml.contains("fontId=\"1\" fillId=\"2\""));
        assert_eq!(font_size(10.3), "10.5");
    }
}
