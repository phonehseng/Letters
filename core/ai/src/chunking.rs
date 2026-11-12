//! cutting a document into pieces a model can read.
//!
//! two levels:
//! - a **piece** is one paragraph, or part of a very long one. it has one
//!   page and one rectangle, so it is what "jump to" lands on.
//! - a **chunk** is a few pieces in a row, up to a size budget. chunks are
//!   what gets retrieved and sent. each chunk repeats the last piece of the
//!   one before (the overlap) so a thought that starts at the end of one
//!   chunk is still whole in the next.
//!
//! a chunk never crosses a page break. that keeps "this will send page 4"
//! exact, and lets every chunk be cited by one page number.

use crate::document::{Document, PageRange, Rect};

/// one paragraph, or a slice of a long one.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub page: u32,
    pub rect: Rect,
    pub text: String,
}

/// where one piece sits inside a chunk's text, as byte offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub piece: usize,
    pub start: usize,
    pub end: usize,
}

/// a few pieces in a row, joined by blank lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub text: String,
    pub spans: Vec<Span>,
}

/// how big chunks are, in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkSize {
    pub target_chars: usize,
    pub overlap_chars: usize,
}

/// about 300 tokens a chunk with about 60 repeated: small enough that a
/// handful fit in a small model's context next to the question.
pub const DEFAULT_CHUNK_SIZE: ChunkSize = ChunkSize {
    target_chars: 1200,
    overlap_chars: 240,
};

const PIECE_SEPARATOR: &str = "\n\n";

/// the document's pieces and chunks, both in reading order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chunked {
    pub pieces: Vec<Piece>,
    pub chunks: Vec<Chunk>,
}

impl Chunked {
    pub fn new(document: &Document, range: PageRange, size: ChunkSize) -> Chunked {
        let pieces = split_into_pieces(document, range, size.target_chars);
        let chunks = group_into_chunks(&pieces, size);
        Chunked { pieces, chunks }
    }

    /// the page each chunk starts on.
    pub fn first_page(&self, chunk: &Chunk) -> u32 {
        chunk
            .spans
            .first()
            .map_or(0, |span| self.pieces[span.piece].page)
    }
}

/// turns paragraphs into pieces no longer than `max_chars`.
pub fn split_into_pieces(document: &Document, range: PageRange, max_chars: usize) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for page in document.pages_in(range) {
        for paragraph in &page.paragraphs {
            for text in split_long_text(paragraph.text.trim(), max_chars) {
                pieces.push(Piece {
                    page: page.number,
                    rect: paragraph.rect,
                    text: text.to_string(),
                });
            }
        }
    }
    pieces
}

/// cuts text into slices of at most `max_chars` characters.
///
/// it prefers to cut after a sentence, then at a space, and only cuts a
/// word in half when a "word" is longer than the limit (a long url).
pub fn split_long_text(text: &str, max_chars: usize) -> Vec<&str> {
    let max_chars = max_chars.max(1);
    let mut slices = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        if rest.chars().count() <= max_chars {
            slices.push(rest);
            break;
        }
        let limit = byte_index_of_char(rest, max_chars);
        let cut = best_cut(&rest[..limit]).unwrap_or(limit);
        slices.push(rest[..cut].trim_end());
        rest = rest[cut..].trim_start();
    }
    slices
}

fn byte_index_of_char(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(index, _)| index)
}

/// the last good place to cut inside `window`, as a byte offset.
fn best_cut(window: &str) -> Option<usize> {
    // only cut at a sentence end in the second half, so slices don't
    // become tiny just because a sentence ended early
    let half = window.len() / 2;
    let sentence_end = [". ", "? ", "! ", "; "]
        .iter()
        .filter_map(|mark| window.rfind(mark).map(|at| at + mark.len()))
        .filter(|at| *at > half)
        .max();
    sentence_end.or_else(|| window.rfind(' ').filter(|at| *at > 0))
}

/// gathers pieces into chunks of about `size.target_chars`.
pub fn group_into_chunks(pieces: &[Piece], size: ChunkSize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for index in 0..pieces.len() {
        let next_len = pieces[index].text.len();
        let new_page = current
            .last()
            .is_some_and(|&last| pieces[last].page != pieces[index].page);
        if new_page {
            chunks.push(build_chunk(pieces, &current));
            current.clear();
        } else if !current.is_empty() && length_of(pieces, &current) + next_len > size.target_chars
        {
            chunks.push(build_chunk(pieces, &current));
            current = overlap_tail(pieces, &current, size, next_len);
        }
        current.push(index);
    }
    if !current.is_empty() {
        chunks.push(build_chunk(pieces, &current));
    }
    chunks
}

