//! the one-byte encodings simple pdf fonts use, and glyph names.
//!
//! a simple font maps each byte (0-255) to a glyph name through a base
//! encoding (standard, WinAnsi or MacRoman) changed by /Differences. the
//! glyph name says which character it is ("eacute" is "é"). the tables
//! here are the encodings from the pdf standard (annex D) and the glyph
//! names of the Adobe Glyph List for the characters those encodings hold.

/// the base encodings a simple font can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BaseEncoding {
    Standard,
    WinAnsi,
    MacRoman,
}

impl BaseEncoding {
    pub fn from_name(name: &[u8]) -> Option<BaseEncoding> {
        match name {
            b"StandardEncoding" => Some(BaseEncoding::Standard),
            b"WinAnsiEncoding" => Some(BaseEncoding::WinAnsi),
            b"MacRomanEncoding" => Some(BaseEncoding::MacRoman),
            _ => None,
        }
    }

    /// the glyph name the encoding gives a byte, if any.
    pub fn glyph_name(self, code: u8) -> Option<&'static str> {
        let character = match self {
            BaseEncoding::WinAnsi => win_ansi_char(code)?,
            BaseEncoding::MacRoman => mac_roman_char(code)?,
            BaseEncoding::Standard => return standard_glyph_name(code),
        };
        name_of_char(character)
    }
}

/// the characters in WinAnsiEncoding codes 128 to 159; '\0' marks a gap.
const WIN_ANSI_128_TO_159: [char; 32] = [
    '€', '\0', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\0', 'Ž', '\0', '\0', '‘',
    '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\0', 'ž', 'Ÿ',
];

pub(crate) fn win_ansi_char(code: u8) -> Option<char> {
    match code {
        32..=126 | 160..=255 => Some(char::from(code)),
        128..=159 => Some(WIN_ANSI_128_TO_159[usize::from(code - 128)]).filter(|c| *c != '\0'),
        _ => None,
    }
}

/// the byte WinAnsiEncoding gives a character, if it has one.
pub(crate) fn win_ansi_code(character: char) -> Option<u8> {
    (32..=255u8).find(|code| win_ansi_char(*code) == Some(character))
}

/// MacRomanEncoding codes 128 to 255 as the pdf standard lists them.
const MAC_ROMAN_HIGH: &str = "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»…\u{a0}ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄¤‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ\u{f8ff}ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";

fn mac_roman_char(code: u8) -> Option<char> {
    match code {
        32..=126 => Some(char::from(code)),
        128..=255 => MAC_ROMAN_HIGH.chars().nth(usize::from(code - 128)),
        _ => None,
    }
}

/// StandardEncoding: ascii except for curly quotes at 0x27 and 0x60, and a
/// different set of signs and accents above 0xa0.
fn standard_glyph_name(code: u8) -> Option<&'static str> {
    const HIGH: [(u8, &str); 56] = [
        (0xa1, "exclamdown"),
        (0xa2, "cent"),
        (0xa3, "sterling"),
        (0xa4, "fraction"),
        (0xa5, "yen"),
        (0xa6, "florin"),
        (0xa7, "section"),
        (0xa8, "currency"),
        (0xa9, "quotesingle"),
        (0xaa, "quotedblleft"),
        (0xab, "guillemotleft"),
        (0xac, "guilsinglleft"),
        (0xad, "guilsinglright"),
        (0xae, "fi"),
        (0xaf, "fl"),
        (0xb1, "endash"),
        (0xb2, "dagger"),
        (0xb3, "daggerdbl"),
        (0xb4, "periodcentered"),
        (0xb6, "paragraph"),
        (0xb7, "bullet"),
        (0xb8, "quotesinglbase"),
        (0xb9, "quotedblbase"),
        (0xba, "quotedblright"),
        (0xbb, "guillemotright"),
        (0xbc, "ellipsis"),
        (0xbd, "perthousand"),
        (0xbf, "questiondown"),
        (0xc1, "grave"),
        (0xc2, "acute"),
        (0xc3, "circumflex"),
        (0xc4, "tilde"),
        (0xc5, "macron"),
        (0xc6, "breve"),
        (0xc7, "dotaccent"),
        (0xc8, "dieresis"),
        (0xca, "ring"),
        (0xcb, "cedilla"),
        (0xcd, "hungarumlaut"),
        (0xce, "ogonek"),
        (0xcf, "caron"),
        (0xd0, "emdash"),
        (0xe1, "AE"),
        (0xe3, "ordfeminine"),
        (0xe8, "Lslash"),
        (0xe9, "Oslash"),
        (0xea, "OE"),
        (0xeb, "ordmasculine"),
        (0xf1, "ae"),
        (0xf5, "dotlessi"),
        (0xf8, "lslash"),
        (0xf9, "oslash"),
        (0xfa, "oe"),
        (0xfb, "germandbls"),
        (0x27, "quoteright"),
        (0x60, "quoteleft"),
    ];
    if let Some((_, name)) = HIGH.iter().find(|(entry, _)| *entry == code) {
        return Some(*name);
    }
    match code {
        32..=126 => name_of_char(char::from(code)),
        _ => None,
    }
}

