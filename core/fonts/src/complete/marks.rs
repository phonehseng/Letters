//! which accented letters are a base letter plus a mark, for the latin
//! letters of western, central and northern european languages (latin-1
//! and latin extended-a). written out by hand from the unicode
//! decompositions, so no normalisation library is needed.

/// an accent or other mark that sits on a letter.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub enum Mark {
    Grave,
    Acute,
    Circumflex,
    Tilde,
    Diaeresis,
    Ring,
    Cedilla,
    Caron,
    Macron,
    Breve,
    DotAbove,
    Ogonek,
    DoubleAcute,
}

impl Mark {
    /// true for marks drawn under the letter.
    pub fn is_below(self) -> bool {
        matches!(self, Mark::Cedilla | Mark::Ogonek)
    }

    /// the spacing characters that draw the mark alone, which a font may
    /// have even when it lacks the accented letter.
    pub fn spacing_chars(self) -> &'static [char] {
        match self {
            Mark::Grave => &['`', '\u{02CB}'],
            Mark::Acute => &['\u{00B4}', '\u{02CA}'],
            Mark::Circumflex => &['\u{02C6}', '^'],
            Mark::Tilde => &['\u{02DC}'],
            Mark::Diaeresis => &['\u{00A8}'],
            Mark::Ring => &['\u{02DA}'],
            Mark::Cedilla => &['\u{00B8}'],
            Mark::Caron => &['\u{02C7}'],
            Mark::Macron => &['\u{00AF}', '\u{02C9}'],
            Mark::Breve => &['\u{02D8}'],
            Mark::DotAbove => &['\u{02D9}'],
            Mark::Ogonek => &['\u{02DB}'],
            Mark::DoubleAcute => &['\u{02DD}'],
        }
    }

    /// the combining character, which some fonts also have.
    pub fn combining_char(self) -> char {
        match self {
            Mark::Grave => '\u{0300}',
            Mark::Acute => '\u{0301}',
            Mark::Circumflex => '\u{0302}',
            Mark::Tilde => '\u{0303}',
            Mark::Macron => '\u{0304}',
            Mark::Breve => '\u{0306}',
            Mark::DotAbove => '\u{0307}',
            Mark::Diaeresis => '\u{0308}',
            Mark::Ring => '\u{030A}',
            Mark::DoubleAcute => '\u{030B}',
            Mark::Caron => '\u{030C}',
            Mark::Cedilla => '\u{0327}',
            Mark::Ogonek => '\u{0328}',
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mark::Grave => "grave accent",
            Mark::Acute => "acute accent",
            Mark::Circumflex => "circumflex",
            Mark::Tilde => "tilde",
            Mark::Diaeresis => "diaeresis",
            Mark::Ring => "ring",
            Mark::Cedilla => "cedilla",
            Mark::Caron => "caron",
            Mark::Macron => "macron",
            Mark::Breve => "breve",
            Mark::DotAbove => "dot",
            Mark::Ogonek => "ogonek",
            Mark::DoubleAcute => "double acute accent",
        }
    }
}

use Mark::*;

