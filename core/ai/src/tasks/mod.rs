//! the four document tasks, and the plan-then-run door into them.
//!
//! ```text
//! let tools = AiTools::from_settings(&settings, &key_store)?;   // refuses when off
//! let plan = tools.plan(&document, TaskRequest::Summarize { .. })?;
//! show(plan.disclosure().sentence());                          // nothing sent yet
//! let ack = plan.disclosure().acknowledge();                   // the person said yes
//! let output = tools.run(plan, Some(ack), &mut on_progress, &cancel)?;
//! ```
//!
//! "find where" also works with no provider at all through
//! `find_where_offline`.

pub mod ask;
pub mod budget;
pub mod citations;
pub mod digest;
pub mod find;
pub mod runner;
pub mod thinking;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use crate::cancel::{CancelToken, Progress};
use crate::chunking::{ChunkSize, Chunked, Piece, DEFAULT_CHUNK_SIZE};
use crate::disclosure::{self, Acknowledgement, Disclosure, QueryKind};
use crate::document::{Document, PageRange};
use crate::prompts;
use crate::provider::{Provider, ProviderInfo};
use crate::secret::KeyStore;
use crate::settings::Settings;
use crate::tokens::TokenEstimator;
use crate::AiError;

use self::citations::Answer;
use self::digest::{KeyPoint, Summary, SummaryLength};
use self::find::FoundPassage;
use self::runner::Runner;

/// what the person asked for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "camelCase")]
pub enum TaskRequest {
    /// `pages: None` means the whole document.
    Summarize {
        pages: Option<PageRange>,
        length: SummaryLength,
    },
    KeyPoints {
        pages: Option<PageRange>,
        count: usize,
    },
    Ask {
        question: String,
        pages: Option<PageRange>,
    },
    FindWhere {
        topic: String,
        pages: Option<PageRange>,
        limit: usize,
    },
}

/// what a task produced.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "result", rename_all = "camelCase")]
pub enum TaskOutput {
    Summary(Summary),
    KeyPoints(Vec<KeyPoint>),
    Answer(Answer),
    Passages(Vec<FoundPassage>),
}

/// a task worked out but not run: what it would send is in its
/// disclosure, and nothing has been sent.
///
/// it is used up by `run`, so one "yes" runs it once.
pub struct PlannedTask {
    disclosure: Disclosure,
    work: Work,
}

impl PlannedTask {
    pub fn disclosure(&self) -> &Disclosure {
        &self.disclosure
    }
}

impl std::fmt::Debug for PlannedTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlannedTask")
            .field("disclosure", &self.disclosure)
            .finish_non_exhaustive()
    }
}

/// the prepared input of each task, private so only `run` can use it.
enum Work {
    Summarize {
        pieces: Vec<Piece>,
        range: PageRange,
        length: SummaryLength,
    },
    KeyPoints {
        pieces: Vec<Piece>,
        range: PageRange,
        count: usize,
    },
    Ask {
        question: String,
        chunked: Chunked,
        selection: ask::Selection,
    },
    FindByMeaning {
        topic: String,
        chunked: Chunked,
        candidates: Vec<usize>,
        limit: usize,
    },
    /// the answer is known without the model ("doesn't say", or a find
    /// done by word matches alone).
    Ready(TaskOutput),
}

/// the ai tools, bound to the one provider the person set up.
pub struct AiTools {
    provider: Box<dyn Provider>,
}

impl AiTools {
    /// refuses unless the tools are turned on and a provider is chosen.
    pub fn from_settings(settings: &Settings, keys: &dyn KeyStore) -> Result<AiTools, AiError> {
        Ok(AiTools {
            provider: settings.build_provider(keys)?,
        })
    }

    /// for a provider made some other way (tests, or a provider the shell
    /// built itself from the person's settings).
    pub fn with_provider(provider: Box<dyn Provider>) -> AiTools {
        AiTools { provider }
    }

    pub fn provider_info(&self) -> ProviderInfo {
        self.provider.describe()
    }