/// Adobe Glyph List names for every character the three encodings hold.
const GLYPH_NAMES: &[(char, &str)] = &[
    (' ', "space"),
    ('!', "exclam"),
    ('"', "quotedbl"),
    ('#', "numbersign"),
    ('$', "dollar"),
    ('%', "percent"),
    ('&', "ampersand"),
    ('\'', "quotesingle"),
    ('(', "parenleft"),
    (')', "parenright"),
    ('*', "asterisk"),
    ('+', "plus"),
    (',', "comma"),
    ('-', "hyphen"),
    ('.', "period"),
    ('/', "slash"),
    ('0', "zero"),
    ('1', "one"),
    ('2', "two"),
    ('3', "three"),
    ('4', "four"),
    ('5', "five"),
    ('6', "six"),
    ('7', "seven"),
    ('8', "eight"),
    ('9', "nine"),
    (':', "colon"),
    (';', "semicolon"),
    ('<', "less"),
    ('=', "equal"),
    ('>', "greater"),
    ('?', "question"),
    ('@', "at"),
    ('[', "bracketleft"),
    ('\\', "backslash"),
    (']', "bracketright"),
    ('^', "asciicircum"),
    ('_', "underscore"),
    ('`', "grave"),
    ('{', "braceleft"),
    ('|', "bar"),
    ('}', "braceright"),
    ('~', "asciitilde"),
    ('€', "Euro"),
    ('‚', "quotesinglbase"),
    ('ƒ', "florin"),
    ('„', "quotedblbase"),
    ('…', "ellipsis"),
    ('†', "dagger"),
    ('‡', "daggerdbl"),
    ('ˆ', "circumflex"),
    ('‰', "perthousand"),
    ('Š', "Scaron"),
    ('‹', "guilsinglleft"),
    ('Œ', "OE"),
    ('Ž', "Zcaron"),
    ('‘', "quoteleft"),
    ('’', "quoteright"),
    ('“', "quotedblleft"),
    ('”', "quotedblright"),
    ('•', "bullet"),
    ('–', "endash"),
    ('—', "emdash"),
    ('˜', "tilde"),
    ('™', "trademark"),
    ('š', "scaron"),
    ('›', "guilsinglright"),
    ('œ', "oe"),
    ('ž', "zcaron"),
    ('Ÿ', "Ydieresis"),
    ('\u{a0}', "space"),
    ('¡', "exclamdown"),
    ('¢', "cent"),
    ('£', "sterling"),
    ('¤', "currency"),
    ('¥', "yen"),
    ('¦', "brokenbar"),
    ('§', "section"),
    ('¨', "dieresis"),
    ('©', "copyright"),
    ('ª', "ordfeminine"),
    ('«', "guillemotleft"),
    ('¬', "logicalnot"),
    ('\u{ad}', "hyphen"),
    ('®', "registered"),
    ('¯', "macron"),
    ('°', "degree"),
    ('±', "plusminus"),
    ('²', "twosuperior"),
    ('³', "threesuperior"),
    ('´', "acute"),
    ('µ', "mu"),
    ('¶', "paragraph"),
    ('·', "periodcentered"),
    ('¸', "cedilla"),
    ('¹', "onesuperior"),
    ('º', "ordmasculine"),
    ('»', "guillemotright"),
    ('¼', "onequarter"),
    ('½', "onehalf"),
    ('¾', "threequarters"),
    ('¿', "questiondown"),
    ('À', "Agrave"),
    ('Á', "Aacute"),
    ('Â', "Acircumflex"),
    ('Ã', "Atilde"),
    ('Ä', "Adieresis"),
    ('Å', "Aring"),
    ('Æ', "AE"),
    ('Ç', "Ccedilla"),
    ('È', "Egrave"),
    ('É', "Eacute"),
    ('Ê', "Ecircumflex"),
    ('Ë', "Edieresis"),
    ('Ì', "Igrave"),
    ('Í', "Iacute"),
    ('Î', "Icircumflex"),
    ('Ï', "Idieresis"),
    ('Ð', "Eth"),
    ('Ñ', "Ntilde"),
    ('Ò', "Ograve"),
    ('Ó', "Oacute"),
    ('Ô', "Ocircumflex"),
    ('Õ', "Otilde"),
    ('Ö', "Odieresis"),
    ('×', "multiply"),
    ('Ø', "Oslash"),
    ('Ù', "Ugrave"),
    ('Ú', "Uacute"),
    ('Û', "Ucircumflex"),
    ('Ü', "Udieresis"),
    ('Ý', "Yacute"),
    ('Þ', "Thorn"),
    ('ß', "germandbls"),
    ('à', "agrave"),
    ('á', "aacute"),
    ('â', "acircumflex"),
    ('ã', "atilde"),
    ('ä', "adieresis"),
    ('å', "aring"),
    ('æ', "ae"),
    ('ç', "ccedilla"),
    ('è', "egrave"),
    ('é', "eacute"),
    ('ê', "ecircumflex"),
    ('ë', "edieresis"),
    ('ì', "igrave"),
    ('í', "iacute"),
    ('î', "icircumflex"),
    ('ï', "idieresis"),
    ('ð', "eth"),
    ('ñ', "ntilde"),
    ('ò', "ograve"),
    ('ó', "oacute"),
    ('ô', "ocircumflex"),
    ('õ', "otilde"),
    ('ö', "odieresis"),
    ('÷', "divide"),
    ('ø', "oslash"),
    ('ù', "ugrave"),
    ('ú', "uacute"),
    ('û', "ucircumflex"),
    ('ü', "udieresis"),
    ('ý', "yacute"),
    ('þ', "thorn"),
    ('ÿ', "ydieresis"),
    ('⁄', "fraction"),
    ('ﬁ', "fi"),
    ('ﬂ', "fl"),
    ('ı', "dotlessi"),
    ('Ł', "Lslash"),
    ('ł', "lslash"),
    ('˘', "breve"),
    ('˙', "dotaccent"),
    ('˚', "ring"),
    ('˝', "hungarumlaut"),
    ('˛', "ogonek"),
    ('ˇ', "caron"),
    ('−', "minus"),
    ('≠', "notequal"),
    ('∞', "infinity"),
    ('≤', "lessequal"),
    ('≥', "greaterequal"),
    ('∂', "partialdiff"),
    ('∑', "summation"),
    ('∏', "product"),
    ('π', "pi"),
    ('∫', "integral"),
    ('Ω', "Omega"),
    ('√', "radical"),
    ('≈', "approxequal"),
    ('∆', "Delta"),
    ('◊', "lozenge"),
    ('\u{f8ff}', "apple"),
];