/// accented letter, base letter, mark.
const DECOMPOSITIONS: &[(char, char, Mark)] = &[
    ('À', 'A', Grave),
    ('Á', 'A', Acute),
    ('Â', 'A', Circumflex),
    ('Ã', 'A', Tilde),
    ('Ä', 'A', Diaeresis),
    ('Å', 'A', Ring),
    ('Ç', 'C', Cedilla),
    ('È', 'E', Grave),
    ('É', 'E', Acute),
    ('Ê', 'E', Circumflex),
    ('Ë', 'E', Diaeresis),
    ('Ì', 'I', Grave),
    ('Í', 'I', Acute),
    ('Î', 'I', Circumflex),
    ('Ï', 'I', Diaeresis),
    ('Ñ', 'N', Tilde),
    ('Ò', 'O', Grave),
    ('Ó', 'O', Acute),
    ('Ô', 'O', Circumflex),
    ('Õ', 'O', Tilde),
    ('Ö', 'O', Diaeresis),
    ('Ù', 'U', Grave),
    ('Ú', 'U', Acute),
    ('Û', 'U', Circumflex),
    ('Ü', 'U', Diaeresis),
    ('Ý', 'Y', Acute),
    ('à', 'a', Grave),
    ('á', 'a', Acute),
    ('â', 'a', Circumflex),
    ('ã', 'a', Tilde),
    ('ä', 'a', Diaeresis),
    ('å', 'a', Ring),
    ('ç', 'c', Cedilla),
    ('è', 'e', Grave),
    ('é', 'e', Acute),
    ('ê', 'e', Circumflex),
    ('ë', 'e', Diaeresis),
    ('ì', 'ı', Grave),
    ('í', 'ı', Acute),
    ('î', 'ı', Circumflex),
    ('ï', 'ı', Diaeresis),
    ('ñ', 'n', Tilde),
    ('ò', 'o', Grave),
    ('ó', 'o', Acute),
    ('ô', 'o', Circumflex),
    ('õ', 'o', Tilde),
    ('ö', 'o', Diaeresis),
    ('ù', 'u', Grave),
    ('ú', 'u', Acute),
    ('û', 'u', Circumflex),
    ('ü', 'u', Diaeresis),
    ('ý', 'y', Acute),
    ('ÿ', 'y', Diaeresis),
    ('Ÿ', 'Y', Diaeresis),
    ('Ā', 'A', Macron),
    ('ā', 'a', Macron),
    ('Ă', 'A', Breve),
    ('ă', 'a', Breve),
    ('Ą', 'A', Ogonek),
    ('ą', 'a', Ogonek),
    ('Ć', 'C', Acute),
    ('ć', 'c', Acute),
    ('Ĉ', 'C', Circumflex),
    ('ĉ', 'c', Circumflex),
    ('Ċ', 'C', DotAbove),
    ('ċ', 'c', DotAbove),
    ('Č', 'C', Caron),
    ('č', 'c', Caron),
    ('Ď', 'D', Caron),
    ('Ē', 'E', Macron),
    ('ē', 'e', Macron),
    ('Ĕ', 'E', Breve),
    ('ĕ', 'e', Breve),
    ('Ė', 'E', DotAbove),
    ('ė', 'e', DotAbove),
    ('Ę', 'E', Ogonek),
    ('ę', 'e', Ogonek),
    ('Ě', 'E', Caron),
    ('ě', 'e', Caron),
    ('Ĝ', 'G', Circumflex),
    ('ĝ', 'g', Circumflex),
    ('Ğ', 'G', Breve),
    ('ğ', 'g', Breve),
    ('Ġ', 'G', DotAbove),
    ('ġ', 'g', DotAbove),
    ('Ĥ', 'H', Circumflex),
    ('ĥ', 'h', Circumflex),
    ('Ĩ', 'I', Tilde),
    ('ĩ', 'ı', Tilde),
    ('Ī', 'I', Macron),
    ('ī', 'ı', Macron),
    ('Ĭ', 'I', Breve),
    ('ĭ', 'ı', Breve),
    ('Į', 'I', Ogonek),
    ('į', 'i', Ogonek),
    ('İ', 'I', DotAbove),
    ('Ĵ', 'J', Circumflex),
    ('ĵ', 'ȷ', Circumflex),
    ('Ĺ', 'L', Acute),
    ('ĺ', 'l', Acute),
    ('Ń', 'N', Acute),
    ('ń', 'n', Acute),
    ('Ň', 'N', Caron),
    ('ň', 'n', Caron),
    ('Ō', 'O', Macron),
    ('ō', 'o', Macron),
    ('Ŏ', 'O', Breve),
    ('ŏ', 'o', Breve),
    ('Ő', 'O', DoubleAcute),
    ('ő', 'o', DoubleAcute),
    ('Ŕ', 'R', Acute),
    ('ŕ', 'r', Acute),
    ('Ř', 'R', Caron),
    ('ř', 'r', Caron),
    ('Ś', 'S', Acute),
    ('ś', 's', Acute),
    ('Ŝ', 'S', Circumflex),
    ('ŝ', 's', Circumflex),
    ('Ş', 'S', Cedilla),
    ('ş', 's', Cedilla),
    ('Š', 'S', Caron),
    ('š', 's', Caron),
    ('Ţ', 'T', Cedilla),
    ('ţ', 't', Cedilla),
    ('Ť', 'T', Caron),
    ('Ũ', 'U', Tilde),
    ('ũ', 'u', Tilde),
    ('Ū', 'U', Macron),
    ('ū', 'u', Macron),
    ('Ŭ', 'U', Breve),
    ('ŭ', 'u', Breve),
    ('Ů', 'U', Ring),
    ('ů', 'u', Ring),
    ('Ű', 'U', DoubleAcute),
    ('ű', 'u', DoubleAcute),
    ('Ų', 'U', Ogonek),
    ('ų', 'u', Ogonek),
    ('Ŵ', 'W', Circumflex),
    ('ŵ', 'w', Circumflex),
    ('Ŷ', 'Y', Circumflex),
    ('ŷ', 'y', Circumflex),
    ('Ź', 'Z', Acute),
    ('ź', 'z', Acute),
    ('Ż', 'Z', DotAbove),
    ('ż', 'z', DotAbove),
    ('Ž', 'Z', Caron),
    ('ž', 'z', Caron),
];

