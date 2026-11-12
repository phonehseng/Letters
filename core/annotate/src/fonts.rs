//! the standard pdf fonts that text boxes can use.
//!
//! every pdf reader has these twelve fonts built in, so nothing has to be
//! embedded. their character widths are known (see font_metrics.rs), so
//! Letters can wrap text exactly where every viewer will draw it.

use lopdf::{Dictionary, Object};
use serde::{Deserialize, Serialize};

use crate::font_metrics::{
    COURIER, FIRST_CODE, HELVETICA, HELVETICA_BOLD, TIMES_BOLD, TIMES_BOLD_ITALIC, TIMES_ITALIC,
    TIMES_ROMAN,
};
use crate::pdf_values::name;

/// one of the standard fonts every pdf reader has built in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StandardFont {
    #[default]
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    TimesRoman,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
}

/// every font with its pdf name and the short resource name acrobat
/// uses for it in /DA strings, so other apps recognise ours.
const FONT_NAMES: [(StandardFont, &str, &str); 12] = [
    (StandardFont::Helvetica, "Helvetica", "Helv"),
    (StandardFont::HelveticaBold, "Helvetica-Bold", "HeBo"),
    (StandardFont::HelveticaOblique, "Helvetica-Oblique", "HeOb"),
    (
        StandardFont::HelveticaBoldOblique,
        "Helvetica-BoldOblique",
        "HeBO",
    ),
    (StandardFont::TimesRoman, "Times-Roman", "TiRo"),
    (StandardFont::TimesBold, "Times-Bold", "TiBo"),
    (StandardFont::TimesItalic, "Times-Italic", "TiIt"),
    (StandardFont::TimesBoldItalic, "Times-BoldItalic", "TiBI"),
    (StandardFont::Courier, "Courier", "Cour"),
    (StandardFont::CourierBold, "Courier-Bold", "CoBo"),
    (StandardFont::CourierOblique, "Courier-Oblique", "CoOb"),
    (
        StandardFont::CourierBoldOblique,
        "Courier-BoldOblique",
        "CoBO",
    ),
];

