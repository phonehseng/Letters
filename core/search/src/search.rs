//! answering a query: which passages match, how well, and what to show.
//!
//! a search runs in two rounds. the first uses only the word lists in
//! memory: every word of the query is widened to the spellings that
//! count for it (the word itself, its other english forms, words it
//! begins while typing, and near misses for a typo), and passages are
//! scored with bm25, the usual formula that rewards rare words and does
//! not let long pages win just by being long. the second round reads
//! the best few hundred passages from disk to check phrases, reward
//! words that sit close together, and cut a snippet around them.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::expand::strip_mark;
use crate::index::{path_key, Index};
use crate::model::DocRecord;
use crate::normalize::{tokenize, Token};
use crate::query::{parse, ParsedQuery, Part};
use crate::results::{Correction, DocumentHit, Filters, PassageHit, SearchRequest, SearchResults};
use crate::snippet::{closest_window, contains_phrase, make_snippet};
use crate::store::{PassageReader, StoredPassage};
use crate::SearchError;

/// how much each kind of match counts, against the word exactly as typed.
pub(crate) const EXACT: f32 = 1.0;
pub(crate) const SAME_STEM: f32 = 0.85;
pub(crate) const PREFIX: f32 = 0.75;
pub(crate) const TYPO: f32 = 0.6;
/// a passage under a heading (or in a document titled) with the word
const HEADING_BONUS: f32 = 1.5;
/// passages read from disk for the second round
const SECOND_ROUND: usize = 300;
pub(crate) const MAX_PREFIX_WORDS: usize = 50;
pub(crate) const MAX_STEM_SCAN: usize = 3000;
pub(crate) const MAX_TYPO_WORDS: usize = 5;
const BM25_K1: f32 = 1.2;
const BM25_B: f32 = 0.75;

/// one spelling a query word stands for.
#[derive(Clone, Debug)]
pub(crate) struct Expansion {
    pub(crate) key: String,
    pub(crate) weight: f32,
}

/// a clause after the first round: matching passages and their scores.
#[derive(Default)]
pub(crate) struct ClauseMatches {
    /// passage -> (score, true when only a phrase alternative matched,
    /// which the second round must confirm)
    pub(crate) units: HashMap<u32, (f32, bool)>,
    /// the text words that count as this clause, for proximity and highlights
    pub(crate) words: HashSet<String>,
    pub(crate) phrases: Vec<Vec<String>>,
}

/// a passage that passed the second round: its unit number and score, the
/// passage as stored, its words, and which of them matched the query.
type CheckedPassage = (u32, f32, StoredPassage, Vec<Token>, Vec<usize>);

impl Index {
    pub fn search(&self, request: &SearchRequest) -> Result<SearchResults, SearchError> {
        let started = Instant::now();
        let parsed = parse(&request.query);
        if parsed.is_empty() {
            return Ok(SearchResults::default());
        }
        let mut searcher = Searcher::new(self, request);
        let mut results = searcher.run(&parsed)?;
        results.micros = started.elapsed().as_micros() as u64;
        Ok(results)
    }
}

pub(crate) struct Searcher<'a> {
    pub(crate) index: &'a Index,
    pub(crate) request: &'a SearchRequest,
    allowed_docs: Vec<bool>,
    unit_count: f32,
    average_words: f32,
    pub(crate) corrections: Vec<Correction>,
}

