//! the plain sentences that describe changes.
//!
//! every change carries one sentence a student can read without knowing
//! anything about pdf: "Changed “fox” to “cat”", "Font size changed from 11
//! to 12 in “Introduction”". they are made here so the wording stays
//! consistent. sentences have no full stop; the report adds one.

use crate::source::Colour;

/// quoted text is cut to about this many characters, at a word boundary.
const MAX_QUOTE_CHARS: usize = 60;

/// text in curly quotes, shortened with an ellipsis when it is long.
pub(crate) fn quote(text: &str) -> String {
    format!("“{}”", shorten(text, MAX_QUOTE_CHARS))
}

/// cuts text to at most `limit` characters, at a space when possible.
pub(crate) fn shorten(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let cut: String = text.chars().take(limit).collect();
    let at_space = cut.rfind(' ').filter(|&position| position > limit / 2);
    let kept = match at_space {
        Some(position) => &cut[..position],
        None => cut.as_str(),
    };
    format!("{}…", kept.trim_end())
}

pub(crate) fn added(text: &str) -> String {
    format!("Added {}", quote(text))
}

pub(crate) fn removed(text: &str) -> String {
    format!("Removed {}", quote(text))
}

pub(crate) fn replaced(old: &str, new: &str) -> String {
    format!("Changed {} to {}", quote(old), quote(new))
}

pub(crate) fn spacing_changed(text: &str) -> String {
    format!("Spacing or hyphenation changed in {}", quote(text))
}

/// a passage that moved. pages are 0-based here and shown 1-based.
pub(crate) fn moved(
    text: &str,
    from_page: u32,
    to_page: u32,
    same_pair: bool,
    edited: bool,
) -> String {
    let place = if same_pair {
        "to a different place on the page".to_owned()
    } else {
        format!(
            "from old page {} to new page {}",
            from_page + 1,
            to_page + 1
        )
    };
    let edits = if edited { ", with small edits" } else { "" };
    format!("Moved {} {place}{edits}", quote(text))
}

/// a number of points without a needless ".0".
pub(crate) fn points(value: f32) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded - rounded.round()).abs() < 0.05 {
        format!("{}", rounded.round() as i64)
    } else {
        format!("{rounded:.1}")
    }
}

/// joins parts as "a", "a and b" or "a, b and c".
pub(crate) fn join_parts(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// makes the first letter a capital.
pub(crate) fn capitalised(text: &str) -> String {
    let mut letters = text.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// a colour's everyday name when it is close to one, otherwise its hex code.
pub(crate) fn colour_name(colour: Colour) -> String {
    const NAMED: [(&str, Colour); 16] = [
        ("black", Colour { r: 0, g: 0, b: 0 }),
        (
            "white",
            Colour {
                r: 255,
                g: 255,
                b: 255,
            },
        ),
        (
            "grey",
            Colour {
                r: 128,
                g: 128,
                b: 128,
            },
        ),
        (
            "dark grey",
            Colour {
                r: 64,
                g: 64,
                b: 64,
            },
        ),
        (
            "light grey",
            Colour {
                r: 192,
                g: 192,
                b: 192,
            },
        ),
        ("red", Colour { r: 255, g: 0, b: 0 }),
        ("dark red", Colour { r: 139, g: 0, b: 0 }),
        ("green", Colour { r: 0, g: 128, b: 0 }),
        ("bright green", Colour { r: 0, g: 255, b: 0 }),
        ("blue", Colour { r: 0, g: 0, b: 255 }),
        ("dark blue", Colour { r: 0, g: 0, b: 139 }),
        (
            "yellow",
            Colour {
                r: 255,
                g: 255,
                b: 0,
            },
        ),
        (
            "orange",
            Colour {
                r: 255,
                g: 165,
                b: 0,
            },
        ),
        (
            "purple",
            Colour {
                r: 128,
                g: 0,
                b: 128,
            },
        ),
        (
            "pink",
            Colour {
                r: 255,
                g: 192,
                b: 203,
            },
        ),
        (
            "brown",
            Colour {
                r: 139,
                g: 69,
                b: 19,
            },
        ),
    ];
    NAMED
        .iter()
        .find(|(_, named)| named.distance(&colour) <= 24)
        .map(|(name, _)| (*name).to_owned())
        .unwrap_or_else(|| format!("#{:02X}{:02X}{:02X}", colour.r, colour.g, colour.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_quotes_are_cut_at_a_word() {
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen";
        let quoted = quote(long);
        assert!(quoted.ends_with("…”"), "{quoted}");
        assert!(quoted.chars().count() <= MAX_QUOTE_CHARS + 3);
        assert_eq!(quote("short"), "“short”");
    }

    #[test]
    fn numbers_drop_needless_decimals() {
        assert_eq!(points(12.0), "12");
        assert_eq!(points(11.96), "12");
        assert_eq!(points(10.5), "10.5");
    }

    #[test]
    fn parts_join_like_a_sentence() {
        let parts = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(join_parts(&parts), "a, b and c");
        assert_eq!(join_parts(&parts[..1]), "a");
    }

    #[test]
    fn colours_get_everyday_names() {
        assert_eq!(colour_name(Colour::new(250, 5, 5)), "red");
        assert_eq!(colour_name(Colour::new(18, 52, 86)), "#123456");
    }

    #[test]
    fn moves_name_their_pages_from_one() {
        assert_eq!(
            moved("text", 0, 2, false, false),
            "Moved “text” from old page 1 to new page 3"
        );
    }
}
