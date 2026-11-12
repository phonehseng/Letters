//! leaving out a model's reasoning.
//!
//! some models write out their reasoning first, between `<think>` and
//! `</think>`, and only then the answer. some servers pass that block on
//! inside the answer text. the person asked for a summary, not for the
//! reasoning, so it is neither streamed to the screen nor kept.

const OPEN: &str = "<think>";
const CLOSE: &str = "</think>";

/// the answer without a leading reasoning block. an unfinished block
/// means the model never got to the answer, so nothing is left.
pub fn strip_thinking(text: &str) -> String {
    let start = text.trim_start();
    match start.strip_prefix(OPEN) {
        Some(rest) => match rest.find(CLOSE) {
            Some(end) => rest[end + CLOSE.len()..].trim_start().to_string(),
            None => String::new(),
        },
        None => text.to_string(),
    }
}

/// does the same to an answer while it streams in.
pub struct ThinkingFilter {
    state: State,
}

enum State {
    /// not yet sure whether the answer starts with a reasoning block;
    /// holds what has arrived so far.
    Undecided(String),
    /// inside the block; only the last few characters are kept, enough
    /// to spot the closing tag when it arrives split in two.
    Thinking(String),
    /// past it (or there was none): everything is passed on.
    Answering,
}

impl Default for ThinkingFilter {
    fn default() -> Self {
        ThinkingFilter {
            state: State::Undecided(String::new()),
        }
    }
}

impl ThinkingFilter {
    pub fn push(&mut self, text: &str, show: &mut dyn FnMut(&str)) {
        match &mut self.state {
            State::Answering => show(text),
            State::Undecided(held) => {
                held.push_str(text);
                let start = held.trim_start();
                if let Some(rest) = start.strip_prefix(OPEN) {
                    let rest = rest.to_string();
                    self.state = State::Thinking(String::new());
                    self.push(&rest, show);
                } else if !OPEN.starts_with(start) {
                    let everything = std::mem::take(held);
                    self.state = State::Answering;
                    show(&everything);
                }
            }
            State::Thinking(tail) => {
                tail.push_str(text);
                if let Some(end) = tail.find(CLOSE) {
                    let rest = tail[end + CLOSE.len()..].trim_start().to_string();
                    self.state = State::Answering;
                    if !rest.is_empty() {
                        show(&rest);
                    }
                } else {
                    keep_tail(tail, CLOSE.len());
                }
            }
        }
    }

    /// passes on anything held back while undecided.
    pub fn finish(&mut self, show: &mut dyn FnMut(&str)) {
        if let State::Undecided(held) = &self.state {
            if !held.trim().is_empty() {
                show(held);
            }
        }
        self.state = State::Answering;
    }
}

/// drops all but about the last `chars` characters.
fn keep_tail(text: &mut String, chars: usize) {
    let count = text.chars().count();
    if count > chars {
        let cut = text.char_indices().nth(count - chars).map_or(0, |(i, _)| i);
        text.drain(..cut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn streamed(pieces: &[&str]) -> String {
        let mut filter = ThinkingFilter::default();
        let mut shown = String::new();
        for piece in pieces {
            filter.push(piece, &mut |t| shown.push_str(t));
        }
        filter.finish(&mut |t| shown.push_str(t));
        shown
    }

    #[test]
    fn reasoning_is_left_out() {
        assert_eq!(
            strip_thinking("<think>hmm, cells</think>\n\nCells make energy."),
            "Cells make energy."
        );
        assert_eq!(strip_thinking("<think>still thinking"), "");
        assert_eq!(strip_thinking("Plain answer."), "Plain answer.");
    }

    #[test]
    fn reasoning_is_not_streamed_even_when_split() {
        assert_eq!(
            streamed(&["<thi", "nk>reason", "ing here</th", "ink>\nThe ans", "wer."]),
            "The answer."
        );
        assert_eq!(streamed(&["The ", "answer."]), "The answer.");
        assert_eq!(streamed(&["<"]), "<");
    }
}
