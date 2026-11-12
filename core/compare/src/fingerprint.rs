//! a small summary of a page's text, used to match pages between versions.
//!
//! every run of three words on the page is hashed into a number. two pages
//! with mostly the same text share most of these numbers, wherever the text
//! sits; two different pages share almost none. comparing two sets of
//! numbers is far cheaper than comparing pages word by word, and a set is
//! only a few kilobytes, so every page of both documents can be held at once.

use crate::hashing::Fnv64;

/// at most this many numbers are kept per page; a page with more text is
/// summarised by its first ones, which is enough to recognise it.
const MAX_SHINGLES: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PageFingerprint {
    /// sorted, without repeats
    pub shingles: Vec<u64>,
    pub word_count: usize,
}

impl PageFingerprint {
    pub fn of_words<'a>(keys: impl Iterator<Item = &'a str>) -> PageFingerprint {
        let hashes: Vec<u64> = keys.map(word_hash).collect();
        let mut shingles: Vec<u64> = if hashes.len() < 3 {
            // too few words for runs of three: the words themselves will do
            hashes.clone()
        } else {
            hashes
                .windows(3)
                .map(|run| {
                    let mut hash = Fnv64::new();
                    for word in run {
                        hash.write_u64(*word);
                    }
                    hash.finish()
                })
                .collect()
        };
        shingles.truncate(MAX_SHINGLES);
        shingles.sort_unstable();
        shingles.dedup();
        PageFingerprint {
            shingles,
            word_count: hashes.len(),
        }
    }

    /// how alike two pages are, from 0 to 1: the share of their runs of
    /// words that both have (the jaccard index).
    pub fn similarity(&self, other: &PageFingerprint) -> f32 {
        let shared = shared_count(&self.shingles, &other.shingles);
        let all = self.shingles.len() + other.shingles.len() - shared;
        if all == 0 {
            return 0.0;
        }
        shared as f32 / all as f32
    }
}

fn word_hash(word: &str) -> u64 {
    crate::hashing::hash_str(word)
}

/// how many numbers two sorted lists have in common.
fn shared_count(a: &[u64], b: &[u64]) -> usize {
    let (mut i, mut j, mut shared) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
        }
    }
    shared
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print(text: &str) -> PageFingerprint {
        PageFingerprint::of_words(text.split_whitespace())
    }

    #[test]
    fn the_same_text_is_fully_similar() {
        let a = print("the quick brown fox jumps over the lazy dog");
        assert_eq!(a.similarity(&a.clone()), 1.0);
    }

    #[test]
    fn one_changed_word_keeps_most_of_the_similarity() {
        let a = print("one two three four five six seven eight nine ten eleven twelve");
        let b = print("one two three four five six SEVEN eight nine ten eleven twelve");
        let similarity = a.similarity(&b);
        assert!(similarity > 0.5 && similarity < 1.0, "{similarity}");
    }

    #[test]
    fn different_text_shares_nothing() {
        let a = print("alpha beta gamma delta epsilon");
        let b = print("one two three four five");
        assert_eq!(a.similarity(&b), 0.0);
        assert_eq!(
            PageFingerprint::default().similarity(&PageFingerprint::default()),
            0.0
        );
    }
}
