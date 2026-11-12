//! how wide each character of helvetica is.
//!
//! the latin part of the text layer is written in helvetica, which pdf
//! readers have built in. to stretch a word exactly over its box, the
//! writer needs the word's natural width: the sum of its characters'
//! widths. giving every character its real width (rather than one width
//! for all) also makes each letter's selection box sit over that letter.
//!
//! source: adobe's core 14 font metrics (afm) for helvetica, which adobe
//! publishes for free use, read out for each windows "ansi" code. widths
//! are in thousandths of the font size; the five codes the set leaves
//! unused get the width of a space.

/// the first character code in the table.
pub const FIRST_CODE: u8 = 32;

#[rustfmt::skip]
pub const WIDTHS: [u16; 224] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // 32
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 48
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // 64
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 80
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // 96
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, 278, // 112
    556, 278, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 278, 611, 278, // 128
    278, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 278, 500, 667, // 144
    278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333, // 160
    400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611, // 176
    667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278, // 192
    722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611, // 208
    556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278, // 224
    556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500, // 240
];

/// the width of one character code, in thousandths of the font size.
pub fn width(code: u8) -> u16 {
    code.checked_sub(FIRST_CODE)
        .map_or(WIDTHS[0], |index| WIDTHS[index as usize])
}

/// the width of a whole word's codes, in thousandths of the font size.
pub fn text_width(codes: &[u8]) -> u32 {
    codes.iter().map(|&code| u32::from(width(code))).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_widths() {
        assert_eq!(width(b' '), 278);
        assert_eq!(width(b'W'), 944);
        assert_eq!(width(b'i'), 222);
        // ñ, at code 0xf1, is as wide as n
        assert_eq!(width(0xF1), width(b'n'));
        assert_eq!(text_width(b"Hello"), 722 + 556 + 222 + 222 + 556);
    }
}
