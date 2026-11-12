//! widths of the standard pdf fonts, for files that don't list them.
//!
//! a pdf may use Helvetica, Times or Courier without saying how wide each
//! letter is, because every reader is expected to know. to place characters
//! correctly we need the same numbers. they come from Adobe's published
//! font metrics (the core 14 AFM files), for the printable ascii letters;
//! anything else gets the font's typical width. Arial is metric compatible
//! with Helvetica and Times New Roman with Times, so they share the tables.
//!
//! the report writer uses the same tables to wrap its own text.

/// metrics of one standard font, in thousandths of the font size.
pub(crate) struct StandardFont {
    /// widths of the characters from space (32) to tilde (126)
    widths: &'static [u16; 95],
    /// width of anything outside that range
    other: u16,
    pub ascent: f32,
    pub descent: f32,
}

impl StandardFont {
    /// the width of one character, in thousandths of the font size.
    pub fn char_width(&self, ch: char) -> f32 {
        let code = ch as u32;
        if (32..=126).contains(&code) {
            self.widths[(code - 32) as usize] as f32
        } else if ch == '\u{a0}' {
            self.widths[0] as f32
        } else {
            self.other as f32
        }
    }

    /// the width of a whole string at `size` points.
    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|ch| self.char_width(ch)).sum::<f32>() * size / 1000.0
    }
}

pub(crate) static HELVETICA: StandardFont = StandardFont {
    widths: &HELVETICA_WIDTHS,
    other: 556,
    ascent: 718.0,
    descent: -207.0,
};

pub(crate) static HELVETICA_BOLD: StandardFont = StandardFont {
    widths: &HELVETICA_BOLD_WIDTHS,
    other: 556,
    ascent: 718.0,
    descent: -207.0,
};

pub(crate) static TIMES: StandardFont = StandardFont {
    widths: &TIMES_WIDTHS,
    other: 500,
    ascent: 683.0,
    descent: -217.0,
};

pub(crate) static COURIER: StandardFont = StandardFont {
    widths: &[600; 95],
    other: 600,
    ascent: 629.0,
    descent: -157.0,
};

/// finds the standard font a font name stands for, if any.
pub(crate) fn standard_font_for(base_name: &str) -> Option<&'static StandardFont> {
    let name = base_name.to_ascii_lowercase();
    let bold = name.contains("bold") || name.contains("black") || name.contains("heavy");
    if name.starts_with("helvetica") || name.starts_with("arial") {
        Some(if bold { &HELVETICA_BOLD } else { &HELVETICA })
    } else if name.starts_with("times") {
        // the bold times widths are close enough to the roman ones for placing text
        Some(&TIMES)
    } else if name.starts_with("courier") {
        Some(&COURIER)
    } else {
        None
    }
}

#[rustfmt::skip]
static HELVETICA_WIDTHS: [u16; 95] = [
    // space ! " # $ % & ' ( ) * + , - . /
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    // 0 to 9
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556,
    // : ; < = > ? @
    278, 278, 584, 584, 584, 556, 1015,
    // A to Z
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833,
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611,
    // [ \ ] ^ _ `
    278, 278, 278, 469, 556, 333,
    // a to z
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833,
    556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500,
    // { | } ~
    334, 260, 334, 584,
];

#[rustfmt::skip]
static HELVETICA_BOLD_WIDTHS: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556,
    333, 333, 584, 584, 584, 611, 975,
    722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833,
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611,
    333, 278, 333, 584, 556, 333,
    556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889,
    611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500,
    389, 280, 389, 584,
];

#[rustfmt::skip]
static TIMES_WIDTHS: [u16; 95] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278,
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500,
    278, 278, 564, 564, 564, 444, 921,
    722, 667, 667, 722, 611, 556, 722, 722, 333, 389, 722, 611, 889,
    722, 722, 556, 722, 667, 556, 611, 722, 722, 944, 722, 722, 611,
    333, 278, 333, 469, 500, 333,
    444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778,
    500, 500, 500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444,
    480, 200, 480, 541,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helvetica_letters_have_their_published_widths() {
        assert_eq!(HELVETICA.char_width(' '), 278.0);
        assert_eq!(HELVETICA.char_width('A'), 667.0);
        assert_eq!(HELVETICA.char_width('m'), 833.0);
        assert_eq!(HELVETICA.char_width('~'), 584.0);
        assert_eq!(HELVETICA_BOLD.char_width('b'), 611.0);
        assert_eq!(TIMES.char_width('a'), 444.0);
    }

    #[test]
    fn arial_borrows_the_helvetica_widths() {
        let arial = standard_font_for("Arial-BoldMT").unwrap();
        assert_eq!(arial.char_width('b'), 611.0);
        assert!(standard_font_for("Garamond").is_none());
    }

    #[test]
    fn text_width_scales_with_size() {
        // "Hi" is 722 + 222 thousandths of an em
        assert!((HELVETICA.text_width("Hi", 10.0) - 9.44).abs() < 0.001);
    }
}
