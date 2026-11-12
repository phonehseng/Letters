//! helpers shared by the index tests: documents made from plain strings,
//! and a generated library for the speed and size measurements.

#![allow(dead_code)]

use letters_search::{DocKind, ExtractedDocument, Fingerprint, Index, Passage};

/// a document whose passages are the given texts, each under `heading`.
pub fn document(title: &str, pages: &[(&str, &str)]) -> ExtractedDocument {
    ExtractedDocument {
        title: title.to_string(),
        pages: pages.len() as u32,
        passages: pages
            .iter()
            .enumerate()
            .map(|(number, (heading, text))| Passage {
                number: number as u32,
                page: number as u32,
                heading: heading.to_string(),
                text: text.to_string(),
            })
            .collect(),
        notes: Vec::new(),
    }
}

pub fn print(modified: u64) -> Fingerprint {
    Fingerprint {
        size: 1,
        modified,
        hash: modified as u32,
    }
}

pub fn add(index: &mut Index, path: &str, title: &str, pages: &[(&str, &str)]) {
    index
        .add_document(path, DocKind::Pdf, print(1), document(title, pages))
        .unwrap();
}

/// a small, fast random number generator, so the generated library is
/// the same on every run.
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Random {
        Random(seed.wrapping_mul(6364136223846793005).wrapping_add(1))
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }
}

/// a vocabulary shaped like real text: a few very common words, many rare
/// ones (zipf's law), so word lists have realistic lengths.
pub struct Vocabulary {
    words: Vec<String>,
}

const COMMON: &[&str] = &[
    "the", "of", "and", "to", "in", "is", "that", "for", "it", "as", "was", "with", "be", "by",
    "on", "not", "this", "are", "or", "which", "from", "at", "an", "but", "have", "were", "their",
    "cell", "energy", "water", "theory", "history", "students", "chapter", "results", "method",
];

impl Vocabulary {
    pub fn new(size: usize, random: &mut Random) -> Vocabulary {
        let syllables = [
            "ba", "co", "di", "fe", "ga", "hi", "jo", "ku", "la", "me", "no", "pa", "qui", "ro",
            "sa", "te", "vi", "wo", "xa", "ze", "tion", "ment", "ing", "er", "al", "ous", "ic",
        ];
        let mut words: Vec<String> = COMMON.iter().map(|w| w.to_string()).collect();
        while words.len() < size {
            let count = 2 + random.below(3);
            let word: String = (0..count)
                .map(|_| syllables[random.below(syllables.len())])
                .collect();
            words.push(word);
        }
        Vocabulary { words }
    }

    /// a word, with common words far more likely than rare ones.
    pub fn word(&self, random: &mut Random) -> &str {
        // squaring a uniform number leans the choice towards the start
        let u = (random.next() % 1_000_000) as f64 / 1_000_000.0;
        let index = (u * u * u * self.words.len() as f64) as usize;
        &self.words[index.min(self.words.len() - 1)]
    }

    pub fn sentence(&self, random: &mut Random, words: usize) -> String {
        let mut text = String::with_capacity(words * 7);
        for i in 0..words {
            if i > 0 {
                text.push(if i % 14 == 0 { '.' } else { ' ' });
                if i % 14 == 0 {
                    text.push(' ');
                }
            }
            text.push_str(self.word(random));
        }
        text
    }
}