impl<'a> Searcher<'a> {
    fn new(index: &'a Index, request: &'a SearchRequest) -> Searcher<'a> {
        let allowed_docs = index
            .docs
            .iter()
            .map(|doc| doc.live && passes_filters(doc, &request.filters))
            .collect();
        let total_words: u64 = index.units.iter().map(|unit| u64::from(unit.words)).sum();
        let unit_count = index.units.len().max(1) as f32;
        Searcher {
            index,
            request,
            allowed_docs,
            unit_count,
            average_words: (total_words as f32 / unit_count).max(1.0),
            corrections: Vec::new(),
        }
    }

    fn mark(&self) -> &'static str {
        if self.request.filters.titles_only {
            "\u{1}"
        } else {
            ""
        }
    }

    fn run(&mut self, parsed: &ParsedQuery) -> Result<SearchResults, SearchError> {
        let clauses: Vec<ClauseMatches> = parsed
            .clauses
            .iter()
            .map(|clause| self.match_clause(&clause.alternatives))
            .collect();
        let excluded_docs = self.excluded_docs(&parsed.excluded);
        let excluded_phrases: Vec<Vec<String>> = parsed
            .excluded
            .iter()
            .filter_map(|part| match part {
                Part::Phrase(words) => Some(words.clone()),
                Part::Word { .. } => None,
            })
            .collect();
        let candidates = self.combine(&clauses, &excluded_docs);
        let total_candidates = candidates.len();
        let documents_seen: HashSet<u32> = candidates
            .iter()
            .map(|(unit, _)| self.index.units[*unit as usize].doc)
            .collect();
        let limit = if self.request.filters.document.is_some() {
            candidates.len()
        } else {
            SECOND_ROUND
        };
        let checked = self.second_round(candidates, limit, &clauses, &excluded_phrases)?;
        let all_checked = total_candidates <= limit;
        let mut results = self.group(checked);
        results.total_passages = if all_checked {
            results.documents.iter().map(|d| d.matching_passages).sum()
        } else {
            total_candidates
        };
        results.total_documents = if all_checked {
            results.total_documents
        } else {
            documents_seen.len()
        };
        results.corrections = std::mem::take(&mut self.corrections);
        Ok(results)
    }

    // ---- first round: the word lists ----

    fn match_clause(&mut self, alternatives: &[Part]) -> ClauseMatches {
        let mut matches = ClauseMatches::default();
        for part in alternatives {
            match part {
                Part::Word { term, prefix } => {
                    let (text, headings) = self.expand_both(term, *prefix);
                    for expansion in &text {
                        matches.words.insert(strip_mark(&expansion.key).to_string());
                        self.score_key(&expansion.key, expansion.weight, false, &mut matches);
                    }
                    for expansion in &headings {
                        matches.words.insert(strip_mark(&expansion.key).to_string());
                    }
                    self.add_heading_bonus(&headings, &mut matches);
                }
                Part::Phrase(words) => {
                    self.match_phrase_candidates(words, &mut matches);
                    matches.words.extend(words.iter().cloned());
                    matches.phrases.push(words.clone());
                }
            }
        }
        matches
    }

    /// adds every passage holding `key` to the clause.
    fn score_key(&self, key: &str, weight: f32, phrase_only: bool, matches: &mut ClauseMatches) {
        let Some(list) = self.index.terms.get(key) else {
            return;
        };
        let idf = self.idf(list.len());
        for posting in list.iter() {
            let meta = &self.index.units[posting.unit as usize];
            if !self.allowed_docs[meta.doc as usize] {
                continue;
            }
            let score = weight * idf * self.saturate(posting.frequency, meta.words);
            let entry = matches
                .units
                .entry(posting.unit)
                .or_insert((0.0, phrase_only));
            entry.0 = entry.0.max(score);
            // a plain word match settles the clause; no phrase check needed
            entry.1 &= phrase_only;
        }
    }

    fn idf(&self, document_frequency: usize) -> f32 {
        let df = document_frequency as f32;
        (1.0 + (self.unit_count - df + 0.5) / (df + 0.5)).ln()
    }

    fn saturate(&self, frequency: u32, words: u32) -> f32 {
        let tf = frequency as f32;
        let length = words.max(1) as f32 / self.average_words;
        tf * (BM25_K1 + 1.0) / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * length))
    }

    fn add_heading_bonus(&self, expansions: &[Expansion], matches: &mut ClauseMatches) {
        let mut boosted = HashSet::new();
        for expansion in expansions {
            let Some(list) = self.index.terms.get(&expansion.key) else {
                continue;
            };
            for posting in list.iter() {
                if !boosted.insert(posting.unit) {
                    continue;
                }
                if let Some(entry) = matches.units.get_mut(&posting.unit) {
                    entry.0 += HEADING_BONUS * expansion.weight;
                } else if self.allowed_docs[self.index.units[posting.unit as usize].doc as usize] {
                    // the word is only in the title or a heading: still a match
                    matches
                        .units
                        .insert(posting.unit, (HEADING_BONUS * expansion.weight, false));
                }
            }
        }
    }

    /// passages holding every word of a phrase; the order is checked later.
    fn match_phrase_candidates(&self, words: &[String], matches: &mut ClauseMatches) {
        let mark = self.mark();
        let mut lists: Vec<_> = words
            .iter()
            .map(|word| self.index.terms.get(&format!("{mark}{word}")))
            .collect();
        if lists.iter().any(Option::is_none) {
            return;
        }
        lists.sort_by_key(|list| list.map_or(0, |l| l.len()));
        let smallest: HashMap<u32, u32> = lists[0]
            .unwrap()
            .iter()
            .map(|p| (p.unit, p.frequency))
            .collect();
        let mut common: HashMap<u32, f32> = smallest.keys().map(|unit| (*unit, 0.0)).collect();
        for list in &lists {
            let list = list.unwrap();
            let idf = self.idf(list.len());
            let mut next = HashMap::new();
            for posting in list.iter() {
                if let Some(score) = common.get(&posting.unit) {
                    let words = self.index.units[posting.unit as usize].words;
                    next.insert(
                        posting.unit,
                        score + idf * self.saturate(posting.frequency, words),
                    );
                }
            }
            common = next;
        }
        for (unit, score) in common {
            let meta = &self.index.units[unit as usize];
            if !self.allowed_docs[meta.doc as usize] {
                continue;
            }
            // an exact phrase is worth more than its words scattered
            let entry = matches.units.entry(unit).or_insert((0.0, true));
            entry.0 = entry.0.max(score * 1.5);
        }
    }

    /// documents holding an excluded word anywhere: "-plant" leaves out
    /// whole documents about plants, not just their plant pages. excluded
    /// phrases are checked passage by passage in the second round.
    fn excluded_docs(&self, excluded: &[Part]) -> HashSet<u32> {
        let mut docs = HashSet::new();
        for part in excluded {
            if let Part::Word { term, .. } = part {
                let mut keys = vec![term.clone()];
                keys.extend(self.same_stem_keys(term, ""));
                for key in keys {
                    if let Some(list) = self.index.terms.get(&key) {
                        docs.extend(list.iter().map(|p| self.index.units[p.unit as usize].doc));
                    }
                }
            }
        }
        docs
    }

    /// passages that match every clause, with their summed scores.
    fn combine(&self, clauses: &[ClauseMatches], excluded: &HashSet<u32>) -> Vec<(u32, f32)> {
        let Some(smallest) = clauses.iter().min_by_key(|c| c.units.len()) else {
            return Vec::new();
        };
        let mut combined = Vec::new();
        'units: for unit in smallest.units.keys() {
            if excluded.contains(&self.index.units[*unit as usize].doc) {
                continue;
            }
            let mut score = 0.0;
            for clause in clauses {
                match clause.units.get(unit) {
                    Some((clause_score, _)) => score += clause_score,
                    None => continue 'units,
                }
            }
            combined.push((*unit, score));
        }
        combined.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        combined
    }

    // ---- second round: reading the passages ----

    fn second_round(
        &self,
        candidates: Vec<(u32, f32)>,
        limit: usize,
        clauses: &[ClauseMatches],
        excluded_phrases: &[Vec<String>],
    ) -> Result<Vec<CheckedPassage>, SearchError> {
        let mut reader = PassageReader::open(self.index.passages_file())?;
        let mut checked = Vec::new();
        for (unit, score) in candidates.into_iter().take(limit) {
            let meta = self.index.units[unit as usize];
            let passage = reader.read(&meta)?;
            let tokens = tokenize(&passage.text);
            let terms: Vec<&str> = tokens.iter().map(|t| t.term.as_str()).collect();
            if !self.phrases_hold(unit, clauses, &terms) {
                continue;
            }
            if excluded_phrases.iter().any(|p| contains_phrase(&terms, p)) {
                continue;
            }
            let (window, matched) = closest_window(&terms, clauses);
            let closeness = if clauses.len() > 1 {
                window.map_or(0.0, |span| clauses.len() as f32 / span as f32)
            } else {
                0.0
            };
            checked.push((
                unit,
                score * (1.0 + 0.5 * closeness),
                passage,
                tokens,
                matched,
            ));
        }
        Ok(checked)
    }

    /// every clause matched only through a phrase must find it in order.
    fn phrases_hold(&self, unit: u32, clauses: &[ClauseMatches], terms: &[&str]) -> bool {
        clauses.iter().all(|clause| match clause.units.get(&unit) {
            Some((_, true)) => {
                // titles only: the phrase was checked against heading words
                self.request.filters.titles_only
                    || clause.phrases.iter().any(|p| contains_phrase(terms, p))
            }
            _ => true,
        })
    }

    fn group(&self, checked: Vec<CheckedPassage>) -> SearchResults {
        let mut by_doc: HashMap<u32, Vec<PassageHit>> = HashMap::new();
        for (unit, score, passage, tokens, matched) in checked {
            let doc = self.index.units[unit as usize].doc;
            let (snippet, highlights) = make_snippet(&passage.text, &tokens, &matched);
            by_doc.entry(doc).or_default().push(PassageHit {
                number: passage.number,
                page: passage.page,
                heading: passage.heading,
                snippet,
                highlights,
                score,
            });
        }
        let total_documents = by_doc.len();
        let mut documents: Vec<DocumentHit> = by_doc
            .into_iter()
            .map(|(doc, passages)| self.document_hit(&self.index.docs[doc as usize], passages))
            .collect();
        documents.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.path.cmp(&b.path)));
        documents.truncate(self.request.max_documents.max(1));
        SearchResults {
            documents,
            total_documents,
            ..SearchResults::default()
        }
    }

    fn document_hit(&self, doc: &DocRecord, mut passages: Vec<PassageHit>) -> DocumentHit {
        passages.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.number.cmp(&b.number)));
        let best = passages.first().map_or(0.0, |p| p.score);
        // a document with many good pages beats one with a single lucky page
        let rest: f32 = passages.iter().skip(1).take(4).map(|p| p.score).sum();
        let score = (best + 0.15 * rest) * self.recency(doc.fingerprint.modified);
        let matching_passages = passages.len();
        if self.request.filters.document.is_some() {
            // within one document, pages read best in their own order
            passages.sort_by_key(|p| p.number);
        }
        passages.truncate(self.request.passages_per_document.max(1));
        DocumentHit {
            path: doc.path.clone(),
            title: doc.title.clone(),
            kind: doc.kind,
            modified: doc.fingerprint.modified,
            pages: doc.pages,
            notes: doc.notes.clone(),
            matching_passages,
            passages,
            score,
        }
    }

    /// up to 15% more for a document changed recently, fading over months.
    fn recency(&self, modified: u64) -> f32 {
        if self.request.now == 0 || modified == 0 || modified > self.request.now {
            return 1.0;
        }
        let days = (self.request.now - modified) as f32 / 86_400_000.0;
        1.0 + 0.15 * (-days / 90.0).exp()
    }
}

fn passes_filters(doc: &DocRecord, filters: &Filters) -> bool {
    if !filters.kinds.is_empty() && !filters.kinds.contains(&doc.kind) {
        return false;
    }
    if let Some(document) = &filters.document {
        if path_key(document) != path_key(&doc.path) {
            return false;
        }
    }
    if let Some(folder) = &filters.folder {
        let folder = path_key(folder.trim_end_matches(['\\', '/']));
        let path = path_key(&doc.path);
        let inside = path
            .strip_prefix(&folder)
            .is_some_and(|rest| rest.starts_with(['\\', '/']));
        if !inside {
            return false;
        }
    }
    let modified = doc.fingerprint.modified;
    if filters.modified_after.is_some_and(|after| modified < after) {
        return false;
    }
    if filters
        .modified_before
        .is_some_and(|before| modified > before)
    {
        return false;
    }
    true
}
