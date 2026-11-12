//! how wide letters are in the standard pdf fonts.
//!
//! every pdf reader has the 14 "standard" fonts built in, so a form can use
//! them without embedding a font file. to centre, wrap and fit text we
//! need each letter's width. the numbers below are the advance widths
//! (in thousandths of the font size) from adobe's core 14 font metrics
//! (afm) files for helvetica, helvetica-bold, times-roman and courier,
//! which adobe allows anyone to copy and use. accented letters take the
//! width of their base letter, as they do in those files.
//!
//! oblique and italic styles share their upright widths here (exact for
//! helvetica, close for times). other fonts are measured from the /Widths
//! array in the pdf when it has one, and with helvetica's widths otherwise.

use lopdf::{Document, Object};

use crate::objects::{get_array, get_integer, get_name, number_of, resolve};

/// the standard fonts this crate knows the widths of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardFont {
    Helvetica,
    HelveticaBold,
    TimesRoman,
    Courier,
}

impl StandardFont {
    /// picks the closest known font for a /BaseFont name, such as
    /// "Helvetica-Bold", "ArialMT" or "ABCDEF+TimesNewRomanPSMT".
    pub fn closest_to(base_font: &str) -> StandardFont {
        let lower = base_font.to_ascii_lowercase();
        if lower.contains("courier") || lower.contains("mono") {
            StandardFont::Courier
        } else if lower.contains("times") || (lower.contains("serif") && !lower.contains("sans")) {
            StandardFont::TimesRoman
        } else if lower.contains("bold") || lower.contains("black") {
            StandardFont::HelveticaBold
        } else {
            StandardFont::Helvetica
        }
    }

    /// the /BaseFont name to write when this crate adds the font itself.
    pub fn base_font_name(self) -> &'static str {
        match self {
            StandardFont::Helvetica => "Helvetica",
            StandardFont::HelveticaBold => "Helvetica-Bold",
            StandardFont::TimesRoman => "Times-Roman",
            StandardFont::Courier => "Courier",
        }
    }

    /// how far above and below the baseline the letters reach, in
    /// thousandths of the font size.
    pub fn ascent_descent(self) -> (f32, f32) {
        match self {
            StandardFont::Helvetica | StandardFont::HelveticaBold => (718.0, -207.0),
            StandardFont::TimesRoman => (683.0, -217.0),
            StandardFont::Courier => (629.0, -157.0),
        }
    }

    /// the width of one winansi byte, in thousandths of the font size.
    pub fn width(self, byte: u8) -> f32 {
        if self == StandardFont::Courier {
            return 600.0;
        }
        let table = self.tables();
        let width = match byte {
            0x20..=0x7E => table.ascii[(byte - 0x20) as usize],
            0x80..=0xBF => table.high[(byte - 0x80) as usize],
            0xC0..=0xFF => accented_width(self, byte),
            _ => 0,
        };
        // codes the encoding leaves unused get the width of a space
        if width == 0 && byte >= 0x20 {
            table.ascii[0] as f32
        } else {
            width as f32
        }
    }

    fn tables(self) -> &'static WidthTables {
        match self {
            StandardFont::Helvetica | StandardFont::Courier => &HELVETICA,
            StandardFont::HelveticaBold => &HELVETICA_BOLD,
            StandardFont::TimesRoman => &TIMES_ROMAN,
        }
    }
}

struct WidthTables {
    /// 0x20..=0x7e
    ascii: [u16; 95],
    /// 0x80..=0xbf (0 where winansi has no character)
    high: [u16; 64],
    /// the letters in 0xc0..=0xff that are not an accented base letter:
    /// Æ Ð × Ø Þ ß æ ð ÷ ø þ, in that order
    special: [u16; 11],
    /// the accented i (ì í î ï): helvetica draws them wider than a plain i
    accented_i: u16,
}

#[rustfmt::skip]
const HELVETICA: WidthTables = WidthTables {
    ascii: [
        278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
        556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
        1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
        667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
        333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
        556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
    ],
    high: [
        556, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0,
        0, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667,
        278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333,
        400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611,
    ],
    special: [1000, 722, 584, 778, 667, 611, 889, 556, 584, 611, 556],
    accented_i: 278,
};

#[rustfmt::skip]
const HELVETICA_BOLD: WidthTables = WidthTables {
    ascii: [
        278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
        556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
        975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
        667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
        333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
        611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
    ],
    high: [
        556, 0, 278, 556, 500, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0,
        0, 278, 278, 500, 500, 350, 556, 1000, 333, 1000, 556, 333, 944, 0, 500, 667,
        278, 333, 556, 556, 556, 556, 280, 556, 333, 737, 370, 556, 584, 333, 737, 333,
        400, 584, 333, 333, 333, 611, 556, 278, 333, 333, 365, 556, 834, 834, 834, 611,
    ],
    special: [1000, 722, 584, 778, 667, 611, 889, 611, 584, 611, 611],
    accented_i: 278,
};

#[rustfmt::skip]
const TIMES_ROMAN: WidthTables = WidthTables {
    ascii: [
        250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278,
        500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444,
        921, 722, 667, 667, 722, 611, 556, 722, 722, 333, 389, 722, 611, 889, 722, 722,
        556, 722, 667, 556, 611, 722, 722, 944, 722, 722, 611, 333, 278, 333, 469, 500,
        333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778, 500, 500,
        500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541,
    ],
    high: [
        500, 0, 333, 500, 444, 1000, 500, 500, 333, 1000, 556, 333, 889, 0, 611, 0,
        0, 333, 333, 444, 444, 350, 500, 1000, 333, 980, 389, 333, 722, 0, 444, 722,
        250, 333, 500, 500, 500, 500, 200, 500, 333, 760, 276, 500, 564, 333, 760, 333,
        400, 564, 300, 300, 333, 500, 453, 250, 333, 300, 310, 500, 750, 750, 750, 444,
    ],
    special: [889, 722, 564, 722, 556, 500, 667, 500, 564, 500, 500],
    accented_i: 278,
};

