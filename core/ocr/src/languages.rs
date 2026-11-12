//! the language codes tesseract uses, and their names in plain english.
//!
//! tesseract names its language data files by three letter codes
//! (iso 639-2), sometimes with a script after an underscore, such as
//! "chi_sim" for simplified chinese.

use crate::OcrError;

/// the languages fetched by tools/fetch-tesseract and offered by default.
pub const BUNDLED_LANGUAGES: [&str; 4] = ["eng", "spa", "fra", "deu"];

/// code and english name of languages a person is likely to pick.
/// a code missing here still works; it is just shown as the code.
const NAMES: &[(&str, &str)] = &[
    ("ara", "Arabic"),
    ("ces", "Czech"),
    ("chi_sim", "Chinese (simplified)"),
    ("chi_tra", "Chinese (traditional)"),
    ("dan", "Danish"),
    ("deu", "German"),
    ("ell", "Greek"),
    ("eng", "English"),
    ("fin", "Finnish"),
    ("fra", "French"),
    ("heb", "Hebrew"),
    ("hin", "Hindi"),
    ("hun", "Hungarian"),
    ("ita", "Italian"),
    ("jpn", "Japanese"),
    ("kor", "Korean"),
    ("lat", "Latin"),
    ("nld", "Dutch"),
    ("nor", "Norwegian"),
    ("pol", "Polish"),
    ("por", "Portuguese"),
    ("ron", "Romanian"),
    ("rus", "Russian"),
    ("spa", "Spanish"),
    ("swe", "Swedish"),
    ("tur", "Turkish"),
    ("ukr", "Ukrainian"),
    ("vie", "Vietnamese"),
];

/// the english name of a language code, or the code itself when unknown.
pub fn language_name(code: &str) -> String {
    NAMES
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, name)| (*name).to_owned())
        .unwrap_or_else(|| code.to_owned())
}

/// checks that every code looks like a tesseract language code.
///
/// the codes become part of tesseract's command line, so anything that
/// could be read as an option or a path is refused here.
pub fn check_codes(codes: &[String]) -> Result<(), OcrError> {
    if codes.is_empty() {
        return Err(OcrError::NoLanguage);
    }
    for code in codes {
        let well_formed = !code.is_empty()
            && code.len() <= 16
            && code
                .chars()
                .all(|letter| letter.is_ascii_lowercase() || letter == '_')
            && !code.starts_with('_');
        if !well_formed {
            return Err(OcrError::UnknownLanguage(code.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_have_names() {
        assert_eq!(language_name("spa"), "Spanish");
        assert_eq!(language_name("xyz"), "xyz");
    }

    #[test]
    fn odd_codes_are_refused() {
        let refused = |code: &str| check_codes(&[code.to_owned()]).is_err();
        assert!(refused("--psm"));
        assert!(refused("../eng"));
        assert!(refused("ENG"));
        assert!(refused(""));
        assert!(!refused("chi_sim"));
        assert_eq!(check_codes(&[]), Err(OcrError::NoLanguage));
    }
}
