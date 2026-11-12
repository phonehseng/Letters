//! "jump to where it talks about x".
//!
//! without a provider, or with one that can't make embeddings, this is
//! pure bm25 over the document's paragraphs: nothing is sent anywhere.
//! with embeddings, whole chunks are ranked by a mix of word matches and
//! meaning, and each good chunk is turned back into the paragraph inside
//! it that best matches, because a paragraph is what the viewer can jump
//! to and highlight.

use serde::Serialize;

use crate::chunking::Chunked;
use crate::document::Rect;
use crate::retrieval::{self, cosine_similarity, Bm25};
use crate::tasks::runner::Runner;
use crate::AiError;

/// the most chunks embedded for one search. a 500 page book has about
/// this many; past it, the chunks with the best word matches are used.
pub const MAX_EMBEDDED_CHUNKS: usize = 512;

/// how much of a paragraph is shown in the results list.
const SNIPPET_CHARS: usize = 240;

/// one place in the document that matches.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundPassage {
    pub page: u32,
    pub rect: Rect,
    /// the start of the paragraph, for the results list.
    pub text: String,
    /// higher is better. only useful for comparing results of one search.
    pub score: f32,
}

/// ranks paragraphs by word matches alone.
pub fn find_by_words(chunked: &Chunked, topic: &str, limit: usize) -> Vec<FoundPassage> {
    let index = Bm25::new(chunked.pieces.iter().map(|piece| piece.text.as_str()));
    retrieval::rank(&index.scores(topic), limit)
        .into_iter()
        .map(|(piece, score)| found(chunked, piece, score))
        .collect()
}

/// ranks chunks by words and meaning, then points at the best paragraph
/// of each.
pub fn find_by_meaning(
    runner: &Runner,
    chunked: &Chunked,
    topic: &str,
    candidates: &[usize],
    limit: usize,
) -> Result<Vec<FoundPassage>, AiError> {
    let chunk_index = Bm25::new(chunked.chunks.iter().map(|chunk| chunk.text.as_str()));
    let all_lexical = chunk_index.scores(topic);
    let lexical: Vec<f32> = candidates.iter().map(|&c| all_lexical[c]).collect();
    let texts: Vec<&str> = candidates
        .iter()
        .map(|&c| chunked.chunks[c].text.as_str())
        .collect();
    let Some(semantic) = similarities(runner, topic, &texts)? else {
        return Ok(find_by_words(chunked, topic, limit));
    };
    let mixed = retrieval::mix_scores(&lexical, &semantic);
    let piece_scores =
        Bm25::new(chunked.pieces.iter().map(|piece| piece.text.as_str())).scores(topic);
    let mut results: Vec<FoundPassage> = Vec::new();
    let mut used_pieces = Vec::new();
    for (position, score) in retrieval::rank(&mixed, candidates.len()) {
        let chunk = &chunked.chunks[candidates[position]];
        let best_piece = chunk
            .spans
            .iter()
            .map(|span| span.piece)
            .filter(|piece| !used_pieces.contains(piece))
            .max_by(|a, b| piece_scores[*a].total_cmp(&piece_scores[*b]).then(b.cmp(a)));
        if let Some(piece) = best_piece {
            used_pieces.push(piece);
            results.push(found(chunked, piece, score));
        }
        if results.len() >= limit {
            break;
        }
    }
    Ok(results)
}

/// how alike the query is to each text, or none without embeddings.
pub fn similarities(
    runner: &Runner,
    query: &str,
    texts: &[&str],
) -> Result<Option<Vec<f32>>, AiError> {
    let mut inputs = Vec::with_capacity(texts.len() + 1);
    inputs.push(query.to_string());
    inputs.extend(texts.iter().map(|text| text.to_string()));
    let Some(vectors) = runner.embed_all(&inputs)? else {
        return Ok(None);
    };
    let (query_vector, text_vectors) = vectors.split_first().ok_or(AiError::MalformedResponse)?;
    Ok(Some(
        text_vectors
            .iter()
            .map(|v| cosine_similarity(query_vector, v))
            .collect(),
    ))
}

/// which chunks to embed: all of them when there are few, otherwise the
/// ones with the best word matches, then the rest in reading order.
/// the result is in reading order.
pub fn embedding_candidates(lexical: &[f32], cap: usize) -> Vec<usize> {
    if lexical.len() <= cap {
        return (0..lexical.len()).collect();
    }
    let mut order: Vec<usize> = (0..lexical.len()).collect();
    order.sort_by(|a, b| lexical[*b].total_cmp(&lexical[*a]).then(a.cmp(b)));
    order.truncate(cap);
    order.sort_unstable();
    order
}

fn found(chunked: &Chunked, piece: usize, score: f32) -> FoundPassage {
    let piece = &chunked.pieces[piece];
    FoundPassage {
        page: piece.page,
        rect: piece.rect,
        text: snippet(&piece.text),
        score,
    }
}

fn snippet(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= SNIPPET_CHARS {
        return text;
    }
    let cut: String = text.chars().take(SNIPPET_CHARS).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_keep_the_best_matches_in_reading_order() {
        assert_eq!(embedding_candidates(&[0.0, 1.0, 0.0], 5), vec![0, 1, 2]);
        assert_eq!(
            embedding_candidates(&[0.0, 3.0, 0.0, 1.0, 2.0], 3),
            vec![1, 3, 4]
        );
        // with no matches at all, the first chunks are used
        assert_eq!(embedding_candidates(&[0.0; 5], 2), vec![0, 1]);
    }

    #[test]
    fn long_paragraphs_are_shortened() {
        let text = "word ".repeat(100);
        let short = snippet(&text);
        assert!(short.ends_with('…'));
        assert!(short.chars().count() <= SNIPPET_CHARS + 1);
    }
}
