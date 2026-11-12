//! checking that what an answer quotes is really in the document.
//!
//! the model is asked to back each claim with exact words from a numbered
//! passage. it can still misquote, paraphrase or invent, so every quote is
//! looked up in the passages that were actually sent. a quote that isn't
//! found word for word (spacing aside, since pdf text often has odd line
//! breaks) is dropped and listed as unverified, and its marker is taken out
//! of the answer.

use serde::Serialize;

use crate::chunking::{Chunk, Chunked};
use crate::document::Rect;
use crate::prompts::{DOESNT_SAY, NOT_IN_DOCUMENT};

/// one checked quote and where it is.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    /// the [n] in the answer text this belongs to.
    pub marker: usize,
    pub page: u32,
    pub rect: Rect,
    pub quote: String,
}

/// an answer to a question about the document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub text: String,
    pub citations: Vec<Citation>,
    /// quotes the model gave that aren't in the document. the ui can say
    /// "Some of this couldn't be checked against the document."
    pub unverified_quotes: Vec<String>,
    /// false when the document doesn't answer the question.
    pub found: bool,
}

impl Answer {
    pub fn doesnt_say() -> Answer {
        Answer {
            text: DOESNT_SAY.to_string(),
            citations: Vec::new(),
            unverified_quotes: Vec::new(),
            found: false,
        }
    }

    /// true when at least one claim is backed by a checked quote.
    pub fn is_grounded(&self) -> bool {
        !self.citations.is_empty()
    }
}

/// the model's answer split into its text and its (marker, quote) list.
pub fn split_sources(raw: &str) -> (String, Vec<(usize, String)>) {
    let (body, sources) = match raw.find("SOURCES:") {
        Some(at) => (&raw[..at], &raw[at + "SOURCES:".len()..]),
        None => (raw, ""),
    };
    let quotes = sources.lines().filter_map(parse_source_line).collect();
    (body.trim().to_string(), quotes)
}

/// `[2] "exact words"` becomes (2, "exact words").
fn parse_source_line(line: &str) -> Option<(usize, String)> {
    let line = line.trim().trim_start_matches(['-', '*']).trim();
    let rest = line.strip_prefix('[')?;
    let (number, quote) = rest.split_once(']')?;
    let marker = number.trim().parse().ok()?;
    let quote = quote.trim().trim_start_matches(':').trim();
    let quote = strip_quote_marks(quote);
    (!quote.is_empty()).then(|| (marker, quote.to_string()))
}

fn strip_quote_marks(text: &str) -> &str {
    let marks: &[char] = &['"', '“', '”', '\'', '‘', '’'];
    text.trim_start_matches(marks)
        .trim_end_matches(marks)
        .trim()
}

/// true when the model said the passages don't answer the question.
pub fn says_not_found(raw: &str) -> bool {
    let body = split_sources(raw).0;
    body.contains(NOT_IN_DOCUMENT) && body.len() < NOT_IN_DOCUMENT.len() + 40
}

/// builds the checked answer from what the model wrote.
///
/// `sent` holds the chunk indexes in the order they were numbered, so
/// marker [1] is `sent[0]`.
pub fn verify_answer(raw: &str, sent: &[usize], chunked: &Chunked) -> Answer {
    if says_not_found(raw) {
        return Answer::doesnt_say();
    }
    let (mut text, quotes) = split_sources(raw);
    let mut citations = Vec::new();
    let mut unverified = Vec::new();
    for (marker, quote) in quotes {
        match locate(&quote, marker, sent, chunked) {
            Some((page, rect)) => citations.push(Citation {
                marker,
                page,
                rect,
                quote,
            }),
            None => unverified.push(quote),
        }
    }
    text = remove_unbacked_markers(&text, &citations);
    Answer {
        text,
        citations,
        unverified_quotes: unverified,
        found: true,
    }
}

/// finds a quote, trying the passage it was attributed to first.
fn locate(quote: &str, marker: usize, sent: &[usize], chunked: &Chunked) -> Option<(u32, Rect)> {
    let wanted = normalise(quote);
    // a few words at least, so "the" can't verify as a quote
    if wanted.split(' ').count() < 2 && wanted.chars().count() < 12 {
        return None;
    }
    let attributed = marker.checked_sub(1).and_then(|i| sent.get(i));
    let order = attributed.into_iter().chain(sent.iter());
    order
        .filter_map(|&index| chunked.chunks.get(index))
        .find_map(|chunk| find_in_chunk(&wanted, chunk, chunked))
}

