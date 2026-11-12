//! reading what the person typed into the search field.
//!
//! the syntax is the one search engines taught everybody:
//!
//! - `cell membrane`: both words must be there, in any order
//! - `"cell membrane"`: those words next to each other, in that order
//! - `-plant`: leave out anything with "plant"
//! - `mitosis OR meiosis`: either word will do
//! - the last word is matched as a beginning while it is being typed, so
//!   `photosyn` already finds "photosynthesis"
//!
//! "search this document" and "search everywhere" both use it.

use crate::normalize::terms;

/// one thing to look for: a word or a run of words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Word {
        term: String,
        /// true for the word being typed at the end of the query
        prefix: bool,
    },
    Phrase(Vec<String>),
}

/// things of which at least one must match (one, unless OR joined several).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clause {
    pub alternatives: Vec<Part>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    /// every clause must match
    pub clauses: Vec<Clause>,
    /// none of these may match
    pub excluded: Vec<Part>,
}

impl ParsedQuery {
    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    /// every word the query looks for, for highlighting.
    pub fn wanted_terms(&self) -> Vec<String> {
        let mut wanted = Vec::new();
        for clause in &self.clauses {
            for part in &clause.alternatives {
                match part {
                    Part::Word { term, .. } => wanted.push(term.clone()),
                    Part::Phrase(words) => wanted.extend(words.iter().cloned()),
                }
            }
        }
        wanted
    }
}

/// one piece of the raw query before words are folded.
#[derive(Debug, PartialEq)]
enum Piece {
    Text { text: String, negated: bool },
    Quoted { text: String, negated: bool },
    Or,
}

pub fn parse(query: &str) -> ParsedQuery {
    let pieces = split_pieces(query);
    // a query still being typed ends in a letter: its last word is a prefix
    let typing_last_word = query.chars().last().is_some_and(char::is_alphanumeric);
    let mut parsed = ParsedQuery::default();
    let mut join_next = false;
    let count = pieces.len();
    for (index, piece) in pieces.into_iter().enumerate() {
        let is_last = index + 1 == count;
        match piece {
            Piece::Or => join_next = !parsed.clauses.is_empty(),
            Piece::Text { text, negated } | Piece::Quoted { text, negated } if negated => {
                if let Some(part) = part_from(&text, false) {
                    parsed.excluded.push(part);
                }
            }
            Piece::Text { text, .. } => {
                let prefix = is_last && typing_last_word;
                if let Some(part) = part_from(&text, prefix) {
                    add_part(&mut parsed, part, join_next);
                }
                join_next = false;
            }
            Piece::Quoted { text, .. } => {
                if let Some(part) = phrase_from(&text) {
                    add_part(&mut parsed, part, join_next);
                }
                join_next = false;
            }
        }
    }
    parsed
}

fn add_part(parsed: &mut ParsedQuery, part: Part, join_previous: bool) {
    match parsed.clauses.last_mut() {
        Some(clause) if join_previous => clause.alternatives.push(part),
        _ => parsed.clauses.push(Clause {
            alternatives: vec![part],
        }),
    }
}

/// a bare piece is a word, or a phrase when it holds several ("e-mail").
fn part_from(text: &str, prefix: bool) -> Option<Part> {
    let mut words = terms(text);
    match words.len() {
        0 => None,
        1 => Some(Part::Word {
            term: words.remove(0),
            prefix,
        }),
        _ => Some(Part::Phrase(words)),
    }
}

/// a quoted single word is just that word, matched exactly as typed.
fn phrase_from(text: &str) -> Option<Part> {
    let words = terms(text);
    match words.len() {
        0 => None,
        _ => Some(Part::Phrase(words)),
    }
}

fn split_pieces(query: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut chars = query.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        let negated = ch == '-' || ch == '\u{2212}';
        if negated {
            chars.next();
            if chars.peek().is_none_or(|c| c.is_whitespace()) {
                continue;
            }
        }
        if chars.peek().is_some_and(|c| is_quote(*c)) {
            chars.next();
            let text: String = chars.by_ref().take_while(|c| !is_quote(*c)).collect();
            pieces.push(Piece::Quoted { text, negated });
            continue;
        }
        let mut text = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || is_quote(c) {
                break;
            }
            text.push(c);
            chars.next();
        }
        if !negated && (text == "OR" || text == "|") {
            pieces.push(Piece::Or);
        } else {
            pieces.push(Piece::Text { text, negated });
        }
    }
    pieces
}

fn is_quote(ch: char) -> bool {
    matches!(ch, '"' | '\u{201C}' | '\u{201D}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(term: &str, prefix: bool) -> Part {
        Part::Word {
            term: term.into(),
            prefix,
        }
    }

    fn phrase(words: &[&str]) -> Part {
        Part::Phrase(words.iter().map(|w| w.to_string()).collect())
    }

    fn clause(parts: Vec<Part>) -> Clause {
        Clause {
            alternatives: parts,
        }
    }

    #[test]
    fn plain_words_must_all_match_and_the_last_is_a_prefix() {
        let parsed = parse("Cell Membr");
        assert_eq!(
            parsed.clauses,
            vec![
                clause(vec![word("cell", false)]),
                clause(vec![word("membr", true)])
            ]
        );
        // a trailing space means the word is finished
        assert_eq!(
            parse("cell ").clauses,
            vec![clause(vec![word("cell", false)])]
        );
    }

    #[test]
    fn quotes_make_a_phrase() {
        let parsed = parse("\"the cell membrane\" water");
        assert_eq!(
            parsed.clauses,
            vec![
                clause(vec![phrase(&["the", "cell", "membrane"])]),
                clause(vec![word("water", true)]),
            ]
        );
        // curly quotes from a word processor work too, and an open quote is closed
        assert_eq!(
            parse("“open ended").clauses,
            vec![clause(vec![phrase(&["open", "ended"])])]
        );
    }

    #[test]
    fn minus_excludes() {
        let parsed = parse("cell -plant -\"cell wall\"");
        assert_eq!(parsed.clauses, vec![clause(vec![word("cell", false)])]);
        assert_eq!(
            parsed.excluded,
            vec![word("plant", false), phrase(&["cell", "wall"])]
        );
        // a lone minus is ignored
        assert_eq!(parse("a - b").clauses.len(), 2);
    }

    #[test]
    fn or_joins_neighbours() {
        let parsed = parse("mitosis OR meiosis cell");
        assert_eq!(
            parsed.clauses,
            vec![
                clause(vec![word("mitosis", false), word("meiosis", false)]),
                clause(vec![word("cell", true)]),
            ]
        );
        // lower case "or" is an ordinary word
        assert_eq!(parse("this or that").clauses.len(), 3);
        // a leading OR has nothing to join
        assert_eq!(parse("OR cell").clauses.len(), 1);
    }

    #[test]
    fn hyphenated_words_are_a_phrase_and_accents_fold() {
        assert_eq!(
            parse("e-mail café ").clauses,
            vec![
                clause(vec![phrase(&["e", "mail"])]),
                clause(vec![word("cafe", false)])
            ]
        );
        assert!(parse("  ").is_empty());
        assert!(parse("-only").is_empty());
    }
}
