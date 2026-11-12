//! which writing systems and languages a font can draw.
//!
//! for a font on this computer the answer comes from its character map:
//! a writing system counts when the font has every one of a few letters
//! that only fonts made for it have (a font with "а" and "я" was made
//! for Cyrillic). for a font in the online library it comes from the
//! catalogue's subsets, which the library's makers checked the same way.

use serde::Serialize;

/// one writing system or language a person can filter by.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Writing {
    /// a short stable id, also used in the catalogue: "vietnamese"
    pub id: &'static str,
    /// the name shown to the person: "Vietnamese"
    pub name: &'static str,
    /// letters a font must have to count as supporting it
    pub letters: &'static str,
    /// the online library subsets that mean the font supports it
    pub subsets: &'static [&'static str],
    /// other words someone might type for it
    pub aliases: &'static [&'static str],
}

const WESTERN: &[&str] = &["latin"];
const CENTRAL: &[&str] = &["latin-ext"];

/// every writing system and language the library knows, in the order
/// the filter lists them.
pub const WRITING: &[Writing] = &[
    w(
        "latin",
        "Latin (English)",
        "AZaz09",
        WESTERN,
        &["english", "basic latin"],
    ),
    w("french", "French", "éèêàçôûœ", WESTERN, &["français"]),
    w("german", "German", "äöüßÄÖÜ", WESTERN, &["deutsch"]),
    w("spanish", "Spanish", "ñáéíóú¿¡", WESTERN, &["español"]),
    w(
        "portuguese",
        "Portuguese",
        "ãõçâêô",
        WESTERN,
        &["português"],
    ),
    w("italian", "Italian", "àèéìòù", WESTERN, &["italiano"]),
    w("polish", "Polish", "ąćęłńśźż", CENTRAL, &["polski"]),
    w(
        "czech",
        "Czech",
        "čďěňřšťůž",
        CENTRAL,
        &["čeština", "slovak"],
    ),
    w("turkish", "Turkish", "ğışİŞĞ", CENTRAL, &["türkçe"]),
    w("romanian", "Romanian", "ăâîșț", CENTRAL, &["română"]),
    w("hungarian", "Hungarian", "őűŐŰ", CENTRAL, &["magyar"]),
    w(
        "vietnamese",
        "Vietnamese",
        "ăâđêôơưạảấầẩẫậắằẳẵặẹẻẽếềểễệỉịọỏốồổỗộớờởỡợụủứừửữựỳỵỷỹĐ",
        &["vietnamese"],
        &["tiếng việt", "viet"],
    ),
    w("greek", "Greek", "ΑΒΓΔΩαβγδωάέ", &["greek"], &["ελληνικά"]),
    w(
        "cyrillic",
        "Cyrillic (Russian)",
        "АБВГДЯабвгдя",
        &["cyrillic"],
        &["russian", "русский", "bulgarian", "serbian"],
    ),
    w(
        "ukrainian",
        "Ukrainian",
        "ЄІЇҐєіїґ",
        &["cyrillic", "cyrillic-ext"],
        &["українська"],
    ),
    w("armenian", "Armenian", "ԱԲԳաբգ", &["armenian"], &[]),
    w("georgian", "Georgian", "აბგდევ", &["georgian"], &[]),
    w("hebrew", "Hebrew", "אבגדהו", &["hebrew"], &["עברית"]),
    w(
        "arabic",
        "Arabic",
        "ابتثجحخ",
        &["arabic"],
        &["العربية", "persian", "farsi", "urdu"],
    ),
    w(
        "devanagari",
        "Devanagari (Hindi)",
        "अआइकखग",
        &["devanagari"],
        &["hindi", "marathi", "nepali", "sanskrit"],
    ),
    w("bengali", "Bengali", "অআইকখগ", &["bengali"], &["bangla"]),
    w(
        "gurmukhi",
        "Gurmukhi (Punjabi)",
        "ਅਆਇਕਖਗ",
        &["gurmukhi"],
        &["punjabi"],
    ),
    w("gujarati", "Gujarati", "અઆઇકખગ", &["gujarati"], &[]),
    w("tamil", "Tamil", "அஆஇகஙச", &["tamil"], &[]),
    w("telugu", "Telugu", "అఆఇకఖగ", &["telugu"], &[]),
    w("kannada", "Kannada", "ಅಆಇಕಖಗ", &["kannada"], &[]),
    w("malayalam", "Malayalam", "അആഇകഖഗ", &["malayalam"], &[]),
    w("sinhala", "Sinhala", "අආඇකඛග", &["sinhala"], &[]),
    w("thai", "Thai", "กขคงจฉ", &["thai"], &["ภาษาไทย"]),
    w("lao", "Lao", "ກຂຄງຈ", &["lao"], &[]),
    w("khmer", "Khmer", "កខគឃង", &["khmer"], &["cambodian"]),
    w(
        "myanmar",
        "Myanmar (Burmese)",
        "ကခဂဃင",
        &["myanmar"],
        &["burmese"],
    ),
    w(
        "ethiopic",
        "Ethiopic (Amharic)",
        "ሀለሐመሠ",
        &["ethiopic"],
        &["amharic"],
    ),
    w(
        "japanese",
        "Japanese",
        "あいうアイウ日本語",
        &["japanese"],
        &["日本語", "kana", "hiragana", "katakana"],
    ),
    w(
        "korean",
        "Korean",
        "가나다한글",
        &["korean"],
        &["한국어", "hangul"],
    ),
    w(
        "chinese-simplified",
        "Chinese (Simplified)",
        "中国汉语们这说",
        &["chinese-simplified"],
        &["chinese", "简体中文", "mandarin"],
    ),
    w(
        "chinese-traditional",
        "Chinese (Traditional)",
        "中國漢語們這說",
        &["chinese-traditional", "chinese-hongkong"],
        &["繁體中文", "cantonese", "taiwan"],
    ),
    w(
        "math",
        "Math Symbols",
        "∀∂∃∅∇∈∑√∞∫≈≠≤≥",
        &["math"],
        &["mathematics", "maths"],
    ),
];

