//! widening a query word into the spellings that count for it: the word
//! itself, its other english forms, words it begins while typing, and near
//! misses for a typo. each is weighted below the word exactly as typed.

use std::collections::HashSet;
use std::ops::Bound;

use crate::fuzzy::{allowed_typos, distance_within};
use crate::index::HEADING_MARK;
use crate::postings::PostingList;
use crate::results::Correction;
use crate::search::{
    Expansion, Searcher, EXACT, MAX_PREFIX_WORDS, MAX_STEM_SCAN, MAX_TYPO_WORDS, PREFIX, SAME_STEM,
    TYPO,
};
use crate::stem::stem;

impl Searcher<'_> {
    /// a query word's spellings among the text words and among the title
    /// and heading words. a typo is looked for only when neither has the
    /// word in any form, and the correction is reported.
    pub(crate) fn expand_both(
        &mut self,
        term: &str,
        prefix: bool,
    ) -> (Vec<Expansion>, Vec<Expansion>) {
        let heading_mark = HEADING_MARK.to_string();
        if self.request.filters.titles_only {
            let mut headings = self.expand_word(term, prefix, &heading_mark);
            if headings.is_empty() {
                headings = self.typo_expansions(term, &heading_mark);
            }
            return (headings.clone(), headings);
        }
        let mut text = self.expand_word(term, prefix, "");
        let mut headings = self.expand_word(term, prefix, &heading_mark);
        if text.is_empty() && headings.is_empty() {
            text = self.typo_expansions(term, "");
            headings = text
                .iter()
                .map(|e| Expansion {
                    key: format!("{HEADING_MARK}{}", e.key),
                    weight: e.weight,
                })
                .collect();
        }
        (text, headings)
    }

    /// the spellings a query word stands for under `mark`, best first:
    /// the word itself, its other english forms, and words it begins.
    fn expand_word(&self, term: &str, prefix: bool, mark: &str) -> Vec<Expansion> {
        let exact_key = format!("{mark}{term}");
        let mut seen = HashSet::new();
        let mut expansions = Vec::new();
        let mut add = |key: String, weight: f32, expansions: &mut Vec<Expansion>| {
            if seen.insert(key.clone()) {
                expansions.push(Expansion { key, weight });
            }
        };
        if self.index.terms.contains_key(&exact_key) {
            add(exact_key.clone(), EXACT, &mut expansions);
        }
        for key in self.same_stem_keys(term, mark) {
            add(key, SAME_STEM, &mut expansions);
        }
        if prefix && term.chars().count() >= 2 {
            for key in self.prefix_keys(term, mark) {
                add(key, PREFIX, &mut expansions);
            }
        }
        expansions
    }

    fn typo_expansions(&mut self, term: &str, mark: &str) -> Vec<Expansion> {
        let typos = self.typo_keys(term, mark);
        if let Some(best) = typos.first() {
            self.corrections.push(Correction {
                typed: term.to_string(),
                used: strip_mark(best).to_string(),
            });
        }
        typos
            .into_iter()
            .map(|key| Expansion { key, weight: TYPO })
            .collect()
    }

    /// dictionary words with the same english stem ("ran" is not found,
    /// but "running", "runs" and "run" are). stems are nearly always the
    /// start of the word, so only words sharing that start are checked.
    pub(crate) fn same_stem_keys(&self, term: &str, mark: &str) -> Vec<String> {
        let target = stem(term);
        let base: String = target
            .chars()
            .take(target.chars().count().saturating_sub(1).max(3))
            .collect();
        let start = format!("{mark}{base}");
        self.keys_starting_with(&start)
            .take(MAX_STEM_SCAN)
            .filter(|key| {
                let word = strip_mark(key);
                word != term && stem(word) == target
            })
            .map(str::to_string)
            .collect()
    }

    /// the most common words that begin with what is being typed.
    fn prefix_keys(&self, term: &str, mark: &str) -> Vec<String> {
        let start = format!("{mark}{term}");
        let mut found: Vec<(&str, usize)> = self
            .keys_starting_with(&start)
            .map(|key| (key, self.index.terms[key].len()))
            .collect();
        found.sort_by(|a, b| b.1.cmp(&a.1));
        found
            .into_iter()
            .take(MAX_PREFIX_WORDS)
            .map(|(key, _)| key.to_string())
            .collect()
    }

    /// near misses for a typed word, closest and most common first.
    fn typo_keys(&self, term: &str, mark: &str) -> Vec<String> {
        let limit = allowed_typos(term);
        if limit == 0 {
            return Vec::new();
        }
        let length = term.chars().count();
        let mut found: Vec<(usize, usize, &str)> = Vec::new();
        for (key, list) in self.dictionary_part(mark) {
            let word = strip_mark(key);
            // cheap tests first: most of the dictionary is ruled out by length
            if word.len().abs_diff(length) > limit + 2
                || word.chars().count().abs_diff(length) > limit
            {
                continue;
            }
            if let Some(distance) = distance_within(term, word, limit) {
                found.push((distance, list.len(), key));
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        found
            .into_iter()
            .take(MAX_TYPO_WORDS)
            .map(|(_, _, key)| key.to_string())
            .collect()
    }

    fn keys_starting_with<'k>(&'k self, start: &'k str) -> impl Iterator<Item = &'k str> + 'k {
        self.index
            .terms
            .range::<str, _>((Bound::Included(start), Bound::Unbounded))
            .map(|(key, _)| key.as_str())
            .take_while(move |key| key.starts_with(start))
    }

    /// the text words, or the title and heading words.
    fn dictionary_part(
        &self,
        mark: &str,
    ) -> Box<dyn Iterator<Item = (&String, &PostingList)> + '_> {
        if mark.is_empty() {
            Box::new(
                self.index
                    .terms
                    .range::<str, _>((Bound::Included("\u{2}"), Bound::Unbounded)),
            )
        } else {
            Box::new(
                self.index
                    .terms
                    .range::<str, _>((Bound::Included("\u{1}"), Bound::Excluded("\u{2}"))),
            )
        }
    }
}

/// a dictionary key without the title and heading mark.
pub(crate) fn strip_mark(key: &str) -> &str {
    key.strip_prefix(HEADING_MARK).unwrap_or(key)
}