/// the glyph name of a character, when it has a standard one.
pub(crate) fn name_of_char(character: char) -> Option<&'static str> {
    if character.is_ascii_alphabetic() {
        // letters are named by themselves; the table can't hold &'static
        // names made at run time, so look them up in a fixed string
        const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let index = LETTERS.find(character)?;
        return Some(&LETTERS[index..index + 1]);
    }
    GLYPH_NAMES
        .iter()
        .find(|(entry, _)| *entry == character)
        .map(|(_, name)| *name)
}

/// the character(s) a glyph name stands for, following the Adobe Glyph
/// List rules: a known name, "uniXXXX" (one or more), or "uXXXX[XX]".
/// suffixes after a dot ("a.sc") and ligature parts ("f_i") are handled.
pub(crate) fn chars_of_name(name: &str) -> Option<String> {
    let base = name.split('.').next().unwrap_or(name);
    if base.contains('_') {
        return base.split('_').map(chars_of_name).collect();
    }
    if base.len() == 1 && base.as_bytes()[0].is_ascii_alphabetic() {
        return Some(base.to_owned());
    }
    if let Some((character, _)) = GLYPH_NAMES.iter().find(|(_, entry)| *entry == base) {
        return Some(character.to_string());
    }
    if let Some(hex) = base.strip_prefix("uni") {
        if hex.len() >= 4 && hex.len() % 4 == 0 {
            return hex
                .as_bytes()
                .chunks(4)
                .map(|chunk| {
                    let value = u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
                    char::from_u32(value)
                })
                .collect();
        }
    }
    if let Some(hex) = base.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) {
            let value = u32::from_str_radix(hex, 16).ok()?;
            return char::from_u32(value).map(String::from);
        }
    }
    // names like "nbspace" and "sfthyphen" that a few fonts use
    match base {
        "nbspace" | "nonbreakingspace" => Some("\u{a0}".into()),
        "sfthyphen" | "softhyphen" => Some("\u{ad}".into()),
        "periodcentered" | "middot" => Some("·".into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_encodings_name_their_bytes() {
        assert_eq!(BaseEncoding::WinAnsi.glyph_name(0xe9), Some("eacute"));
        assert_eq!(BaseEncoding::WinAnsi.glyph_name(0x80), Some("Euro"));
        assert_eq!(BaseEncoding::WinAnsi.glyph_name(0x81), None);
        assert_eq!(BaseEncoding::Standard.glyph_name(0x27), Some("quoteright"));
        assert_eq!(BaseEncoding::Standard.glyph_name(0xe1), Some("AE"));
        assert_eq!(BaseEncoding::MacRoman.glyph_name(0x8e), Some("eacute"));
        assert_eq!(BaseEncoding::MacRoman.glyph_name(0xff), Some("caron"));
        assert_eq!(BaseEncoding::Standard.glyph_name(b'A'), Some("A"));
    }

    #[test]
    fn glyph_names_become_characters() {
        assert_eq!(chars_of_name("eacute").as_deref(), Some("é"));
        assert_eq!(chars_of_name("uni00E9").as_deref(), Some("é"));
        assert_eq!(chars_of_name("u1F600").as_deref(), Some("😀"));
        assert_eq!(chars_of_name("a.sc").as_deref(), Some("a"));
        assert_eq!(chars_of_name("f_i").as_deref(), Some("fi"));
        assert_eq!(chars_of_name("g123"), None);
        assert_eq!(win_ansi_code('é'), Some(0xe9));
    }
}