/// the base letter and mark of an accented letter. "í" is a dotless ı
/// with an acute: the dot gives way to the accent.
pub fn decompose(c: char) -> Option<(char, Mark)> {
    DECOMPOSITIONS
        .iter()
        .find(|(accented, _, _)| *accented == c)
        .map(|(_, base, mark)| (*base, *mark))
}

/// every accented letter that carries `mark`, for finding the mark in a
/// letter the font already has.
pub fn letters_with(mark: Mark) -> impl Iterator<Item = (char, char)> {
    DECOMPOSITIONS
        .iter()
        .filter(move |(_, _, m)| *m == mark)
        .map(|(accented, base, _)| (*accented, *base))
}

/// the letters a ligature stands for.
pub fn ligature_parts(c: char) -> Option<&'static str> {
    match c {
        'ﬀ' => Some("ff"),
        'ﬁ' => Some("fi"),
        'ﬂ' => Some("fl"),
        'ﬃ' => Some("ffi"),
        'ﬄ' => Some("ffl"),
        'ﬆ' => Some("st"),
        _ => None,
    }
}

/// superscript digits and their ordinary digit.
pub fn superscript_base(c: char) -> Option<char> {
    match c {
        '¹' => Some('1'),
        '²' => Some('2'),
        '³' => Some('3'),
        '⁰' => Some('0'),
        '⁴'..='⁹' => char::from_digit(u32::from(c) - 0x2070, 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accented_letters_come_apart() {
        assert_eq!(decompose('é'), Some(('e', Acute)));
        assert_eq!(decompose('Ü'), Some(('U', Diaeresis)));
        assert_eq!(decompose('í'), Some(('ı', Acute)));
        assert_eq!(decompose('ž'), Some(('z', Caron)));
        assert_eq!(decompose('e'), None);
        assert!(letters_with(Grave).any(|(accented, _)| accented == 'è'));
        assert_eq!(ligature_parts('ﬁ'), Some("fi"));
        assert_eq!(superscript_base('⁵'), Some('5'));
        assert_eq!(superscript_base('²'), Some('2'));
    }

    #[test]
    fn the_table_has_no_repeats() {
        let mut seen = std::collections::BTreeSet::new();
        for (accented, _, _) in DECOMPOSITIONS {
            assert!(seen.insert(*accented), "{accented} twice");
        }
    }
}