const fn w(
    id: &'static str,
    name: &'static str,
    letters: &'static str,
    subsets: &'static [&'static str],
    aliases: &'static [&'static str],
) -> Writing {
    Writing {
        id,
        name,
        letters,
        subsets,
        aliases,
    }
}

/// the writing system with this id.
pub fn writing(id: &str) -> Option<&'static Writing> {
    WRITING.iter().find(|writing| writing.id == id)
}

/// the writing system someone means by a word ("greek", "Tiếng Việt").
pub fn writing_named(word: &str) -> Option<&'static Writing> {
    let word = word.trim().to_lowercase();
    WRITING.iter().find(|writing| {
        writing.id == word
            || writing.name.to_lowercase() == word
            || writing
                .name
                .to_lowercase()
                .split([' ', '(', ')'])
                .any(|part| !part.is_empty() && part == word)
            || writing.aliases.iter().any(|alias| *alias == word)
    })
}

/// the writing systems a font draws, given a way to ask whether it has a
/// character.
pub fn writing_of(has: impl Fn(char) -> bool) -> Vec<String> {
    WRITING
        .iter()
        .filter(|writing| writing.letters.chars().all(&has))
        .map(|writing| writing.id.to_string())
        .collect()
}

/// the writing systems an online library family covers, from its subsets.
/// western european languages need only "latin"; the rest name their own.
pub fn writing_of_subsets(subsets: &[String]) -> Vec<String> {
    WRITING
        .iter()
        .filter(|writing| {
            writing
                .subsets
                .iter()
                .any(|subset| subsets.iter().any(|have| have == subset))
        })
        .map(|writing| writing.id.to_string())
        .collect()
}

/// sorted, merged ranges of the given code points.
pub fn merge_ranges(mut points: Vec<u32>) -> Vec<(u32, u32)> {
    points.sort_unstable();
    points.dedup();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for point in points {
        match ranges.last_mut() {
            Some(last) if last.1 + 1 == point => last.1 = point,
            _ => ranges.push((point, point)),
        }
    }
    ranges
}

/// true when `c` is in one of the sorted ranges.
pub fn ranges_contain(ranges: &[(u32, u32)], c: char) -> bool {
    let point = c as u32;
    let after = ranges.partition_point(|range| range.0 <= point);
    after > 0 && ranges[after - 1].1 >= point
}

