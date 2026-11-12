//! turning text into the words the index stores.
//!
//! a word is a run of letters and digits (an apostrophe inside a word,
//! as in "don't", is kept). every word is folded to lower case and has
//! its accents taken off, so "Élan", "élan" and "elan" are the same word
//! to the index. each word remembers where it sat in the original text,
//! so snippets can highlight the words as they were written.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// one word of a text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// the folded form the index stores ("elan")
    pub term: String,
    /// byte offsets of the word in the original text
    pub start: usize,
    pub end: usize,
}

/// words longer than this are not words people search for (hashes, base64),
/// and keeping them would only grow the dictionary.
pub const MAX_TERM_CHARS: usize = 40;

/// lower case without accents: "Ça Étè" becomes "ca ete".
pub fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for ch in text.nfd() {
        if is_combining_mark(ch) {
            continue;
        }
        fold_char_into(ch, &mut folded);
    }
    folded
}

/// letters that do not come apart into a base letter and an accent
/// still need a plain spelling, or "straße" could never match "strasse".
fn fold_char_into(ch: char, out: &mut String) {
    match ch {
        'ß' => out.push_str("ss"),
        'æ' | 'Æ' => out.push_str("ae"),
        'œ' | 'Œ' => out.push_str("oe"),
        'ø' | 'Ø' => out.push('o'),
        'đ' | 'Đ' => out.push('d'),
        'ł' | 'Ł' => out.push('l'),
        'ı' => out.push('i'),
        // curly apostrophes become the straight one so "don’t" equals "don't"
        '\u{2019}' | '\u{2018}' => out.push('\''),
        _ => out.extend(ch.to_lowercase()),
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric()
}

fn is_apostrophe(ch: char) -> bool {
    matches!(ch, '\'' | '\u{2019}')
}

/// splits a text into its words, in order.
pub fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if !is_word_char(ch) {
            continue;
        }
        let mut end = start + ch.len_utf8();
        while let Some(&(index, next)) = chars.peek() {
            if is_word_char(next) {
                end = index + next.len_utf8();
                chars.next();
            } else if is_apostrophe(next) && followed_by_letter(text, index + next.len_utf8()) {
                // "don't" stays one word, but a closing quote does not join
                end = index + next.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        push_token(text, start, end, &mut tokens);
    }
    tokens
}

fn followed_by_letter(text: &str, at: usize) -> bool {
    text[at..].chars().next().is_some_and(is_word_char)
}

fn push_token(text: &str, start: usize, end: usize, tokens: &mut Vec<Token>) {
    let term = fold(&text[start..end]).replace('\'', "");
    if term.is_empty() || term.chars().count() > MAX_TERM_CHARS {
        return;
    }
    tokens.push(Token { term, start, end });
}

/// the folded words of a text without their places, for queries and titles.
pub fn terms(text: &str) -> Vec<String> {
    tokenize(text).into_iter().map(|token| token.term).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_and_accents() {
        assert_eq!(fold("Élan Vital"), "elan vital");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("Œuvre ÇA"), "oeuvre ca");
        assert_eq!(fold("naïve façade"), "naive facade");
    }

    #[test]
    fn splits_words_and_keeps_their_places() {
        let text = "The cat's “whiskers” — 42 times!";
        let tokens = tokenize(text);
        let words: Vec<&str> = tokens.iter().map(|t| t.term.as_str()).collect();
        assert_eq!(words, vec!["the", "cats", "whiskers", "42", "times"]);
        assert_eq!(&text[tokens[1].start..tokens[1].end], "cat's");
        assert_eq!(&text[tokens[2].start..tokens[2].end], "whiskers");
    }

    #[test]
    fn a_closing_quote_does_not_join_words() {
        assert_eq!(terms("the dogs' bowls"), vec!["the", "dogs", "bowls"]);
        assert_eq!(terms("don’t stop"), vec!["dont", "stop"]);
    }

    #[test]
    fn very_long_words_are_left_out() {
        let long = "a".repeat(MAX_TERM_CHARS + 1);
        assert_eq!(terms(&format!("short {long} word")), vec!["short", "word"]);
    }

    #[test]
    fn non_latin_scripts_are_words_too() {
        assert_eq!(terms("Привет мир"), vec!["привет", "мир"]);
        assert_eq!(terms("日本語"), vec!["日本語"]);
    }
}