/// the pieces at the end of a finished chunk that start the next one.
fn overlap_tail(
    pieces: &[Piece],
    finished: &[usize],
    size: ChunkSize,
    next_len: usize,
) -> Vec<usize> {
    let mut tail = Vec::new();
    let mut repeated_text = 0;
    for &index in finished.iter().rev() {
        let piece_len = pieces[index].text.len();
        let fits_overlap = repeated_text + piece_len <= size.overlap_chars;
        // counted with separators, the same way group_into_chunks counts
        let mut candidate = tail.clone();
        candidate.push(index);
        let fits_chunk = length_of(pieces, &candidate) + next_len <= size.target_chars;
        if !fits_overlap || !fits_chunk {
            break;
        }
        repeated_text += piece_len;
        tail.insert(0, index);
    }
    // a chunk made only of repeated text would add nothing new
    if tail.len() == finished.len() {
        tail.remove(0);
    }
    tail
}

fn length_of(pieces: &[Piece], indexes: &[usize]) -> usize {
    indexes
        .iter()
        .map(|&index| pieces[index].text.len() + PIECE_SEPARATOR.len())
        .sum()
}

fn build_chunk(pieces: &[Piece], indexes: &[usize]) -> Chunk {
    let mut text = String::new();
    let mut spans = Vec::with_capacity(indexes.len());
    for &index in indexes {
        if !text.is_empty() {
            text.push_str(PIECE_SEPARATOR);
        }
        let start = text.len();
        text.push_str(&pieces[index].text);
        spans.push(Span {
            piece: index,
            start,
            end: text.len(),
        });
    }
    Chunk { text, spans }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Page, Paragraph};

    fn paragraph(text: &str) -> Paragraph {
        Paragraph {
            text: text.to_string(),
            rect: Rect::default(),
        }
    }

    fn document(pages: &[&[&str]]) -> Document {
        Document {
            pages: pages
                .iter()
                .enumerate()
                .map(|(index, paragraphs)| Page {
                    number: index as u32 + 1,
                    paragraphs: paragraphs.iter().map(|text| paragraph(text)).collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn long_text_is_cut_after_sentences() {
        let text = "One two three. Four five six. Seven eight nine.";
        let slices = split_long_text(text, 20);
        assert_eq!(
            slices,
            vec!["One two three.", "Four five six.", "Seven eight nine."]
        );
        for slice in &slices {
            assert!(slice.chars().count() <= 20);
        }
    }

    #[test]
    fn a_very_long_word_is_cut_anyway() {
        let text = "x".repeat(25);
        let slices = split_long_text(&text, 10);
        assert_eq!(
            slices.iter().map(|s| s.len()).collect::<Vec<_>>(),
            vec![10, 10, 5]
        );
    }

    #[test]
    fn cutting_respects_character_boundaries() {
        // each é is two bytes; cutting by bytes would panic
        let text = "é".repeat(9);
        let slices = split_long_text(&text, 4);
        assert_eq!(slices.concat(), "é".repeat(9));
    }

    #[test]
    fn empty_paragraphs_are_skipped_and_pages_are_kept() {
        let doc = document(&[&["Alpha.", "   "], &["Beta."]]);
        let pieces = split_into_pieces(&doc, PageRange::new(1, 2), 100);
        assert_eq!(pieces.len(), 2);
        assert_eq!((pieces[0].page, pieces[1].page), (1, 2));
    }

    #[test]
    fn chunks_stay_within_budget_and_overlap_by_a_piece() {
        let words: Vec<String> = (0..12)
            .map(|i| format!("Paragraph number {i:02} here."))
            .collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let doc = document(&[&refs]);
        let size = ChunkSize {
            target_chars: 100,
            overlap_chars: 30,
        };
        let chunked = Chunked::new(&doc, PageRange::new(1, 1), size);
        assert!(chunked.chunks.len() > 1);
        for chunk in &chunked.chunks {
            assert!(chunk.text.len() <= size.target_chars, "{}", chunk.text);
        }
        for pair in chunked.chunks.windows(2) {
            let last_of_first = pair[0].spans.last().unwrap().piece;
            let first_of_second = pair[1].spans.first().unwrap().piece;
            assert_eq!(last_of_first, first_of_second, "one piece is repeated");
        }
        // every piece appears somewhere
        let mut seen: Vec<usize> = chunked
            .chunks
            .iter()
            .flat_map(|c| c.spans.iter().map(|s| s.piece))
            .collect();
        seen.dedup();
        assert_eq!(seen, (0..12).collect::<Vec<_>>());
    }

    #[test]
    fn spans_point_at_the_piece_text() {
        let doc = document(&[&["First."], &["Second."]]);
        let chunked = Chunked::new(&doc, PageRange::new(1, 2), DEFAULT_CHUNK_SIZE);
        // one chunk a page, however small
        assert_eq!(chunked.chunks.len(), 2);
        let chunk = &chunked.chunks[0];
        for span in &chunk.spans {
            assert_eq!(
                &chunk.text[span.start..span.end],
                chunked.pieces[span.piece].text
            );
        }
        assert_eq!(chunked.first_page(chunk), 1);
    }

    #[test]
    fn a_range_limits_the_pages() {
        let doc = document(&[&["One."], &["Two."], &["Three."]]);
        let chunked = Chunked::new(&doc, PageRange::new(2, 2), DEFAULT_CHUNK_SIZE);
        assert_eq!(chunked.chunks.len(), 1);
        assert_eq!(chunked.chunks[0].text, "Two.");
    }
}
