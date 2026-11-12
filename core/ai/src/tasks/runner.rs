//! the shared machinery of a running task: sending requests with the
//! permit, reporting progress, checking for Stop and keeping the token
//! estimate up to date.

use std::cell::{Cell, RefCell};

use crate::cancel::{CancelToken, Progress, Stage};
use crate::provider::{ChatRequest, Completion, Permit, Provider};
use crate::tasks::budget;
use crate::tasks::thinking::{strip_thinking, ThinkingFilter};
use crate::tokens::TokenEstimator;
use crate::AiError;

/// how many texts are embedded per call, so progress moves and Stop is
/// noticed between calls.
const EMBED_BATCH: usize = 32;

pub struct Runner<'a> {
    provider: &'a dyn Provider,
    permit: Permit,
    context_tokens: usize,
    thinking_tokens: usize,
    // cells, so a streaming callback can report progress while a request
    // is in flight without fighting the borrow checker
    estimator: Cell<TokenEstimator>,
    progress: RefCell<&'a mut dyn FnMut(Progress)>,
    cancel: &'a CancelToken,
}

impl<'a> Runner<'a> {
    pub fn new(
        provider: &'a dyn Provider,
        permit: Permit,
        progress: &'a mut dyn FnMut(Progress),
        cancel: &'a CancelToken,
    ) -> Self {
        let info = provider.describe();
        Runner {
            context_tokens: info.context_tokens,
            thinking_tokens: info.thinking_tokens,
            provider,
            permit,
            estimator: Cell::new(TokenEstimator::default()),
            progress: RefCell::new(progress),
            cancel,
        }
    }

    pub fn emit(&self, progress: Progress) {
        (self.progress.borrow_mut())(progress);
    }

    pub fn step(&self, done: usize, total: usize, stage: Stage) {
        self.emit(Progress::Step { done, total, stage });
    }

    pub fn check_cancelled(&self) -> Result<(), AiError> {
        self.cancel.check()
    }

    /// room for document text in one request, using what has been
    /// learned about token counts so far.
    pub fn input_chars(
        &self,
        output_tokens: usize,
        overhead_chars: usize,
    ) -> Result<usize, AiError> {
        budget::input_chars(
            self.context_tokens,
            &self.estimator.get(),
            output_tokens + self.thinking_tokens,
            overhead_chars,
        )
    }

    /// sends a request, passing streamed text to `on_text`.
    ///
    /// a model that reasons first gets its extra room on top of the answer
    /// allowance, and its reasoning is left out of what comes back. an
    /// answer with nothing left in it is an error, not an empty summary.
    pub fn complete(
        &self,
        request: &ChatRequest,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<Completion, AiError> {
        self.cancel.check()?;
        let mut request = request.clone();
        request.max_tokens += self.thinking_tokens;
        let mut filter = ThinkingFilter::default();
        let mut completion = self.provider.complete(
            &request,
            &self.permit,
            &mut |text| filter.push(text, on_text),
            self.cancel,
        )?;
        filter.finish(on_text);
        if let Some(usage) = completion.usage {
            let mut estimator = self.estimator.get();
            estimator.learn(request.len_chars(), usage.prompt_tokens);
            self.estimator.set(estimator);
        }
        completion.text = strip_thinking(&completion.text);
        if completion.text.trim().is_empty() {
            return Err(AiError::NoAnswer);
        }
        Ok(completion)
    }

    /// sends a request whose answer is shown as it is written.
    pub fn complete_shown(&self, request: &ChatRequest) -> Result<Completion, AiError> {
        self.complete(request, &mut |text| {
            self.emit(Progress::Text(text.to_string()))
        })
    }

    /// sends a request whose answer is only a step on the way (notes).
    pub fn complete_hidden(&self, request: &ChatRequest) -> Result<Completion, AiError> {
        self.complete(request, &mut |_| {})
    }

    /// embeddings for many texts, in batches, or none if the provider
    /// can't make them.
    pub fn embed_all(&self, texts: &[String]) -> Result<Option<Vec<Vec<f32>>>, AiError> {
        let batches = texts.len().div_ceil(EMBED_BATCH);
        let mut vectors = Vec::with_capacity(texts.len());
        for (index, batch) in texts.chunks(EMBED_BATCH).enumerate() {
            self.step(index, batches, Stage::Searching);
            self.cancel.check()?;
            match self.provider.embed(batch, &self.permit, self.cancel)? {
                Some(found) if found.len() == batch.len() => vectors.extend(found),
                Some(_) => return Err(AiError::MalformedResponse),
                None => return Ok(None),
            }
        }
        self.step(batches, batches, Stage::Searching);
        Ok(Some(vectors))
    }
}