/// widths for 0xc0..=0xff, which are mostly base letters with accents.
fn accented_width(font: StandardFont, byte: u8) -> u16 {
    let table = font.tables();
    let ascii = |letter: u8| table.ascii[(letter - 0x20) as usize];
    let special = |index: usize| table.special[index];
    match byte {
        0xC0..=0xC5 => ascii(b'A'),
        0xC6 => special(0),
        0xC7 => ascii(b'C'),
        0xC8..=0xCB => ascii(b'E'),
        0xCC..=0xCF => ascii(b'I'),
        0xD0 => special(1),
        0xD1 => ascii(b'N'),
        0xD2..=0xD6 => ascii(b'O'),
        0xD7 => special(2),
        0xD8 => special(3),
        0xD9..=0xDC => ascii(b'U'),
        0xDD => ascii(b'Y'),
        0xDE => special(4),
        0xDF => special(5),
        0xE0..=0xE5 => ascii(b'a'),
        0xE6 => special(6),
        0xE7 => ascii(b'c'),
        0xE8..=0xEB => ascii(b'e'),
        0xEC..=0xEF => table.accented_i,
        0xF0 => special(7),
        0xF1 => ascii(b'n'),
        0xF2..=0xF6 => ascii(b'o'),
        0xF7 => special(8),
        0xF8 => special(9),
        0xF9..=0xFC => ascii(b'u'),
        0xFD | 0xFF => ascii(b'y'),
        _ => special(10),
    }
}

/// what an appearance needs to know to measure text in one font.
#[derive(Clone, Debug, PartialEq)]
pub struct FontMetrics {
    pub standard: StandardFont,
    /// the font's own widths by character code, when the pdf gives them
    widths: Option<(u8, Vec<f32>)>,
}

impl FontMetrics {
    pub fn standard(font: StandardFont) -> Self {
        Self {
            standard: font,
            widths: None,
        }
    }

    /// reads a font dictionary from the pdf: its /Widths when present,
    /// and the closest standard font for everything else.
    pub fn from_font_dict(doc: &Document, font: &lopdf::Dictionary) -> Self {
        let base_font = get_name(doc, font, b"BaseFont")
            .map(|name| String::from_utf8_lossy(&name).into_owned())
            .unwrap_or_default();
        let standard = StandardFont::closest_to(&base_font);
        let first_char = get_integer(doc, font, b"FirstChar")
            .unwrap_or(0)
            .clamp(0, 255) as u8;
        let widths = get_array(doc, font, b"Widths").map(|values| {
            values
                .iter()
                .map(|value| number_of(resolve(doc, value)).unwrap_or(0.0))
                .collect::<Vec<f32>>()
        });
        let widths = widths
            .filter(|values| !values.is_empty())
            .map(|values| (first_char, values));
        Self { standard, widths }
    }

    /// the width of one byte in thousandths of the font size.
    pub fn byte_width(&self, byte: u8) -> f32 {
        if let Some((first, widths)) = &self.widths {
            if byte >= *first {
                if let Some(&width) = widths.get((byte - first) as usize) {
                    if width > 0.0 {
                        return width;
                    }
                }
            }
        }
        self.standard.width(byte)
    }

    /// the width of some winansi bytes at a font size, in points.
    pub fn text_width(&self, bytes: &[u8], size: f32) -> f32 {
        bytes.iter().map(|&byte| self.byte_width(byte)).sum::<f32>() * size / 1000.0
    }
}

/// reads a font object (dictionary or reference) into metrics.
pub fn metrics_of(doc: &Document, font: &Object) -> FontMetrics {
    match resolve(doc, font).as_dict() {
        Ok(dict) => FontMetrics::from_font_dict(doc, dict),
        Err(_) => FontMetrics::standard(StandardFont::Helvetica),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_helvetica_widths() {
        let font = StandardFont::Helvetica;
        assert_eq!(font.width(b' '), 278.0);
        assert_eq!(font.width(b'W'), 944.0);
        assert_eq!(font.width(b'i'), 222.0);
        assert_eq!(font.width(0xE9), 556.0); // é like e
        assert_eq!(font.width(0x80), 556.0); // the euro sign
    }

    #[test]
    fn courier_is_monospaced() {
        assert!((0x20..=0xFF).all(|byte| StandardFont::Courier.width(byte) == 600.0));
    }

    #[test]
    fn text_width_scales_with_size() {
        let metrics = FontMetrics::standard(StandardFont::Helvetica);
        // "Hi" = 722 + 222 thousandths
        assert!((metrics.text_width(b"Hi", 10.0) - 9.44).abs() < 0.001);
    }

    #[test]
    fn font_names_map_to_their_family() {
        assert_eq!(StandardFont::closest_to("ArialMT"), StandardFont::Helvetica);
        assert_eq!(
            StandardFont::closest_to("Helvetica-Bold"),
            StandardFont::HelveticaBold
        );
        assert_eq!(
            StandardFont::closest_to("ABCDEF+TimesNewRomanPSMT"),
            StandardFont::TimesRoman
        );
        assert_eq!(
            StandardFont::closest_to("CourierNewPSMT"),
            StandardFont::Courier
        );
    }
}
