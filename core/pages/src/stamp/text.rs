//! drawing one line of text with the fonts every pdf reader has built in.
//!
//! the standard fonts need nothing embedded, which keeps stamps tiny. the
//! catch is that they only know the 218 characters of the "WinAnsi"
//! encoding (western european letters and common symbols), so text is
//! checked against it and anything else is refused with a clear message
//! instead of being drawn as garbage.

use lopdf::{Dictionary, Object};
use serde::{Deserialize, Serialize};

use crate::PagesError;

/// the built-in fonts a stamp can use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StampFont {
    #[default]
    Helvetica,
    HelveticaBold,
    Courier,
}

impl StampFont {
    fn base_font(self) -> &'static str {
        match self {
            StampFont::Helvetica => "Helvetica",
            StampFont::HelveticaBold => "Helvetica-Bold",
            StampFont::Courier => "Courier",
        }
    }

    /// how tall capital letters are, in thousandths of the font size.
    /// used to place text by its top edge and to centre it.
    pub fn cap_height(self) -> f32 {
        match self {
            StampFont::Helvetica | StampFont::HelveticaBold => 718.0,
            StampFont::Courier => 571.0,
        }
    }

    /// the font's dictionary, ready to add to a document.
    pub fn dictionary(self) -> Dictionary {
        let mut font = Dictionary::new();
        font.set("Type", Object::Name(b"Font".to_vec()));
        font.set("Subtype", Object::Name(b"Type1".to_vec()));
        font.set(
            "BaseFont",
            Object::Name(self.base_font().as_bytes().to_vec()),
        );
        font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        font
    }

    /// the width of one encoded character, in thousandths of the font size.
    fn char_width(self, code: u8) -> f32 {
        let widths = match self {
            StampFont::Courier => return 600.0,
            StampFont::Helvetica => &HELVETICA_WIDTHS,
            StampFont::HelveticaBold => &HELVETICA_BOLD_WIDTHS,
        };
        let ascii = match code {
            32..=126 => code,
            // accented letters are as wide as their plain letter, near enough
            0xC0..=0xFF => match ACCENT_BASES[(code - 0xC0) as usize] {
                b'?' => return AVERAGE_WIDTH,
                base => base,
            },
            _ => return AVERAGE_WIDTH,
        };
        f32::from(widths[(ascii - 32) as usize])
    }

    /// how wide encoded text is when drawn at `size` points.
    pub fn text_width(self, encoded: &[u8], size: f32) -> f32 {
        let thousandths: f32 = encoded.iter().map(|&code| self.char_width(code)).sum();
        thousandths * size / 1000.0
    }
}

/// a stand-in width for the few symbols without a table entry. it only
/// affects how text is centred, never what is drawn.
const AVERAGE_WIDTH: f32 = 556.0;

/// widths of the printable ascii characters (32 to 126) in helvetica,
/// from adobe's published font metrics.
#[rustfmt::skip]
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// the same for helvetica bold.
#[rustfmt::skip]
const HELVETICA_BOLD_WIDTHS: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// for each code from 0xC0 to 0xFF, the plain letter it is an accented
/// form of, or '?' when there is none.
const ACCENT_BASES: &[u8; 64] = b"AAAAAA?CEEEEIIIIDNOOOOO?OUUUUYP?aaaaaa?ceeeeiiiidnooooo?ouuuuypy";

/// the characters in codes 0x80 to 0x9F, where WinAnsi differs from
/// latin-1. a space marks a code with no character.
const WIN_ANSI_EXTRAS: [char; 32] = [
    '€', ' ', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', ' ', 'Ž', ' ', ' ', '‘', '’',
    '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', ' ', 'ž', 'Ÿ',
];

/// the WinAnsi code for one character, if it has one.
fn win_ansi_code(character: char) -> Option<u8> {
    match character as u32 {
        0x20..=0x7E | 0xA0..=0xFF => Some(character as u8),
        _ => WIN_ANSI_EXTRAS
            .iter()
            .position(|&extra| extra == character && extra != ' ')
            .map(|position| 0x80 + position as u8),
    }
}

/// turns text into WinAnsi codes, or explains which character can't be drawn.
pub(crate) fn encode_win_ansi(text: &str) -> Result<Vec<u8>, PagesError> {
    text.chars()
        .map(|character| match character {
            '\t' => Ok(b' '),
            '\n' | '\r' => Err(PagesError::InvalidSetting(
                "A stamp is one line of text. Remove the line break.".into(),
            )),
            _ => win_ansi_code(character).ok_or(PagesError::UnsupportedCharacter(character)),
        })
        .collect()
}

/// writes encoded text as a pdf string for a content stream.
///
/// brackets and backslashes are escaped, and anything outside plain ascii
/// is written as an octal escape so the content stream stays ascii.
pub(crate) fn pdf_string(encoded: &[u8]) -> String {
    let mut text = String::with_capacity(encoded.len() + 2);
    text.push('(');
    for &code in encoded {
        match code {
            b'(' | b')' | b'\\' => {
                text.push('\\');
                text.push(code as char);
            }
            0x20..=0x7E => text.push(code as char),
            _ => text.push_str(&format!("\\{code:03o}")),
        }
    }
    text.push(')');
    text
}

/// a colour with red, green and blue from 0 to 255.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb {
        red: 0,
        green: 0,
        blue: 0,
    };

    /// the operator that sets this as the fill colour.
    pub(crate) fn fill_operator(self) -> String {
        let part = |value: u8| crate::tree::number(f32::from(value) / 255.0);
        format!(
            "{} {} {} rg",
            part(self.red),
            part(self.green),
            part(self.blue)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_western_european_text_is_encoded() {
        assert_eq!(encode_win_ansi("Page 1").unwrap(), b"Page 1");
        assert_eq!(encode_win_ansi("café").unwrap(), b"caf\xE9");
        assert_eq!(encode_win_ansi("€ – ’").unwrap(), b"\x80 \x96 \x92");
    }

    #[test]
    fn other_scripts_are_refused_by_name() {
        assert_eq!(
            encode_win_ansi("ページ"),
            Err(PagesError::UnsupportedCharacter('ペ'))
        );
        assert!(matches!(
            encode_win_ansi("two\nlines"),
            Err(PagesError::InvalidSetting(_))
        ));
    }

    #[test]
    fn strings_are_escaped_for_content_streams() {
        assert_eq!(pdf_string(b"a(b)c\\"), "(a\\(b\\)c\\\\)");
        assert_eq!(pdf_string(b"caf\xE9"), "(caf\\351)");
    }

    #[test]
    fn widths_come_from_the_font_metrics() {
        // "Page" in helvetica: 667 + 556 + 556 + 556 thousandths
        let width = StampFont::Helvetica.text_width(b"Page", 10.0);
        assert!((width - 23.35).abs() < 0.001);
        assert_eq!(StampFont::Courier.text_width(b"abc", 10.0), 18.0);
        // an accented letter is as wide as the plain one
        assert_eq!(
            StampFont::Helvetica.text_width(b"\xE9", 10.0),
            StampFont::Helvetica.text_width(b"e", 10.0)
        );
    }

    #[test]
    fn colours_become_fill_operators() {
        let red = Rgb {
            red: 255,
            green: 0,
            blue: 0,
        };
        assert_eq!(red.fill_operator(), "1 0 0 rg");
    }
}
