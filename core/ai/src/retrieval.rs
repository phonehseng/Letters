//! finding the passages that talk about something.
//!
//! the built-in ranker is bm25, the classic search-engine formula: a
//! passage scores higher when it contains the query's words often, when
//! those words are rare in the rest of the document, and when the passage
//! is not just long. it needs no model, so "jump to where it talks about
//! x" works offline and for free.
//!
//! when the provider can turn text into embeddings (lists of numbers where
//! similar meanings sit close together), both scores are mixed, so a
//! passage about "money" can be found by asking about "finance".

use std::collections::HashMap;

/// how strongly repeating a word keeps raising the score.
const BM25_K1: f32 = 1.2;
/// how much long passages are held back.
const BM25_B: f32 = 0.75;

/// how much of the mixed score comes from word matches (the rest from
/// embeddings). even weighting is a plain, explainable starting point.
pub const LEXICAL_WEIGHT: f32 = 0.5;

/// common words that say nothing about a topic.
const STOP_WORDS: &[&str] = &[
    "a", "about", "an", "and", "are", "as", "at", "be", "been", "but", "by", "can", "did", "do",
    "does", "for", "from", "had", "has", "have", "how", "i", "if", "in", "into", "is", "it", "its",
    "me", "my", "of", "on", "or", "our", "so", "that", "the", "their", "them", "there", "these",
    "they", "this", "to", "was", "we", "were", "what", "when", "where", "which", "who", "why",
    "will", "with", "would", "you", "your", "talk", "talks", "say", "says", "document", "page",
];

/// splits text into lower-case, stemmed words, leaving out stop words.
pub fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        // single letters are left over from "mitochondria's" and the like
        .filter(|word| word.chars().count() > 1 || word.chars().any(|c| c.is_ascii_digit()))
        .map(str::to_lowercase)
        .filter(|word| !STOP_WORDS.contains(&word.as_str()))
        .map(|word| stem(&word))
        .collect()
}

/// a deliberately simple english stemmer.
///
/// it only strips the endings that most often hide a match: plurals,
/// "-ing", "-ed", "-ly" and a final "e", so "studies" finds "study" and
/// "running" finds "runs". a full stemmer would catch more but would be
/// much harder to read, and ranking only needs most words to line up.
pub fn stem(word: &str) -> String {
    if word.len() <= 3 || !word.is_ascii() {
        return word.to_string();
    }
    let mut word = strip_plural(word);
    for suffix in ["ing", "ed"] {
        if word.ends_with(suffix) && word.len() - suffix.len() >= 3 {
            word.truncate(word.len() - suffix.len());
            undouble(&mut word);
            break;
        }
    }
    if word.ends_with("ly") && word.len() >= 5 {
        word.truncate(word.len() - 2);
    }
    if word.ends_with('e') && word.len() >= 4 {
        word.pop();
    }
    word
}

fn strip_plural(word: &str) -> String {
    if let Some(base) = word
        .strip_suffix("ies")
        .or_else(|| word.strip_suffix("ied"))
    {
        if base.len() >= 2 {
            return format!("{base}y");
        }
    }
    if let Some(base) = word.strip_suffix("sses") {
        return format!("{base}ss");
    }
    let keeps_s = ["ss", "us", "is"]
        .iter()
        .any(|ending| word.ends_with(ending));
    match word.strip_suffix('s') {
        Some(base) if !keeps_s => base.to_string(),
        _ => word.to_string(),
    }
}

/// "runn" back to "run", but "fall" stays "fall".
fn undouble(word: &mut String) {
    let bytes = word.as_bytes();
    let n = bytes.len();
    if n >= 2 && bytes[n - 1] == bytes[n - 2] && !b"aeioulsz".contains(&bytes[n - 1]) {
        word.pop();
    }
}

/// a bm25 index over a list of passages.
pub struct Bm25 {
    passages: Vec<HashMap<String, u32>>,
    lengths: Vec<usize>,
    average_length: f32,
    document_frequency: HashMap<String, usize>,
}