impl StandardFont {
    /// the name pdf uses for the font, e.g. "Times-Bold".
    pub fn base_font_name(self) -> &'static str {
        self.names().1
    }

    /// the short name used inside /DA strings and appearance resources.
    pub(crate) fn resource_name(self) -> &'static str {
        self.names().2
    }

    fn names(self) -> (StandardFont, &'static str, &'static str) {
        *FONT_NAMES
            .iter()
            .find(|(font, _, _)| *font == self)
            .expect("every font is in the table")
    }

    /// recognises a font from the name another app used for it, either
    /// the pdf name or acrobat's short name. common system font names
    /// map to the standard font that looks most like them.
    pub(crate) fn from_name(font_name: &str) -> Option<StandardFont> {
        let wanted = font_name.trim_start_matches('/');
        if let Some((font, _, _)) = FONT_NAMES
            .iter()
            .find(|(_, pdf_name, short)| *pdf_name == wanted || *short == wanted)
        {
            return Some(*font);
        }
        let lower = wanted.to_ascii_lowercase();
        let bold = lower.contains("bold");
        let slanted = lower.contains("italic") || lower.contains("oblique");
        let family =
            if lower.contains("times") || lower.contains("serif") && !lower.contains("sans") {
                0
            } else if lower.contains("cour") || lower.contains("mono") {
                1
            } else if lower.contains("helv") || lower.contains("arial") || lower.contains("sans") {
                2
            } else {
                return None;
            };
        Some(match (family, bold, slanted) {
            (0, false, false) => StandardFont::TimesRoman,
            (0, true, false) => StandardFont::TimesBold,
            (0, false, true) => StandardFont::TimesItalic,
            (0, true, true) => StandardFont::TimesBoldItalic,
            (1, false, false) => StandardFont::Courier,
            (1, true, false) => StandardFont::CourierBold,
            (1, false, true) => StandardFont::CourierOblique,
            (1, true, true) => StandardFont::CourierBoldOblique,
            (_, false, false) => StandardFont::Helvetica,
            (_, true, false) => StandardFont::HelveticaBold,
            (_, false, true) => StandardFont::HelveticaOblique,
            (_, true, true) => StandardFont::HelveticaBoldOblique,
        })
    }

    fn widths(self) -> &'static [u16; 224] {
        match self {
            StandardFont::Helvetica | StandardFont::HelveticaOblique => &HELVETICA,
            StandardFont::HelveticaBold | StandardFont::HelveticaBoldOblique => &HELVETICA_BOLD,
            StandardFont::TimesRoman => &TIMES_ROMAN,
            StandardFont::TimesBold => &TIMES_BOLD,
            StandardFont::TimesItalic => &TIMES_ITALIC,
            StandardFont::TimesBoldItalic => &TIMES_BOLD_ITALIC,
            _ => &COURIER,
        }
    }

    /// the width of one encoded character, in thousandths of the font size.
    pub(crate) fn char_width(self, code: u8) -> f32 {
        match code.checked_sub(FIRST_CODE) {
            Some(index) => f32::from(self.widths()[index as usize]),
            // control codes are never drawn
            None => 0.0,
        }
    }

    /// how wide encoded text is at `size` points.
    pub(crate) fn measure(self, encoded: &[u8], size: f32) -> f32 {
        let thousandths: f32 = encoded.iter().map(|&code| self.char_width(code)).sum();
        thousandths * size / 1000.0
    }

    /// how far the tallest letters rise above the baseline, and the
    /// deepest descend below it, in thousandths of the font size.
    /// these are the Ascender and Descender lines of the same AFM files.
    pub(crate) fn ascent_and_descent(self) -> (f32, f32) {
        match self {
            StandardFont::Helvetica
            | StandardFont::HelveticaBold
            | StandardFont::HelveticaOblique
            | StandardFont::HelveticaBoldOblique => (718.0, 207.0),
            StandardFont::TimesRoman => (683.0, 217.0),
            StandardFont::TimesBold => (676.0, 205.0),
            StandardFont::TimesItalic | StandardFont::TimesBoldItalic => (683.0, 205.0),
            _ => (629.0, 157.0),
        }
    }

    /// the font dictionary for an appearance stream.
    ///
    /// the widths are written out even though readers know them,
    /// because then every reader spaces the letters exactly as Letters
    /// measured them and wrapped lines never run past the box.
    pub(crate) fn dictionary(self) -> Dictionary {
        let mut font = Dictionary::new();
        font.set("Type", name("Font"));
        font.set("Subtype", name("Type1"));
        font.set("BaseFont", name(self.base_font_name()));
        font.set("Encoding", name("WinAnsiEncoding"));
        font.set("FirstChar", Object::Integer(i64::from(FIRST_CODE)));
        font.set("LastChar", Object::Integer(255));
        let widths = self
            .widths()
            .iter()
            .map(|&width| Object::Integer(i64::from(width)))
            .collect();
        font.set("Widths", Object::Array(widths));
        font
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_with_the_afm_widths() {
        // "Hello": H 722, e 556, l 222, l 222, o 556 = 2278 in helvetica
        let width = StandardFont::Helvetica.measure(b"Hello", 10.0);
        assert!((width - 22.78).abs() < 1e-4);
        // courier is 600 for every letter
        assert_eq!(StandardFont::Courier.measure(b"iiiii", 10.0), 30.0);
        // many bold letters are wider: "b" is 611 in helvetica-bold, 556 in helvetica
        assert!(
            StandardFont::HelveticaBold.measure(b"b", 10.0)
                > StandardFont::Helvetica.measure(b"b", 10.0)
        );
        // times-roman "W" is 944
        assert!((StandardFont::TimesRoman.measure(b"W", 1000.0) - 944.0).abs() < 1e-3);
    }

    #[test]
    fn recognises_font_names_from_other_apps() {
        assert_eq!(
            StandardFont::from_name("/Helv"),
            Some(StandardFont::Helvetica)
        );
        assert_eq!(
            StandardFont::from_name("TiBo"),
            Some(StandardFont::TimesBold)
        );
        assert_eq!(
            StandardFont::from_name("Courier-Oblique"),
            Some(StandardFont::CourierOblique)
        );
        assert_eq!(
            StandardFont::from_name("Arial,Bold"),
            Some(StandardFont::HelveticaBold)
        );
        assert_eq!(
            StandardFont::from_name("TimesNewRomanPS-ItalicMT"),
            Some(StandardFont::TimesItalic)
        );
        assert_eq!(StandardFont::from_name("Wingdings"), None);
    }

    #[test]
    fn font_dictionaries_carry_their_widths() {
        let font = StandardFont::TimesRoman.dictionary();
        let widths = font.get(b"Widths").unwrap().as_array().unwrap();
        assert_eq!(widths.len(), 224);
        assert_eq!(
            font.get(b"BaseFont").unwrap().as_name().unwrap(),
            b"Times-Roman"
        );
    }
}
