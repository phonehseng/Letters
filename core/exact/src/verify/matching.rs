//! pairing the glyphs of two pdfs in reading order, the way python's
//! `difflib.SequenceMatcher` (without junk heuristics) does, so the numbers
//! match the prototype's harness.
//!
//! a plain zip goes wrong as soon as word drops or adds one char, so the
//! sequences are aligned on their text first and only equal runs are paired.

use std::collections::HashMap;

/// a block of equal items: `a[a_start..a_start+size] == b[b_start..b_start+size]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub a_start: usize,
    pub b_start: usize,
    pub size: usize,
}

fn longest_match(
    a: &[&str],
    positions: &HashMap<&str, Vec<usize>>,
    a_range: (usize, usize),
    b_range: (usize, usize),
) -> Block {
    let (a_low, a_high) = a_range;
    let (b_low, b_high) = b_range;
    let mut best = Block {
        a_start: a_low,
        b_start: b_low,
        size: 0,
    };
    let mut lengths: HashMap<usize, usize> = HashMap::new();
    for (i, item) in a.iter().enumerate().take(a_high).skip(a_low) {
        let mut next: HashMap<usize, usize> = HashMap::new();
        for &j in positions.get(item).map(Vec::as_slice).unwrap_or(&[]) {
            if j < b_low {
                continue;
            }
            if j >= b_high {
                break;
            }
            let k = if j > 0 {
                lengths.get(&(j - 1)).copied().unwrap_or(0)
            } else {
                0
            } + 1;
            next.insert(j, k);
            if k > best.size {
                best = Block {
                    a_start: i + 1 - k,
                    b_start: j + 1 - k,
                    size: k,
                };
            }
        }
        lengths = next;
    }
    best
}

/// the matching blocks of two sequences, in order.
pub fn matching_blocks(a: &[&str], b: &[&str]) -> Vec<Block> {
    let mut positions: HashMap<&str, Vec<usize>> = HashMap::new();
    for (j, item) in b.iter().enumerate() {
        positions.entry(item).or_default().push(j);
    }
    let mut queue = vec![(0, a.len(), 0, b.len())];
    let mut blocks = Vec::new();
    while let Some((a_low, a_high, b_low, b_high)) = queue.pop() {
        let block = longest_match(a, &positions, (a_low, a_high), (b_low, b_high));
        if block.size == 0 {
            continue;
        }
        blocks.push(block);
        if a_low < block.a_start && b_low < block.b_start {
            queue.push((a_low, block.a_start, b_low, block.b_start));
        }
        if block.a_start + block.size < a_high && block.b_start + block.size < b_high {
            queue.push((
                block.a_start + block.size,
                a_high,
                block.b_start + block.size,
                b_high,
            ));
        }
    }
    blocks.sort_by_key(|b| (b.a_start, b.b_start));
    blocks
}

/// index pairs (in a, in b) of every matched item.
pub fn matched_pairs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    matching_blocks(a, b)
        .into_iter()
        .flat_map(|block| (0..block.size).map(move |k| (block.a_start + k, block.b_start + k)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<String> {
        text.chars().map(String::from).collect()
    }

    #[test]
    fn alignment_survives_a_dropped_char() {
        // the prototype's case (test_units.py)
        let (a, b) = (chars("abcdef"), chars("abdef"));
        let a: Vec<&str> = a.iter().map(String::as_str).collect();
        let b: Vec<&str> = b.iter().map(String::as_str).collect();
        let pairs = matched_pairs(&a, &b);
        let matched: String = pairs.iter().map(|&(i, _)| a[i]).collect();
        assert_eq!(matched, "abdef");
        assert!(pairs.iter().all(|&(i, j)| a[i] == b[j]));
    }

    #[test]
    fn the_longest_block_comes_first_like_difflib() {
        // "xabc" and "abcx" are both 4 long; difflib takes the one that
        // starts earliest in a: SequenceMatcher(None, "xabcx",
        // "abcxabc").find_longest_match(0, 5, 0, 7) is (0, 3, 4)
        let a = ["x", "a", "b", "c", "x"];
        let b = ["a", "b", "c", "x", "a", "b", "c"];
        let blocks = matching_blocks(&a, &b);
        assert_eq!(
            blocks[0],
            Block {
                a_start: 0,
                b_start: 3,
                size: 4
            }
        );
    }
}
