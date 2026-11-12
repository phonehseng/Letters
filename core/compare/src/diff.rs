//! the difference between two lists of words, as the fewest additions and removals.
//!
//! words are turned into numbers first (the same word, the same number), so
//! this file only ever compares numbers. it uses Myers' algorithm, the one
//! behind `git diff`: it finds the shortest edit by exploring edits of
//! growing length, so similar lists finish fast. its memory grows with the
//! square of the number of edits, so it gives up beyond `MAX_EDITS`; very
//! different lists are then split at words that appear exactly once in
//! both ("patience" anchors) and each piece is compared on its own.

use std::collections::HashMap;
use std::ops::Range;

/// the most edits Myers' algorithm looks for before the lists are split.
/// at 1,000 edits it keeps about 8 MB of history.
const MAX_EDITS: usize = 1000;

/// one step from the old list to the new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    /// the same words on both sides
    Same {
        left: Range<usize>,
        right: Range<usize>,
    },
    /// words only the old list has; they sat before `right_at` in the new list
    Removed { left: Range<usize>, right_at: usize },
    /// words only the new list has; they sit before `left_at` in the old list
    Added { right: Range<usize>, left_at: usize },
}

/// one word's fate, before steps are gathered into ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Same(usize, usize),
    Removed(usize),
    Added(usize),
}

/// the edits that turn `left` into `right`.
pub(crate) fn diff(left: &[u32], right: &[u32]) -> Vec<Edit> {
    let mut steps = Vec::with_capacity(left.len().max(right.len()));
    diff_range(left, right, 0, 0, &mut steps);
    gather(&steps)
}

