//! where a line may break: the unicode line breaking algorithm (uax #14)
//! as the base, with the differences word was measured to have.

use unicode_linebreak::{linebreaks, BreakOpportunity};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Break {
    /// no break before this character
    #[default]
    None,
    /// a line may start here
    Allowed,
    /// a line must start here (after a line break character)
    Mandatory,
}

/// the break opportunity before each character of `text`: `result[i]` says
/// whether a line may start at char `i`. the end of the text is not listed.
pub fn break_opportunities(text: &str) -> Vec<Break> {
    let count = text.chars().count();
    let mut result = vec![Break::None; count];
    // uax #14 reports byte offsets; map them to char indexes
    let mut byte_to_char = vec![0usize; text.len() + 1];
    for (index, (at, _)) in text.char_indices().enumerate() {
        byte_to_char[at] = index;
    }
    byte_to_char[text.len()] = count;
    for (at, opportunity) in linebreaks(text) {
        let index = byte_to_char[at];
        if index >= count {
            continue;
        }
        result[index] = match opportunity {
            BreakOpportunity::Mandatory => Break::Mandatory,
            BreakOpportunity::Allowed => Break::Allowed,
        };
    }
    adjust_for_word(text, &mut result);
    result
}

/// word's differences from uax #14 that matter for latin text:
///
/// - a slash between letters ("and/or") is not a break in word; uax #14
///   allows a break after it.
fn adjust_for_word(text: &str, result: &mut [Break]) {
    let chars: Vec<char> = text.chars().collect();
    for index in 1..chars.len() {
        if chars[index - 1] == '/'
            && result[index] == Break::Allowed
            && chars[index].is_alphanumeric()
        {
            result[index] = Break::None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starts(text: &str) -> Vec<usize> {
        break_opportunities(text)
            .iter()
            .enumerate()
            .filter(|(_, b)| **b != Break::None)
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn spaces_hang_with_the_word_before() {
        // a line can start at "b" and "c": the spaces stay on the line before
        assert_eq!(starts("a  b c"), vec![3, 5]);
    }

    #[test]
    fn hyphens_allow_a_break_after_them() {
        assert_eq!(starts("well-known"), vec![5]);
    }

    #[test]
    fn non_breaking_space_and_hyphen_do_not_break() {
        assert!(starts("10\u{a0}kg").is_empty());
        assert!(starts("x\u{2011}y").is_empty());
    }

    #[test]
    fn line_breaks_are_mandatory() {
        let breaks = break_opportunities("a\nb");
        assert_eq!(breaks[2], Break::Mandatory);
    }

    #[test]
    fn slashes_between_words_do_not_break() {
        assert!(starts("and/or").is_empty());
    }

    #[test]
    fn opening_brackets_and_quotes() {
        // "(see" can start a line, the bracket stays with its word
        assert_eq!(starts("as (see"), vec![3]);
    }
}
