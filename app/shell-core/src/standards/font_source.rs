//! fonts for a pdf/a conversion, from the font library on this computer.
//!
//! a document that only names a font ("Helvetica", "Arial-BoldMT") must
//! have one embedded to be pdf/a. the conversion asks for the font by its
//! postscript-style name; the library knows families ("Arial"). so the
//! name is turned into the family names it may stand for, then the
//! metric-compatible substitutes core/fonts lists (Helvetica: Liberation
//! Sans), and the first family the library has a face of is used.

use letters_fonts::FontLibrary;
use letters_standards::{FontFile, FontRequest, FontSource};

/// css weights the library's closest-face lookup takes
const REGULAR_WEIGHT: u16 = 400;
const BOLD_WEIGHT: u16 = 700;

/// a family to look for, and whether it has the asked font's widths.
#[derive(Debug, PartialEq)]
pub struct Candidate {
    pub family: String,
    pub same_widths: bool,
}

/// the families a font name may stand for, best first: the name itself,
/// then the metric-compatible substitutes. every one of them has the same
/// widths; anything else would be a guess the conversion can't vouch for.
pub fn candidates(request: &FontRequest) -> Vec<Candidate> {
    let mut families = vec![family_of(&request.name)];
    families.extend(request.metric_compatible.iter().cloned());
    let mut seen = Vec::new();
    families
        .into_iter()
        .filter(|family| !family.is_empty())
        .filter(|family| {
            let new = !seen.contains(family);
            seen.push(family.clone());
            new
        })
        .map(|family| Candidate {
            family,
            same_widths: true,
        })
        .collect()
}

/// "Arial-BoldMT" is Arial, "TimesNewRomanPS-ItalicMT" is Times New Roman,
/// "Helvetica,Bold" is Helvetica: the part before the style, with the
/// "MT" and "PS" a foundry adds dropped and words split at capitals.
pub fn family_of(name: &str) -> String {
    let base = name.split(['-', ',']).next().unwrap_or(name);
    let mut base = base.trim();
    for suffix in ["PSMT", "MT", "PS"] {
        if let Some(stripped) = base.strip_suffix(suffix) {
            if !stripped.is_empty() {
                base = stripped;
                break;
            }
        }
    }
    split_words(base)
}

/// "TimesNewRoman" becomes "Times New Roman"; a name with spaces stays.
fn split_words(name: &str) -> String {
    if name.contains(' ') {
        return name.to_owned();
    }
    let mut words = String::new();
    let mut previous_lower = false;
    for character in name.chars() {
        if character.is_uppercase() && previous_lower {
            words.push(' ');
        }
        previous_lower = character.is_lowercase();
        words.push(character);
    }
    words
}

/// the font library as the conversion's font source.
pub struct LibraryFonts<'a> {
    pub library: &'a FontLibrary,
}

impl FontSource for LibraryFonts<'_> {
    fn find(&self, request: &FontRequest) -> Option<FontFile> {
        let weight = if request.bold {
            BOLD_WEIGHT
        } else {
            REGULAR_WEIGHT
        };
        let index = self.library.index();
        candidates(request).into_iter().find_map(|candidate| {
            let face = index.closest_face(&candidate.family, weight, request.italic)?;
            let data = self.library.face_bytes(&face.key).ok()?;
            Some(FontFile {
                data,
                // face_bytes gives one face as a single plain font
                index: 0,
                family: candidate.family,
                same_widths: candidate.same_widths,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postscript_names_become_family_names() {
        assert_eq!(family_of("Arial-BoldMT"), "Arial");
        assert_eq!(family_of("TimesNewRomanPS-ItalicMT"), "Times New Roman");
        assert_eq!(family_of("Helvetica,Bold"), "Helvetica");
        assert_eq!(family_of("CourierNewPSMT"), "Courier New");
        assert_eq!(family_of("Liberation Sans"), "Liberation Sans");
        assert_eq!(family_of("MT"), "MT");
    }

    #[test]
    fn the_name_comes_first_then_its_substitutes_once_each() {
        let request = FontRequest {
            name: "Helvetica-Bold".into(),
            bold: true,
            italic: false,
            metric_compatible: vec!["Liberation Sans".into(), "Helvetica".into()],
        };
        let found: Vec<String> = candidates(&request)
            .into_iter()
            .map(|each| each.family)
            .collect();
        assert_eq!(found, vec!["Helvetica", "Liberation Sans"]);
    }
}
