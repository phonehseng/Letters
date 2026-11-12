//! matching the pages of the old version with the pages of the new one.
//!
//! pages are matched before anything else, so that one added page doesn't
//! make every later page look completely changed. it works in four steps:
//! 1. candidates: for each old page, the new pages that share the most runs
//!    of words with it (found through an index, so 500 pages stay quick)
//! 2. the backbone: the best set of candidate pairs that keeps both
//!    documents in order (a weighted longest increasing subsequence)
//! 3. moved pages: a page left over on both sides that still clearly
//!    matches is a page that moved
//! 4. gaps: pages left over between two backbone pairs are paired with each
//!    other in order, as changed pages; any left after that were added or
//!    removed

use std::collections::HashMap;

use crate::fingerprint::PageFingerprint;

/// pages sharing at least this much of their text may be the same page.
const CANDIDATE_SIMILARITY: f32 = 0.2;

/// a page that changed position must still match this well, because
/// pairing pages out of order is a strong claim.
const MOVED_SIMILARITY: f32 = 0.5;

/// how many candidate new pages are kept for each old page.
const CANDIDATES_PER_PAGE: usize = 3;

/// a run of words found on more pages than this is boilerplate (a header
/// or footer) and says nothing about which page is which.
const MAX_PAGES_PER_SHINGLE: usize = 8;

/// how one page of either document is accounted for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PageMatch {
    /// old page `left` became new page `right`; `moved` when it is out of order
    Pair {
        left: u32,
        right: u32,
        similarity: f32,
        moved: bool,
    },
    /// a page only the old document has
    Removed { left: u32 },
    /// a page only the new document has
    Added { right: u32 },
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    left: usize,
    right: usize,
    similarity: f32,
}

/// matches the pages of two documents. the answer accounts for every page
/// of both exactly once, in the order the new document reads.
pub(crate) fn align_pages(left: &[PageFingerprint], right: &[PageFingerprint]) -> Vec<PageMatch> {
    let candidates = find_candidates(left, right);
    let backbone = ordered_backbone(&candidates, right.len());
    let mut left_used = vec![false; left.len()];
    let mut right_used = vec![false; right.len()];
    let mut matches: Vec<PageMatch> = Vec::new();
    for candidate in &backbone {
        left_used[candidate.left] = true;
        right_used[candidate.right] = true;
    }
    let moved = moved_pages(&candidates, &mut left_used, &mut right_used);
    let gap_pairs = fill_gaps(&backbone, left, right, &mut left_used, &mut right_used);

    for candidate in backbone.iter().chain(&gap_pairs) {
        matches.push(pair(candidate, false));
    }
    for candidate in &moved {
        matches.push(pair(candidate, true));
    }
    for (page, used) in left_used.iter().enumerate() {
        if !used {
            matches.push(PageMatch::Removed { left: page as u32 });
        }
    }
    for (page, used) in right_used.iter().enumerate() {
        if !used {
            matches.push(PageMatch::Added { right: page as u32 });
        }
    }
    sort_for_reading(&mut matches);
    matches
}

fn pair(candidate: &Candidate, moved: bool) -> PageMatch {
    PageMatch::Pair {
        left: candidate.left as u32,
        right: candidate.right as u32,
        similarity: candidate.similarity,
        moved,
    }
}

/// for each old page, the few new pages most like it.
fn find_candidates(left: &[PageFingerprint], right: &[PageFingerprint]) -> Vec<Candidate> {
    // which new pages each run of words appears on
    let mut index: HashMap<u64, Vec<u32>> = HashMap::new();
    for (page, print) in right.iter().enumerate() {
        for shingle in &print.shingles {
            index.entry(*shingle).or_default().push(page as u32);
        }
    }
    let mut candidates = Vec::new();
    let mut shared: HashMap<u32, usize> = HashMap::new();
    for (page, print) in left.iter().enumerate() {
        shared.clear();
        for shingle in &print.shingles {
            match index.get(shingle) {
                Some(pages) if pages.len() <= MAX_PAGES_PER_SHINGLE => {
                    for &other in pages {
                        *shared.entry(other).or_default() += 1;
                    }
                }
                _ => {}
            }
        }
        let mut found: Vec<Candidate> = shared
            .keys()
            .map(|&other| Candidate {
                left: page,
                right: other as usize,
                similarity: print.similarity(&right[other as usize]),
            })
            .filter(|candidate| candidate.similarity >= CANDIDATE_SIMILARITY)
            .collect();
        found.sort_by(|a, b| {
            b.similarity
                .total_cmp(&a.similarity)
                .then(a.right.cmp(&b.right))
        });
        found.truncate(CANDIDATES_PER_PAGE);
        candidates.extend(found);
    }
    candidates
}

