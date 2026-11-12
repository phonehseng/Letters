//! answering a question from the document, with checked quotes.
//!
//! 1. **retrieve**: rank the chunks against the question (word matches,
//!    plus meaning when the provider has embeddings) and keep the best few
//!    that fit in the context.
//! 2. **nothing relevant, no request**: if no chunk matches at all, the
//!    answer is "The document doesn’t say." without asking the model.
//! 3. **answer**: the model sees only the chosen chunks, numbered, and must
//!    cite them with exact quotes.
//! 4. **check**: every quote is looked up in the chunks that were sent
//!    (`citations.rs`).

use crate::cancel::Stage;
use crate::chunking::Chunked;
use crate::prompts::{self, build_request, NOT_IN_DOCUMENT};
use crate::retrieval::{self, Bm25};
use crate::tasks::budget::{passage_len, Passage};
use crate::tasks::citations::{verify_answer, Answer};
use crate::tasks::find::{self, MAX_EMBEDDED_CHUNKS};
use crate::tasks::runner::Runner;
use crate::AiError;

/// the most passages sent with one question.
pub const MAX_ANSWER_PASSAGES: usize = 6;
pub const ANSWER_TOKENS: usize = 500;
/// with embeddings, how alike a chunk must be to count as relevant even
/// when it shares no words with the question.
const MEANING_THRESHOLD: f32 = 0.3;

/// which chunks a question will use, decided when planning.
#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    /// already chosen by word matches; exactly these are sent.
    Chosen(Vec<usize>),
    /// these candidates are embedded first and the best are chosen then.
    ByMeaning(Vec<usize>),
}

/// word-match scores for every chunk.
pub fn lexical_scores(chunked: &Chunked, question: &str) -> Vec<f32> {
    Bm25::new(chunked.chunks.iter().map(|chunk| chunk.text.as_str())).scores(question)
}

/// decides what a question will send, before anything is sent.
pub fn select(
    chunked: &Chunked,
    question: &str,
    use_meaning: bool,
    budget_chars: usize,
) -> Selection {
    let lexical = lexical_scores(chunked, question);
    if use_meaning {
        return Selection::ByMeaning(find::embedding_candidates(&lexical, MAX_EMBEDDED_CHUNKS));
    }
    let ranked: Vec<usize> = retrieval::rank(&lexical, MAX_ANSWER_PASSAGES)
        .into_iter()
        .map(|(chunk, _)| chunk)
        .collect();
    Selection::Chosen(fit(chunked, &ranked, budget_chars))
}

/// keeps the best chunks, in rank order, while they fit.
fn fit(chunked: &Chunked, ranked: &[usize], budget_chars: usize) -> Vec<usize> {
    let mut chosen = Vec::new();
    let mut used = 0;
    for &chunk in ranked.iter().take(MAX_ANSWER_PASSAGES) {
        let cost = passage_len(&passage(chunked, chosen.len() + 1, chunk));
        if used + cost > budget_chars {
            break;
        }
        used += cost;
        chosen.push(chunk);
    }
    chosen
}

/// the room for passages next to the question.
pub fn budget_chars(runner: &Runner, question: &str) -> Result<usize, AiError> {
    runner.input_chars(
        ANSWER_TOKENS,
        prompts::overhead_chars(prompts::ASK, Some(question)),
    )
}

pub fn answer(
    runner: &Runner,
    chunked: &Chunked,
    question: &str,
    selection: &Selection,
) -> Result<Answer, AiError> {
    let chosen = match selection {
        Selection::Chosen(chosen) => chosen.clone(),
        Selection::ByMeaning(candidates) => {
            choose_by_meaning(runner, chunked, question, candidates)?
        }
    };
    if chosen.is_empty() {
        return Ok(Answer::doesnt_say());
    }
    let passages: Vec<Passage> = chosen
        .iter()
        .enumerate()
        .map(|(index, &chunk)| passage(chunked, index + 1, chunk))
        .collect();
    let request = build_request(prompts::ASK, &passages, Some(question), ANSWER_TOKENS);
    runner.step(0, 1, Stage::Writing);
    let mut filter = AnswerFilter::default();
    let completion = runner.complete(&request, &mut |text| {
        filter.push(text, &mut |shown| {
            runner.emit(crate::Progress::Text(shown.to_string()))
        })
    })?;
    filter.finish(&mut |shown| runner.emit(crate::Progress::Text(shown.to_string())));
    runner.step(1, 1, Stage::Writing);
    Ok(verify_answer(&completion.text, &chosen, chunked))
}

