//! fitting text into the model's context.
//!
//! a model reads a fixed number of tokens at once, and that number has to
//! hold the instructions, the document text *and* the answer. so the room
//! for document text is what is left after the other two, with a little
//! spare because token counts are only estimated.

use std::ops::Range;

use crate::chunking::Piece;
use crate::prompts;
use crate::tokens::TokenEstimator;
use crate::AiError;

/// tokens kept spare in every request, for estimation errors.
const SPARE_TOKENS: usize = 64;

/// below this, there is no point sending anything: the model couldn't
/// read even one paragraph.
const MIN_INPUT_TOKENS: usize = 100;

/// how many characters of document text fit in one request.
pub fn input_chars(
    context_tokens: usize,
    estimator: &TokenEstimator,
    output_tokens: usize,
    overhead_chars: usize,
) -> Result<usize, AiError> {
    let overhead_tokens = (overhead_chars as f32 / estimator.chars_per_token()).ceil() as usize;
    let available = context_tokens.saturating_sub(output_tokens + overhead_tokens + SPARE_TOKENS);
    if available < MIN_INPUT_TOKENS {
        return Err(AiError::OutOfContext);
    }
    Ok(estimator.chars_for(available))
}

/// a labelled block of document text, as it goes into a request.
pub type Passage = (String, String);

/// what a passage costs inside a request: its label, text and line breaks.
pub fn passage_len(passage: &Passage) -> usize {
    passage.0.chars().count() + passage.1.chars().count() + 3
}

/// splits passages, in order, into groups that each fit `budget`.
///
/// refuses when a single passage is bigger than the whole budget, since
/// no grouping could make it fit.
pub fn pack(passages: &[Passage], budget: usize) -> Result<Vec<Range<usize>>, AiError> {
    let mut groups = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (index, passage) in passages.iter().enumerate() {
        let len = passage_len(passage);
        if len > budget {
            return Err(AiError::OutOfContext);
        }
        if used + len > budget {
            groups.push(start..index);
            start = index;
            used = 0;
        }
        used += len;
    }
    if start < passages.len() {
        groups.push(start..passages.len());
    }
    Ok(groups)
}

/// the total size of some passages.
pub fn total_len(passages: &[Passage]) -> usize {
    passages.iter().map(passage_len).sum()
}

/// turns pieces into passages, one per page, so each is labelled with its
/// page once instead of once per paragraph.
///
/// a page's pieces are split into several passages when together they
/// would be longer than `max_chars`, so packing can still split them.
pub fn page_passages(pieces: &[Piece], max_chars: usize) -> Vec<Passage> {
    let mut passages: Vec<(u32, String)> = Vec::new();
    for piece in pieces {
        match passages.last_mut() {
            Some((page, text))
                if *page == piece.page && text.len() + piece.text.len() + 2 <= max_chars =>
            {
                text.push_str("\n\n");
                text.push_str(&piece.text);
            }
            _ => passages.push((piece.page, piece.text.clone())),
        }
    }
    passages
        .into_iter()
        .map(|(page, text)| (prompts::page_label(page), text))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Rect;

    fn piece(page: u32, text: &str) -> Piece {
        Piece {
            page,
            rect: Rect::default(),
            text: text.to_string(),
        }
    }

    #[test]
    fn room_is_what_the_rules_and_answer_leave() {
        let estimator = TokenEstimator::default();
        // 4096 - 500 answer - 100 overhead - 64 spare = 3432 tokens
        assert_eq!(input_chars(4096, &estimator, 500, 400).unwrap(), 3432 * 4);
        assert_eq!(
            input_chars(600, &estimator, 500, 400),
            Err(AiError::OutOfContext)
        );
    }

    #[test]
    fn packing_keeps_order_and_budget() {
        let passages: Vec<Passage> = (0..5)
            .map(|i| (format!("[page {i}]"), "x".repeat(40)))
            .collect();
        let each = passage_len(&passages[0]);
        let groups = pack(&passages, each * 2).unwrap();
        assert_eq!(groups, vec![0..2, 2..4, 4..5]);
        assert_eq!(pack(&passages, each - 1), Err(AiError::OutOfContext));
    }

    #[test]
    fn pieces_on_one_page_share_a_label() {
        let pieces = [piece(1, "a"), piece(1, "b"), piece(2, "c"), piece(1, "d")];
        let passages = page_passages(&pieces, 100);
        assert_eq!(
            passages,
            vec![
                ("[page 1]".to_string(), "a\n\nb".to_string()),
                ("[page 2]".to_string(), "c".to_string()),
                ("[page 1]".to_string(), "d".to_string()),
            ]
        );
        // a small limit keeps them apart
        assert_eq!(page_passages(&pieces[..2], 2).len(), 2);
    }
}