/// looks for normalised text in a chunk and says which piece it starts in.
fn find_in_chunk(wanted: &str, chunk: &Chunk, chunked: &Chunked) -> Option<(u32, Rect)> {
    let mut joined = String::new();
    let mut starts = Vec::new();
    for span in &chunk.spans {
        if !joined.is_empty() {
            joined.push(' ');
        }
        starts.push((joined.len(), span.piece));
        joined.push_str(&normalise(&chunked.pieces[span.piece].text));
    }
    let at = joined.find(wanted)?;
    let piece = starts.iter().rev().find(|(start, _)| *start <= at)?.1;
    let piece = &chunked.pieces[piece];
    Some((piece.page, piece.rect))
}

/// collapses runs of spaces and line breaks into one space.
pub fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// takes "[3]" out of the answer when no checked quote backs passage 3.
fn remove_unbacked_markers(text: &str, citations: &[Citation]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let marker = after.find(']').and_then(|close| {
            after[..close]
                .trim()
                .parse::<usize>()
                .ok()
                .map(|n| (n, close))
        });
        match marker {
            Some((number, close)) => {
                if citations.iter().any(|c| c.marker == number) {
                    out.push_str(&rest[open..open + close + 2]);
                } else {
                    // also drop the space before the marker
                    while out.ends_with(' ') {
                        out.pop();
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunking::{ChunkSize, Chunked};
    use crate::document::{Document, Page, PageRange, Paragraph};

    fn chunked() -> Chunked {
        let paragraph = |text: &str, y: f32| Paragraph {
            text: text.to_string(),
            rect: Rect {
                x: 0.0,
                y,
                width: 100.0,
                height: 10.0,
            },
        };
        let document = Document {
            pages: vec![
                Page {
                    number: 1,
                    paragraphs: vec![paragraph(
                        "The mitochondria is the\npowerhouse of the cell.",
                        10.0,
                    )],
                },
                Page {
                    number: 2,
                    paragraphs: vec![paragraph("Plants turn sunlight into sugar.", 20.0)],
                },
            ],
        };
        let size = ChunkSize {
            target_chars: 60,
            overlap_chars: 0,
        };
        Chunked::new(&document, PageRange::new(1, 2), size)
    }

    #[test]
    fn real_quotes_are_kept_with_their_place() {
        let chunked = chunked();
        assert_eq!(chunked.chunks.len(), 2);
        let raw = "Mitochondria power the cell [1], and plants make sugar [2].\nSOURCES:\n[1] \"the powerhouse of the cell\"\n[2] “turn sunlight into sugar”";
        let answer = verify_answer(raw, &[0, 1], &chunked);
        assert!(answer.found && answer.is_grounded());
        assert_eq!(answer.citations.len(), 2);
        assert_eq!(
            (answer.citations[0].page, answer.citations[0].rect.y),
            (1, 10.0)
        );
        assert_eq!(answer.citations[1].page, 2);
        assert!(answer.unverified_quotes.is_empty());
        assert_eq!(
            answer.text,
            "Mitochondria power the cell [1], and plants make sugar [2]."
        );
    }

    #[test]
    fn invented_quotes_are_dropped_and_listed() {
        let chunked = chunked();
        let raw = "Cells are tiny [1] and plants are green [2].\nSOURCES:\n[1] \"the powerhouse of the cell\"\n[2] \"plants are always green\"";
        let answer = verify_answer(raw, &[0, 1], &chunked);
        assert_eq!(answer.citations.len(), 1);
        assert_eq!(
            answer.unverified_quotes,
            vec!["plants are always green".to_string()]
        );
        // the unbacked marker is gone from the text
        assert_eq!(answer.text, "Cells are tiny [1] and plants are green.");
    }

    #[test]
    fn a_quote_must_be_word_for_word() {
        let chunked = chunked();
        // close, but not what the page says
        let raw = "x [1]\nSOURCES:\n[1] \"the power house of the cell\"";
        assert!(!verify_answer(raw, &[0], &chunked).is_grounded());
        // a single short word can't count as evidence
        let raw = "x [1]\nSOURCES:\n[1] \"cell\"";
        assert!(!verify_answer(raw, &[0], &chunked).is_grounded());
    }

    #[test]
    fn a_quote_is_only_checked_against_text_that_was_sent() {
        let chunked = chunked();
        // chunk 1 (page 2) wasn't sent, so its words can't back the answer
        let raw = "x [1]\nSOURCES:\n[1] \"turn sunlight into sugar\"";
        assert!(!verify_answer(raw, &[0], &chunked).is_grounded());
    }

    #[test]
    fn not_in_document_becomes_doesnt_say() {
        let answer = verify_answer("NOT_IN_DOCUMENT", &[0], &chunked());
        assert_eq!(answer, Answer::doesnt_say());
        assert_eq!(answer.text, "The document doesn’t say.");
    }

    #[test]
    fn source_lines_are_read_loosely() {
        let (_, quotes) =
            split_sources("a\nSOURCES:\n- [1]: \"one two\"\n[x] bad\n[2] 'three four'\n");
        assert_eq!(
            quotes,
            vec![(1, "one two".into()), (2, "three four".into())]
        );
    }
}