fn choose_by_meaning(
    runner: &Runner,
    chunked: &Chunked,
    question: &str,
    candidates: &[usize],
) -> Result<Vec<usize>, AiError> {
    let all_lexical = lexical_scores(chunked, question);
    let lexical: Vec<f32> = candidates.iter().map(|&c| all_lexical[c]).collect();
    let texts: Vec<&str> = candidates
        .iter()
        .map(|&c| chunked.chunks[c].text.as_str())
        .collect();
    let semantic =
        find::similarities(runner, question, &texts)?.unwrap_or_else(|| vec![0.0; texts.len()]);
    // a chunk counts only if it shares a word with the question or is
    // clearly close in meaning; otherwise "doesn't say" is the honest answer
    let relevant: Vec<f32> = retrieval::mix_scores(&lexical, &semantic)
        .into_iter()
        .zip(lexical.iter().zip(&semantic))
        .map(|(mixed, (lex, sem))| {
            if *lex > 0.0 || *sem >= MEANING_THRESHOLD {
                mixed
            } else {
                0.0
            }
        })
        .collect();
    let ranked: Vec<usize> = retrieval::rank(&relevant, MAX_ANSWER_PASSAGES)
        .into_iter()
        .map(|(position, _)| candidates[position])
        .collect();
    Ok(fit(chunked, &ranked, budget_chars(runner, question)?))
}

/// one numbered passage, labelled with its page (a chunk never crosses
/// a page break, so one page number is always right).
pub fn passage(chunked: &Chunked, number: usize, chunk: usize) -> Passage {
    let chunk = &chunked.chunks[chunk];
    (
        prompts::passage_label(number, chunked.first_page(chunk)),
        chunk.text.clone(),
    )
}

/// passes the answer on as it streams, but not the SOURCES list or a
/// bare NOT_IN_DOCUMENT, which are for this crate rather than the person.
#[derive(Default)]
struct AnswerFilter {
    pending: String,
    stopped: bool,
}

const SOURCES_MARK: &str = "SOURCES:";

impl AnswerFilter {
    fn push(&mut self, text: &str, show: &mut dyn FnMut(&str)) {
        if self.stopped {
            return;
        }
        self.pending.push_str(text);
        if let Some(at) = self.pending.find(SOURCES_MARK) {
            show_non_empty(self.pending[..at].trim_end(), show);
            self.stopped = true;
            return;
        }
        let start = self.pending.trim_start();
        if start.starts_with(NOT_IN_DOCUMENT) {
            self.stopped = true;
            return;
        }
        if NOT_IN_DOCUMENT.starts_with(start) {
            // could still turn into NOT_IN_DOCUMENT; wait and see
            return;
        }
        // hold back enough to recognise a SOURCES: split across pieces
        let keep_from = safe_cut(
            &self.pending,
            self.pending.len().saturating_sub(SOURCES_MARK.len()),
        );
        show_non_empty(&self.pending[..keep_from], show);
        self.pending.drain(..keep_from);
    }

    fn finish(&mut self, show: &mut dyn FnMut(&str)) {
        if !self.stopped && !self.pending.trim().starts_with(NOT_IN_DOCUMENT) {
            show_non_empty(&self.pending, show);
        }
        self.pending.clear();
        self.stopped = true;
    }
}

fn show_non_empty(text: &str, show: &mut dyn FnMut(&str)) {
    if !text.is_empty() {
        show(text);
    }
}

/// the nearest character boundary at or before `index`.
fn safe_cut(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filtered(pieces: &[&str]) -> String {
        let mut filter = AnswerFilter::default();
        let mut shown = String::new();
        for piece in pieces {
            filter.push(piece, &mut |text| shown.push_str(text));
        }
        filter.finish(&mut |text| shown.push_str(text));
        shown
    }

    #[test]
    fn sources_are_not_streamed() {
        assert_eq!(
            filtered(&["The answer", " is 4 [1].\nSOU", "RCES:\n[1] \"x y\""]),
            "The answer is 4 [1]."
        );
    }

    #[test]
    fn not_in_document_is_not_streamed() {
        assert_eq!(filtered(&["NOT_", "IN_DOC", "UMENT"]), "");
        assert_eq!(filtered(&["NOT", " really, it is."]), "NOT really, it is.");
    }

    #[test]
    fn plain_answers_stream_whole() {
        assert_eq!(filtered(&["héllo ", "wörld"]), "héllo wörld");
    }
}
