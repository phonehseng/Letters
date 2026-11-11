//! finding words in the text of a page.
//!
//! this file knows nothing about pdfium. the engine hands it the
//! characters of one page, in the order pdfium numbers them, and gets
//! back which character positions match. keeping the matching here
//! means it can be tested with plain strings.
//!
//! matching works on characters rather than through pdfium's own search
//! because the result needs to know where each match starts, both for
//! the highlight rectangles and for the snippet around it, and because
//! a phrase broken over two lines should still be found.

/// the most matches one search returns. a list longer than this is
/// no help to anyone, and it keeps the answer small.
pub const MAX_SEARCH_MATCHES: usize = 500;

/// how many characters of context the snippet shows on each side.
const SNIPPET_CONTEXT_CHARS: usize = 30;

/// a rectangle on a page in points, measured from the page's top left
/// corner with y growing downward, the way the viewer lays pages out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// one place the search text was found.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchMatch {
    /// zero based page number
    pub page: u32,
    /// one rectangle per line the match covers
    pub rects: Vec<TextRect>,
    /// the match with some text either side, on one line
    pub snippet: String,
}

/// everything a search found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchResults {
    pub matches: Vec<SearchMatch>,
    /// true when there were more matches than `MAX_SEARCH_MATCHES`
    pub truncated: bool,
}

/// the text being looked for, folded the same way page text is folded.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchQuery {
    folded: Vec<char>,
}

impl SearchQuery {
    /// prepares a query, or returns none when there is nothing to look for.
    pub fn new(text: &str) -> Option<Self> {
        let folded: Vec<char> = fold(text.trim().chars().enumerate())
            .into_iter()
            .map(|(letter, _)| letter)
            .collect();
        if folded.is_empty() {
            None
        } else {
            Some(Self { folded })
        }
    }
}

/// a run of matching characters on one page, as pdfium character indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharRange {
    pub start: usize,
    /// how many page characters the match covers, including any line
    /// break characters inside it
    pub count: usize,
}

/// finds every place the query appears in a page's characters.
///
/// matches do not overlap: after one is found the search carries on
/// from its end, the way a reader would count them.
pub fn find_matches(page_chars: &[char], query: &SearchQuery) -> Vec<CharRange> {
    let folded = fold(page_chars.iter().copied().enumerate());
    let needle = &query.folded;
    let mut found = Vec::new();
    let mut position = 0;
    while position + needle.len() <= folded.len() {
        let window = &folded[position..position + needle.len()];
        if window
            .iter()
            .zip(needle)
            .all(|((letter, _), wanted)| letter == wanted)
        {
            let start = window[0].1;
            let end = window[needle.len() - 1].1;
            found.push(CharRange {
                start,
                count: end - start + 1,
            });
            position += needle.len();
        } else {
            position += 1;
        }
    }
    found
}

/// the match with about thirty characters either side, on one line.
pub fn snippet(page_chars: &[char], range: CharRange) -> String {
    let start = range.start.saturating_sub(SNIPPET_CONTEXT_CHARS);
    let end = (range.start + range.count + SNIPPET_CONTEXT_CHARS).min(page_chars.len());
    let mut text = String::new();
    for &letter in &page_chars[start.min(end)..end] {
        let is_space = letter.is_whitespace() || letter.is_control();
        if !is_space {
            text.push(letter);
        } else if !text.is_empty() && !text.ends_with(' ') {
            text.push(' ');
        }
    }
    text.trim_end().to_owned()
}

/// lowercases letters and turns every run of white space into a single
/// space, remembering which original character each folded one came from.
///
/// line breaks in a pdf's text are often several characters ("\r\n"),
/// so folding them is what lets "light energy" match across two lines.
fn fold(chars: impl Iterator<Item = (usize, char)>) -> Vec<(char, usize)> {
    let mut folded: Vec<(char, usize)> = Vec::new();
    for (index, letter) in chars {
        if letter.is_whitespace() || letter.is_control() {
            let after_space = folded.last().is_some_and(|(last, _)| *last == ' ');
            if !after_space {
                folded.push((' ', index));
            }
        } else {
            // a few letters lowercase to more than one character (the
            // turkish dotted capital i), each keeps the original's index
            for lower in letter.to_lowercase() {
                folded.push((lower, index));
            }
        }
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn query(text: &str) -> SearchQuery {
        SearchQuery::new(text).unwrap()
    }

    #[test]
    fn blank_queries_look_for_nothing() {
        assert_eq!(SearchQuery::new(""), None);
        assert_eq!(SearchQuery::new("  \t\n "), None);
    }

    #[test]
    fn finds_every_match_ignoring_case() {
        let page = chars("Cells need Light. light makes LIGHT work.");
        let found = find_matches(&page, &query("light"));
        let starts: Vec<usize> = found.iter().map(|range| range.start).collect();
        assert_eq!(starts, [11, 18, 30]);
        assert!(found.iter().all(|range| range.count == 5));
    }

    #[test]
    fn matches_do_not_overlap() {
        let found = find_matches(&chars("aaaa"), &query("aa"));
        assert_eq!(
            found,
            [
                CharRange { start: 0, count: 2 },
                CharRange { start: 2, count: 2 }
            ]
        );
    }

    #[test]
    fn a_phrase_is_found_across_a_line_break() {
        let page = chars("turns light\r\nenergy into sugar");
        let found = find_matches(&page, &query("Light  Energy"));
        // "light\r\nenergy" is 13 characters on the page
        assert_eq!(
            found,
            [CharRange {
                start: 6,
                count: 13
            }]
        );
    }

    #[test]
    fn surrounding_spaces_in_the_query_are_ignored() {
        let found = find_matches(&chars("a cell wall"), &query("  cell "));
        assert_eq!(found, [CharRange { start: 2, count: 4 }]);
    }

    #[test]
    fn no_match_gives_an_empty_list() {
        assert!(find_matches(&chars("mitochondria"), &query("nucleus")).is_empty());
        assert!(find_matches(&[], &query("x")).is_empty());
    }

    #[test]
    fn snippet_shows_context_on_one_line() {
        let page = chars("first line\r\nthe word is here\r\nlast line");
        let range = find_matches(&page, &query("word"))[0];
        assert_eq!(
            snippet(&page, range),
            "first line the word is here last line"
        );
    }

    #[test]
    fn snippet_is_cut_about_thirty_characters_either_side() {
        let before = "b".repeat(50);
        let after = "a".repeat(50);
        let page = chars(&format!("{before}MATCH{after}"));
        let range = find_matches(&page, &query("match"))[0];
        let text = snippet(&page, range);
        assert_eq!(text, format!("{}MATCH{}", "b".repeat(30), "a".repeat(30)));
    }
}
