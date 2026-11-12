//! recognising list markers: bullets ("•", "–") and numbers or letters
//! ("1.", "2)", "(a)", "iv.").

/// a marker found at the start of some text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Marker {
    /// the marker as printed, without the space after it
    pub text: String,
    /// numbered or lettered rather than bulleted
    pub ordered: bool,
    /// a symbol made only to be a bullet (not "-" or "*", which also
    /// start ordinary lines)
    pub strong: bool,
}

const BULLETS: &[char] = &[
    '•', '◦', '▪', '▫', '■', '□', '●', '○', '‣', '⁃', '►', '✓', '✔', '❖', '➢', '\u{f0b7}',
];
const WEAK_BULLETS: &[char] = &['-', '*', '–', '—', '+'];

/// the list marker the text starts with, if it starts with one followed
/// by a space (or is nothing but the marker).
pub(crate) fn list_marker(text: &str) -> Option<Marker> {
    let text = text.trim_start();
    let first = text.chars().next()?;
    let rest = &text[first.len_utf8()..];
    let followed_by_space = rest.is_empty() || rest.starts_with(char::is_whitespace);
    if BULLETS.contains(&first) && followed_by_space {
        return Some(Marker {
            text: first.to_string(),
            ordered: false,
            strong: true,
        });
    }
    if WEAK_BULLETS.contains(&first) && followed_by_space {
        return Some(Marker {
            text: first.to_string(),
            ordered: false,
            strong: false,
        });
    }
    numbered_marker(text)
}

/// "12.", "3)", "(4)", "b.", "C)", "iv." at the start of the text.
fn numbered_marker(text: &str) -> Option<Marker> {
    let token: String = text.chars().take_while(|c| !c.is_whitespace()).collect();
    let after = &text[token.len()..];
    if !(after.is_empty() || after.starts_with(char::is_whitespace)) {
        return None;
    }
    let (inner, closer) = if let Some(inner) = token.strip_prefix('(') {
        (inner.strip_suffix(')')?, ')')
    } else if let Some(inner) = token.strip_suffix('.') {
        (inner, '.')
    } else {
        (token.strip_suffix(')')?, ')')
    };
    let numbered =
        !inner.is_empty() && inner.len() <= 3 && inner.chars().all(|c| c.is_ascii_digit());
    let lettered = inner.chars().count() == 1
        && inner
            .chars()
            .all(|c| c.is_ascii_lowercase() || (c.is_ascii_uppercase() && closer == ')'));
    let roman = (1..=6).contains(&inner.len()) && inner.chars().all(|c| "ivxlc".contains(c));
    (numbered || lettered || roman).then(|| Marker {
        text: token.clone(),
        ordered: true,
        strong: numbered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker_text(text: &str) -> Option<String> {
        list_marker(text).map(|m| m.text)
    }

    #[test]
    fn bullets_and_numbers_are_markers() {
        assert_eq!(marker_text("• Apples"), Some("•".into()));
        assert_eq!(marker_text("- Pears"), Some("-".into()));
        assert_eq!(marker_text("12. Plums"), Some("12.".into()));
        assert_eq!(marker_text("3) Figs"), Some("3)".into()));
        assert_eq!(marker_text("(a) Dates"), Some("(a)".into()));
        assert_eq!(marker_text("iv. Limes"), Some("iv.".into()));
        assert_eq!(marker_text("•"), Some("•".into()));
        assert!(list_marker("2. Kiwis").unwrap().ordered);
        assert!(!list_marker("• Kiwis").unwrap().ordered);
    }

    #[test]
    fn ordinary_text_is_not_a_marker() {
        assert_eq!(marker_text("2023 was a year"), None);
        assert_eq!(marker_text("J. K. Rowling"), None);
        assert_eq!(marker_text("-5 degrees"), None);
        assert_eq!(marker_text("1.5 metres"), None);
        assert_eq!(marker_text("Hello."), None);
        assert_eq!(marker_text("2024. The year"), None);
    }
}