    /// works out what a task would send. sends nothing.
    pub fn plan(&self, document: &Document, request: TaskRequest) -> Result<PlannedTask, AiError> {
        let info = self.provider.describe();
        let size = chunk_size(&info)?;
        match request {
            TaskRequest::Summarize { pages, length } => {
                let (pieces, range) = pieces_for(document, pages, size)?;
                let disclosure = disclose_pieces(&info, &pieces, None);
                Ok(PlannedTask {
                    disclosure,
                    work: Work::Summarize {
                        pieces,
                        range,
                        length,
                    },
                })
            }
            TaskRequest::KeyPoints { pages, count } => {
                let (pieces, range) = pieces_for(document, pages, size)?;
                let disclosure = disclose_pieces(&info, &pieces, None);
                Ok(PlannedTask {
                    disclosure,
                    work: Work::KeyPoints {
                        pieces,
                        range,
                        count,
                    },
                })
            }
            TaskRequest::Ask { question, pages } => {
                plan_ask(&info, document, question, pages, size)
            }
            TaskRequest::FindWhere {
                topic,
                pages,
                limit,
            } => plan_find(&info, document, topic, pages, limit, size),
        }
    }

    /// runs a planned task.
    ///
    /// text that would leave this computer needs `acknowledgement` to be
    /// the person's yes to this plan's disclosure; otherwise it is refused
    /// before anything is sent.
    pub fn run(
        &self,
        task: PlannedTask,
        acknowledgement: Option<Acknowledgement>,
        progress: &mut dyn FnMut(Progress),
        cancel: &CancelToken,
    ) -> Result<TaskOutput, AiError> {
        let permit = disclosure::authorise(
            &task.disclosure,
            acknowledgement.as_ref(),
            &self.provider.describe(),
        )?;
        cancel.check()?;
        let runner = Runner::new(self.provider.as_ref(), permit, progress, cancel);
        match task.work {
            Work::Ready(output) => Ok(output),
            Work::Summarize {
                pieces,
                range,
                length,
            } => digest::summarize(&runner, &pieces, range, length).map(TaskOutput::Summary),
            Work::KeyPoints {
                pieces,
                range,
                count,
            } => digest::key_points(&runner, &pieces, range, count).map(TaskOutput::KeyPoints),
            Work::Ask {
                question,
                chunked,
                selection,
            } => ask::answer(&runner, &chunked, &question, &selection).map(TaskOutput::Answer),
            Work::FindByMeaning {
                topic,
                chunked,
                candidates,
                limit,
            } => find::find_by_meaning(&runner, &chunked, &topic, &candidates, limit)
                .map(TaskOutput::Passages),
        }
    }
}

/// "jump to where it talks about x" with no provider: word matches only,
/// entirely on this computer.
pub fn find_where_offline(
    document: &Document,
    topic: &str,
    pages: Option<PageRange>,
    limit: usize,
) -> Result<Vec<FoundPassage>, AiError> {
    let topic = non_empty(topic)?;
    let range = document.range_or_all(pages)?;
    let chunked = Chunked::new(document, range, DEFAULT_CHUNK_SIZE);
    Ok(find::find_by_words(&chunked, topic, limit.max(1)))
}

fn plan_ask(
    info: &ProviderInfo,
    document: &Document,
    question: String,
    pages: Option<PageRange>,
    size: ChunkSize,
) -> Result<PlannedTask, AiError> {
    let question = non_empty(&question)?.to_string();
    let chunked = chunked_for(document, pages, size)?;
    let budget = budget::input_chars(
        info.context_tokens,
        &TokenEstimator::default(),
        ask::ANSWER_TOKENS + info.thinking_tokens,
        prompts::overhead_chars(prompts::ASK, Some(&question)),
    )?;
    let selection = ask::select(&chunked, &question, info.supports_embeddings, budget);
    let sent = match &selection {
        ask::Selection::Chosen(chosen) | ask::Selection::ByMeaning(chosen) => chosen.clone(),
    };
    if sent.is_empty() {
        return Ok(ready(info, TaskOutput::Answer(Answer::doesnt_say())));
    }
    let disclosure = disclose_chunks(
        info,
        &chunked,
        &sent,
        Some((QueryKind::Question, &question)),
    );
    Ok(PlannedTask {
        disclosure,
        work: Work::Ask {
            question,
            chunked,
            selection,
        },
    })
}

