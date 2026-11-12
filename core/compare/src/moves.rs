//! finding passages that moved, so they aren't reported as removed and added.
//!
//! comparing word lists in order sees a moved paragraph as "removed here"
//! and "added there". after every page pair is compared, each removed
//! passage is checked against each added passage, on any page: when they
//! hold nearly the same words, the two become one "moved" change.

use std::collections::HashMap;

/// passages must share at least this much of their words to count as the
/// same passage. a little room is left for small edits made while moving.
const MIN_SIMILARITY: f32 = 0.85;

/// the most removed or added passages checked against each other. beyond
/// this (a rewritten document) moves are not looked for, to stay quick.
const MAX_PASSAGES: usize = 2000;

/// a removed or added passage, and where its change is in the result.
#[derive(Clone, Debug)]
pub(crate) struct Passage {
    pub pair: usize,
    pub change: usize,
    pub keys: Vec<String>,
}

/// a removed passage and the added passage it became.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Move {
    /// index into the removed list
    pub removed: usize,
    /// index into the added list
    pub added: usize,
    pub similarity: f32,
}

/// pairs removed passages with added passages that hold the same words.
/// each passage is used at most once, best matches first.
pub(crate) fn find_moves(removed: &[Passage], added: &[Passage], min_words: usize) -> Vec<Move> {
    if removed.len() > MAX_PASSAGES || added.len() > MAX_PASSAGES {
        return Vec::new();
    }
    let long_enough = |passage: &Passage| passage.keys.len() >= min_words.max(1);
    let mut found: Vec<Move> = Vec::new();
    for (r, old) in removed
        .iter()
        .enumerate()
        .filter(|(_, passage)| long_enough(passage))
    {
        for (a, new) in added
            .iter()
            .enumerate()
            .filter(|(_, passage)| long_enough(passage))
        {
            let similarity = word_similarity(&old.keys, &new.keys);
            if similarity >= MIN_SIMILARITY {
                found.push(Move {
                    removed: r,
                    added: a,
                    similarity,
                });
            }
        }
    }
    found.sort_by(|x, y| y.similarity.total_cmp(&x.similarity));
    let mut removed_used = vec![false; removed.len()];
    let mut added_used = vec![false; added.len()];
    found.retain(|found| {
        let free = !removed_used[found.removed] && !added_used[found.added];
        if free {
            removed_used[found.removed] = true;
            added_used[found.added] = true;
        }
        free
    });
    found
}

/// how many words two passages share, counting repeats, as a share of
/// both (the dice coefficient). 1 means the same words, in any order.
fn word_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    // very different lengths can't be the same passage
    let (short, long) = (a.len().min(b.len()), a.len().max(b.len()));
    if (short as f32) < MIN_SIMILARITY * long as f32 {
        return 0.0;
    }
    let mut counts: HashMap<&str, i32> = HashMap::new();
    for word in a {
        *counts.entry(word.as_str()).or_default() += 1;
    }
    let mut shared = 0;
    for word in b {
        if let Some(count) = counts.get_mut(word.as_str()) {
            if *count > 0 {
                *count -= 1;
                shared += 1;
            }
        }
    }
    2.0 * shared as f32 / (a.len() + b.len()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passage(pair: usize, text: &str) -> Passage {
        Passage {
            pair,
            change: 0,
            keys: text.split_whitespace().map(str::to_owned).collect(),
        }
    }

    #[test]
    fn a_passage_found_elsewhere_is_a_move() {
        let removed = [passage(0, "the moved paragraph has these words in it")];
        let added = [
            passage(0, "something else entirely was written here"),
            passage(2, "the moved paragraph has these words in it"),
        ];
        let moves = find_moves(&removed, &added, 4);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].added, 1);
        assert_eq!(moves[0].similarity, 1.0);
    }

    #[test]
    fn short_passages_are_not_moves() {
        let removed = [passage(0, "the cat")];
        let added = [passage(0, "the cat")];
        assert!(find_moves(&removed, &added, 4).is_empty());
    }

    #[test]
    fn small_edits_still_count_as_a_move() {
        let removed = [passage(
            0,
            "one two three four five six seven eight nine ten",
        )];
        let added = [passage(
            1,
            "one two three four five six seven eight nine eleven",
        )];
        let moves = find_moves(&removed, &added, 4);
        assert_eq!(moves.len(), 1);
        assert!(moves[0].similarity < 1.0);
    }
}