/// the heaviest chain of candidates in which both page numbers go up.
///
/// every pair in the chain is worth 1 plus its similarity, so the chain
/// prefers matching more pages, and then matching them better. the best
/// chain ending at each new page is kept in a fenwick tree (an array that
/// answers "best so far before page j" quickly).
fn ordered_backbone(candidates: &[Candidate], right_count: usize) -> Vec<Candidate> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    // same old page: larger new page first, so one old page can't be used twice
    order.sort_by(|&a, &b| {
        candidates[a]
            .left
            .cmp(&candidates[b].left)
            .then(candidates[b].right.cmp(&candidates[a].right))
    });
    let mut tree = BestBefore::new(right_count);
    let mut best_ending: Vec<(f32, Option<usize>)> = vec![(0.0, None); candidates.len()];
    for &index in &order {
        let candidate = candidates[index];
        let (before_score, before) = tree.best_before(candidate.right);
        let score = before_score + 1.0 + candidate.similarity;
        best_ending[index] = (score, before);
        tree.offer(candidate.right, score, index);
    }
    let Some(mut last) =
        (0..candidates.len()).max_by(|&a, &b| best_ending[a].0.total_cmp(&best_ending[b].0))
    else {
        return Vec::new();
    };
    let mut chain = vec![candidates[last]];
    while let Some(previous) = best_ending[last].1 {
        chain.push(candidates[previous]);
        last = previous;
    }
    chain.reverse();
    chain
}

/// a fenwick tree over new page numbers holding the best chain score
/// ending at or before each page.
struct BestBefore {
    nodes: Vec<(f32, Option<usize>)>,
}

impl BestBefore {
    fn new(size: usize) -> Self {
        Self {
            nodes: vec![(0.0, None); size + 1],
        }
    }

    /// the best score of a chain ending before new page `page`.
    fn best_before(&self, page: usize) -> (f32, Option<usize>) {
        let mut best = (0.0, None);
        let mut position = page; // positions are 1-based, so this is "pages < page"
        while position > 0 {
            if self.nodes[position].0 > best.0 {
                best = self.nodes[position];
            }
            // step back to the node before this one's range: drop the lowest set bit
            position &= position - 1;
        }
        best
    }

    fn offer(&mut self, page: usize, score: f32, candidate: usize) {
        let mut position = page + 1;
        while position < self.nodes.len() {
            if score > self.nodes[position].0 {
                self.nodes[position] = (score, Some(candidate));
            }
            // move on to the next node that covers this page: add the lowest set bit
            position += position.isolate_lowest_one();
        }
    }
}

/// pairs left-over pages that still clearly match: pages that moved.
fn moved_pages(
    candidates: &[Candidate],
    left_used: &mut [bool],
    right_used: &mut [bool],
) -> Vec<Candidate> {
    let mut strong: Vec<Candidate> = candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.similarity >= MOVED_SIMILARITY)
        .filter(|candidate| !left_used[candidate.left] && !right_used[candidate.right])
        .collect();
    strong.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
    let mut moved = Vec::new();
    for candidate in strong {
        if !left_used[candidate.left] && !right_used[candidate.right] {
            left_used[candidate.left] = true;
            right_used[candidate.right] = true;
            moved.push(candidate);
        }
    }
    moved
}

/// pairs the pages left between two backbone pairs, in order.
///
/// when both sides have the same number of left-over pages they are paired
/// one to one: those are pages that changed too much to recognise. when
/// the numbers differ, the most similar pages are paired and the rest
/// count as added or removed.
fn fill_gaps(
    backbone: &[Candidate],
    left: &[PageFingerprint],
    right: &[PageFingerprint],
    left_used: &mut [bool],
    right_used: &mut [bool],
) -> Vec<Candidate> {
    let mut bounds: Vec<(usize, usize)> = vec![(0, 0)];
    bounds.extend(
        backbone
            .iter()
            .map(|candidate| (candidate.left + 1, candidate.right + 1)),
    );
    let mut ends: Vec<(usize, usize)> = backbone
        .iter()
        .map(|candidate| (candidate.left, candidate.right))
        .collect();
    ends.push((left.len(), right.len()));

    let mut pairs = Vec::new();
    for (start, end) in bounds.into_iter().zip(ends) {
        let left_gap: Vec<usize> = (start.0..end.0).filter(|&page| !left_used[page]).collect();
        let right_gap: Vec<usize> = (start.1..end.1).filter(|&page| !right_used[page]).collect();
        for candidate in pair_gap(&left_gap, &right_gap, left, right) {
            left_used[candidate.left] = true;
            right_used[candidate.right] = true;
            pairs.push(candidate);
        }
    }
    pairs
}