impl Bm25 {
    pub fn new<'a>(texts: impl IntoIterator<Item = &'a str>) -> Bm25 {
        let mut passages = Vec::new();
        let mut lengths = Vec::new();
        let mut document_frequency: HashMap<String, usize> = HashMap::new();
        for text in texts {
            let words = terms(text);
            lengths.push(words.len());
            let mut counts: HashMap<String, u32> = HashMap::new();
            for word in words {
                *counts.entry(word).or_default() += 1;
            }
            for word in counts.keys() {
                *document_frequency.entry(word.clone()).or_default() += 1;
            }
            passages.push(counts);
        }
        let total: usize = lengths.iter().sum();
        let average_length = (total as f32 / lengths.len().max(1) as f32).max(1.0);
        Bm25 {
            passages,
            lengths,
            average_length,
            document_frequency,
        }
    }

    /// one score per passage, in passage order. zero means no query word
    /// appears in it at all.
    pub fn scores(&self, query: &str) -> Vec<f32> {
        let mut query_terms = terms(query);
        query_terms.sort();
        query_terms.dedup();
        (0..self.passages.len())
            .map(|index| self.score(index, &query_terms))
            .collect()
    }

    fn score(&self, index: usize, query_terms: &[String]) -> f32 {
        let counts = &self.passages[index];
        let length_ratio = self.lengths[index] as f32 / self.average_length;
        query_terms
            .iter()
            .filter_map(|term| {
                let frequency = *counts.get(term)? as f32;
                let saturation = frequency * (BM25_K1 + 1.0)
                    / (frequency + BM25_K1 * (1.0 - BM25_B + BM25_B * length_ratio));
                Some(self.rarity(term) * saturation)
            })
            .sum()
    }

    /// inverse document frequency: rare words count for more. the "+ 1"
    /// keeps it positive even for a word in every passage.
    fn rarity(&self, term: &str) -> f32 {
        let total = self.passages.len() as f32;
        let containing = *self.document_frequency.get(term).unwrap_or(&0) as f32;
        (1.0 + (total - containing + 0.5) / (containing + 0.5)).ln()
    }
}

/// how alike two embeddings are, from -1 to 1.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// mixes word-match scores with embedding similarities.
///
/// bm25 scores have no fixed scale, so they are divided by the best one
/// first; similarities below zero count as zero.
pub fn mix_scores(lexical: &[f32], semantic: &[f32]) -> Vec<f32> {
    let best = lexical.iter().cloned().fold(0.0_f32, f32::max);
    lexical
        .iter()
        .zip(semantic)
        .map(|(lex, sem)| {
            let lex = if best > 0.0 { lex / best } else { 0.0 };
            LEXICAL_WEIGHT * lex + (1.0 - LEXICAL_WEIGHT) * sem.max(0.0)
        })
        .collect()
}

/// passage indexes from best to worst, leaving out zero scores.
///
/// ties keep reading order, so equal passages come in page order.
pub fn rank(scores: &[f32], limit: usize) -> Vec<(usize, f32)> {
    let mut ranked: Vec<(usize, f32)> = scores
        .iter()
        .enumerate()
        .filter(|(_, score)| **score > 0.0)
        .map(|(index, score)| (index, *score))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.truncate(limit);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stemming_lines_up_common_forms() {
        assert_eq!(stem("studies"), stem("study"));
        assert_eq!(stem("studied"), stem("study"));
        assert_eq!(stem("running"), stem("runs"));
        assert_eq!(stem("planets"), stem("planet"));
        assert_eq!(stem("classes"), "class");
        assert_eq!(stem("making"), stem("make"));
        assert_eq!(stem("quickly"), "quick");
        assert_eq!(stem("falling"), "fall");
        // short words and other alphabets are left alone
        assert_eq!(stem("bus"), "bus");
        assert_eq!(stem("über"), "über");
    }

    #[test]
    fn terms_drop_stop_words_and_punctuation() {
        assert_eq!(
            terms("What is the Mitochondria's job?"),
            vec!["mitochondria", "job"]
        );
    }

    fn corpus() -> Vec<&'static str> {
        vec![
            "The mitochondria is the powerhouse of the cell and makes energy.",
            "Plants use photosynthesis to turn sunlight into sugar.",
            "Photosynthesis happens in chloroplasts. Photosynthesis needs light, water and carbon dioxide.",
            "The French revolution began in 1789.",
            "Cells divide by mitosis.",
        ]
    }

    #[test]
    fn bm25_puts_the_most_relevant_passage_first() {
        let index = Bm25::new(corpus());
        let ranked = rank(&index.scores("photosynthesis"), 10);
        let order: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
        // passage 2 says it twice and is about nothing else
        assert_eq!(order, vec![2, 1]);
    }

    #[test]
    fn bm25_prefers_rare_words() {
        let index = Bm25::new(corpus());
        let ranked = rank(&index.scores("cell energy revolution"), 10);
        let order: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
        assert_eq!(order[0], 0, "cell and energy both match passage 0");
        assert!(order.contains(&3));
        assert!(order.contains(&4), "cells matches cell after stemming");
    }

    #[test]
    fn nothing_relevant_means_nothing_ranked() {
        let index = Bm25::new(corpus());
        assert!(rank(&index.scores("volcano"), 10).is_empty());
        assert!(rank(&index.scores("what is the"), 10).is_empty());
    }

    #[test]
    fn cosine_and_mixing() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
        let mixed = mix_scores(&[2.0, 0.0, 1.0], &[0.0, 0.8, -0.5]);
        assert_eq!(mixed, vec![0.5, 0.4, 0.25]);
    }

    #[test]
    fn ties_keep_reading_order() {
        let ranked = rank(&[1.0, 2.0, 1.0, 0.0], 10);
        assert_eq!(ranked, vec![(1, 2.0), (0, 1.0), (2, 1.0)]);
    }
}