/// the characters each online library subset holds, as the library's
/// own stylesheets declare them (the unicode-range of each subset). a
/// family not downloaded yet "can draw" a character when one of its
/// subsets covers it; the answer says "probably" for that reason.
pub fn subset_ranges(subset: &str) -> &'static [(u32, u32)] {
    match subset {
        "latin" => &[
            (0x0000, 0x00ff),
            (0x0131, 0x0131),
            (0x0152, 0x0153),
            (0x02bb, 0x02bc),
            (0x02c6, 0x02c6),
            (0x02da, 0x02da),
            (0x02dc, 0x02dc),
            (0x0304, 0x0304),
            (0x0308, 0x0308),
            (0x0329, 0x0329),
            (0x2000, 0x206f),
            (0x20ac, 0x20ac),
            (0x2122, 0x2122),
            (0x2191, 0x2191),
            (0x2193, 0x2193),
            (0x2212, 0x2212),
            (0x2215, 0x2215),
            (0xfeff, 0xfeff),
            (0xfffd, 0xfffd),
        ],
        "latin-ext" => &[
            (0x0100, 0x02ba),
            (0x02bd, 0x02c5),
            (0x02c7, 0x02cc),
            (0x02ce, 0x02d7),
            (0x02dd, 0x02ff),
            (0x0304, 0x0304),
            (0x0308, 0x0308),
            (0x0329, 0x0329),
            (0x1d00, 0x1dbf),
            (0x1e00, 0x1e9f),
            (0x1ef2, 0x1eff),
            (0x2020, 0x2020),
            (0x20a0, 0x20ab),
            (0x20ad, 0x20c0),
            (0x2113, 0x2113),
            (0x2c60, 0x2c7f),
            (0xa720, 0xa7ff),
        ],
        "vietnamese" => &[
            (0x0102, 0x0103),
            (0x0110, 0x0111),
            (0x0128, 0x0129),
            (0x0168, 0x0169),
            (0x01a0, 0x01a1),
            (0x01af, 0x01b0),
            (0x0300, 0x0301),
            (0x0303, 0x0304),
            (0x0308, 0x0309),
            (0x0323, 0x0323),
            (0x0329, 0x0329),
            (0x1ea0, 0x1ef9),
            (0x20ab, 0x20ab),
        ],
        "greek" => &[
            (0x0370, 0x0377),
            (0x037a, 0x037f),
            (0x0384, 0x038a),
            (0x038c, 0x038c),
            (0x038e, 0x03a1),
            (0x03a3, 0x03ff),
        ],
        "greek-ext" => &[(0x1f00, 0x1fff)],
        "cyrillic" => &[
            (0x0301, 0x0301),
            (0x0400, 0x045f),
            (0x0490, 0x0491),
            (0x04b0, 0x04b1),
            (0x2116, 0x2116),
        ],
        "cyrillic-ext" => &[
            (0x0460, 0x052f),
            (0x1c80, 0x1c8a),
            (0x20b4, 0x20b4),
            (0x2de0, 0x2dff),
            (0xa640, 0xa69f),
            (0xfe2e, 0xfe2f),
        ],
        "armenian" => &[(0x0530, 0x058f), (0xfb13, 0xfb17)],
        "georgian" => &[(0x10a0, 0x10ff), (0x1c90, 0x1cbf), (0x2d00, 0x2d2f)],
        "hebrew" => &[
            (0x0590, 0x05ff),
            (0x200c, 0x2010),
            (0x20aa, 0x20aa),
            (0x25cc, 0x25cc),
            (0xfb1d, 0xfb4f),
        ],
        "arabic" => &[
            (0x0600, 0x06ff),
            (0x0750, 0x077f),
            (0x0870, 0x08ff),
            (0xfb50, 0xfdff),
            (0xfe70, 0xfeff),
        ],
        "devanagari" => &[
            (0x0900, 0x097f),
            (0x1cd0, 0x1cf9),
            (0x200c, 0x200d),
            (0x20a8, 0x20a8),
            (0x20b9, 0x20b9),
            (0x25cc, 0x25cc),
            (0xa830, 0xa839),
            (0xa8e0, 0xa8ff),
        ],
        "bengali" => &[(0x0980, 0x09fe), (0x200c, 0x200d), (0x25cc, 0x25cc)],
        "gurmukhi" => &[(0x0a00, 0x0a76), (0x200c, 0x200d), (0x25cc, 0x25cc)],
        "gujarati" => &[(0x0a80, 0x0aff), (0x200c, 0x200d), (0x25cc, 0x25cc)],
        "tamil" => &[
            (0x0b80, 0x0bfa),
            (0x200c, 0x200d),
            (0x25cc, 0x25cc),
            (0x11fc0, 0x11fff),
        ],
        "telugu" => &[(0x0c00, 0x0c7f), (0x200c, 0x200d), (0x25cc, 0x25cc)],
        "kannada" => &[(0x0c80, 0x0cf3), (0x200c, 0x200d), (0x25cc, 0x25cc)],
        "malayalam" => &[(0x0d00, 0x0d7f), (0x200c, 0x200d), (0x25cc, 0x25cc)],
        "sinhala" => &[
            (0x0d80, 0x0dff),
            (0x200c, 0x200d),
            (0x25cc, 0x25cc),
            (0x111e1, 0x111f4),
        ],
        "thai" => &[(0x0e01, 0x0e5b), (0x200c, 0x200d), (0x25cc, 0x25cc)],
        "lao" => &[(0x0e81, 0x0edf), (0x200c, 0x200d), (0x25cc, 0x25cc)],
        "khmer" => &[
            (0x1780, 0x17ff),
            (0x19e0, 0x19ff),
            (0x200c, 0x200d),
            (0x25cc, 0x25cc),
        ],
        "myanmar" => &[
            (0x1000, 0x109f),
            (0x200c, 0x200d),
            (0x25cc, 0x25cc),
            (0xa9e0, 0xa9fe),
            (0xaa60, 0xaa7f),
        ],
        "ethiopic" => &[(0x1200, 0x139f), (0x2d80, 0x2dde), (0xab01, 0xab2e)],
        "japanese" => &[(0x3000, 0x30ff), (0x4e00, 0x9fff), (0xff00, 0xffef)],
        "korean" => &[(0x1100, 0x11ff), (0x3130, 0x318f), (0xac00, 0xd7af)],
        "chinese-simplified" | "chinese-traditional" | "chinese-hongkong" => {
            &[(0x3000, 0x303f), (0x4e00, 0x9fff), (0xff00, 0xffef)]
        }
        "math" => &[
            (0x2200, 0x22ff),
            (0x27c0, 0x27ef),
            (0x2980, 0x2aff),
            (0x1d400, 0x1d7ff),
        ],
        "symbols" => &[
            (0x2190, 0x21ff),
            (0x2300, 0x23ff),
            (0x2460, 0x27bf),
            (0x2b00, 0x2bff),
        ],
        _ => &[],
    }
}