fn plan_find(
    info: &ProviderInfo,
    document: &Document,
    topic: String,
    pages: Option<PageRange>,
    limit: usize,
    size: ChunkSize,
) -> Result<PlannedTask, AiError> {
    let topic = non_empty(&topic)?.to_string();
    let limit = limit.max(1);
    let chunked = chunked_for(document, pages, size)?;
    if !info.supports_embeddings {
        // nothing to gain from the provider: answer here and send nothing
        let found = find::find_by_words(&chunked, &topic, limit);
        return Ok(ready(info, TaskOutput::Passages(found)));
    }
    let lexical = ask::lexical_scores(&chunked, &topic);
    let candidates = find::embedding_candidates(&lexical, find::MAX_EMBEDDED_CHUNKS);
    let disclosure = disclose_chunks(
        info,
        &chunked,
        &candidates,
        Some((QueryKind::Search, &topic)),
    );
    Ok(PlannedTask {
        disclosure,
        work: Work::FindByMeaning {
            topic,
            chunked,
            candidates,
            limit,
        },
    })
}

/// a plan whose answer is already known and sends nothing.
fn ready(info: &ProviderInfo, output: TaskOutput) -> PlannedTask {
    PlannedTask {
        disclosure: Disclosure::new(info, Vec::new(), &[], None),
        work: Work::Ready(output),
    }
}

/// chunks small enough that several fit next to a question, even for a
/// model with a small context.
fn chunk_size(info: &ProviderInfo) -> Result<ChunkSize, AiError> {
    let room = budget::input_chars(
        info.context_tokens,
        &TokenEstimator::default(),
        ask::ANSWER_TOKENS + info.thinking_tokens,
        prompts::overhead_chars(prompts::ASK, None),
    )?;
    let target = (room / 3).clamp(200, DEFAULT_CHUNK_SIZE.target_chars);
    Ok(ChunkSize {
        target_chars: target,
        overlap_chars: target / 5,
    })
}

fn pieces_for(
    document: &Document,
    pages: Option<PageRange>,
    size: ChunkSize,
) -> Result<(Vec<Piece>, PageRange), AiError> {
    let range = document.range_or_all(pages)?;
    let pieces = crate::chunking::split_into_pieces(document, range, size.target_chars);
    if pieces.is_empty() {
        return Err(AiError::NothingToRead);
    }
    Ok((pieces, range))
}

fn chunked_for(
    document: &Document,
    pages: Option<PageRange>,
    size: ChunkSize,
) -> Result<Chunked, AiError> {
    let range = document.range_or_all(pages)?;
    let chunked = Chunked::new(document, range, size);
    if chunked.pieces.is_empty() {
        return Err(AiError::NothingToRead);
    }
    Ok(chunked)
}

fn non_empty(text: &str) -> Result<&str, AiError> {
    let text = text.trim();
    if text.is_empty() {
        Err(AiError::EmptyQuery)
    } else {
        Ok(text)
    }
}

/// the disclosure for sending these pieces (in reading order).
fn disclose_pieces<'a>(
    info: &ProviderInfo,
    pieces: impl IntoIterator<Item = &'a Piece>,
    query: Option<(QueryKind, &str)>,
) -> Disclosure {
    let mut pages = Vec::new();
    let mut texts = Vec::new();
    for piece in pieces {
        if pages.last() != Some(&piece.page) {
            pages.push(piece.page);
        }
        texts.push(piece.text.as_str());
    }
    Disclosure::new(info, pages, &texts, query)
}

/// the disclosure for some chunks, counting each piece once even where
/// chunks overlap.
fn disclose_chunks(
    info: &ProviderInfo,
    chunked: &Chunked,
    chunks: &[usize],
    query: Option<(QueryKind, &str)>,
) -> Disclosure {
    let mut piece_indexes: Vec<usize> = chunks
        .iter()
        .flat_map(|&chunk| chunked.chunks[chunk].spans.iter().map(|span| span.piece))
        .collect();
    piece_indexes.sort_unstable();
    piece_indexes.dedup();
    let pieces = piece_indexes.iter().map(|&index| &chunked.pieces[index]);
    disclose_pieces(info, pieces, query)
}
