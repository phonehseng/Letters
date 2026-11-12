//! guessing how many tokens a piece of text costs.
//!
//! models read text in tokens, pieces of words. counting them exactly
//! needs the model's own tokenizer, which this crate doesn't have, so it
//! guesses from the length: about four characters per token for english.
//! when the server reports how many tokens a request really used, the
//! guess is corrected for the rest of the task.

/// characters per token for english text when nothing better is known.
pub const DEFAULT_CHARS_PER_TOKEN: f32 = 4.0;

/// a token guesser that learns from what the server reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TokenEstimator {
    chars_per_token: f32,
}

impl Default for TokenEstimator {
    fn default() -> Self {
        TokenEstimator {
            chars_per_token: DEFAULT_CHARS_PER_TOKEN,
        }
    }
}

impl TokenEstimator {
    /// the guessed token count, rounded up so budgets err on the safe side.
    pub fn estimate(&self, text: &str) -> usize {
        let chars = text.chars().count();
        (chars as f32 / self.chars_per_token).ceil() as usize
    }

    /// how many characters roughly fit in the given number of tokens.
    pub fn chars_for(&self, tokens: usize) -> usize {
        (tokens as f32 * self.chars_per_token) as usize
    }

    /// corrects the ratio from a request whose real token count is known.
    ///
    /// the ratio is kept between 1.5 and 8 so one odd answer (a page of
    /// numbers, say) can't make every later budget wildly wrong.
    pub fn learn(&mut self, chars_sent: usize, tokens_reported: usize) {
        if chars_sent < 200 || tokens_reported == 0 {
            return;
        }
        let observed = chars_sent as f32 / tokens_reported as f32;
        self.chars_per_token = observed.clamp(1.5, 8.0);
    }

    pub fn chars_per_token(&self) -> f32 {
        self.chars_per_token
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_characters_make_a_token() {
        let estimator = TokenEstimator::default();
        assert_eq!(estimator.estimate(""), 0);
        assert_eq!(estimator.estimate("abcd"), 1);
        assert_eq!(estimator.estimate("abcde"), 2);
        // characters, not bytes: accented letters count once
        assert_eq!(estimator.estimate("éééé"), 1);
    }

    #[test]
    fn reported_counts_correct_the_guess() {
        let mut estimator = TokenEstimator::default();
        estimator.learn(3000, 1000);
        assert_eq!(estimator.chars_per_token(), 3.0);
        assert_eq!(estimator.estimate(&"x".repeat(300)), 100);
    }

    #[test]
    fn odd_reports_are_kept_in_bounds() {
        let mut estimator = TokenEstimator::default();
        estimator.learn(10_000, 1);
        assert_eq!(estimator.chars_per_token(), 8.0);
        // too small a sample to learn anything from
        estimator.learn(50, 50);
        assert_eq!(estimator.chars_per_token(), 8.0);
    }
}