/// compares `left` and `right`, which start at `left_start` and
/// `right_start` in the whole lists, adding the steps found.
fn diff_range(
    left: &[u32],
    right: &[u32],
    left_start: usize,
    right_start: usize,
    steps: &mut Vec<Step>,
) {
    // the same words at both ends need no searching
    let prefix = left.iter().zip(right).take_while(|(a, b)| a == b).count();
    let suffix = left[prefix..]
        .iter()
        .rev()
        .zip(right[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    for offset in 0..prefix {
        steps.push(Step::Same(left_start + offset, right_start + offset));
    }
    let middle_left = &left[prefix..left.len() - suffix];
    let middle_right = &right[prefix..right.len() - suffix];
    let (middle_left_start, middle_right_start) = (left_start + prefix, right_start + prefix);
    if middle_left.is_empty() || middle_right.is_empty() {
        steps
            .extend((0..middle_left.len()).map(|offset| Step::Removed(middle_left_start + offset)));
        steps
            .extend((0..middle_right.len()).map(|offset| Step::Added(middle_right_start + offset)));
    } else if let Some(found) = myers(middle_left, middle_right) {
        steps.extend(found.into_iter().map(|step| match step {
            Step::Same(a, b) => Step::Same(a + middle_left_start, b + middle_right_start),
            Step::Removed(a) => Step::Removed(a + middle_left_start),
            Step::Added(b) => Step::Added(b + middle_right_start),
        }));
    } else {
        diff_by_anchors(
            middle_left,
            middle_right,
            middle_left_start,
            middle_right_start,
            steps,
        );
    }
    let (left_end, right_end) = (left_start + left.len(), right_start + right.len());
    for offset in (0..suffix).rev() {
        steps.push(Step::Same(left_end - 1 - offset, right_end - 1 - offset));
    }
}

/// Myers' greedy algorithm. `None` when the lists need more than `MAX_EDITS` edits.
fn myers(left: &[u32], right: &[u32]) -> Option<Vec<Step>> {
    let n = left.len() as isize;
    let m = right.len() as isize;
    let limit = ((n + m) as usize).min(MAX_EDITS) as isize;
    // furthest[k + offset] is how far along `left` the best path on diagonal k got
    let offset = limit + 1;
    let mut furthest = vec![0isize; (2 * limit + 3) as usize];
    // a copy of the diagonals -d..=d before each round d, to walk back afterwards
    let mut history: Vec<Vec<isize>> = Vec::new();
    for d in 0..=limit {
        history.push(furthest[(offset - d) as usize..=(offset + d) as usize].to_vec());
        for k in (-d..=d).step_by(2) {
            let index = (offset + k) as usize;
            let mut x = if k == -d || (k != d && furthest[index - 1] < furthest[index + 1]) {
                furthest[index + 1]
            } else {
                furthest[index - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && left[x as usize] == right[y as usize] {
                x += 1;
                y += 1;
            }
            furthest[index] = x;
            if x >= n && y >= m {
                return Some(walk_back(&history, n, m, d));
            }
        }
    }
    None
}

/// rebuilds the path from the end, round by round.
fn walk_back(history: &[Vec<isize>], n: isize, m: isize, edits: isize) -> Vec<Step> {
    let mut steps = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (1..=edits).rev() {
        let before = &history[d as usize];
        // before[k + d] holds diagonal k after round d - 1
        let at = |k: isize| before[(k + d) as usize];
        let k = x - y;
        let previous_k = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let previous_x = at(previous_k);
        let previous_y = previous_x - previous_k;
        while x > previous_x && y > previous_y {
            x -= 1;
            y -= 1;
            steps.push(Step::Same(x as usize, y as usize));
        }
        if x == previous_x {
            y -= 1;
            steps.push(Step::Added(y as usize));
        } else {
            x -= 1;
            steps.push(Step::Removed(x as usize));
        }
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        steps.push(Step::Same(x as usize, y as usize));
    }
    steps.reverse();
    steps
}

/// splits two very different lists at words that appear exactly once in
/// each, keeping the longest run of such words that is in order on both
/// sides, and compares the pieces between them.
fn diff_by_anchors(
    left: &[u32],
    right: &[u32],
    left_start: usize,
    right_start: usize,
    steps: &mut Vec<Step>,
) {
    let anchors = unique_anchors(left, right);
    if anchors.is_empty() {
        // nothing in common to hold on to: everything was rewritten
        steps.extend((0..left.len()).map(|offset| Step::Removed(left_start + offset)));
        steps.extend((0..right.len()).map(|offset| Step::Added(right_start + offset)));
        return;
    }
    let (mut a, mut b) = (0, 0);
    for (anchor_a, anchor_b) in anchors {
        diff_range(
            &left[a..anchor_a],
            &right[b..anchor_b],
            left_start + a,
            right_start + b,
            steps,
        );
        steps.push(Step::Same(left_start + anchor_a, right_start + anchor_b));
        a = anchor_a + 1;
        b = anchor_b + 1;
    }
    diff_range(
        &left[a..],
        &right[b..],
        left_start + a,
        right_start + b,
        steps,
    );
}

/// positions of words found exactly once in each list, the longest chain
/// of them that is in order on both sides.
fn unique_anchors(left: &[u32], right: &[u32]) -> Vec<(usize, usize)> {
    let mut seen: HashMap<u32, (usize, usize, Option<usize>, usize)> = HashMap::new();
    for (position, word) in left.iter().enumerate() {
        let entry = seen.entry(*word).or_insert((0, position, None, 0));
        entry.0 += 1;
    }
    for (position, word) in right.iter().enumerate() {
        if let Some(entry) = seen.get_mut(word) {
            entry.3 += 1;
            entry.2 = Some(position);
        }
    }
    let mut pairs: Vec<(usize, usize)> = seen
        .values()
        .filter(|(left_count, _, _, right_count)| *left_count == 1 && *right_count == 1)
        .filter_map(|(_, a, b, _)| b.map(|b| (*a, b)))
        .collect();
    pairs.sort_unstable();
    longest_increasing_by_right(&pairs)
}

/// the longest run of pairs whose right positions go up (patience sorting).
fn longest_increasing_by_right(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    // tails[i] is the pair index ending the best run of length i + 1
    let mut tails: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = vec![None; pairs.len()];
    for (index, pair) in pairs.iter().enumerate() {
        let place = tails.partition_point(|&tail| pairs[tail].1 < pair.1);
        if place > 0 {
            previous[index] = Some(tails[place - 1]);
        }
        if place == tails.len() {
            tails.push(index);
        } else {
            tails[place] = index;
        }
    }
    let mut run = Vec::new();
    let mut at = tails.last().copied();
    while let Some(index) = at {
        run.push(pairs[index]);
        at = previous[index];
    }
    run.reverse();
    run
}

/// gathers single steps into ranges.
fn gather(steps: &[Step]) -> Vec<Edit> {
    let mut edits: Vec<Edit> = Vec::new();
    let (mut next_left, mut next_right) = (0, 0);
    for step in steps {
        match *step {
            Step::Same(a, b) => {
                match edits.last_mut() {
                    Some(Edit::Same { left, right }) if left.end == a && right.end == b => {
                        left.end += 1;
                        right.end += 1;
                    }
                    _ => edits.push(Edit::Same {
                        left: a..a + 1,
                        right: b..b + 1,
                    }),
                }
                next_left = a + 1;
                next_right = b + 1;
            }
            Step::Removed(a) => {
                match edits.last_mut() {
                    Some(Edit::Removed { left, .. }) if left.end == a => left.end += 1,
                    _ => edits.push(Edit::Removed {
                        left: a..a + 1,
                        right_at: next_right,
                    }),
                }
                next_left = a + 1;
            }
            Step::Added(b) => {
                match edits.last_mut() {
                    Some(Edit::Added { right, .. }) if right.end == b => right.end += 1,
                    _ => edits.push(Edit::Added {
                        right: b..b + 1,
                        left_at: next_left,
                    }),
                }
                next_right = b + 1;
            }
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<u32> {
        text.chars().map(|ch| ch as u32).collect()
    }

    /// applies the edits to check they really turn one list into the other.
    fn rebuild(left: &[u32], right: &[u32], edits: &[Edit]) -> (Vec<u32>, Vec<u32>) {
        let mut old = Vec::new();
        let mut new = Vec::new();
        for edit in edits {
            match edit {
                Edit::Same { left: a, right: b } => {
                    assert_eq!(&left[a.clone()], &right[b.clone()]);
                    old.extend(&left[a.clone()]);
                    new.extend(&right[b.clone()]);
                }
                Edit::Removed { left: a, .. } => old.extend(&left[a.clone()]),
                Edit::Added { right: b, .. } => new.extend(&right[b.clone()]),
            }
        }
        (old, new)
    }

    fn edit_count(edits: &[Edit]) -> usize {
        edits
            .iter()
            .map(|edit| match edit {
                Edit::Same { .. } => 0,
                Edit::Removed { left, .. } => left.len(),
                Edit::Added { right, .. } => right.len(),
            })
            .sum()
    }

    #[test]
    fn the_classic_example_needs_five_edits() {
        // the example from Myers' paper: ABCABBA to CBABAC takes 5 edits
        let (a, b) = (words("ABCABBA"), words("CBABAC"));
        let edits = diff(&a, &b);
        assert_eq!(edit_count(&edits), 5);
        assert_eq!(rebuild(&a, &b, &edits), (a, b));
    }

    #[test]
    fn one_changed_word_is_one_removal_and_one_addition() {
        let (a, b) = (words("the quick brown fox"), words("the quick green fox"));
        let edits = diff(&a, &b);
        assert_eq!(rebuild(&a, &b, &edits), (a.clone(), b.clone()));
        assert!(edits
            .iter()
            .any(|edit| matches!(edit, Edit::Removed { .. })));
        assert!(edits.iter().any(|edit| matches!(edit, Edit::Added { .. })));
    }

    #[test]
    fn identical_and_empty_lists() {
        let a = words("same");
        assert_eq!(
            diff(&a, &a),
            vec![Edit::Same {
                left: 0..4,
                right: 0..4
            }]
        );
        assert_eq!(
            diff(&[], &a),
            vec![Edit::Added {
                right: 0..4,
                left_at: 0
            }]
        );
        assert!(diff(&[], &[]).is_empty());
    }

    #[test]
    fn very_different_long_lists_still_give_a_correct_answer() {
        // more edits than Myers is allowed, with a few shared unique words
        let mut a: Vec<u32> = (0..3000).map(|n| n * 2).collect();
        let mut b: Vec<u32> = (0..3000).map(|n| n * 2 + 1).collect();
        a[1500] = 99_999;
        b[1600] = 99_999;
        let edits = diff(&a, &b);
        assert_eq!(rebuild(&a, &b, &edits), (a.clone(), b.clone()));
        assert!(edits.iter().any(|edit| matches!(edit, Edit::Same { .. })));
    }

    #[test]
    fn insert_positions_point_into_the_other_list() {
        let (a, b) = (words("abd"), words("abcd"));
        let edits = diff(&a, &b);
        assert!(edits.contains(&Edit::Added {
            right: 2..3,
            left_at: 2
        }));
    }
}
