//! the second round's text work: phrases in order, the closest run of
//! matching words, and the snippet with its highlighted words.

use crate::normalize::Token;
use crate::search::ClauseMatches;

pub(crate) fn contains_phrase(terms: &[&str], phrase: &[String]) -> bool {
    if phrase.is_empty() || terms.len() < phrase.len() {
        return false;
    }
    terms
        .windows(phrase.len())
        .any(|window| window.iter().zip(phrase).all(|(a, b)| *a == b))
}

/// the shortest run of words holding one match of every clause, and the
/// positions of every matching word (for highlighting).
pub(crate) fn closest_window(
    terms: &[&str],
    clauses: &[ClauseMatches],
) -> (Option<usize>, Vec<usize>) {
    let mut events: Vec<(usize, usize)> = Vec::new();
    let mut matched = Vec::new();
    for (position, term) in terms.iter().enumerate() {
        let mut any = false;
        for (clause_index, clause) in clauses.iter().enumerate() {
            if clause.words.contains(*term) {
                events.push((position, clause_index));
                any = true;
            }
        }
        if any {
            matched.push(position);
        }
    }
    let needed = clauses.len();
    let mut counts = vec![0usize; needed];
    let mut covered = 0;
    let mut best: Option<usize> = None;
    let mut left = 0;
    for right in 0..events.len() {
        let clause = events[right].1;
        counts[clause] += 1;
        if counts[clause] == 1 {
            covered += 1;
        }
        while covered == needed {
            let span = events[right].0 - events[left].0 + 1;
            best = Some(best.map_or(span, |b| b.min(span)));
            let leaving = events[left].1;
            counts[leaving] -= 1;
            if counts[leaving] == 0 {
                covered -= 1;
            }
            left += 1;
        }
    }
    (best, matched)
}

/// how many characters of context a snippet shows before its first match
const SNIPPET_BEFORE: usize = 60;
const SNIPPET_LENGTH: usize = 220;

/// a short piece of the passage around its matches, with white space
/// tidied, and where the matched words are inside it.
pub(crate) fn make_snippet(
    text: &str,
    tokens: &[Token],
    matched: &[usize],
) -> (String, Vec<[u32; 2]>) {
    if text.trim().is_empty() {
        return (String::new(), Vec::new());
    }
    let anchor = matched.first().map_or(0, |&i| tokens[i].start);
    let start = snippet_start(text, anchor);
    let end = snippet_end(text, start);
    let ranges: Vec<(usize, usize)> = matched
        .iter()
        .map(|&i| (tokens[i].start, tokens[i].end))
        .filter(|(s, e)| *s >= start && *e <= end)
        .collect();
    let mut out = String::new();
    let mut units = 0u32;
    let mut highlights = Vec::new();
    if start > 0 {
        out.push('\u{2026}');
        units += 1;
    }
    let mut open: Option<u32> = None;
    let mut pending_space = false;
    let mut next_range = 0;
    for (offset, ch) in text[start..end].char_indices() {
        let at = start + offset;
        if next_range < ranges.len() && open.is_some() && at == ranges[next_range].1 {
            highlights.push([open.take().unwrap(), units]);
            next_range += 1;
        }
        if ch.is_whitespace() {
            pending_space = !out.is_empty() && !out.ends_with('\u{2026}');
            continue;
        }
        if pending_space {
            out.push(' ');
            units += 1;
            pending_space = false;
        }
        if next_range < ranges.len() && at == ranges[next_range].0 {
            open = Some(units);
        }
        out.push(ch);
        units += ch.len_utf16() as u32;
    }
    if let Some(begin) = open {
        highlights.push([begin, units]);
    }
    if end < text.len() {
        out.push('\u{2026}');
    }
    (out, highlights)
}

fn snippet_start(text: &str, anchor: usize) -> usize {
    if anchor <= SNIPPET_BEFORE {
        return 0;
    }
    let mut start = anchor - SNIPPET_BEFORE;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    // begin at a word, not in the middle of one
    match text[start..anchor].find(char::is_whitespace) {
        Some(space) => start + space + 1,
        None => start,
    }
}

fn snippet_end(text: &str, start: usize) -> usize {
    let mut end = (start + SNIPPET_LENGTH).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == text.len() {
        return end;
    }
    match text[start..end].rfind(char::is_whitespace) {
        Some(space) if space > SNIPPET_LENGTH / 2 => start + space,
        _ => end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::tokenize;

    #[test]
    fn snippets_highlight_words_and_tidy_spaces() {
        let text = "The   cell\nmembrane controls what enters.";
        let tokens = tokenize(text);
        let (snippet, highlights) = make_snippet(text, &tokens, &[1, 2]);
        assert_eq!(snippet, "The cell membrane controls what enters.");
        let words: Vec<String> = highlights
            .iter()
            .map(|[s, e]| snippet[*s as usize..*e as usize].to_string())
            .collect();
        assert_eq!(words, vec!["cell", "membrane"]);
    }

    #[test]
    fn long_passages_are_cut_around_the_match() {
        let text = format!("{} needle {}", "hay ".repeat(100), "straw ".repeat(100));
        let tokens = tokenize(&text);
        let needle = tokens.iter().position(|t| t.term == "needle").unwrap();
        let (snippet, highlights) = make_snippet(&text, &tokens, &[needle]);
        assert!(snippet.starts_with('\u{2026}') && snippet.ends_with('\u{2026}'));
        assert!(snippet.chars().count() < 240);
        let [s, e] = highlights[0];
        let utf16: Vec<u16> = snippet.encode_utf16().collect();
        assert_eq!(
            String::from_utf16(&utf16[s as usize..e as usize]).unwrap(),
            "needle"
        );
    }

    #[test]
    fn phrases_are_found_in_order_only() {
        let terms = ["the", "cell", "membrane", "is"];
        assert!(contains_phrase(&terms, &["cell".into(), "membrane".into()]));
        assert!(!contains_phrase(
            &terms,
            &["membrane".into(), "cell".into()]
        ));
    }
}