fn pair_gap(
    left_gap: &[usize],
    right_gap: &[usize],
    left: &[PageFingerprint],
    right: &[PageFingerprint],
) -> Vec<Candidate> {
    let similarity = |a: usize, b: usize| left[a].similarity(&right[b]);
    if left_gap.len() == right_gap.len() {
        return left_gap
            .iter()
            .zip(right_gap)
            .map(|(&a, &b)| Candidate {
                left: a,
                right: b,
                similarity: similarity(a, b),
            })
            .collect();
    }
    // unequal gaps: keep pages in order, pairing each with the best
    // remaining page on the other side when they share anything at all
    let mut pairs = Vec::new();
    let mut next_right = 0;
    for &a in left_gap {
        let best = (next_right..right_gap.len())
            .map(|position| (position, similarity(a, right_gap[position])))
            .filter(|(_, score)| *score > 0.0)
            .max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((position, score)) = best {
            pairs.push(Candidate {
                left: a,
                right: right_gap[position],
                similarity: score,
            });
            next_right = position + 1;
        }
    }
    if pairs.is_empty() {
        // nothing to go on (blank or scanned pages): pair them in order
        return left_gap
            .iter()
            .zip(right_gap)
            .map(|(&a, &b)| Candidate {
                left: a,
                right: b,
                similarity: 0.0,
            })
            .collect();
    }
    pairs
}

/// puts matches in the order of the new document. a removed page goes
/// right after the new page its old neighbour before it became.
fn sort_for_reading(matches: &mut [PageMatch]) {
    let mut right_of_left: HashMap<u32, u32> = HashMap::new();
    for found in matches.iter() {
        if let PageMatch::Pair { left, right, .. } = found {
            right_of_left.insert(*left, *right);
        }
    }
    let key = |found: &PageMatch| -> (f64, f64) {
        match *found {
            PageMatch::Pair { right, .. } | PageMatch::Added { right } => (right as f64, 0.0),
            PageMatch::Removed { left } => {
                let before = (0..left).rev().find_map(|page| right_of_left.get(&page));
                let place = before.map(|&right| right as f64 + 0.5).unwrap_or(-0.5);
                (place, left as f64)
            }
        }
    };
    matches.sort_by(|a, b| {
        let (a, b) = (key(a), key(b));
        a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a page whose words are unique to its name, so pages are easy to tell apart.
    fn page(name: &str) -> PageFingerprint {
        let words: Vec<String> = (0..40).map(|index| format!("{name}{index}")).collect();
        PageFingerprint::of_words(words.iter().map(String::as_str))
    }

    fn pages(names: &[&str]) -> Vec<PageFingerprint> {
        names.iter().map(|name| page(name)).collect()
    }

    fn summary(matches: &[PageMatch]) -> Vec<String> {
        matches
            .iter()
            .map(|found| match found {
                PageMatch::Pair {
                    left, right, moved, ..
                } => format!("{left}-{right}{}", if *moved { " moved" } else { "" }),
                PageMatch::Removed { left } => format!("{left}-"),
                PageMatch::Added { right } => format!("-{right}"),
            })
            .collect()
    }

    #[test]
    fn identical_documents_pair_page_by_page() {
        let doc = pages(&["a", "b", "c"]);
        assert_eq!(summary(&align_pages(&doc, &doc)), ["0-0", "1-1", "2-2"]);
    }

    #[test]
    fn an_inserted_page_does_not_shift_the_rest() {
        let old = pages(&["a", "b", "c", "d"]);
        let new = pages(&["a", "b", "x", "c", "d"]);
        assert_eq!(
            summary(&align_pages(&old, &new)),
            ["0-0", "1-1", "-2", "2-3", "3-4"]
        );
    }

    #[test]
    fn a_deleted_page_is_listed_where_it_was() {
        let old = pages(&["a", "b", "c", "d"]);
        let new = pages(&["a", "c", "d"]);
        assert_eq!(
            summary(&align_pages(&old, &new)),
            ["0-0", "1-", "2-1", "3-2"]
        );
    }

    #[test]
    fn reordered_pages_are_moved_pairs() {
        let old = pages(&["a", "b", "c", "d", "e"]);
        let new = pages(&["a", "d", "b", "c", "e"]);
        let found = summary(&align_pages(&old, &new));
        assert_eq!(found, ["0-0", "3-1 moved", "1-2", "2-3", "4-4"]);
    }

    #[test]
    fn unrecognisable_pages_pair_in_order() {
        let old = pages(&["a", "b", "c"]);
        let new = pages(&["x", "y", "z"]);
        assert_eq!(summary(&align_pages(&old, &new)), ["0-0", "1-1", "2-2"]);
    }

    #[test]
    fn pages_without_text_pair_in_order() {
        let blank = vec![PageFingerprint::default(); 3];
        assert_eq!(
            summary(&align_pages(&blank, &blank[..2])),
            ["0-0", "1-1", "2-"]
        );
    }
}
