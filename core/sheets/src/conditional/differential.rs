//! a differential format (excel's dxf) as the screen needs it: the font
//! colour, bold, italic, underline, strike and fill a matching rule puts
//! over the cell's own style. the model keeps each one as its original
//! xml (see `StyleTable::differential`), so it is read here when drawn.

use crate::model::Color;
use crate::xlsx::style_parts::color_from_attributes;
use crate::xml::Fragment;

/// what one differential format changes; none leaves the cell's own
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DifferentialLook {
    /// "#RRGGBB"
    pub color: Option<String>,
    pub fill: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
}

/// reads a `<dxf>`; anything it cannot read changes nothing
pub fn read_differential(xml: &str, theme: &[String]) -> DifferentialLook {
    let mut look = DifferentialLook::default();
    let Some(dxf) = Fragment::parse(xml) else {
        return look;
    };
    for (name, child) in &dxf.children {
        match name.as_str() {
            "font" => read_font(child, theme, &mut look),
            "fill" => look.fill = read_fill(child, theme),
            _ => {}
        }
    }
    look
}

/// `<b/>` and `<b val="1"/>` turn bold on; `<b val="0"/>` turns it off,
/// which matters here: a rule can take bold away from a bold cell
fn switch(xml: &str) -> Option<bool> {
    let element = Fragment::parse(xml)?;
    Some(!matches!(
        element.attribute("val"),
        Some("0") | Some("false")
    ))
}

fn read_font(xml: &str, theme: &[String], look: &mut DifferentialLook) {
    let Some(font) = Fragment::parse(xml) else {
        return;
    };
    for (name, child) in &font.children {
        match name.as_str() {
            "b" => look.bold = switch(child),
            "i" => look.italic = switch(child),
            "strike" => look.strike = switch(child),
            "u" => {
                // `<u/>` is a single underline; `val="none"` removes it
                let value =
                    Fragment::parse(child).and_then(|u| u.attribute("val").map(str::to_string));
                look.underline = Some(value.as_deref() != Some("none"));
            }
            "color" => look.color = color_of(child).and_then(|c| c.to_css(theme)),
            _ => {}
        }
    }
}

fn color_of(xml: &str) -> Option<Color> {
    let element = Fragment::parse(xml)?;
    let color = color_from_attributes(&element.attributes);
    // `<color auto="1"/>` is "the usual colour": no change
    (!color.auto || color.rgb.is_some() || color.theme.is_some()).then_some(color)
}

/// a dxf's fill colour. excel writes a rule's solid fill as the pattern's
/// background colour (`<bgColor>`) with no pattern type; libreoffice
/// writes `patternType="solid"` with both colours. the background colour
/// is the one excel shows, so it comes first.
fn read_fill(xml: &str, theme: &[String]) -> Option<String> {
    let fill = Fragment::parse(xml)?;
    let (_, pattern_xml) = fill
        .children
        .iter()
        .find(|(name, _)| name == "patternFill")?;
    let pattern = Fragment::parse(pattern_xml)?;
    if pattern.attribute("patternType") == Some("none") {
        return None;
    }
    let colour = |wanted: &str| {
        pattern
            .children
            .iter()
            .find(|(name, _)| name == wanted)
            .and_then(|(_, xml)| color_of(xml))
            .and_then(|c| c.to_css(theme))
    };
    colour("bgColor").or_else(|| colour("fgColor"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::style::default_theme_colors;

    #[test]
    fn excel_rule_formats_are_read() {
        let theme = default_theme_colors();
        let look = read_differential(
            r#"<dxf><font><color rgb="FF9C0006"/></font><fill><patternFill><bgColor rgb="FFFFC7CE"/></patternFill></fill></dxf>"#,
            &theme,
        );
        assert_eq!(look.color.as_deref(), Some("#9C0006"));
        assert_eq!(look.fill.as_deref(), Some("#FFC7CE"));
        assert_eq!(look.bold, None);
    }

    #[test]
    fn switches_can_turn_things_off() {
        let theme = default_theme_colors();
        let look = read_differential(
            r#"<dxf><font><b val="0"/><i/><u/><strike/><color theme="4"/></font></dxf>"#,
            &theme,
        );
        assert_eq!(look.bold, Some(false));
        assert_eq!(look.italic, Some(true));
        assert_eq!(look.underline, Some(true));
        assert_eq!(look.strike, Some(true));
        assert_eq!(look.color.as_deref(), Some("#4472C4"));
        assert_eq!(look.fill, None);
    }

    #[test]
    fn libreoffice_solid_fills_are_read() {
        let theme = default_theme_colors();
        let look = read_differential(
            r#"<dxf><fill><patternFill patternType="solid"><fgColor rgb="FF00FF00"/></patternFill></fill></dxf>"#,
            &theme,
        );
        assert_eq!(look.fill.as_deref(), Some("#00FF00"));
    }
}