/// whether a family that isn't downloaded can probably draw `c`.
pub fn subsets_contain(subsets: &[String], c: char) -> bool {
    subsets
        .iter()
        .any(|subset| ranges_contain(subset_ranges(subset), c))
}

/// the characters of `sample` a font is missing, ignoring spaces and
/// controls (which no font needs to draw).
pub fn missing_characters(sample: &str, has: impl Fn(char) -> bool) -> Vec<char> {
    let mut missing: Vec<char> = sample
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .filter(|c| !has(*c))
        .collect();
    missing.dedup();
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_decide_writing_systems() {
        let latin_and_greek =
            |c: char| c.is_ascii_alphanumeric() || ('\u{0370}'..='\u{03ff}').contains(&c);
        let found = writing_of(latin_and_greek);
        assert!(found.contains(&"latin".to_string()));
        assert!(found.contains(&"greek".to_string()));
        assert!(!found.contains(&"cyrillic".to_string()));
        assert!(!found.contains(&"vietnamese".to_string()));
    }

    #[test]
    fn subsets_decide_writing_for_library_fonts() {
        let found = writing_of_subsets(&["latin".into(), "vietnamese".into()]);
        assert!(found.contains(&"french".to_string()));
        assert!(found.contains(&"vietnamese".to_string()));
        assert!(!found.contains(&"polish".to_string()));
    }

    #[test]
    fn people_can_name_writing_systems_their_way() {
        assert_eq!(writing_named("Greek").unwrap().id, "greek");
        assert_eq!(writing_named("russian").unwrap().id, "cyrillic");
        assert_eq!(writing_named("Cyrillic").unwrap().id, "cyrillic");
        assert_eq!(writing_named("hindi").unwrap().id, "devanagari");
        assert_eq!(writing_named("tiếng việt").unwrap().id, "vietnamese");
        assert!(writing_named("garamond").is_none());
    }

    #[test]
    fn ranges_answer_quickly() {
        let ranges = merge_ranges(vec![0x41, 0x42, 0x43, 0x61, 0x20ab]);
        assert_eq!(ranges, vec![(0x41, 0x43), (0x61, 0x61), (0x20ab, 0x20ab)]);
        assert!(ranges_contain(&ranges, 'B'));
        assert!(ranges_contain(&ranges, '₫'));
        assert!(!ranges_contain(&ranges, 'D'));
        assert!(!ranges_contain(&[], 'a'));
        let vietnamese = vec!["latin".to_string(), "vietnamese".to_string()];
        assert!("ĐỒNG ₫"
            .chars()
            .filter(|c| *c != ' ')
            .all(|c| subsets_contain(&vietnamese, c)));
        assert!(!subsets_contain(&["latin".to_string()], 'Ồ'));
    }

    #[test]
    fn missing_characters_ignore_spaces() {
        let ascii = |c: char| c.is_ascii();
        assert_eq!(missing_characters("ĐỒNG ₫ ok", ascii), vec!['Đ', 'Ồ', '₫']);
        assert!(missing_characters("plain text", ascii).is_empty());
    }
}
